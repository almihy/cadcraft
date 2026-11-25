//! A drawing read once, kept packed, and asked of: the index Arqiflow's agent reads drawings from
//! (Arqiflow's docs/superpowers/specs/2026-11-16-dwg-drawings-design.md, section 3).
//!
//! A DWG or DXF is read once ([`index`]): the drawing comes back packed ([`pack`], a few MB where its
//! DXF is tens) with its [`summary`]. Opened again from the pack ([`Opened::unpack`]), it answers
//! the agent's questions in its terms: [`Opened::query`] the items by layer, kind, box and text, a
//! page at a time, a block placement one item; [`Opened::block`] what a block definition holds;
//! [`Opened::picture`] a page of it drawn by CADCraft's renderer; [`Opened::shapes`] its lines
//! placed and flattened, blocks exploded, for finding the outlines they close; [`Opened::lines`] its
//! lines by layer in metres, arcs kept as arcs and dimensions as the lines they draw, for a reader
//! of plans.
//!
//! Every length and point given out is in millimetres (a drawing in metres comes ×1000; a unitless
//! one is taken as millimetres), to the hundredth. A drawing far out in a real grid (a survey in
//! UTM, Lambert-93, CC46) comes less an `offset`, a round point near its middle, taken off every
//! point given out; a box asked for may be given with it or without it.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod flat;
mod kinds;
mod lines;
mod pack;
mod picture;
mod query;
mod summary;
mod units;

use std::sync::OnceLock;

use cadcraft_doc::{BlockBoundsCache, Drawing, Entity, Space, entity_bounds_cached};
use cadcraft_geom::{Bounds2, Vec2};

/// Fonts a host gives at run time (a firm's, the computer's, stand-ins): what a picture draws with.
pub use cadcraft_fonts::given;
pub use flat::{Flat, Shape};
pub use kinds::{KINDS, kind_of};
pub use lines::{LineItem, Lines, LinesParams, MOST_PIECES, Piece};
pub use pack::{FORMAT, pack, unpack};
pub use picture::Picture;
pub use query::Query;

/// Past this many millimetres from the origin (20 km) a drawing lies in a real grid, and is given
/// out less an offset (as Arqiflow's engine frame does: `engine/frame.py`, `FAR`).
pub const FAR_MM: f64 = 20_000_000.0;
/// The offset is a round point: a multiple of this many millimetres (10 m).
pub const ROUND_MM: f64 = 10_000.0;

/// A drawing read from a file's bytes, as CADCraft opens it: a DWG (by its first bytes) or a DXF.
pub fn read(bytes: &[u8], name: &str) -> Result<Drawing, String> {
    cadcraft_io::read(bytes, name).map_err(|e| e.to_string())
}

/// A file's drawing read and indexed: the drawing packed, and its summary.
pub fn index(bytes: &[u8], name: &str) -> Result<(Vec<u8>, serde_json::Value), String> {
    let opened = Opened::new(read(bytes, name)?);
    let summary = opened.summary();
    Ok((pack(&opened.drawing)?, summary))
}

/// A drawing open to be asked of.
pub struct Opened {
    pub drawing: Drawing,
    unit: &'static str,
    /// Millimetres to one of the drawing's units.
    mm: f64,
    /// Taken off every point given out, mm.
    offset: Vec2,
    /// The box of each model space item in the store's order, drawing units (empty: nothing drawn).
    boxes: Vec<Bounds2>,
    /// The layers each block holds, nested blocks' included (by block name), worked out once.
    block_layers: OnceLock<std::collections::HashMap<String, std::collections::BTreeSet<String>>>,
    /// The box round what each block draws in its own frame, by name, as it is asked for.
    block_boxes: std::sync::Mutex<std::collections::HashMap<String, Option<Bounds2>>>,
}

impl Opened {
    pub fn new(drawing: Drawing) -> Opened {
        let (unit, mm) = units::of(&drawing);
        let mut cache = BlockBoundsCache::default();
        let boxes: Vec<Bounds2> = drawing.model.iter().map(|e| entity_bounds_cached(&drawing, e, 0, &mut cache)).collect();
        let mut opened = Opened { drawing, unit, mm, offset: Vec2::ZERO, boxes, block_layers: OnceLock::new(), block_boxes: Default::default() };
        if let Some(bulk) = opened.bulk() {
            let middle = bulk.center() * mm;
            if middle.x.abs().max(middle.y.abs()) > FAR_MM {
                opened.offset = Vec2::new((middle.x / ROUND_MM).round() * ROUND_MM, (middle.y / ROUND_MM).round() * ROUND_MM);
            }
        }
        opened
    }

    /// A drawing [`pack`]ed, opened.
    pub fn unpack(bytes: &[u8]) -> Result<Opened, String> {
        Ok(Opened::new(unpack(bytes)?))
    }

    /// What the drawing holds, in short (see [`summary`](crate::summary)).
    pub fn summary(&self) -> serde_json::Value {
        summary::summary(self)
    }

    /// The fonts the drawing's text styles name that are not here (registered, or built in), each
    /// once, as the styles spell them: what the host may find and give
    /// ([cadcraft_fonts::given::add_font]) before a picture, so its text draws as the drawing's.
    pub fn fonts_missing(&self) -> Vec<String> {
        cadcraft_fonts::given::missing(self.drawing.text_styles.iter().map(|ts| (ts.font.as_str(), ts.big_font.as_str())))
    }

    /// The Arabic a text in style `style` shows when the style's font draws Arabic letters for
    /// Latin keys (`xarab.shx`: "Hgl" shows «الم»), else None ([cadcraft_fonts::keyboard]).
    pub fn shown(&self, style: &str, text: &str) -> Option<String> {
        let ts = self.drawing.text_style(style)?;
        cadcraft_fonts::keyboard::shown(&ts.font, text)
    }

    /// The offset taken off every point given out, mm ([0, 0] for a drawing near its origin).
    pub fn offset(&self) -> [f64; 2] {
        [self.offset.x, self.offset.y]
    }

    /// A point of the drawing given out: mm, less the offset, to the hundredth.
    pub(crate) fn out(&self, p: Vec2) -> [f64; 2] {
        [hundredth(p.x * self.mm - self.offset.x), hundredth(p.y * self.mm - self.offset.y)]
    }

    /// A point of a block's own frame given out: mm, to the hundredth (no offset: a block's frame is its own).
    pub(crate) fn out_own(&self, p: Vec2) -> [f64; 2] {
        [hundredth(p.x * self.mm), hundredth(p.y * self.mm)]
    }

    /// A length given out, mm to the hundredth.
    pub(crate) fn len(&self, l: f64) -> f64 {
        hundredth(l * self.mm)
    }

    /// A box asked for, mm ([[x0, y0], [x1, y1]] either way round), in the drawing's units: less the
    /// offset, or a point far out taken as the drawing's own.
    pub(crate) fn box_in(&self, b: [[f64; 2]; 2]) -> Bounds2 {
        let one = |p: [f64; 2]| {
            let far = p[0].abs().max(p[1].abs()) > FAR_MM;
            let p = if far || self.offset == Vec2::ZERO { Vec2::new(p[0], p[1]) } else { Vec2::new(p[0], p[1]) + self.offset };
            p * (1.0 / self.mm)
        };
        Bounds2::new(one(b[0]), one(b[1]))
    }

    /// A box asked for in a frame of the drawing's own that is never far out (a sheet), mm, in the
    /// drawing's units: as given, without the offset.
    pub(crate) fn box_own(&self, b: [[f64; 2]; 2]) -> Bounds2 {
        let one = |p: [f64; 2]| Vec2::new(p[0], p[1]) * (1.0 / self.mm);
        Bounds2::new(one(b[0]), one(b[1]))
    }

    /// The box of the drawing's bulk, drawing units: its model space items' boxes, the strays far
    /// from the rest (a line left kilometres away) left out. None when it draws nothing.
    pub(crate) fn bulk(&self) -> Option<Bounds2> {
        bulk(&self.boxes)
    }

    /// The box round all of model space, drawing units: of what has a finite box (a construction
    /// line or ray, endless, is left out).
    pub(crate) fn extents(&self) -> Bounds2 {
        self.boxes.iter().filter(|b| !b.is_empty() && b.min.is_finite() && b.max.is_finite()).fold(Bounds2::EMPTY, |b, x| b.union(x))
    }

    pub(crate) fn model_items(&self) -> impl Iterator<Item = (&Entity, &Bounds2)> {
        self.drawing.model.iter().map(|e| &**e).zip(self.boxes.iter())
    }

    /// The space a sheet's name names (none: model space), or why not.
    pub(crate) fn space(&self, sheet: Option<&str>) -> Result<Space, String> {
        match sheet {
            None => Ok(Space::Model),
            Some(s) if s.eq_ignore_ascii_case("model") => Ok(Space::Model),
            Some(s) => match self.drawing.layouts.iter().find(|l| l.name.eq_ignore_ascii_case(s)) {
                Some(l) => Ok(Space::Paper(l.name.clone())),
                None => {
                    let names: Vec<&str> = self.sheets().iter().map(|l| l.name.as_str()).collect();
                    Err(format!("the drawing has no sheet `{s}` (it has: Model, {})", names.join(", ")))
                }
            },
        }
    }

    /// Its sheets (layouts) that draw anything, in their tab order.
    pub(crate) fn sheets(&self) -> Vec<&cadcraft_doc::Layout> {
        let mut out: Vec<&cadcraft_doc::Layout> = self.drawing.layouts.iter().filter(|l| !l.entities.is_empty()).collect();
        out.sort_by_key(|l| l.tab_order);
        out
    }

    /// The layers block `name` holds, its nested blocks' included.
    pub(crate) fn layers_in_block(&self, name: &str) -> Option<&std::collections::BTreeSet<String>> {
        self.block_layers.get_or_init(|| flat::block_layers(&self.drawing)).get(name)
    }
}

/// A number to the hundredth.
pub(crate) fn hundredth(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// The box round the boxes' bulk, so that a few strays (a line left kilometres away) do not make the
/// drawing kilometres wide: those whose centres lie within the middle half of them (by x and by y)
/// or less than three times its spread past it, as a box plot tells outliers.
fn bulk(boxes: &[Bounds2]) -> Option<Bounds2> {
    let live: Vec<&Bounds2> = boxes.iter().filter(|b| !b.is_empty() && b.min.is_finite() && b.max.is_finite()).collect();
    if live.is_empty() {
        return None;
    }
    let mut xs: Vec<f64> = live.iter().map(|b| b.center().x).collect();
    let mut ys: Vec<f64> = live.iter().map(|b| b.center().y).collect();
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    let at = |v: &[f64], q: f64| v.get(((v.len() - 1) as f64 * q).round() as usize).copied().unwrap_or(0.0);
    let reach = |v: &[f64]| {
        let (lo, hi) = (at(v, 0.25), at(v, 0.75));
        // Items that mostly line up (one row of sheets) still have a spread: a share of the whole.
        let spread = (hi - lo).max((at(v, 1.0) - at(v, 0.0)) * 0.01).max(1e-9);
        (lo - 3.0 * spread, hi + 3.0 * spread)
    };
    let ((x0, x1), (y0, y1)) = (reach(&xs), reach(&ys));
    let keep = Bounds2::new(Vec2::new(x0, y0), Vec2::new(x1, y1));
    let b = live.iter().filter(|b| keep.contains(b.center())).fold(Bounds2::EMPTY, |acc, b| acc.union(b));
    (!b.is_empty()).then_some(b)
}

#[cfg(test)]
mod tests;
