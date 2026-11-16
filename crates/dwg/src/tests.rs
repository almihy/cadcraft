//! Round trips of small synthetic drawings: Drawing → DWG → Drawing, for each entity kind, and
//! edits written into the document a drawing was read from.

use cadcraft_color::Color;
use cadcraft_doc::*;
use cadcraft_geom::{FitOptions, PolyVertex, Spline, Vec2, Vec3};

fn v(x: f64, y: f64) -> Vec3 {
    Vec3::new(x, y, 0.0)
}

/// Whether two JSON values are the same, reals within a billionth.
fn same(a: &serde_json::Value, b: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    use serde_json::Value as J;
    match (a, b) {
        (J::Object(x), J::Object(y)) => {
            let keys: std::collections::BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                same(x.get(k).unwrap_or(&J::Null), y.get(k).unwrap_or(&J::Null), &format!("{path}.{k}"), out);
            }
        }
        (J::Array(x), J::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                same(p, q, &format!("{path}[{i}]"), out);
            }
        }
        (J::Number(x), J::Number(y)) => {
            let (p, q) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
            if (p - q).abs() > 1e-9 * p.abs().max(q.abs()).max(1.0) {
                out.push(format!("{path}: {p} vs {q}"));
            }
        }
        _ if a != b => out.push(format!("{path}: {a} vs {b}")),
        _ => {}
    }
}

fn diff(a: &Entity, b: &Entity) -> Vec<String> {
    let mut out = Vec::new();
    same(&serde_json::to_value(a).unwrap(), &serde_json::to_value(b).unwrap(), "", &mut out);
    out
}

fn line(h: u64) -> Entity {
    Entity::new(Handle(h), EntityKind::Line(Line { a: v(0.0, 0.0), b: v(10.0, 5.0) }))
}

/// One entity of every kind both models have, with properties set off their defaults.
fn kinds() -> Vec<EntityKind> {
    let pv = |x: f64, y: f64, b: f64| PolyVertex::with_bulge(Vec2::new(x, y), b);
    let text = Text {
        insert: v(1.0, 2.0),
        align_pt: Some(v(4.0, 2.0)),
        height: 0.5,
        value: "Hello ^ wörld".into(),
        rotation: 0.3,
        width_factor: 0.8,
        oblique: 0.1,
        style: "Standard".into(),
        halign: HAlign::Center,
        valign: VAlign::Middle,
    };
    let mtext = MText {
        insert: v(3.0, 4.0),
        height: 0.25,
        width: 12.0,
        attach: 5,
        rotation: 0.5,
        style: "Standard".into(),
        contents: "first\\Psecond {\\fArial|b1;bold}".into(),
        line_spacing: 1.5,
        line_spacing_exact: true,
        columns: None,
    };
    let spline = Spline::from_control(vec![Vec2::new(0.0, 0.0), Vec2::new(2.0, 3.0), Vec2::new(5.0, -1.0), Vec2::new(8.0, 2.0)], 3);
    let fitted = Spline::fit_with(&[Vec2::new(0.0, 0.0), Vec2::new(3.0, 2.0), Vec2::new(6.0, 0.0)], false, FitOptions::default());
    // Each kind with the definition points it has.
    let dim = |kind: DimKind| Dimension {
        defpt: v(10.0, 5.0),
        text_mid: v(5.0, 6.0),
        p13: if matches!(kind, DimKind::Radius | DimKind::Diameter) { Vec3::ZERO } else { v(1.0, 0.5) },
        p14: if matches!(kind, DimKind::Radius | DimKind::Diameter) { Vec3::ZERO } else { v(10.0, 0.5) },
        p15: if matches!(kind, DimKind::Radius | DimKind::Diameter | DimKind::Angular | DimKind::Angular3P) { v(3.0, 3.0) } else { Vec3::ZERO },
        p16: if matches!(kind, DimKind::Angular) { v(4.0, 4.0) } else { Vec3::ZERO },
        kind,
        text: "<> mm".into(),
        style: "Standard".into(),
        // An ordinate dimension measures from its leader end to its feature along its axis.
        measurement: if matches!(kind, DimKind::Ordinate { .. }) { 9.0 } else { 10.0 },
        text_rotation: 0.0,
        user_text_pos: true,
        block: Some("*D1".into()),
        insertion_point: Vec3::ZERO,
        overrides: serde_json::Map::from_iter([("arrowSize".to_string(), serde_json::json!(0.5)), ("decimals".to_string(), serde_json::json!(1))]),
        assoc: Vec::new(),
        marks: DimMarks::default(),
    };
    vec![
        EntityKind::Line(Line { a: Vec3::new(1.0, 2.0, 3.0), b: v(10.0, 5.0) }),
        EntityKind::Point(Point { p: v(1.0, 1.0), angle: 0.25 }),
        EntityKind::Circle(Circle { center: v(5.0, 5.0), radius: 2.5 }),
        EntityKind::Arc(Arc { center: v(5.0, 5.0), radius: 2.0, start: 0.5, end: 2.5 }),
        EntityKind::Ellipse(Ellipse { center: v(5.0, 5.0), major: v(4.0, 1.0), ratio: 0.5, start: 0.2, end: 4.0 }),
        EntityKind::LwPolyline(LwPolyline {
            vertices: vec![pv(0.0, 0.0, 0.5), PolyVertex { p: Vec2::new(4.0, 0.0), bulge: 0.0, start_width: 0.1, end_width: 0.2 }, pv(4.0, 3.0, 0.0)],
            closed: true,
            const_width: 0.0,
            elevation: 1.5,
            plinegen: true,
        }),
        EntityKind::Polyline3d(Polyline3d { points: vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 1.0, 2.0)], closed: false }),
        EntityKind::Spline(spline),
        EntityKind::Spline(fitted),
        EntityKind::Ray(RayLine { base: v(1.0, 1.0), dir: Vec3::new(0.6, 0.8, 0.0) }),
        EntityKind::XLine(RayLine { base: v(2.0, 1.0), dir: Vec3::new(1.0, 0.0, 0.0) }),
        EntityKind::Text(text.clone()),
        EntityKind::MText(mtext),
        EntityKind::AttDef(Attrib {
            tag: "ROOM".into(),
            text: Text { align_pt: None, halign: HAlign::Left, valign: VAlign::Baseline, ..text.clone() },
            invisible: false,
            constant: false,
            prompt: "Room name".into(),
            props: AttribProps::default(),
            multiline: None,
            modes: AttribModes { verify: true, preset: false, lock_position: true },
        }),
        EntityKind::Insert(Insert {
            block: "DOOR".into(),
            insert: v(20.0, 10.0),
            scale: Vec3::new(2.0, 2.0, 1.0),
            rotation: 0.75,
            attribs: Vec::new(),
            cols: 1,
            rows: 1,
            col_spacing: 0.0,
            row_spacing: 0.0,
            clip: None,
        }),
        EntityKind::Dimension(dim(DimKind::Linear { rotation: 0.0 })),
        EntityKind::Dimension(dim(DimKind::Aligned)),
        EntityKind::Dimension(dim(DimKind::Radius)),
        EntityKind::Dimension(dim(DimKind::Diameter)),
        EntityKind::Dimension(dim(DimKind::Angular)),
        EntityKind::Dimension(dim(DimKind::Angular3P)),
        EntityKind::Dimension(dim(DimKind::Ordinate { x_type: true })),
        EntityKind::Leader(Leader { vertices: vec![v(0.0, 0.0), v(2.0, 2.0), v(4.0, 2.0)], arrow: true, spline: false, style: "Standard".into() }),
        EntityKind::Hatch(Hatch {
            pattern: "SOLID".into(),
            solid: true,
            loops: vec![HatchLoop { vertices: vec![pv(0.0, 0.0, 0.0), pv(5.0, 0.0, 0.4), pv(5.0, 5.0, 0.0), pv(0.0, 5.0, 0.0)], outer: true, sources: Vec::new() }],
            scale: 1.0,
            angle: 0.0,
            associative: false,
            style: 0,
            elevation: 0.0,
            gradient: None,
            origin: Vec2::ZERO,
            background: None,
            pattern_lines: Vec::new(),
        }),
        EntityKind::Solid(Solid { corners: [v(0.0, 0.0), v(1.0, 0.0), v(0.0, 1.0), v(1.0, 1.0)] }),
        EntityKind::Trace(Solid { corners: [v(2.0, 0.0), v(3.0, 0.0), v(2.0, 1.0), v(3.0, 1.0)] }),
        EntityKind::Face3d(Face3d { corners: [Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 1.0), Vec3::new(1.0, 1.0, 2.0), Vec3::new(0.0, 1.0, 1.0)], hidden_edges: 5 }),
        EntityKind::Tolerance(Tolerance { insert: v(3.0, 3.0), dir: Vec3::new(1.0, 0.0, 0.0), style: "Standard".into(), text: "{\\Fgdt;j}%%v0.1".into() }),
        EntityKind::Wipeout(Wipeout { boundary: vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(4.0, 2.0), Vec2::new(0.0, 2.0)] }),
    ]
}

fn drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    d.layers.push(Layer { color: Color::Index(3), locked: true, ..Layer::new("WALLS") });
    let mut door = Block::new("DOOR");
    door.base = v(1.0, 0.0);
    door.entities.push(line(0x50));
    d.blocks.insert("DOOR".into(), std::sync::Arc::new(door));
    let mut dim_block = Block::new("*D1");
    dim_block.entities.push(line(0x51));
    d.blocks.insert("*D1".into(), std::sync::Arc::new(dim_block));
    d.handseed = 0x1000;
    for (i, k) in kinds().into_iter().enumerate() {
        let common = Common { layer: "WALLS".into(), color: Color::Index(u8::try_from(i % 200 + 1).unwrap_or(1)), ..Common::default() };
        d.model.push(Entity { handle: Handle(0x100 + i as u64), common, kind: k });
    }
    d
}

fn round_trip(d: &Drawing) -> Drawing {
    let (bytes, unwritten) = crate::write(d, "AC1032").unwrap();
    assert!(unwritten.is_empty(), "{unwritten:?}");
    crate::read_here(&bytes, crate::Limits::DEFAULT).unwrap().0
}

#[test]
fn every_kind_reads_back_as_written() {
    let d = drawing();
    let back = round_trip(&d);
    assert_eq!(back.model.len(), d.model.len());
    let mut problems = Vec::new();
    for e in d.model.iter() {
        let Some(b) = back.model.get(e.handle) else {
            problems.push(format!("{}: missing", e.kind.dxf_name()));
            continue;
        };
        // Anonymous blocks are numbered afresh when written.
        let mut b = (**b).clone();
        if let (EntityKind::Dimension(x), EntityKind::Dimension(y)) = (&e.kind, &mut b.kind)
            && x.block.as_deref().is_some_and(|n| n.starts_with("*D"))
            && y.block.as_deref().is_some_and(|n| n.starts_with("*D"))
        {
            y.block.clone_from(&x.block);
        }
        for p in diff(e, &b) {
            problems.push(format!("{} {:X}{p}", e.kind.dxf_name(), e.handle.0));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    let walls = back.layer("WALLS").unwrap();
    assert_eq!((walls.color, walls.locked), (Color::Index(3), true));
    let door = back.block("DOOR").unwrap();
    assert_eq!((door.base, door.entities.len()), (v(1.0, 0.0), 1));
}

#[test]
fn an_unchanged_drawing_is_written_back_as_the_file_had_it() {
    let first = crate::write(&drawing(), "AC1032").unwrap().0;
    let (d, _) = crate::read_here(&first, crate::Limits::DEFAULT).unwrap();
    assert!(d.foreign.dwg_source.is_some());
    let again = crate::write(&d, "AC1032").unwrap().0;
    let (d2, _) = crate::read_here(&again, crate::Limits::DEFAULT).unwrap();
    for e in d.model.iter() {
        assert!(diff(e, d2.model.get(e.handle).unwrap()).is_empty(), "{:?}", e.kind.dxf_name());
    }
    assert_eq!(d2.model.handles(), d.model.handles());
}

#[test]
fn edits_are_written_into_the_source_document() {
    let first = crate::write(&drawing(), "AC1032").unwrap().0;
    let (mut d, _) = crate::read_here(&first, crate::Limits::DEFAULT).unwrap();
    // Erase the first entity, move and re-layer a circle, add a line, a layer and a block.
    let handles = d.model.handles();
    d.model.remove(handles[0]);
    let circle = handles[2];
    d.modify_entity(circle, |e| {
        if let EntityKind::Circle(c) = &mut e.kind {
            c.center = v(50.0, 60.0);
        }
        e.common.layer = "NEW".into();
    })
    .unwrap();
    d.layers.push(Layer::new("NEW"));
    let mut b = Block::new("CHAIR");
    b.entities.push(line(0x2000));
    d.blocks.insert("CHAIR".into(), std::sync::Arc::new(b));
    d.handseed = d.handseed.max(0x2001);
    let added = d.add(&Space::Model, Common::default(), EntityKind::Line(Line { a: v(7.0, 7.0), b: v(8.0, 9.0) })).unwrap();
    d.model.send_to_back(added);
    let (bytes, unwritten) = crate::write(&d, "AC1032").unwrap();
    assert!(unwritten.is_empty());
    let (back, _) = crate::read_here(&bytes, crate::Limits::DEFAULT).unwrap();
    assert!(back.model.get(handles[0]).is_none());
    assert_eq!(back.model.len(), d.model.len());
    let c = back.model.get(circle).unwrap();
    assert!(matches!(&c.kind, EntityKind::Circle(x) if x.center == v(50.0, 60.0)));
    assert_eq!(c.common.layer, "NEW");
    assert!(back.layer("NEW").is_some());
    assert_eq!(back.block("CHAIR").map(|b| b.entities.len()), Some(1));
    assert_eq!(back.model.handles().first(), Some(&added));
    for e in d.model.iter() {
        assert!(diff(e, back.model.get(e.handle).unwrap()).is_empty(), "{:?}", e.kind.dxf_name());
    }
}

#[test]
fn every_version_is_written_and_read_back() {
    for (code, _) in crate::VERSIONS {
        let (bytes, _) = crate::write(&drawing(), code).unwrap();
        assert_eq!(crate::version(&bytes).as_deref(), Some(*code));
        let (back, notes) = crate::read_here(&bytes, crate::Limits::DEFAULT).unwrap();
        assert!(notes.is_empty(), "{code}: {notes:?}");
        assert_eq!(back.model.len(), drawing().model.len(), "{code}");
    }
}

#[test]
fn damaged_and_oversized_files_are_refused() {
    assert!(crate::read_here(b"AC1032 and then nothing", crate::Limits::DEFAULT).is_err());
    let bytes = crate::write(&drawing(), "AC1032").unwrap().0;
    let e = crate::read_here(&bytes, crate::Limits { max_dwg_bytes: 10, ..crate::Limits::DEFAULT }).unwrap_err();
    assert!(e.contains("larger than"), "{e}");
    // A cut-off file is refused or read in part, never a crash.
    let _ = crate::read_here(&bytes[..bytes.len() / 3], crate::Limits::DEFAULT);
    assert!(crate::write(&drawing(), "AC1009").is_err());
}

/// The kinds translated natively beside the plain entities: a table (merged title, a cell's own
/// height), a multiline in a style of its own, a multileader with text, a helix, a clipped block
/// reference and an image, with table, multiline and multileader styles.
fn styled_drawing() -> Drawing {
    let mut d = Drawing::new_metric();
    d.handseed = 0x1000;
    d.table_styles.push(TableStyle { name: "Schedule".into(), text_height: 2.5, margin: 0.75, title: true, header: true, ..TableStyle::default() });
    d.mline_styles.push(MLineStyle {
        name: "Wall".into(),
        description: "Two leaves".into(),
        elements: vec![
            MLineElement { offset: 1.5, color: Color::Index(1), linetype: "ByLayer".into() },
            MLineElement { offset: -1.5, color: Color::Index(5), linetype: "ByLayer".into() },
        ],
        joints: true,
        ..MLineStyle::default()
    });
    d.mleader_styles.push(MLeaderStyle { name: "Callout".into(), arrow_size: 3.0, text_height: 2.0, landing_gap: 0.5, dogleg: 4.0, ..MLeaderStyle::default() });
    let mut chair = Block::new("CHAIR");
    chair.entities.push(line(0x60));
    d.blocks.insert("CHAIR".into(), std::sync::Arc::new(chair));
    let cell = |t: &str| TableCell { text: t.into(), ..TableCell::default() };
    let table = Table {
        insert: v(100.0, 50.0),
        col_widths: vec![20.0, 30.0],
        row_heights: vec![8.0, 6.0, 6.0],
        cells: vec![
            vec![TableCell { merged: Some((1, 2)), ..cell("Rooms") }, cell("")],
            vec![cell("Name"), cell("Area")],
            vec![cell("Office"), TableCell { text_height: Some(3.5), ..cell("12.5") }],
        ],
        style: "Schedule".into(),
        text_height: 2.5,
        title: true,
        header: false,
        ..Table::default()
    };
    let style = d.mline_styles[0].clone();
    let mline = MLine {
        style: style.name.clone(),
        def: style,
        scale: 1.0,
        justification: MLineJust::Zero,
        closed: false,
        no_start_caps: false,
        no_end_caps: false,
        vertices: vec![MLineVertex::new(v(0.0, 0.0)), MLineVertex::new(v(40.0, 0.0)), MLineVertex::new(v(40.0, 30.0))],
    };
    let mleader = MLeader {
        leaders: vec![vec![v(0.0, 60.0), v(10.0, 70.0)]],
        landing: v(15.0, 70.0),
        dogleg: 4.0,
        text: Some(MText { insert: v(19.5, 70.0), height: 2.0, width: 30.0, attach: 1, rotation: 0.0, style: "Standard".into(), contents: "Note".into(), line_spacing: 1.0, line_spacing_exact: false, columns: None }),
        style: "Callout".into(),
        arrow_size: 3.0,
        spline: false,
        block: None,
    };
    let helix = Helix { center: v(200.0, 0.0), base_radius: 5.0, top_radius: 5.0, height: 20.0, turns: 3.0, start_angle: 0.0, ccw: true, constrain: HelixConstrain::default() }.sanitized();
    let insert = Insert {
        block: "CHAIR".into(),
        insert: v(300.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
        rotation: 0.0,
        attribs: Vec::new(),
        cols: 1,
        rows: 1,
        col_spacing: 0.0,
        row_spacing: 0.0,
        clip: Some(ClipBoundary::new(vec![Vec2::new(-1.0, -1.0), Vec2::new(5.0, 4.0)])),
    };
    let image = Image { insert: v(0.0, 100.0), u: Vec3::new(0.1, 0.0, 0.0), v: Vec3::new(0.0, 0.1, 0.0), size: Vec2::new(640.0, 480.0), path: "plans/site.png".into(), name: "site".into(), ..Image::default() };
    for (i, k) in [EntityKind::Table(table), EntityKind::MLine(mline), EntityKind::MLeader(mleader), EntityKind::Helix(helix), EntityKind::Insert(insert), EntityKind::Image(image)].into_iter().enumerate() {
        d.model.push(Entity { handle: Handle(0x200 + i as u64), common: Common::default(), kind: k });
    }
    d
}

fn same_entities(d: &Drawing, back: &Drawing) -> Vec<String> {
    let mut problems = Vec::new();
    for e in d.model.iter() {
        match back.model.get(e.handle) {
            None => problems.push(format!("{}: missing", e.kind.dxf_name())),
            Some(b) => problems.extend(diff(e, b).into_iter().map(|p| format!("{}{p}", e.kind.dxf_name()))),
        }
    }
    problems
}

#[test]
fn tables_multilines_multileaders_helices_clips_and_images_read_back_as_written() {
    let d = styled_drawing();
    let back = round_trip(&d);
    let problems = same_entities(&d, &back);
    assert!(problems.is_empty(), "{problems:#?}");
    for name in ["Schedule"] {
        assert_eq!(back.table_styles.iter().find(|s| s.name == name), d.table_styles.iter().find(|s| s.name == name), "table style {name}");
    }
    assert_eq!(back.mline_styles.iter().find(|s| s.name == "Wall"), d.mline_styles.iter().find(|s| s.name == "Wall"));
    assert_eq!(back.mleader_styles.iter().find(|s| s.name == "Callout"), d.mleader_styles.iter().find(|s| s.name == "Callout"));
}

#[test]
fn tables_and_multilines_are_edited_in_the_file_they_came_from() {
    let first = crate::write(&styled_drawing(), "AC1032").unwrap().0;
    let (mut d, _) = crate::read_here(&first, crate::Limits::DEFAULT).unwrap();
    // A second save of the unchanged drawing reads back the same.
    let again = crate::write(&d, "AC1032").unwrap().0;
    let unchanged = crate::read_here(&again, crate::Limits::DEFAULT).unwrap().0;
    let problems = same_entities(&d, &unchanged);
    assert!(problems.is_empty(), "{problems:#?}");
    // Change a cell, move the multiline's last vertex and take the clip off.
    d.modify_entity(Handle(0x200), |e| {
        if let EntityKind::Table(t) = &mut e.kind {
            t.cells[2][0].text = "Meeting".into();
            t.col_widths[1] = 35.0;
        }
    })
    .unwrap();
    d.modify_entity(Handle(0x201), |e| {
        if let EntityKind::MLine(m) = &mut e.kind
            && let Some(last) = m.vertices.last_mut()
        {
            last.p = v(40.0, 45.0);
        }
    })
    .unwrap();
    d.modify_entity(Handle(0x204), |e| {
        if let EntityKind::Insert(i) = &mut e.kind {
            i.clip = None;
        }
    })
    .unwrap();
    let (bytes, unwritten) = crate::write(&d, "AC1032").unwrap();
    assert!(unwritten.is_empty(), "{unwritten:?}");
    let back = crate::read_here(&bytes, crate::Limits::DEFAULT).unwrap().0;
    let problems = same_entities(&d, &back);
    assert!(problems.is_empty(), "{problems:#?}");
}
