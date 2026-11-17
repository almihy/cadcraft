//! Measure snaps to the points a drawing is made of: the ends of its lines and arcs, the corners
//! of short polylines (rectangles, a door's leaf), point objects. They are kept in a grid over the
//! drawing so the one nearest the pointer is found at once, however large the drawing.

use std::collections::HashMap;

use cadcraft_geom::{Bounds2, Vec2};
use cadcraft_render::{DPrim, DisplayList, Kind};

/// Cells along the longer side of the drawing.
const CELLS: f64 = 1024.0;
/// A polyline with at most this many points snaps at each of them; a longer one (a curve drawn
/// as many short pieces) only at its ends.
const CORNERS: u32 = 6;

pub struct Snaps {
    origin: Vec2,
    cell: f64,
    /// Points sorted by their cell, and where each cell's run starts and ends.
    points: Vec<Vec2>,
    runs: HashMap<(i32, i32), (u32, u32)>,
}

impl Snaps {
    pub fn new<'a>(list: &DisplayList, prims: impl Iterator<Item = &'a DPrim>) -> Snaps {
        let b: Bounds2 = list.bounds;
        let size = if b.is_empty() { 1.0 } else { b.width().max(b.height()).max(1e-9) };
        let origin = if b.is_empty() { Vec2::ZERO } else { b.min };
        let cell = size / CELLS;
        let mut pts: Vec<Vec2> = Vec::new();
        for p in prims {
            match p.kind {
                Kind::Polyline => {
                    let v = list.points(p);
                    if p.len <= CORNERS {
                        pts.extend_from_slice(v);
                    } else if let (Some(a), Some(z)) = (v.first(), v.last()) {
                        pts.push(*a);
                        pts.push(*z);
                    }
                }
                Kind::Point => pts.extend(list.points(p).first().copied()),
                _ => {}
            }
        }
        let key = |p: &Vec2| (((p.x - origin.x) / cell).floor() as i32, ((p.y - origin.y) / cell).floor() as i32);
        pts.retain(|p| p.is_finite());
        pts.sort_by_key(key);
        pts.dedup();
        let mut runs = HashMap::new();
        let mut start = 0usize;
        while start < pts.len() {
            let k = pts.get(start).map(key).unwrap_or_default();
            let mut end = start + 1;
            while pts.get(end).is_some_and(|p| key(p) == k) {
                end += 1;
            }
            runs.insert(k, (start as u32, end as u32));
            start = end;
        }
        Snaps { origin, cell, points: pts, runs }
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// The point nearest `at` within `radius` drawing units, if any. Looks at no more than a few
    /// thousand cells: zoomed far out, nothing snaps (nothing could be told apart).
    pub fn nearest(&self, at: Vec2, radius: f64) -> Option<Vec2> {
        if !(radius.is_finite() && radius > 0.0) {
            return None;
        }
        let span = (radius / self.cell).ceil() as i32;
        if span > 24 {
            return None;
        }
        let (cx, cy) = (((at.x - self.origin.x) / self.cell).floor() as i32, ((at.y - self.origin.y) / self.cell).floor() as i32);
        let mut best: Option<(f64, Vec2)> = None;
        for x in cx - span..=cx + span {
            for y in cy - span..=cy + span {
                let Some(&(a, b)) = self.runs.get(&(x, y)) else { continue };
                for p in self.points.get(a as usize..b as usize).unwrap_or(&[]) {
                    let d = p.dist(at);
                    if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, *p));
                    }
                }
            }
        }
        best.map(|(_, p)| p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_color::Rgb;
    use cadcraft_doc::Handle;

    fn line(list: &mut DisplayList, a: Vec2, b: Vec2) {
        let start = list.verts.len() as u32;
        list.verts.extend([a, b]);
        list.prims.push(DPrim { handle: Handle(1), color: Rgb(0, 0, 0), aci7: true, lw: 0.0, transparency: 0, kind: Kind::Polyline, start, len: 2 });
        list.bounds = Bounds2::from_points(list.verts.iter().copied());
    }

    #[test]
    fn snaps_to_the_nearest_end_within_reach() {
        let mut list = DisplayList::default();
        line(&mut list, Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0));
        line(&mut list, Vec2::new(0.0, 50.0), Vec2::new(100.0, 50.0));
        let s = Snaps::new(&list, list.prims.iter());
        assert_eq!(s.len(), 4);
        assert_eq!(s.nearest(Vec2::new(99.0, 1.0), 2.0), Some(Vec2::new(100.0, 0.0)));
        assert_eq!(s.nearest(Vec2::new(50.0, 25.0), 2.0), None);
    }
}
