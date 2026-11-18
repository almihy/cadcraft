//! A drawing's lines placed and flattened: block placements exploded (nested ones too, arrays cell
//! by cell), each curve as the shape it draws, in millimetres less the offset. What finding the
//! closed outlines a drawing's lines make works on (Arqiflow's engine, `cad_outlines`), and any
//! reader that needs the lines as drawn rather than as the file keeps them.

use std::collections::{BTreeSet, HashMap};

use cadcraft_doc::{Drawing, Entity, EntityKind};
use cadcraft_geom::{Mat3, Polyline, Vec2};

use crate::Opened;

/// The deepest blocks are exploded through (a block placing itself is cut there).
const DEPTH: usize = 16;
/// Stands for "has a text" among a block's layers.
const TEXT: &str = "\u{0}text";

/// A shape as it is drawn, mm less the offset.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Line([f64; 2], [f64; 2]),
    Polyline {
        points: Vec<[f64; 2]>,
        closed: bool,
    },
    Circle {
        c: [f64; 2],
        r: f64,
    },
    /// Degrees counter-clockwise, from `start` to `end`.
    Arc {
        c: [f64; 2],
        r: f64,
        start: f64,
        end: f64,
    },
    Text {
        at: [f64; 2],
        height: f64,
        angle: f64,
        text: String,
    },
    Fill {
        rings: Vec<Vec<[f64; 2]>>,
        solid: bool,
    },
}

/// A shape flattened from the drawing: the handle of the model space item it came from (a block
/// placement's for what the block draws), its layer (a block's contents on layer 0 take the
/// placement's), and the shape.
#[derive(Clone, Debug, PartialEq)]
pub struct Flat {
    pub handle: String,
    pub layer: String,
    pub shape: Shape,
}

impl Opened {
    /// Model space flattened: the lines, curves and fills on `layers` (by name, any case; none: on
    /// every layer that is on), and with `texts` every text on any layer (a placement's attribute
    /// values among them), each from an item touching `area` (mm, given as [`Opened::query`] takes
    /// a box; none: anywhere). Past `most` lines and fills, an error says how many there are.
    pub fn shapes(&self, layers: Option<&[String]>, area: Option<[[f64; 2]; 2]>, texts: bool, most: usize) -> Result<Vec<Flat>, String> {
        let wanted: Option<BTreeSet<String>> = match layers {
            None => None,
            Some(names) => Some(
                names
                    .iter()
                    .map(|n| self.drawing.layer(n.trim()).map(|l| l.name.to_lowercase()).ok_or_else(|| format!("the drawing has no layer `{n}`")))
                    .collect::<Result<_, _>>()?,
            ),
        };
        let area = area.map(|b| self.box_in(b));
        let mut walk = Walk { o: self, wanted, texts, out: Vec::new(), lines: 0, most, tol: 1.0 / self.mm };
        for (e, b) in self.model_items() {
            if area.is_some_and(|a| b.is_empty() || !b.intersects(&a)) {
                continue;
            }
            let handle = format!("{:X}", e.handle.0);
            walk.entity(e, &handle, &e.common.layer, Mat3::IDENTITY, 0)?;
        }
        Ok(walk.out)
    }
}

struct Walk<'a> {
    o: &'a Opened,
    wanted: Option<BTreeSet<String>>,
    texts: bool,
    out: Vec<Flat>,
    lines: usize,
    most: usize,
    /// A millimetre in the drawing's units: how near a curve's chords keep to it.
    tol: f64,
}

impl Walk<'_> {
    fn takes(&self, layer: &str) -> bool {
        match &self.wanted {
            Some(w) => w.contains(&layer.to_lowercase()),
            None => self.o.drawing.layer(layer).is_none_or(|l| l.on && !l.frozen),
        }
    }

    fn entity(&mut self, e: &Entity, handle: &str, layer: &str, m: Mat3, depth: usize) -> Result<(), String> {
        if !e.common.visible {
            return Ok(());
        }
        if let EntityKind::Insert(ins) = &e.kind {
            return self.insert(ins, handle, layer, m, depth);
        }
        let is_text = matches!(e.kind, EntityKind::Text(_) | EntityKind::MText(_));
        if is_text && !self.texts {
            return Ok(());
        }
        if !is_text && !self.takes(layer) {
            return Ok(());
        }
        let mut kind = e.kind.clone();
        if m != Mat3::IDENTITY {
            kind.transform(&m);
        }
        let Some(shape) = self.shape(&kind) else { return Ok(()) };
        if !matches!(shape, Shape::Text { .. }) {
            self.lines += 1;
            if self.lines > self.most {
                return Err(format!("the pick takes more than {} lines and shapes; narrow it with `layers` or `box`", self.most));
            }
        }
        self.out.push(Flat { handle: handle.to_string(), layer: layer.to_string(), shape });
        Ok(())
    }

    fn insert(&mut self, ins: &cadcraft_doc::Insert, handle: &str, layer: &str, m: Mat3, depth: usize) -> Result<(), String> {
        let d: &Drawing = &self.o.drawing;
        let Some(blk) = d.block(&ins.block) else { return Ok(()) };
        if self.texts {
            for a in ins.attribs.iter().filter(|a| !a.invisible && !a.text.value.is_empty()) {
                let mut t = EntityKind::Text(a.text.clone());
                if m != Mat3::IDENTITY {
                    t.transform(&m);
                }
                if let Some(shape) = self.shape(&t) {
                    self.out.push(Flat { handle: handle.to_string(), layer: layer.to_string(), shape });
                }
            }
        }
        if depth >= DEPTH {
            return Ok(());
        }
        // Explode only what may hold something wanted: a block with a layer asked for, or with texts when they are.
        if let Some(inside) = self.o.layers_in_block(&blk.name) {
            let wanted_here = match &self.wanted {
                None => true,
                Some(w) => inside.iter().any(|l| w.contains(l)) || (inside.contains("0") && w.contains(&layer.to_lowercase())),
            };
            if !wanted_here && !(self.texts && inside.contains(TEXT)) {
                return Ok(());
            }
        }
        let cols = ins.cols.clamp(1, 10_000);
        let rows = ins.rows.clamp(1, 10_000);
        for r in 0..rows {
            for c in 0..cols {
                let off = Vec2::new(ins.col_spacing * f64::from(c), ins.row_spacing * f64::from(r)).rotate(ins.rotation);
                let local = Mat3::translate(off).then_before(ins.transform(blk.base.xy()));
                let here = m.then_before(local);
                for child in blk.entities.iter() {
                    let child_layer = if child.common.layer == "0" { layer } else { child.common.layer.as_str() };
                    self.entity(child, handle, child_layer, here, depth + 1)?;
                }
            }
        }
        Ok(())
    }

    /// The shape an entity (placed already) draws, in mm less the offset; none for what draws no line, fill or text.
    fn shape(&self, k: &EntityKind) -> Option<Shape> {
        let o = self.o;
        let pts = |ps: Vec<Vec2>| ps.into_iter().map(|p| o.out(p)).collect::<Vec<_>>();
        Some(match k {
            EntityKind::Line(l) => Shape::Line(o.out(l.a.xy()), o.out(l.b.xy())),
            EntityKind::LwPolyline(p) => {
                let points = if p.vertices.iter().any(|v| v.bulge != 0.0) {
                    let mut ps = Polyline { vertices: p.vertices.clone(), closed: p.closed }.tessellate(self.tol);
                    if p.closed && ps.len() > 1 && ps.first() == ps.last() {
                        ps.pop();
                    }
                    ps
                } else {
                    p.vertices.iter().map(|v| v.p).collect()
                };
                Shape::Polyline { points: pts(points), closed: p.closed }
            }
            EntityKind::Polyline3d(p) => Shape::Polyline { points: pts(p.points.iter().map(|q| q.xy()).collect()), closed: p.closed },
            EntityKind::Circle(c) => Shape::Circle { c: o.out(c.center.xy()), r: o.len(c.radius) },
            EntityKind::Arc(a) => Shape::Arc {
                c: o.out(a.center.xy()),
                r: o.len(a.radius),
                start: a.start.to_degrees().rem_euclid(360.0),
                end: a.end.to_degrees().rem_euclid(360.0),
            },
            EntityKind::Ellipse(el) => {
                let g = cadcraft_geom::Ellipse { center: el.center.xy(), major: el.major.xy(), ratio: el.ratio, start: el.start, end: el.end };
                let mut ps = Vec::new();
                g.tessellate(self.tol, &mut ps);
                let full = (cadcraft_geom::ccw_sweep(el.start, el.end) - cadcraft_geom::TAU).abs() < 1e-9;
                if full && ps.len() > 1 && ps.first() == ps.last() {
                    ps.pop();
                }
                Shape::Polyline { points: pts(ps), closed: full }
            }
            EntityKind::Spline(s) => Shape::Polyline { points: pts(s.tessellate(self.tol)), closed: false },
            EntityKind::Hatch(h) => Shape::Fill {
                rings: h.loops.iter().map(|l| pts(Polyline { vertices: l.vertices.clone(), closed: true }.tessellate(self.tol))).collect(),
                solid: h.solid,
            },
            EntityKind::Solid(s) | EntityKind::Trace(s) => {
                let c = &s.corners;
                Shape::Fill { rings: vec![pts(vec![c[0].xy(), c[1].xy(), c[3].xy(), c[2].xy()])], solid: true }
            }
            EntityKind::Text(t) => Shape::Text {
                at: o.out(t.justify_point()),
                height: o.len(t.height),
                angle: t.rotation.to_degrees().rem_euclid(360.0),
                text: o.shown(&t.style, &t.value).unwrap_or_else(|| t.value.clone()),
            },
            EntityKind::MText(t) => Shape::Text {
                at: o.out(t.insert.xy()),
                height: o.len(t.height),
                angle: t.rotation.to_degrees().rem_euclid(360.0),
                text: {
                    let s = cadcraft_fonts::plain_mtext(&t.contents);
                    o.shown(&t.style, &s).unwrap_or(s)
                },
            },
            _ => return None,
        })
    }
}

/// The layers each block holds (lower case), its nested blocks' included, and TEXT when it holds a
/// text: what tells a pick whether a placement of it is worth exploding.
pub(crate) fn block_layers(d: &Drawing) -> HashMap<String, BTreeSet<String>> {
    fn of(d: &Drawing, name: &str, out: &mut HashMap<String, BTreeSet<String>>, depth: usize) -> BTreeSet<String> {
        if let Some(found) = out.get(name) {
            return found.clone();
        }
        let mut here = BTreeSet::new();
        let Some(blk) = d.block(name) else { return here };
        out.insert(blk.name.clone(), BTreeSet::new()); // a block placing itself stops here
        for e in blk.entities.iter() {
            here.insert(e.common.layer.to_lowercase());
            match &e.kind {
                EntityKind::Insert(i) if depth < DEPTH => {
                    if !i.attribs.is_empty() {
                        here.insert(TEXT.into());
                    }
                    here.extend(of(d, &i.block, out, depth + 1));
                }
                EntityKind::Text(_) | EntityKind::MText(_) => {
                    here.insert(TEXT.into());
                }
                _ => {}
            }
        }
        out.insert(blk.name.clone(), here.clone());
        here
    }
    let mut out = HashMap::new();
    for name in d.blocks.keys() {
        of(d, name, &mut out, 0);
    }
    out
}
