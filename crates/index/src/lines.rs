//! A drawing's lines as a reader of plans takes them: model space flattened (block placements
//! exploded, dimensions as the lines they draw), each straight piece a segment and each circular
//! piece a true arc, by layer, in metres.
//!
//! Where [`Opened::shapes`] gives shapes for finding outlines (millimetres to the hundredth, a
//! curve as its chords), this keeps what was drawn: a polyline's bulged pieces, an arc inside a
//! mirrored or turned block, and an ellipse that is a circle all come as arcs with their centre,
//! radius and angles; only what is no circle (an ellipse, a spline, an arc in a block scaled
//! unevenly) comes as chords. Coordinates are not rounded. Fills (hatches, solids, wipeouts), texts and
//! points are left out and counted; a placement's clip (XCLIP) is not applied.
//!
//! The drawing's unit is a parameter: a file's header often names another unit than the one the
//! drawing was made in (`$INSUNITS` inches on a plan drawn in metres), and only its reader knows.

use std::collections::{BTreeMap, HashMap};

use cadcraft_doc::{Drawing, Entity, EntityKind, Prim};
use cadcraft_geom::{Arc, Ellipse, Mat3, Segment, TAU, Vec2, ccw_sweep};
use serde_json::{Value, json};

use crate::{Opened, kind_of, units};

/// The deepest blocks are exploded through (a block placing itself is cut there).
const DEPTH: usize = 16;
/// How near a curve's chords keep to it when none is asked for, metres.
const CHORD_M: f64 = 0.001;
/// An ellipse whose axes differ by less than this share is a circle.
const ROUND: f64 = 1e-9;

/// What [`Opened::line_items`] takes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinesParams {
    /// The layers to take (by name, any case, whatever their state); none: every layer that is on
    /// and thawed.
    pub layers: Option<Vec<String>>,
    /// The unit the drawing was made in ("mm", "cm", "m", "in", "ft", ...); none: the header's.
    pub unit: Option<String>,
    /// How near a curve's chords keep to it, metres; none: a millimetre.
    pub chord: Option<f64>,
    /// Whether a solid (a filled triangle or four-sided shape: an arrowhead, a dimension's tick) is
    /// given as the edges of the triangles it is filled by, as a program that draws fills as
    /// meshes shows it; without it a solid, like every fill, is left out.
    pub solids: bool,
}

/// One piece of a drawing's lines, metres less the offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Piece {
    /// `[x0, y0, x1, y1]`.
    Seg([f64; 4]),
    /// `[cx, cy, r, a0, a1, mx, my]`: the centre, the radius, the angles it runs between
    /// counter-clockwise (radians; `a0` in (-π, π], `a1` = `a0` + its sweep) and the point
    /// halfway along.
    Arc([f64; 7]),
    /// `[cx, cy, r]`.
    Circle([f64; 3]),
}

/// A piece with where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct LineItem {
    /// The handle of the model space item it came from (a block placement's for what it draws).
    pub handle: String,
    pub layer: String,
    /// The kind of the entity that drew it ([`kind_of`]), inside a block or not.
    pub kind: &'static str,
    /// The block it was drawn in (the innermost; a dimension's own block too), if any.
    pub block: Option<String>,
    /// Whether it was drawn on layer 0 inside a block (its layer is then the placement's, as
    /// AutoCAD draws it).
    pub zero_in_block: bool,
    /// Whether it is a line a dimension draws.
    pub dimension: bool,
    pub piece: Piece,
}

/// What [`Opened::line_items`] gives.
#[derive(Clone, Debug, PartialEq)]
pub struct Lines {
    pub items: Vec<LineItem>,
    /// The unit the drawing was taken in, and the header's.
    pub unit: &'static str,
    pub header_unit: &'static str,
    /// Metres to one of the drawing's units, as taken.
    pub metres: f64,
    /// Taken off every point, metres.
    pub offset: [f64; 2],
    /// What draws no line and was left out, counted by kind (fills, texts, points, images, ...).
    pub left_out: BTreeMap<&'static str, usize>,
}

impl Opened {
    /// Model space's lines, each with where it came from: see [`Opened::lines`] for them by layer.
    pub fn line_items(&self, p: &LinesParams) -> Result<Lines, String> {
        let (unit, mm) = match &p.unit {
            None => (self.unit, self.mm),
            Some(u) => units::named(u).ok_or_else(|| format!("unknown unit `{u}`; a drawing's unit is one of: {}", units::NAMES.join(", ")))?,
        };
        let metres = mm / 1000.0;
        let chord = match p.chord {
            None => CHORD_M,
            Some(c) if c.is_finite() && c > 0.0 => c,
            Some(c) => return Err(format!("`chord` is metres, more than 0, not {c}")),
        };
        let wanted: Option<Vec<String>> = match &p.layers {
            None => None,
            Some(names) => Some(
                names
                    .iter()
                    .map(|n| self.drawing.layer(n.trim()).map(|l| l.name.to_lowercase()).ok_or_else(|| format!("the drawing has no layer `{n}`")))
                    .collect::<Result<_, _>>()?,
            ),
        };
        // The offset is kept in the header's millimetres: in the drawing's own units here.
        let off = self.offset * (1.0 / self.mm);
        let mut walk = Walk {
            d: &self.drawing,
            wanted,
            shown: HashMap::new(),
            solids: p.solids,
            off,
            metres,
            tol: chord / metres,
            items: Vec::new(),
            left_out: BTreeMap::new(),
        };
        for e in self.drawing.model.iter() {
            let handle = format!("{:X}", e.handle.0);
            walk.entity(e, &handle, &At { layer: &e.common.layer, block: None, zero: false, dimension: false }, Mat3::IDENTITY, 0);
        }
        Ok(Lines { items: walk.items, unit, header_unit: self.unit, metres, offset: [off.x * metres, off.y * metres], left_out: walk.left_out })
    }

    /// Model space's lines by layer (JSON params: `layers`?, `unit`?, `chord`?, `solids`?), as
    /// JSON:
    ///
    /// - `segs`, `arcs`, `circles`: by layer name, lists of `[x0, y0, x1, y1]`,
    ///   `[cx, cy, r, a0, a1, mx, my]` and `[cx, cy, r]` (see [`Piece`]), metres less `offset`;
    /// - `order`: the layers of each in the order the drawing first draws on them (a JSON object
    ///   keeps none);
    /// - `bbox`: `[x0, y0, x1, y1]` round all of them (null when there is nothing);
    /// - `unit`, `header_unit`, `metres` (to one unit, as taken), `offset`, `left_out`.
    pub fn lines(&self, params: &Value) -> Result<Value, String> {
        let p = LinesParams::from_json(params)?;
        Ok(self.line_items(&p)?.to_json())
    }
}

impl LinesParams {
    /// The parameters a JSON object names (null or an empty one: none).
    pub fn from_json(v: &Value) -> Result<LinesParams, String> {
        const KEYS: [&str; 4] = ["layers", "unit", "chord", "solids"];
        let empty = serde_json::Map::new();
        let m = match v {
            Value::Null => &empty,
            Value::Object(m) => m,
            other => return Err(format!("lines take an object of parameters ({}), not {other}", KEYS.join(", "))),
        };
        if let Some(k) = m.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(format!("unknown parameter `{k}`; lines take: {}", KEYS.join(", ")));
        }
        let given = |k: &str| m.get(k).filter(|v| !v.is_null());
        Ok(LinesParams {
            layers: match given("layers") {
                None => None,
                Some(v) => Some(serde_json::from_value(v.clone()).map_err(|_| format!("`layers` is a list of layer names, not {v}"))?),
            },
            unit: match given("unit") {
                None => None,
                Some(v) => Some(v.as_str().ok_or_else(|| format!("`unit` is a unit's name ({}), not {v}", units::NAMES.join(", ")))?.to_string()),
            },
            chord: match given("chord") {
                None => None,
                Some(v) => Some(v.as_f64().ok_or_else(|| format!("`chord` is metres, more than 0, not {v}"))?),
            },
            solids: match given("solids") {
                None => false,
                Some(v) => v.as_bool().ok_or_else(|| format!("`solids` is true or false, not {v}"))?,
            },
        })
    }
}

impl Lines {
    /// The lines by layer, as [`Opened::lines`] gives them.
    pub fn to_json(&self) -> Value {
        let mut by: [By; 3] = Default::default();
        let mut bbox: Option<[f64; 4]> = None;
        let mut grow = |x: f64, y: f64| {
            let b = bbox.get_or_insert([x, y, x, y]);
            *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
        };
        for it in &self.items {
            match it.piece {
                Piece::Seg(s) => {
                    grow(s[0], s[1]);
                    grow(s[2], s[3]);
                    by[0].push(&it.layer, json!(s));
                }
                Piece::Arc(a) => {
                    for (x, y) in arc_reach(&a) {
                        grow(x, y);
                    }
                    by[1].push(&it.layer, json!(a));
                }
                Piece::Circle(c) => {
                    grow(c[0] - c[2], c[1] - c[2]);
                    grow(c[0] + c[2], c[1] + c[2]);
                    by[2].push(&it.layer, json!(c));
                }
            }
        }
        let [segs, arcs, circles] = by;
        json!({
            "unit": self.unit,
            "header_unit": self.header_unit,
            "metres": self.metres,
            "offset": self.offset,
            "bbox": bbox,
            "order": {"segs": segs.order, "arcs": arcs.order, "circles": circles.order},
            "segs": segs.object(),
            "arcs": arcs.object(),
            "circles": circles.object(),
            "left_out": self.left_out,
        })
    }
}

/// Pieces by layer, in the order the layers are first met.
#[derive(Default)]
struct By {
    order: Vec<String>,
    at: HashMap<String, usize>,
    lists: Vec<Vec<Value>>,
}

impl By {
    fn push(&mut self, layer: &str, v: Value) {
        let i = match self.at.get(layer) {
            Some(i) => *i,
            None => {
                self.order.push(layer.to_string());
                self.lists.push(Vec::new());
                self.at.insert(layer.to_string(), self.order.len() - 1);
                self.order.len() - 1
            }
        };
        if let Some(l) = self.lists.get_mut(i) {
            l.push(v);
        }
    }

    fn object(self) -> Value {
        Value::Object(self.order.into_iter().zip(self.lists).map(|(k, v)| (k, Value::Array(v))).collect())
    }
}

/// The points an arc reaches furthest at: its ends, and each axis direction it sweeps through.
fn arc_reach(a: &[f64; 7]) -> Vec<(f64, f64)> {
    let [cx, cy, r, a0, a1, ..] = *a;
    let mut out = vec![(cx + r * a0.cos(), cy + r * a0.sin()), (cx + r * a1.cos(), cy + r * a1.sin())];
    for q in -4..=8 {
        let t = f64::from(q) * TAU / 4.0;
        if t > a0 && t < a1 {
            out.push((cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    out
}

/// Where an entity is drawn: its layer as resolved, and what it is inside.
#[derive(Clone, Copy)]
struct At<'a> {
    layer: &'a str,
    block: Option<&'a str>,
    zero: bool,
    dimension: bool,
}

struct Walk<'a> {
    d: &'a Drawing,
    wanted: Option<Vec<String>>,
    /// Whether each layer met is taken (by name as spelt).
    shown: HashMap<String, bool>,
    solids: bool,
    /// Taken off every point, drawing units.
    off: Vec2,
    metres: f64,
    /// The chord tolerance in drawing units.
    tol: f64,
    items: Vec<LineItem>,
    left_out: BTreeMap<&'static str, usize>,
}

impl Walk<'_> {
    fn takes(&mut self, layer: &str) -> bool {
        if let Some(t) = self.shown.get(layer) {
            return *t;
        }
        let t = match &self.wanted {
            Some(w) => w.contains(&layer.to_lowercase()),
            None => self.d.layer(layer).is_none_or(|l| l.on && !l.frozen),
        };
        self.shown.insert(layer.to_string(), t);
        t
    }

    /// Whether a placement on `layer` draws at all: one on a frozen layer draws nothing, whatever
    /// layers its block's contents are on (when layers are asked for by name, their state is not
    /// looked at).
    fn placed(&self, layer: &str) -> bool {
        self.wanted.is_some() || self.d.layer(layer).is_none_or(|l| !l.frozen)
    }

    fn entity(&mut self, e: &Entity, handle: &str, at: &At, m: Mat3, depth: usize) {
        if !e.common.visible {
            return;
        }
        match &e.kind {
            EntityKind::Insert(ins) => {
                if self.placed(at.layer) {
                    self.insert(ins, handle, at, m, depth);
                }
            }
            EntityKind::Dimension(dm) => {
                if self.placed(at.layer) {
                    self.dimension(dm, handle, at, m, depth);
                }
            }
            EntityKind::Solid(s) | EntityKind::Trace(s) if self.solids => {
                if self.takes(at.layer) {
                    // Its corners go round as 0, 1, 3, 2: two triangles on the diagonal from 1 to 2, wound one
                    // way, so the diagonal comes once each way; a three-cornered one (3 on 2) is one triangle.
                    let c = s.corners.map(|p| m.apply(p.xy()));
                    let mut edges = vec![(c[0], c[1]), (c[1], c[2]), (c[2], c[0])];
                    if c[3] != c[2] {
                        edges.extend([(c[2], c[1]), (c[1], c[3]), (c[3], c[2])]);
                    }
                    for (a, b) in edges {
                        self.seg(a, b, handle, at, kind_of(&e.kind));
                    }
                }
            }
            // What draws no line of its own: fills and their edges, texts, points, pictures, frames,
            // and lines without an end.
            EntityKind::Hatch(_)
            | EntityKind::Solid(_)
            | EntityKind::Trace(_)
            | EntityKind::Wipeout(_)
            | EntityKind::Text(_)
            | EntityKind::MText(_)
            | EntityKind::AttDef(_)
            | EntityKind::Point(_)
            | EntityKind::Image(_)
            | EntityKind::Table(_)
            | EntityKind::Tolerance(_)
            | EntityKind::Viewport(_)
            | EntityKind::Ray(_)
            | EntityKind::XLine(_)
            | EntityKind::Unknown(_) => {
                if self.takes(at.layer) {
                    *self.left_out.entry(kind_of(&e.kind)).or_default() += 1;
                }
            }
            k => {
                if self.takes(at.layer) {
                    let kind = kind_of(k);
                    for prim in k.prims() {
                        self.prim(prim, handle, at, kind, &m);
                    }
                }
            }
        }
    }

    fn insert(&mut self, ins: &cadcraft_doc::Insert, handle: &str, at: &At, m: Mat3, depth: usize) {
        let d = self.d;
        let Some(blk) = d.block(&ins.block) else { return };
        if depth >= DEPTH {
            return;
        }
        let cols = ins.cols.clamp(1, 10_000);
        let rows = ins.rows.clamp(1, 10_000);
        for r in 0..rows {
            for c in 0..cols {
                let off = Vec2::new(ins.col_spacing * f64::from(c), ins.row_spacing * f64::from(r)).rotate(ins.rotation);
                let local = Mat3::translate(off).then_before(ins.transform(blk.base.xy()));
                self.contents(blk, handle, at, m.then_before(local), depth);
            }
        }
    }

    /// A dimension's lines: those of the block the file keeps for it (what any reader of the file
    /// draws), moved by its insertion point; without one, the lines its style draws.
    fn dimension(&mut self, dm: &cadcraft_doc::Dimension, handle: &str, at: &At, m: Mat3, depth: usize) {
        let d = self.d;
        let at = At { dimension: true, ..*at };
        if let Some(blk) = dm.block.as_ref().and_then(|n| d.block(n)) {
            if depth < DEPTH {
                self.contents(blk, handle, &at, m.then_before(Mat3::translate(dm.insertion_point.xy())), depth);
            }
            return;
        }
        if !self.takes(at.layer) {
            return;
        }
        for line in cadcraft_render::dimension_in(d, dm).lines {
            for w in line.windows(2) {
                if let [a, b] = w {
                    self.seg(m.apply(*a), m.apply(*b), handle, &at, "dimension");
                }
            }
        }
    }

    fn contents(&mut self, blk: &cadcraft_doc::Block, handle: &str, at: &At, m: Mat3, depth: usize) {
        for child in blk.entities.iter() {
            // What a block draws on layer 0 is on the placement's layer.
            let zero = child.common.layer == "0";
            let layer = if zero { at.layer } else { child.common.layer.as_str() };
            let inside = At { layer, block: Some(blk.name.as_str()), zero, dimension: at.dimension };
            self.entity(child, handle, &inside, m, depth + 1);
        }
    }

    /// A primitive of an entity in its own frame, placed by `m`.
    fn prim(&mut self, prim: Prim, handle: &str, at: &At, kind: &'static str, m: &Mat3) {
        match prim {
            Prim::Seg(Segment::Line(l)) => self.seg(m.apply(l.a), m.apply(l.b), handle, at, kind),
            Prim::Seg(Segment::Arc { arc, .. }) => self.ellipse(round(&arc).transformed(m), handle, at, kind),
            Prim::Circle(c) => self.ellipse(Ellipse::full(c.center, Vec2::new(c.radius, 0.0), 1.0).transformed(m), handle, at, kind),
            Prim::Ellipse(el) => self.ellipse(el.transformed(m), handle, at, kind),
            Prim::Spline(s) => {
                // Chords within the tolerance once placed: the placement's larger scale decides.
                let grown = m.apply_vec(Vec2::new(1.0, 0.0)).len().max(m.apply_vec(Vec2::new(0.0, 1.0)).len());
                let ps = s.tessellate(if grown > 0.0 { self.tol / grown } else { self.tol });
                for w in ps.windows(2) {
                    if let [a, b] = w {
                        self.seg(m.apply(*a), m.apply(*b), handle, at, kind);
                    }
                }
            }
            Prim::Fill(_) | Prim::Point(_) | Prim::Infinite { .. } => {}
        }
    }

    /// An ellipse as placed: a circle or an arc when its axes are equal, else its chords.
    fn ellipse(&mut self, el: Ellipse, handle: &str, at: &At, kind: &'static str) {
        let r = el.major.len();
        if r <= 0.0 || !r.is_finite() {
            return;
        }
        if (el.ratio - 1.0).abs() > ROUND {
            let mut ps = Vec::new();
            el.tessellate(self.tol, &mut ps);
            for w in ps.windows(2) {
                if let [a, b] = w {
                    self.seg(*a, *b, handle, at, kind);
                }
            }
            return;
        }
        let c = self.out(el.center);
        let r = r * self.metres;
        let piece = if el.is_full() {
            Piece::Circle([c[0], c[1], r])
        } else {
            let base = el.major.y.atan2(el.major.x);
            let sweep = ccw_sweep(el.start, el.end);
            // atan2's range, as a reader working from the arc's points has it
            let a0 = (base + el.start).sin().atan2((base + el.start).cos());
            let mid = a0 + sweep / 2.0;
            Piece::Arc([c[0], c[1], r, a0, a0 + sweep, c[0] + r * mid.cos(), c[1] + r * mid.sin()])
        };
        self.push(piece, handle, at, kind);
    }

    fn seg(&mut self, a: Vec2, b: Vec2, handle: &str, at: &At, kind: &'static str) {
        let (a, b) = (self.out(a), self.out(b));
        if a.iter().chain(&b).all(|v| v.is_finite()) {
            self.push(Piece::Seg([a[0], a[1], b[0], b[1]]), handle, at, kind);
        }
    }

    fn push(&mut self, piece: Piece, handle: &str, at: &At, kind: &'static str) {
        self.items.push(LineItem {
            handle: handle.to_string(),
            layer: at.layer.to_string(),
            kind,
            block: at.block.map(str::to_string),
            zero_in_block: at.zero,
            dimension: at.dimension,
            piece,
        });
    }

    /// A point of the drawing given out: metres less the offset, not rounded.
    fn out(&self, p: Vec2) -> [f64; 2] {
        [(p.x - self.off.x) * self.metres, (p.y - self.off.y) * self.metres]
    }
}

/// An arc as the ellipse it is.
fn round(a: &Arc) -> Ellipse {
    Ellipse { center: a.center, major: Vec2::new(a.radius, 0.0), ratio: 1.0, start: a.start, end: a.end }
}
