//! A drawing made from scratch (no DWG source) with a table, a multileader and a multiline in
//! styles of their own, saved as a DWG: `new_dwg out.dwg [AC1032]`. For checking the direct
//! DWG writer in other programs.
use cadcraft_color::Color;
use cadcraft_doc::*;
use cadcraft_geom::Vec3;

fn v(x: f64, y: f64) -> Vec3 {
    Vec3::new(x, y, 0.0)
}

fn main() -> std::result::Result<(), String> {
    let out = std::env::args().nth(1).ok_or("usage: new_dwg out.dwg [version]")?;
    let version = std::env::args().nth(2).unwrap_or_else(|| "AC1032".into());
    let mut d = Drawing::new_metric();
    d.layers.push(Layer { color: Color::Index(1), ..Layer::new("NOTES") });
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
    d.mleader_styles.push(MLeaderStyle { name: "Callout".into(), arrow_size: 3.0, text_height: 2.5, landing_gap: 0.75, dogleg: 4.0, ..MLeaderStyle::default() });
    let cell = |t: &str| TableCell { text: t.into(), ..TableCell::default() };
    let table = Table {
        insert: v(100.0, 60.0),
        col_widths: vec![30.0, 25.0],
        row_heights: vec![9.0, 7.0, 7.0, 7.0],
        cells: vec![
            vec![TableCell { merged: Some((1, 2)), ..cell("Room schedule") }, cell("")],
            vec![cell("Room"), cell("Area m²")],
            vec![cell("Office"), cell("12.5")],
            vec![cell("Meeting"), cell("18.0")],
        ],
        style: "Schedule".into(),
        text_height: 2.5,
        title: true,
        header: true,
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
        vertices: [v(0.0, 0.0), v(60.0, 0.0), v(60.0, 40.0), v(0.0, 40.0)].into_iter().map(MLineVertex::new).collect(),
    };
    let mleader = MLeader {
        leaders: vec![vec![v(30.0, 20.0), v(45.0, 55.0)]],
        landing: v(50.0, 55.0),
        dogleg: 4.0,
        text: Some(MText {
            insert: v(54.75, 55.0),
            height: 2.5,
            width: 40.0,
            attach: 1,
            rotation: 0.0,
            style: "Standard".into(),
            contents: "Cavity wall\\Pas specified".into(),
            line_spacing: 1.0,
            line_spacing_exact: false,
            columns: None,
        }),
        style: "Callout".into(),
        arrow_size: 3.0,
        spline: false,
        block: None,
    };
    for k in [EntityKind::Table(table), EntityKind::MLine(mline), EntityKind::MLeader(mleader)] {
        let common = Common { layer: "NOTES".into(), ..Common::default() };
        d.add(&Space::Model, common, k).map_err(|e| e.to_string())?;
    }
    let v = cadcraft_io::Version::for_acadver(&version).ok_or("version")?;
    let (bytes, unwritten) =
        cadcraft_io::write_noted(&d, &out, &cadcraft_io::SaveOptions { version: Some(v), binary: false }).map_err(|e| e.to_string())?;
    std::fs::write(&out, bytes).map_err(|e| e.to_string())?;
    println!("wrote {out}; left out: {unwritten:?}");
    Ok(())
}
