//! An edit of a DWG read into CADCraft, saved as a DWG: a line added, the first block reference
//! erased and the first dimension moved 50 units right. `edit_dwg in.dwg out.dwg` prints the
//! handles it changed. For checking that the direct DWG writer keeps the rest of the file.
use cadcraft_doc::{Common, EntityKind, Line, Space};
use cadcraft_geom::{Mat3, Vec2, Vec3};

fn main() -> std::result::Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (input, out) = (args.next().ok_or("usage: edit_dwg in.dwg out.dwg")?, args.next().ok_or("usage: edit_dwg in.dwg out.dwg")?);
    let bytes = std::fs::read(&input).map_err(|e| e.to_string())?;
    let mut d = cadcraft_io::read(&bytes, &input).map_err(|e| e.to_string())?;
    let line = d
        .add(&Space::Model, Common::default(), EntityKind::Line(Line { a: Vec3::new(1006.0, -989.0, 0.0), b: Vec3::new(1683.0, -690.0, 0.0) }))
        .map_err(|e| e.to_string())?;
    let insert = d.model.iter().find(|e| matches!(e.kind, EntityKind::Insert(_))).map(|e| e.handle).ok_or("no block reference")?;
    d.model.remove(insert);
    let dim = d.model.iter().find(|e| matches!(e.kind, EntityKind::Dimension(_))).map(|e| e.handle).ok_or("no dimension")?;
    d.modify_entity(dim, |e| e.kind.transform(&Mat3::translate(Vec2::new(50.0, 0.0)))).map_err(|e| e.to_string())?;
    let v = cadcraft_io::save_version(&d);
    let (dwg, unwritten) =
        cadcraft_io::write_noted(&d, &out, &cadcraft_io::SaveOptions { version: Some(v), binary: false }).map_err(|e| e.to_string())?;
    std::fs::write(&out, dwg).map_err(|e| e.to_string())?;
    println!("added line {}, erased block reference {}, moved dimension {} by (50, 0); left out: {unwritten:?}", line.hex(), insert.hex(), dim.hex());
    Ok(())
}
