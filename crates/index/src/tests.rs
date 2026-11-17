#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use cadcraft_doc::{Block, Common, Drawing, EntityKind, HAlign, Insert, Line, LwPolyline, Space, Text, VAlign};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use serde_json::json;

use crate::{Opened, Query, Shape};

fn on(layer: &str) -> Common {
    Common { layer: layer.into(), ..Common::default() }
}

fn text(at: Vec3, value: &str) -> Text {
    Text {
        insert: at,
        align_pt: None,
        height: 0.2,
        value: value.into(),
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: HAlign::Left,
        valign: VAlign::Baseline,
    }
}

/// A plan in metres: a 10 × 8 room on WALLS with its name inside, a chair block placed three times
/// (twice on FURNITURE, once on layer 0), a stray line on WALLS far away. `east`/`north`: where it
/// lies (metres).
fn plan(east: f64, north: f64) -> Drawing {
    let mut d = Drawing::new_metric();
    d.header.set_i64("INSUNITS", 6);
    let o = Vec2::new(east, north);
    let p = |x: f64, y: f64| Vec3::new(east + x, north + y, 0.0);
    let room = LwPolyline {
        vertices: [(0.0, 0.0), (10.0, 0.0), (10.0, 8.0), (0.0, 8.0)]
            .iter()
            .map(|(x, y)| PolyVertex { p: o + Vec2::new(*x, *y), bulge: 0.0, start_width: 0.0, end_width: 0.0 })
            .collect(),
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
    };
    d.add(&Space::Model, on("WALLS"), EntityKind::LwPolyline(room)).unwrap();
    d.add(&Space::Model, on("ROOMS"), EntityKind::Text(text(p(4.0, 4.0), "Living Room"))).unwrap();
    let mut chair = Block::new("CHAIR");
    for (a, b) in [((0.0, 0.0), (0.5, 0.0)), ((0.5, 0.0), (0.5, 0.5)), ((0.5, 0.5), (0.0, 0.5)), ((0.0, 0.5), (0.0, 0.0))] {
        let h = d.new_handle();
        chair.entities.push(cadcraft_doc::Entity {
            handle: h,
            common: on("0"),
            kind: EntityKind::Line(Line { a: Vec3::new(a.0, a.1, 0.0), b: Vec3::new(b.0, b.1, 0.0) }),
        });
    }
    d.blocks.insert("CHAIR".into(), Arc::new(chair));
    for (x, layer) in [(1.0, "FURNITURE"), (2.0, "FURNITURE"), (3.0, "0")] {
        let ins = Insert { block: "CHAIR".into(), insert: p(x, 1.0), rotation: std::f64::consts::FRAC_PI_2, ..Insert::default() };
        d.add(&Space::Model, on(layer), EntityKind::Insert(ins)).unwrap();
    }
    d.add(&Space::Model, on("WALLS"), EntityKind::Line(Line { a: p(5000.0, 0.0), b: p(5001.0, 0.0) })).unwrap();
    d
}

fn ask(o: &Opened, q: serde_json::Value) -> serde_json::Value {
    let q: Query = serde_json::from_value(q).unwrap();
    o.query(&q).unwrap()
}

#[test]
fn the_summary_says_what_is_on_each_layer_in_millimetres() {
    let o = Opened::new(plan(0.0, 0.0));
    let s = o.summary();
    assert_eq!(s["unit"], "m");
    assert_eq!(s["mm"], 1000.0);
    assert_eq!(s["items"], 6);
    assert_eq!(s["kinds"]["block"], 3);
    assert_eq!(s["layers"][0]["name"], "FURNITURE");
    assert_eq!(s["layers"][0]["kinds"]["block"], 2);
    assert_eq!(s["blocks"], json!([{"name": "CHAIR", "placed": 3, "size": [500.0, 500.0]}]));
    // The stray line 5 km away is left out of the extents, and said apart.
    assert_eq!(s["extents"], json!([[0.0, 0.0], [10000.0, 8000.0]]));
    assert_eq!(s["outer"][1][0], 5_001_000.0);
    assert!(s.get("offset").is_none());
}

#[test]
fn a_query_pages_through_what_matches() {
    let o = Opened::new(plan(0.0, 0.0));
    let page = ask(&o, json!({"layers": ["walls"], "limit": 1}));
    assert_eq!((page["total"].clone(), page["next"].clone()), (json!(2), json!(1)));
    let room = &page["items"][0];
    assert_eq!(room["kind"], "polyline");
    assert_eq!(room["points"][2], json!([10000.0, 8000.0]));
    let next = ask(&o, json!({"layers": ["WALLS"], "start": 1}));
    assert_eq!(next["items"][0]["kind"], "line");
    assert!(next["next"].is_null());
    // A block placement is one item, with its box.
    let blocks = ask(&o, json!({"kinds": ["blocks"]}));
    assert_eq!(blocks["total"], 3);
    let chair = &blocks["items"][0];
    assert_eq!((chair["name"].clone(), chair["angle"].clone()), (json!("CHAIR"), json!(90.0)));
    assert_eq!(chair["box"], json!([[500.0, 1000.0], [1000.0, 1500.0]]));
    // A box keeps what touches it; text what holds the words.
    assert_eq!(ask(&o, json!({"box": [[2400, 900], [2600, 1100]]}))["total"], 2, "the third chair, and the room round it");
    assert_eq!(ask(&o, json!({"box": [[2400, 900], [2600, 1100]], "kinds": ["block"]}))["total"], 1);
    let named = ask(&o, json!({"text": "living"}));
    assert_eq!(named["items"][0]["text"], "Living Room");
    assert_eq!(named["items"][0]["at"], json!([4000.0, 4000.0]));
    // A layer or kind that is not there says what there is.
    let bad: Query = serde_json::from_value(json!({"layers": ["DOORS"]})).unwrap();
    assert!(o.query(&bad).unwrap_err().contains("WALLS"));
    let bad: Query = serde_json::from_value(json!({"kinds": ["wall"]})).unwrap();
    assert!(o.query(&bad).unwrap_err().contains("polyline"));
}

#[test]
fn a_block_query_lists_what_the_block_holds() {
    let o = Opened::new(plan(0.0, 0.0));
    let b = ask(&o, json!({"block": "chair"}));
    assert_eq!((b["block"].clone(), b["placed"].clone(), b["total"].clone()), (json!("CHAIR"), json!(3), json!(4)));
    assert_eq!(b["items"][1]["points"], json!([[500.0, 0.0], [500.0, 500.0]]));
}

#[test]
fn a_far_drawing_comes_less_its_offset_and_takes_a_box_either_way() {
    // In a real grid: 500 km east, 2,700 km north (metres).
    let o = Opened::new(plan(499_995.0, 2_699_996.0));
    let s = o.summary();
    assert_eq!(s["offset"], json!([500_000_000.0, 2_700_000_000.0]), "a round point (10 m) near the middle");
    assert_eq!(s["extents"][0], json!([-5000.0, -4000.0]));
    let local = ask(&o, json!({"box": [[-2400, -2900], [-2200, -2700]], "kinds": ["block"]}));
    let own = ask(&o, json!({"box": [[499_997_600.0, 2_699_997_100.0], [499_997_800.0, 2_699_997_300.0]], "kinds": ["block"]}));
    assert_eq!(local["total"], 1);
    assert_eq!(local, own);
}

#[test]
fn the_pack_reads_back_the_same_drawing() {
    let d = plan(0.0, 0.0);
    let o = Opened::new(d.clone());
    let packed = crate::pack(&d).unwrap();
    let back = Opened::unpack(&packed).unwrap();
    assert_eq!(back.summary(), o.summary());
    assert_eq!(back.drawing.model, d.model);
    assert_eq!(back.drawing.blocks["CHAIR"].entities, d.blocks["CHAIR"].entities);
    assert!(Opened::unpack(b"not a pack").is_err());
}

#[test]
fn shapes_explode_blocks_and_keep_texts_for_labels() {
    let o = Opened::new(plan(0.0, 0.0));
    let all = o.shapes(Some(&["FURNITURE".to_string()]), None, true, 1000).unwrap();
    let lines: Vec<_> = all.iter().filter(|f| matches!(f.shape, Shape::Line(..))).collect();
    assert_eq!(lines.len(), 8, "two chairs of four lines on FURNITURE (the third is on 0)");
    assert!(lines.iter().all(|f| f.layer == "FURNITURE"));
    assert!(all.iter().any(|f| matches!(&f.shape, Shape::Text { text, .. } if text == "Living Room")));
    // A chair turned a quarter: its first line runs up from its insertion point.
    assert_eq!(lines[0].shape, Shape::Line([1000.0, 1000.0], [1000.0, 1500.0]));
    assert!(o.shapes(None, None, false, 3).unwrap_err().contains("narrow it"));
}

#[test]
fn a_picture_is_drawn_whole_or_in_part() {
    let o = Opened::new(plan(0.0, 0.0));
    let whole = o.picture(1, None, 400).unwrap();
    assert_eq!((whole.page, whole.pages, whole.name.as_str()), (1, 1, "Model"));
    assert_eq!(whole.width, 400);
    assert!(whole.png.starts_with(b"\x89PNG"));
    let part = o.picture(1, Some([0.0, 0.5, 0.5, 1.0]), 400).unwrap();
    assert!(part.png.len() > 100);
    assert!(o.picture(2, None, 400).is_err());
    assert!(o.picture(1, Some([0.5, 0.0, 0.4, 1.0]), 400).is_err());
}

#[test]
fn a_sheet_takes_its_box_as_given_in_a_far_drawing_too() {
    let mut d = plan(499_995.0, 2_699_996.0);
    d.layouts.push(cadcraft_doc::Layout::new("A-101", 1));
    let sheet = Space::Paper("A-101".into());
    d.add(&sheet, on("TITLE"), EntityKind::Text(text(Vec3::new(0.2, 0.1, 0.0), "Ground floor"))).unwrap();
    let o = Opened::new(d);
    assert!(o.summary().get("offset").is_some());
    let on_sheet = ask(&o, json!({"sheet": "a-101", "box": [[100, 0], [300, 200]]}));
    assert_eq!(on_sheet["total"], 1, "a sheet is never far out: no offset is put on its box");
    assert_eq!(on_sheet["items"][0]["at"], json!([200.0, 100.0]));
}

#[test]
fn the_summary_names_the_most_used_layers_and_says_how_many_there_are() {
    let mut d = plan(0.0, 0.0);
    for k in 0..crate::summary::LAYERS_LISTED + 30 {
        let a = Vec3::new(k as f64, 0.0, 0.0);
        d.add(&Space::Model, on(&format!("L{k:03}")), EntityKind::Line(Line { a, b: a + Vec3::new(0.5, 0.0, 0.0) })).unwrap();
    }
    let s = Opened::new(d).summary();
    assert_eq!(s["layers"].as_array().unwrap().len(), crate::summary::LAYERS_LISTED);
    assert_eq!(s["layers_total"], crate::summary::LAYERS_LISTED + 30 + 4);
    assert_eq!(s["layers"][0]["name"], "FURNITURE", "the most used first");
}
