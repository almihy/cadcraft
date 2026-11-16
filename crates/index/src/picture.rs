//! A page of the drawing drawn as a picture by CADCraft's renderer: black on white, as it prints.

use cadcraft_color::Rgb;
use cadcraft_doc::Space;
use cadcraft_geom::{Bounds2, Vec2};
use cadcraft_render::raster::{RasterOptions, View, render_png};

use crate::Opened;

/// The longest side a picture may be asked for, and the shortest.
pub const MOST_LONG: u32 = 8192;
pub const LEAST_LONG: u32 = 200;
/// The margin round what a page draws, a share of its larger side.
const MARGIN: f64 = 0.02;
/// The smallest part of a page that can be asked for, a share of its side.
const SMALLEST: f64 = 0.001;

/// A page drawn: the PNG, its size in pixels, which page it is of how many, and its name ("Model", or
/// the sheet's).
pub struct Picture {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub page: usize,
    pub pages: usize,
    pub name: String,
}

impl Opened {
    /// How many pages the drawing has: model space, then each sheet that draws anything.
    pub fn pages(&self) -> usize {
        1 + self.sheets().len()
    }

    /// Page `page` (1 model space, then each sheet in its tab order) drawn `long` pixels on its
    /// longer side: whole, or the part `part` names ([left, top, right, bottom] shares of the page
    /// from its top left, 0 to 1).
    pub fn picture(&self, page: usize, part: Option<[f64; 4]>, long: u32) -> Result<Picture, String> {
        let pages = self.pages();
        if page < 1 || page > pages {
            return Err(format!("the drawing has {pages} page{} (1 its model, then each sheet), not page {page}", if pages == 1 { "" } else { "s" }));
        }
        let sheets = self.sheets();
        let (space, name) = match page {
            1 => (Space::Model, "Model".to_string()),
            n => {
                let l = sheets.get(n - 2).ok_or("no such sheet")?;
                (Space::Paper(l.name.clone()), l.name.clone())
            }
        };
        let whole = match &space {
            Space::Model => self.bulk().ok_or("the drawing draws nothing in its model")?,
            Space::Paper(_) => {
                let b = self.drawing.extents(&space);
                if b.is_empty() {
                    return Err(format!("sheet {name} draws nothing"));
                }
                b
            }
        };
        let seen = part_of(&whole, part)?;
        let long = long.clamp(LEAST_LONG, MOST_LONG);
        let (w, h) = (seen.width().max(1e-9), seen.height().max(1e-9));
        let k = f64::from(long) / (w.max(h) * (1.0 + 2.0 * MARGIN));
        let width = ((w * (1.0 + 2.0 * MARGIN) * k).round() as u32).max(1);
        let height = ((h * (1.0 + 2.0 * MARGIN) * k).round() as u32).max(1);
        let view = View { center: seen.center(), scale: k, width, height };
        let px = 1.0 / k; // a pixel in drawing units
        let opts = cadcraft_render::Options {
            tolerance: px * 0.5,
            min_dash: px * 2.0,
            view_height: f64::from(height) * px,
            view: Some(view.world_bounds()),
            transparency: true,
            ..cadcraft_render::Options::default()
        };
        let list = cadcraft_render::build(&self.drawing, &space, &opts);
        let raster = RasterOptions { background: Rgb(255, 255, 255), hairline: 1.0, px_per_mm: 0.0, antialias: true };
        let png = render_png(&list, &view, &raster).ok_or("the page could not be drawn")?;
        Ok(Picture { png, width, height, page, pages, name })
    }
}

/// The part of `whole` that `part` names ([left, top, right, bottom] shares from its top left).
fn part_of(whole: &Bounds2, part: Option<[f64; 4]>) -> Result<Bounds2, String> {
    let Some([l, t, r, b]) = part else { return Ok(*whole) };
    let ok = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
    if !(ok(l) && ok(t) && ok(r) && ok(b)) || r - l < SMALLEST || b - t < SMALLEST {
        return Err("`box` is [left, top, right, bottom], each from 0 to 1 from the page's top left, right past left and bottom past top".into());
    }
    let (w, h) = (whole.width(), whole.height());
    Ok(Bounds2::new(
        Vec2::new(whole.min.x + l * w, whole.max.y - b * h),
        Vec2::new(whole.min.x + r * w, whole.max.y - t * h),
    ))
}
