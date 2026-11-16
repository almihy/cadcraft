//! acadrust's own read of a DWG, without CADCraft: model-space entities by type, layers and
//! block records. For checking what a saved file holds: `acadrust_counts file.dwg`; with `--all`,
//! also every entity of the file (blocks and layouts too) and every object, by type.
fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("usage: acadrust_counts file.dwg [--all]")?;
    let all = std::env::args().any(|a| a == "--all");
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let r = acadrust::DwgReader::from_stream_with_options(std::io::Cursor::new(&bytes[..]), acadrust::DwgReadOptions::failsafe())
        .read_with_stats()
        .map_err(|e| e.to_string())?;
    let d = r.document;
    let kind = |s: String| s.split(['(', ' ', '{']).next().unwrap_or_default().to_string();
    let mut by = std::collections::BTreeMap::new();
    for e in d.model_space_entities() {
        *by.entry(kind(format!("{e:?}"))).or_insert(0usize) += 1;
    }
    println!(
        "model space {} layers {} block records {} skipped records {}",
        d.model_space_entities().count(),
        d.layers.len(),
        d.block_records.len(),
        r.stats.skipped_source_records
    );
    for (k, v) in by {
        println!("  {k}: {v}");
    }
    if all {
        let mut ents = std::collections::BTreeMap::new();
        for e in d.entities() {
            *ents.entry(kind(format!("{e:?}"))).or_insert(0usize) += 1;
        }
        println!("every entity:");
        for (k, v) in ents {
            println!("  {k}: {v}");
        }
        let mut objs = std::collections::BTreeMap::new();
        for o in d.objects.values() {
            *objs.entry(kind(format!("{o:?}"))).or_insert(0usize) += 1;
        }
        println!("objects:");
        for (k, v) in objs {
            println!("  {k}: {v}");
        }
    }
    Ok(())
}
