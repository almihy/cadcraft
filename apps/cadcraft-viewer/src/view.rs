//! The view: which part of the drawing is on the canvas, and how it moves (pan, zoom about a
//! point, fit).

use cadcraft_geom::{Bounds2, Vec2};

/// The furthest in and out a view zooms, as a factor of the fitted scale.
const ZOOM_IN_MOST: f64 = 1e6;
const ZOOM_OUT_MOST: f64 = 1e-2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The drawing point at the canvas centre.
    pub center: Vec2,
    /// Points (CSS pixels) per drawing unit.
    pub scale: f64,
    /// The scale of the last fit, which bounds zooming.
    pub fitted: f64,
}

impl Default for View {
    fn default() -> Self {
        View { center: Vec2::ZERO, scale: 1.0, fitted: 1.0 }
    }
}

impl View {
    /// The view that shows `b` whole in a canvas `w` × `h` points, with a margin.
    pub fn fit(b: &Bounds2, w: f64, h: f64) -> View {
        if b.is_empty() || w < 1.0 || h < 1.0 {
            return View::default();
        }
        let (bw, bh) = (b.width().max(1e-9), b.height().max(1e-9));
        let scale = (w * 0.94 / bw).min(h * 0.94 / bh);
        View { center: b.center(), scale, fitted: scale }
    }

    /// A screen offset from the canvas centre (points, y down) to the drawing point under it.
    pub fn to_world(&self, d: (f64, f64)) -> Vec2 {
        Vec2::new(self.center.x + d.0 / self.scale, self.center.y - d.1 / self.scale)
    }

    /// A drawing point to its screen offset from the canvas centre (points, y down).
    pub fn to_screen(&self, p: Vec2) -> (f64, f64) {
        ((p.x - self.center.x) * self.scale, -(p.y - self.center.y) * self.scale)
    }

    /// Move the drawing by a screen drag of `d` points.
    pub fn pan(&mut self, d: (f64, f64)) {
        self.center = Vec2::new(self.center.x - d.0 / self.scale, self.center.y + d.1 / self.scale);
    }

    /// Zoom by `factor` keeping the drawing point under `at` (offset from the centre) still.
    pub fn zoom(&mut self, factor: f64, at: (f64, f64)) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let before = self.to_world(at);
        let fitted = self.fitted.max(1e-300);
        self.scale = (self.scale * factor).clamp(fitted * ZOOM_OUT_MOST, fitted * ZOOM_IN_MOST);
        let after = self.to_world(at);
        self.center = self.center + (before - after);
    }

    /// The drawing units a view of `h` points covers.
    pub fn height_units(&self, h: f64) -> f64 {
        h / self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_centres_and_frames_the_box() {
        let b = Bounds2::from_points([Vec2::new(0.0, 0.0), Vec2::new(200.0, 100.0)]);
        let v = View::fit(&b, 1000.0, 1000.0);
        assert_eq!(v.center, Vec2::new(100.0, 50.0));
        assert!((v.scale - 4.7).abs() < 1e-9);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let mut v = View { center: Vec2::new(10.0, 10.0), scale: 2.0, fitted: 2.0 };
        let at = (40.0, -30.0);
        let p = v.to_world(at);
        v.zoom(3.0, at);
        let q = v.to_world(at);
        assert!((p.x - q.x).abs() < 1e-9 && (p.y - q.y).abs() < 1e-9);
        assert!((v.scale - 6.0).abs() < 1e-12);
    }

    #[test]
    fn pan_moves_with_the_pointer() {
        let mut v = View { center: Vec2::ZERO, scale: 2.0, fitted: 2.0 };
        let p = v.to_world((0.0, 0.0));
        v.pan((20.0, 10.0));
        // The point that was at the centre is now 20 right, 10 down of it.
        let s = v.to_screen(p);
        assert!((s.0 - 20.0).abs() < 1e-9 && (s.1 - 10.0).abs() < 1e-9);
    }
}
