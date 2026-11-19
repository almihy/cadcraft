//! What a drawing holds, in short: the first thing the agent is told of it, and kept with its pack.
//! Never a full list of what is many (CADCraft's own inspection of a large office drawing runs to
//! 441,000 characters): the blocks most placed, with how many more there are, and the rest by count.

use std::collections::{BTreeMap, HashMap};

use cadcraft_doc::EntityKind;
use serde_json::{Value, json};

use crate::{Opened, kind_of};

/// How many blocks the summary names, the most placed first.
pub const BLOCKS_LISTED: usize = 25;
/// How many layers it names, those with the most items first (a drawing with its references bound
/// can have a thousand), and how many text styles and notes from reading the file.
pub const LAYERS_LISTED: usize = 100;
pub const STYLES_LISTED: usize = 40;
pub const NOTES_LISTED: usize = 20;

/// The summary: {unit, mm (millimetres to one unit), offset (when far out), extents [[x0, y0], [x1,
/// y1]] (mm, of the drawing's bulk; `outer` too when strays lie far beyond it), items (model space),
/// kinds {kind: count}, sheets [{name, items}], layers [{name, items, kinds, off?, frozen?}] (most
/// items first, up to LAYERS_LISTED; `layers_total` when there are more), blocks [{name, placed,
/// size: [w, h]}] (most placed first, up to BLOCKS_LISTED), blocks_total (blocks placed in model
/// space), blocks_defined, text_styles [{name, font}] (up to STYLES_LISTED; `text_styles_total`
/// when there are more), text {items, height} (model space's text and multi-line text: how many,
/// and the height in the middle of them, mm; left out when there is none), and left_out (what
/// reading the file could not take, and why), when anything was: its `notes` up to NOTES_LISTED
/// (`notes_total` when there are more).
pub(crate) fn summary(o: &Opened) -> Value {
    let d = &o.drawing;
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    let mut per_layer: HashMap<&str, (usize, BTreeMap<&str, usize>)> = HashMap::new();
    // Placements by block name: how many, and the box of the first (its own turned frame's size).
    let mut placed: HashMap<&str, (usize, [f64; 2])> = HashMap::new();
    let mut text_heights: Vec<f64> = Vec::new();
    for (e, b) in o.model_items() {
        let k = kind_of(&e.kind);
        match &e.kind {
            EntityKind::Text(t) if t.height.is_finite() && t.height > 0.0 => text_heights.push(t.height),
            EntityKind::MText(t) if t.height.is_finite() && t.height > 0.0 => text_heights.push(t.height),
            _ => {}
        }
        *kinds.entry(k).or_default() += 1;
        let (n, ks) = per_layer.entry(e.common.layer.as_str()).or_default();
        *n += 1;
        *ks.entry(k).or_default() += 1;
        if let EntityKind::Insert(ins) = &e.kind {
            let p = placed.entry(ins.block.as_str()).or_insert_with(|| (0, size_in_own_frame(o, ins, b)));
            p.0 += 1;
        }
    }
    let mut layers: Vec<(&str, usize, BTreeMap<&str, usize>)> = per_layer.into_iter().map(|(l, (n, ks))| (l, n, ks)).collect();
    layers.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let layers_total = layers.len();
    let layers: Vec<Value> = layers
        .into_iter()
        .take(LAYERS_LISTED)
        .map(|(name, n, ks)| {
            let mut row = json!({"name": name, "items": n, "kinds": ks});
            if let Some(l) = d.layer(name) {
                if !l.on {
                    row["off"] = json!(true);
                }
                if l.frozen {
                    row["frozen"] = json!(true);
                }
            }
            row
        })
        .collect();
    let mut blocks: Vec<(&str, usize, [f64; 2])> = placed.iter().map(|(n, (c, s))| (*n, *c, *s)).collect();
    blocks.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let blocks_total = blocks.len();
    let listed: Vec<Value> = blocks.iter().take(BLOCKS_LISTED).map(|(name, n, size)| json!({"name": name, "placed": n, "size": size})).collect();
    let sheets: Vec<Value> = o.sheets().iter().map(|l| json!({"name": l.name, "items": l.entities.len()})).collect();
    let styles: Vec<Value> = d.text_styles.iter().take(STYLES_LISTED).map(|s| json!({"name": s.name, "font": s.font})).collect();
    let mut out = json!({
        "unit": o.unit, "mm": o.mm,
        "items": d.model.len(), "kinds": kinds,
        "sheets": sheets, "layers": layers,
        "blocks": listed, "blocks_total": blocks_total,
        "blocks_defined": d.blocks.values().filter(|b| !b.anonymous && !b.name.starts_with('*')).count(),
        "text_styles": styles,
    });
    if !text_heights.is_empty() {
        let (items, mid) = (text_heights.len(), text_heights.len() / 2);
        let median = *text_heights.select_nth_unstable_by(mid, f64::total_cmp).1;
        out["text"] = json!({"items": items, "height": o.len(median)});
    }
    if layers_total > LAYERS_LISTED {
        out["layers_total"] = json!(layers_total);
    }
    if d.text_styles.len() > STYLES_LISTED {
        out["text_styles_total"] = json!(d.text_styles.len());
    }
    if let Some(bulk) = o.bulk() {
        out["extents"] = json!([o.out(bulk.min), o.out(bulk.max)]);
        let all = o.extents();
        if all.width() > bulk.width() * 1.5 || all.height() > bulk.height() * 1.5 {
            out["outer"] = json!([o.out(all.min), o.out(all.max)]);
        }
    }
    if o.offset != cadcraft_geom::Vec2::ZERO {
        out["offset"] = json!(o.offset());
    }
    let mut left_out = serde_json::Map::new();
    if !d.read_notes.is_empty() {
        left_out.insert("notes".into(), json!(d.read_notes.iter().take(NOTES_LISTED).collect::<Vec<_>>()));
        if d.read_notes.len() > NOTES_LISTED {
            left_out.insert("notes_total".into(), json!(d.read_notes.len()));
        }
    }
    if !d.skipped_records.is_empty() {
        left_out.insert("records".into(), json!(d.skipped_records));
    }
    let unknown: BTreeMap<&str, usize> = d.model.iter().fold(BTreeMap::new(), |mut m, e| {
        if let EntityKind::Unknown(u) = &e.kind {
            *m.entry(u.dxf_type.as_str()).or_default() += 1;
        }
        m
    });
    if !unknown.is_empty() {
        left_out.insert("not_drawn".into(), json!(unknown));
    }
    if !left_out.is_empty() {
        out["left_out"] = Value::Object(left_out);
    }
    out
}

/// The size of a placement's box in its own turned frame, mm: its width along its x and its height.
fn size_in_own_frame(o: &Opened, ins: &cadcraft_doc::Insert, world: &cadcraft_geom::Bounds2) -> [f64; 2] {
    match crate::query::placed_frame(o, ins) {
        Some(c) => [o.len(c[0].dist(c[1])), o.len(c[1].dist(c[2]))],
        None => [o.len(world.width()), o.len(world.height())],
    }
}
