#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use cadcraft_doc::{Block, Common, Drawing, EntityKind, HAlign, Insert, Line, LwPolyline, Space, Text, VAlign};
use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use serde_json::json;

use crate::{LinesParams, Opened, Piece, Query, Shape};

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

/// `block` holding `kinds` on `layer`, in `d`.
fn block_of(d: &mut Drawing, name: &str, kinds: Vec<(&str, EntityKind)>) {
    let mut b = Block::new(name);
    for (layer, kind) in kinds {
        let handle = d.new_handle();
        b.entities.push(cadcraft_doc::Entity { handle, common: on(layer), kind });
    }
    d.blocks.insert(name.into(), Arc::new(b));
}

fn line(a: (f64, f64), b: (f64, f64)) -> EntityKind {
    EntityKind::Line(Line { a: Vec3::new(a.0, a.1, 0.0), b: Vec3::new(b.0, b.1, 0.0) })
}

fn near(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

#[test]
fn lines_come_by_layer_in_metres_with_blocks_exploded() {
    let o = Opened::new(plan(0.0, 0.0));
    let l = o.lines(&json!({})).unwrap();
    assert_eq!((l["unit"].as_str(), l["header_unit"].as_str(), l["metres"].as_f64()), (Some("m"), Some("m"), Some(1.0)));
    assert_eq!(l["order"]["segs"], json!(["WALLS", "FURNITURE", "0"]), "the layers in the order they are first drawn on");
    assert_eq!(l["segs"]["WALLS"][0], json!([0.0, 0.0, 10.0, 0.0]));
    assert_eq!(l["segs"]["WALLS"].as_array().unwrap().len(), 5, "the room's four sides, the last closing it, and the stray line");
    assert_eq!(l["segs"]["FURNITURE"].as_array().unwrap().len(), 8, "a block's layer 0 is the placement's layer");
    assert_eq!(l["segs"]["0"].as_array().unwrap().len(), 4);
    assert_eq!(l["bbox"], json!([0.0, 0.0, 5001.0, 8.0]));
    assert_eq!(l["left_out"]["text"], 1);
    let furniture = o.line_items(&LinesParams::default()).unwrap().items.into_iter().find(|i| i.layer == "FURNITURE").unwrap();
    assert!(furniture.zero_in_block && furniture.block.as_deref() == Some("CHAIR") && furniture.kind == "line");
    // only the layers asked for
    let walls = o.lines(&json!({"layers": ["walls"]})).unwrap();
    assert_eq!(walls["order"]["segs"], json!(["WALLS"]));
    assert!(o.lines(&json!({"layers": ["NONE"]})).unwrap_err().contains("no layer"));
    assert!(o.lines(&json!({"layer": ["WALLS"]})).unwrap_err().contains("unknown parameter `layer`"));
}

#[test]
fn lines_take_the_unit_they_are_told_over_the_headers() {
    // drawn in metres, the header says inches
    let mut d = plan(0.0, 0.0);
    d.header.set_i64("INSUNITS", 1);
    let o = Opened::new(d);
    let as_header = o.lines(&json!({})).unwrap();
    assert_eq!(as_header["unit"], "in");
    assert!(near(&serde_json::from_value::<Vec<f64>>(as_header["segs"]["WALLS"][0].clone()).unwrap(), &[0.0, 0.0, 0.254, 0.0]));
    let told = o.lines(&json!({"unit": "m"})).unwrap();
    assert_eq!((told["unit"].as_str(), told["header_unit"].as_str()), (Some("m"), Some("in")));
    assert_eq!(told["segs"]["WALLS"][0], json!([0.0, 0.0, 10.0, 0.0]));
    assert!(o.lines(&json!({"unit": "cubits"})).unwrap_err().contains("unknown unit"));
}

#[test]
fn a_bulged_polyline_and_a_mirrored_blocks_arc_come_as_arcs() {
    let mut d = Drawing::new_metric();
    d.header.set_i64("INSUNITS", 6);
    // a straight piece, then a half circle from (2, 0) round to (2, 2): centre (2, 1), radius 1
    let v = |x: f64, y: f64, bulge: f64| PolyVertex { p: Vec2::new(x, y), bulge, start_width: 0.0, end_width: 0.0 };
    let p = LwPolyline {
        vertices: vec![v(0.0, 0.0, 0.0), v(2.0, 0.0, 1.0), v(2.0, 2.0, 0.0)],
        closed: false,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
    };
    d.add(&Space::Model, on("WALL"), EntityKind::LwPolyline(p)).unwrap();
    // a quarter circle from 0° to 90° about its block's origin, placed mirrored in x at (10, 0): it runs 90° to 180°
    let quarter = EntityKind::Arc(cadcraft_doc::Arc { center: Vec3::ZERO, radius: 1.0, start: 0.0, end: std::f64::consts::FRAC_PI_2 });
    block_of(&mut d, "LEAF", vec![("DOOR", quarter)]);
    let ins = Insert { block: "LEAF".into(), insert: Vec3::new(10.0, 0.0, 0.0), scale: Vec3::new(-1.0, 1.0, 1.0), ..Insert::default() };
    d.add(&Space::Model, on("0"), EntityKind::Insert(ins)).unwrap();
    let l = Opened::new(d).lines(&json!({})).unwrap();
    assert_eq!(l["segs"]["WALL"], json!([[0.0, 0.0, 2.0, 0.0]]));
    let arc = |layer: &str| serde_json::from_value::<Vec<f64>>(l["arcs"][layer][0].clone()).unwrap();
    let (half, pi) = (std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    assert!(near(&arc("WALL"), &[2.0, 1.0, 1.0, -half, half, 3.0, 1.0]), "{:?}", arc("WALL"));
    let s = std::f64::consts::FRAC_1_SQRT_2;
    assert!(near(&arc("DOOR"), &[10.0, 0.0, 1.0, half, pi, 10.0 - s, s]), "{:?}", arc("DOOR"));
    assert_eq!(l["bbox"], json!([0.0, 0.0, 10.0, 2.0]), "the box takes an arc's reach, not only its ends");
}

#[test]
fn a_circle_stays_one_and_what_is_no_circle_comes_as_chords() {
    let mut d = Drawing::new_metric();
    d.header.set_i64("INSUNITS", 6);
    let circle = EntityKind::Circle(cadcraft_doc::Circle { center: Vec3::ZERO, radius: 1.0 });
    block_of(&mut d, "ROUND", vec![("0", circle)]);
    for (x, sy, layer) in [(0.0, 2.0, "EVEN"), (10.0, 1.0, "STRETCHED")] {
        let ins = Insert { block: "ROUND".into(), insert: Vec3::new(x, 0.0, 0.0), scale: Vec3::new(2.0, sy, 1.0), ..Insert::default() };
        d.add(&Space::Model, on(layer), EntityKind::Insert(ins)).unwrap();
    }
    // an ellipse whose axes are equal is a circle too
    let round = cadcraft_doc::Ellipse {
        center: Vec3::new(20.0, 0.0, 0.0),
        major: Vec3::new(0.0, 3.0, 0.0),
        ratio: 1.0,
        start: 0.0,
        end: std::f64::consts::TAU,
    };
    d.add(&Space::Model, on("EVEN"), EntityKind::Ellipse(round)).unwrap();
    let o = Opened::new(d);
    let l = o.lines(&json!({})).unwrap();
    assert_eq!(l["circles"]["EVEN"], json!([[0.0, 0.0, 2.0], [20.0, 0.0, 3.0]]));
    assert!(l["circles"].get("STRETCHED").is_none() && l["arcs"].get("STRETCHED").is_none());
    let fine = l["segs"]["STRETCHED"].as_array().unwrap().len();
    assert!(fine >= 8, "an ellipse is its chords");
    let coarse = o.lines(&json!({"chord": 0.1})).unwrap()["segs"]["STRETCHED"].as_array().unwrap().len();
    assert!(coarse < fine, "fewer chords when they may stray further: {coarse} of {fine}");
    assert!(o.lines(&json!({"chord": 0})).unwrap_err().contains("chord"));
}

fn dimension(block: Option<&str>) -> cadcraft_doc::Dimension {
    cadcraft_doc::Dimension {
        kind: cadcraft_doc::DimKind::Linear { rotation: 0.0 },
        defpt: Vec3::new(10.0, 2.0, 0.0),
        text_mid: Vec3::new(5.0, 2.0, 0.0),
        p13: Vec3::ZERO,
        p14: Vec3::new(10.0, 0.0, 0.0),
        p15: Vec3::ZERO,
        p16: Vec3::ZERO,
        text: String::new(),
        style: "Standard".into(),
        measurement: 10.0,
        text_rotation: 0.0,
        user_text_pos: false,
        block: block.map(str::to_string),
        overrides: Default::default(),
        assoc: Vec::new(),
        marks: Default::default(),
        insertion_point: Vec3::new(0.0, 100.0, 0.0),
    }
}

#[test]
fn a_dimension_comes_as_the_lines_its_block_draws() {
    let mut d = Drawing::new_metric();
    d.header.set_i64("INSUNITS", 6);
    // the block the file keeps: the dimension line on layer 0, a tick as a solid, its text
    let tick = EntityKind::Solid(cadcraft_doc::Solid {
        corners: [Vec3::new(-0.1, 1.9, 0.0), Vec3::new(-0.09, 1.89, 0.0), Vec3::new(0.09, 2.11, 0.0), Vec3::new(0.1, 2.1, 0.0)],
    });
    block_of(&mut d, "*D1", vec![("0", line((0.0, 2.0), (10.0, 2.0))), ("0", tick), ("0", EntityKind::Text(text(Vec3::new(5.0, 2.1, 0.0), "10")))]);
    d.add(&Space::Model, on("DIMS"), EntityKind::Dimension(dimension(Some("*D1")))).unwrap();
    let o = Opened::new(d);
    let l = o.lines(&json!({})).unwrap();
    assert_eq!(l["segs"]["DIMS"], json!([[0.0, 102.0, 10.0, 102.0]]), "on the dimension's layer, moved by its insertion point");
    assert_eq!(l["left_out"], json!({"hatch": 1, "text": 1}));
    let item = &o.line_items(&LinesParams::default()).unwrap().items[0];
    assert!(item.dimension && item.block.as_deref() == Some("*D1"));
    // solids as the edges of their two triangles, the diagonal once each way
    let with = o.line_items(&LinesParams { solids: true, ..Default::default() }).unwrap();
    let edges: Vec<[f64; 4]> =
        with.items.iter().filter(|i| i.kind == "hatch").map(|i| if let Piece::Seg(s) = i.piece { s } else { [0.0; 4] }).collect();
    assert_eq!(edges.len(), 6);
    assert!(near(&edges[1], &[-0.09, 101.89, 0.09, 102.11]) && near(&edges[3], &[0.09, 102.11, -0.09, 101.89]));
    assert!(!with.left_out.contains_key("hatch"));
}

#[test]
fn a_dimension_without_a_block_draws_its_styles_lines() {
    let mut d = Drawing::new_metric();
    d.header.set_i64("INSUNITS", 6);
    d.add(&Space::Model, on("DIMS"), EntityKind::Dimension(dimension(None))).unwrap();
    let items = Opened::new(d).line_items(&LinesParams::default()).unwrap().items;
    assert!(items.len() >= 3, "a dimension line and two extension lines at least");
    assert!(items.iter().all(|i| i.dimension && i.layer == "DIMS" && matches!(i.piece, Piece::Seg(_))));
}

#[test]
fn a_placement_on_a_frozen_layer_draws_nothing() {
    let mut d = plan(0.0, 0.0);
    let o = Opened::new(d.clone());
    assert_eq!(o.lines(&json!({})).unwrap()["segs"]["FURNITURE"].as_array().unwrap().len(), 8);
    // the chair's lines are on layer 0, so a placement's layer being off hides them as well
    d.layer_mut("FURNITURE").unwrap().on = false;
    assert!(Opened::new(d.clone()).lines(&json!({})).unwrap()["segs"].get("FURNITURE").is_none());
    // lines on a layer of their own in a block show while the placement's layer is only off, and not when it is frozen
    block_of(&mut d, "DESK", vec![("DESKS", line((0.0, 0.0), (1.0, 0.0)))]);
    d.layers.push(cadcraft_doc::Layer::new("DESKS"));
    d.add(&Space::Model, on("FURNITURE"), EntityKind::Insert(Insert { block: "DESK".into(), ..Insert::default() })).unwrap();
    assert_eq!(Opened::new(d.clone()).lines(&json!({})).unwrap()["segs"]["DESKS"].as_array().unwrap().len(), 1);
    d.layer_mut("FURNITURE").unwrap().frozen = true;
    assert!(Opened::new(d.clone()).lines(&json!({})).unwrap()["segs"].get("DESKS").is_none());
    // asked for by name, a layer comes whatever its state
    assert_eq!(Opened::new(d).lines(&json!({"layers": ["DESKS"]})).unwrap()["segs"]["DESKS"].as_array().unwrap().len(), 1);
}
