//! The items of a drawing that match a query, a page at a time, and what a block definition holds.

use std::collections::BTreeSet;

use cadcraft_doc::{Entity, EntityKind, Space};
use cadcraft_geom::{Bounds2, Polyline, Vec2};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Opened, kind_of, kinds};

/// Items a page lists unless asked for another number, and the most it may.
pub const LIMIT: usize = 200;
pub const MOST: usize = 2000;
/// A page ends once its items hold this many points, whatever its `limit`.
pub const POINTS: usize = 20_000;
/// The most points a curve (an arc in a polyline, a hatch's ring) is given as.
const CURVE_POINTS: usize = 2_000;

/// What is asked: items on `layers` (by name, any case), of `kinds` (crate::KINDS), touching `box`
/// ([[x0, y0], [x1, y1]] mm), whose text holds `text` (any case: a text's words, a block
/// placement's attribute values, a dimension's text), from `start`, `limit` of them; in model space,
/// or on `sheet`. With `block`, what that block definition holds instead (in its own frame).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Query {
    pub layers: Option<Vec<String>>,
    pub kinds: Option<Vec<String>>,
    #[serde(rename = "box")]
    pub area: Option<[[f64; 2]; 2]>,
    pub text: Option<String>,
    pub sheet: Option<String>,
    pub block: Option<String>,
    pub start: Option<usize>,
    pub limit: Option<usize>,
}

/// A query checked against the drawing.
struct Asked {
    layers: Option<BTreeSet<String>>,
    kinds: Option<BTreeSet<&'static str>>,
    area: Option<Bounds2>,
    text: Option<String>,
    start: usize,
    limit: usize,
}

impl Opened {
    /// The items that match `q`: {total, start, next (the `start` of the next page, null at the
    /// end), items}. Each item: {h (its handle, the same every read), kind, layer, ...its
    /// geometry}: see [`item`].
    pub fn query(&self, q: &Query) -> Result<Value, String> {
        if let Some(name) = &q.block {
            return self.block(name, q);
        }
        let asked = self.asked(q)?;
        let space = self.space(q.sheet.as_deref())?;
        let (total, items, next) = match &space {
            Space::Model => self.page(self.model_items().map(|(e, b)| (e, *b)), &asked, &|p| self.out(p)),
            Space::Paper(_) => {
                let store = self.drawing.space(&space).ok_or("no such sheet")?;
                let mut cache = cadcraft_doc::BlockBoundsCache::default();
                let boxed: Vec<(&Entity, Bounds2)> =
                    store.iter().map(|e| (&**e, cadcraft_doc::entity_bounds_cached(&self.drawing, e, 0, &mut cache))).collect();
                // A sheet is in paper units, never far out: given as it is, without the offset.
                self.page(boxed.into_iter(), &asked, &|p| self.out_own(p))
            }
        };
        let mut out = json!({"total": total, "start": asked.start, "next": next, "items": items});
        if let Space::Paper(name) = space {
            out["sheet"] = json!(name);
        }
        Ok(out)
    }

    /// What block `name` holds, in its own frame (mm from its base point), as [`Opened::query`]
    /// lists model space: {block, base, placed (how many times model space places it), total,
    /// start, next, items}.
    pub fn block(&self, name: &str, q: &Query) -> Result<Value, String> {
        let b = self.drawing.block(name).ok_or_else(|| {
            let mut near: Vec<&str> = self
                .drawing
                .blocks
                .keys()
                .filter(|k| !k.starts_with('*') && k.to_lowercase().contains(&name.to_lowercase()))
                .map(String::as_str)
                .take(20)
                .collect();
            near.sort_unstable();
            if near.is_empty() {
                format!("the drawing has no block `{name}`")
            } else {
                format!("the drawing has no block `{name}`; blocks with that in their name: {}", near.join(", "))
            }
        })?;
        let asked = Asked { area: None, ..self.asked(&Query { area: None, ..q.clone() })? };
        let mut cache = cadcraft_doc::BlockBoundsCache::default();
        let base = b.base.xy();
        let boxed: Vec<(&Entity, Bounds2)> =
            b.entities.iter().map(|e| (&**e, cadcraft_doc::entity_bounds_cached(&self.drawing, e, 1, &mut cache))).collect();
        let (total, items, next) = self.page(boxed.into_iter(), &asked, &|p| self.out_own(p - base));
        let placed = self.model_items().filter(|(e, _)| matches!(&e.kind, EntityKind::Insert(i) if i.block == b.name)).count();
        Ok(json!({"block": b.name, "base": self.out_own(base), "placed": placed, "total": total, "start": asked.start, "next": next, "items": items}))
    }

    fn asked(&self, q: &Query) -> Result<Asked, String> {
        let layers = match &q.layers {
            None => None,
            Some(names) => {
                let mut keep = BTreeSet::new();
                for n in names {
                    match self.drawing.layer(n.trim()) {
                        Some(l) => {
                            keep.insert(l.name.to_lowercase());
                        }
                        None => {
                            let all: Vec<&str> = self.drawing.layers.iter().map(|l| l.name.as_str()).take(60).collect();
                            let more = if self.drawing.layers.len() > 60 { ", …" } else { "" };
                            return Err(format!("the drawing has no layer `{n}` (it has: {}{more})", all.join(", ")));
                        }
                    }
                }
                Some(keep)
            }
        };
        let kinds = match &q.kinds {
            None => None,
            Some(ks) => Some(
                ks.iter()
                    .map(|k| kinds::named(k).ok_or_else(|| format!("`{k}` is not a kind; the kinds are {}", crate::KINDS.join(", "))))
                    .collect::<Result<BTreeSet<_>, _>>()?,
            ),
        };
        let limit = q.limit.unwrap_or(LIMIT);
        if !(1..=MOST).contains(&limit) {
            return Err(format!("`limit` is how many items a page lists, 1 to {MOST}, not {limit}"));
        }
        let text = q.text.as_ref().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty());
        Ok(Asked { layers, kinds, area: q.area.map(|b| self.box_in(b)), text, start: q.start.unwrap_or(0), limit })
    }

    /// The page of `items` (each with its box, drawing units) that `asked` asks for: how many match in
    /// all, the page's items given out with `pt`, and the `start` of the next page (null at the end).
    fn page<'a>(
        &self,
        items: impl Iterator<Item = (&'a Entity, Bounds2)>,
        asked: &Asked,
        pt: &dyn Fn(Vec2) -> [f64; 2],
    ) -> (usize, Vec<Value>, Value) {
        let (mut total, mut out, mut points) = (0_usize, Vec::new(), 0_usize);
        let mut full = false;
        for (e, b) in items {
            if asked.layers.as_ref().is_some_and(|l| !l.contains(&e.common.layer.to_lowercase())) {
                continue;
            }
            if asked.kinds.as_ref().is_some_and(|k| !k.contains(kind_of(&e.kind))) {
                continue;
            }
            if let Some(area) = &asked.area
                && (b.is_empty() || !b.intersects(area))
            {
                continue;
            }
            if let Some(t) = &asked.text
                && !words(self, e).is_some_and(|w| w.to_lowercase().contains(t.as_str()))
            {
                continue;
            }
            if total >= asked.start && !full {
                let (v, n) = item(self, e, &b, pt);
                out.push(v);
                points += n;
                full = out.len() >= asked.limit || points >= POINTS;
            }
            total += 1;
        }
        let next = asked.start + out.len();
        (total, out, if next < total { json!(next) } else { Value::Null })
    }
}

/// The words an item shows, for `text`: a text's, a placement's attribute values, a dimension's or
/// a multileader's text.
fn words(o: &Opened, e: &Entity) -> Option<String> {
    match &e.kind {
        EntityKind::Text(t) => Some(t.value.clone()),
        EntityKind::MText(t) => Some(cadcraft_fonts::plain_mtext(&t.contents)),
        EntityKind::AttDef(a) => Some(format!("{} {}", a.tag, a.text.value)),
        EntityKind::Insert(i) => {
            let vals: Vec<&str> = i.attribs.iter().map(|a| a.text.value.as_str()).filter(|v| !v.is_empty()).collect();
            (!vals.is_empty()).then(|| vals.join(" "))
        }
        EntityKind::Dimension(d) => Some(dim_text(o, d)),
        EntityKind::MLeader(m) => m.text.as_ref().map(|t| cadcraft_fonts::plain_mtext(&t.contents)),
        _ => None,
    }
}

/// A dimension's text as it reads: its own text with `<>` the measurement, or the measurement.
fn dim_text(o: &Opened, d: &cadcraft_doc::Dimension) -> String {
    let measured = match d.kind {
        cadcraft_doc::DimKind::Angular | cadcraft_doc::DimKind::Angular3P => format!("{}°", round_to(d.measurement.to_degrees(), 2)),
        _ => format!("{}", o.len(d.measurement)),
    };
    let own = cadcraft_fonts::plain_mtext(&d.text);
    if own.is_empty() { measured } else { own.replace("<>", &measured) }
}

fn round_to(x: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (x * k).round() / k
}

fn deg(radians: f64) -> f64 {
    round_to(radians.to_degrees().rem_euclid(360.0), 2)
}

/// An item given out, points through `pt`, with how many points it holds: {h, kind, layer, ...}:
/// line {points}; polyline {points, closed, bulges?}; circle {c, r}; arc {c, r, start, end} (degrees
/// counter-clockwise); ellipse {c, major, ratio, start?, end?}; spline {points, closed}; point {at};
/// text {text, at, height, angle}; block {name, at, angle, scale, box, attributes?, array?};
/// dimension {text, at}; leader {points, text?}; hatch {rings, solid, pattern}; mline {points}; image
/// and table {box}; other {type, box}.
fn item(o: &Opened, e: &Entity, b: &Bounds2, pt: &dyn Fn(Vec2) -> [f64; 2]) -> (Value, usize) {
    let k = kind_of(&e.kind);
    let mut v = json!({"h": format!("{:X}", e.handle.0), "kind": k, "layer": e.common.layer});
    let tol = 1.0 / o.mm; // a millimetre, in the drawing's units
    let pts = |ps: &mut dyn Iterator<Item = Vec2>| -> Vec<[f64; 2]> { ps.map(pt).collect() };
    let bx = |b: &Bounds2| if b.is_empty() { Value::Null } else { json!([pt(b.min), pt(b.max)]) };
    let mut n = 1;
    let mut set = |key: &str, val: Value| {
        v[key] = val;
    };
    match &e.kind {
        EntityKind::Line(l) => {
            set("points", json!([pt(l.a.xy()), pt(l.b.xy())]));
            n = 2;
        }
        EntityKind::LwPolyline(p) => {
            set("points", json!(pts(&mut p.vertices.iter().map(|q| q.p))));
            set("closed", json!(p.closed));
            if p.vertices.iter().any(|q| q.bulge != 0.0) {
                set("bulges", json!(p.vertices.iter().map(|q| round_to(q.bulge, 4)).collect::<Vec<_>>()));
            }
            n = p.vertices.len();
        }
        EntityKind::Polyline3d(p) => {
            set("points", json!(pts(&mut p.points.iter().map(|q| q.xy()))));
            set("closed", json!(p.closed));
            n = p.points.len();
        }
        EntityKind::Circle(c) => {
            set("c", json!(pt(c.center.xy())));
            set("r", json!(o.len(c.radius)));
        }
        EntityKind::Arc(a) => {
            set("c", json!(pt(a.center.xy())));
            set("r", json!(o.len(a.radius)));
            set("start", json!(deg(a.start)));
            set("end", json!(deg(a.end)));
        }
        EntityKind::Ellipse(el) => {
            set("c", json!(pt(el.center.xy())));
            set("major", json!([o.len(el.major.x), o.len(el.major.y)]));
            set("ratio", json!(round_to(el.ratio, 4)));
            let full = (cadcraft_geom::ccw_sweep(el.start, el.end) - cadcraft_geom::TAU).abs() < 1e-9;
            if !full {
                set("start", json!(deg(el.start)));
                set("end", json!(deg(el.end)));
            }
        }
        EntityKind::Spline(s) => {
            let mut ps = s.tessellate(tol * 10.0);
            ps.truncate(CURVE_POINTS);
            n = ps.len();
            set("points", json!(pts(&mut ps.into_iter())));
        }
        EntityKind::Point(p) => set("at", json!(pt(p.p.xy()))),
        EntityKind::Text(t) => {
            set("text", json!(t.value));
            set("at", json!(pt(t.justify_point())));
            set("height", json!(o.len(t.height)));
            set("angle", json!(deg(t.rotation)));
        }
        EntityKind::MText(t) => {
            set("text", json!(cadcraft_fonts::plain_mtext(&t.contents)));
            set("at", json!(pt(t.insert.xy())));
            set("height", json!(o.len(t.height)));
            set("angle", json!(deg(t.rotation)));
        }
        EntityKind::AttDef(a) => {
            set("tag", json!(a.tag));
            set("text", json!(a.text.value));
            set("at", json!(pt(a.text.justify_point())));
            set("height", json!(o.len(a.text.height)));
        }
        EntityKind::Insert(i) => {
            set("name", json!(i.block));
            set("at", json!(pt(i.insert.xy())));
            set("angle", json!(deg(i.rotation)));
            set("scale", json!([round_to(i.scale.x, 4), round_to(i.scale.y, 4)]));
            set("box", bx(b));
            let attrs: serde_json::Map<String, Value> =
                i.attribs.iter().filter(|a| !a.text.value.is_empty()).map(|a| (a.tag.clone(), json!(a.text.value))).collect();
            if !attrs.is_empty() {
                set("attributes", Value::Object(attrs));
            }
            if i.cols > 1 || i.rows > 1 {
                set("array", json!({"cols": i.cols, "rows": i.rows}));
            }
            n = 5;
        }
        EntityKind::Dimension(d) => {
            set("text", json!(dim_text(o, d)));
            set("at", json!(pt(d.text_mid.xy())));
        }
        EntityKind::Leader(l) => {
            set("points", json!(pts(&mut l.vertices.iter().map(|q| q.xy()))));
            n = l.vertices.len();
        }
        EntityKind::MLeader(m) => {
            let line: Vec<[f64; 2]> = m.leaders.first().map(|l| l.iter().map(|q| pt(q.xy())).collect()).unwrap_or_default();
            n = line.len().max(1);
            set("points", json!(line));
            if let Some(t) = &m.text {
                set("text", json!(cadcraft_fonts::plain_mtext(&t.contents)));
            }
        }
        EntityKind::Hatch(h) => {
            let rings: Vec<Vec<[f64; 2]>> = h
                .loops
                .iter()
                .map(|l| {
                    let mut ring = Polyline { vertices: l.vertices.clone(), closed: true }.tessellate(tol * 10.0);
                    ring.truncate(CURVE_POINTS);
                    ring.into_iter().map(pt).collect()
                })
                .collect();
            n = rings.iter().map(Vec::len).sum::<usize>().max(1);
            set("rings", json!(rings));
            set("solid", json!(h.solid));
            if !h.solid {
                set("pattern", json!(h.pattern));
            }
        }
        EntityKind::Solid(s) | EntityKind::Trace(s) => {
            let c = &s.corners;
            set("rings", json!([[pt(c[0].xy()), pt(c[1].xy()), pt(c[3].xy()), pt(c[2].xy())]]));
            set("solid", json!(true));
            n = 4;
        }
        EntityKind::Wipeout(w) => {
            let ring: Vec<[f64; 2]> = w.closed_boundary().iter().map(|p| pt(*p)).collect();
            n = ring.len().max(1);
            set("rings", json!([ring]));
            set("solid", json!(true));
            set("wipeout", json!(true));
        }
        EntityKind::MLine(m) => {
            let line: Vec<[f64; 2]> = m.segments().iter().map(|s| pt(s.start())).collect();
            n = line.len().max(1);
            set("points", json!(line));
            set("style", json!(m.style));
        }
        EntityKind::Image(_) | EntityKind::Table(_) => set("box", bx(b)),
        other => {
            set("type", json!(other.type_name()));
            set("box", bx(b));
        }
    }
    (v, n)
}

/// The four corners of the box round what a placement draws, in its own turned frame (drawing units),
/// counter-clockwise from the low corner of its own frame; none for a block that draws nothing.
pub(crate) fn placed_frame(o: &Opened, ins: &cadcraft_doc::Insert) -> Option<[Vec2; 4]> {
    let blk = o.drawing.block(&ins.block)?;
    let own = o.block_box(&blk.name)?;
    let m = ins.transform(blk.base.xy());
    let c = own.corners();
    Some([m.apply(c[0]), m.apply(c[1]), m.apply(c[2]), m.apply(c[3])])
}

impl Opened {
    /// The box round what block `name` draws in its own frame, drawing units (none: it draws nothing).
    pub(crate) fn block_box(&self, name: &str) -> Option<Bounds2> {
        let mut kept = self.block_boxes.lock().ok()?;
        if let Some(b) = kept.get(name) {
            return *b;
        }
        let blk = self.drawing.block(name)?;
        let mut cache = cadcraft_doc::BlockBoundsCache::default();
        let b = blk.entities.iter().fold(Bounds2::EMPTY, |acc, e| acc.union(&cadcraft_doc::entity_bounds_cached(&self.drawing, e, 1, &mut cache)));
        let b = (!b.is_empty()).then_some(b);
        kept.insert(name.to_string(), b);
        b
    }
}
