//! acadrust's own read of a DWG, without CADCraft: model-space entities by type, layers and
//! block records. For checking what a saved file holds: `acadrust_counts file.dwg`.
fn main() -> Result<(), String> {
    let path = std::env::args().nth(1).ok_or("usage: acadrust_counts file.dwg")?;
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let r = acadrust::DwgReader::from_stream_with_options(std::io::Cursor::new(&bytes[..]), acadrust::DwgReadOptions::failsafe())
        .read_with_stats()
        .map_err(|e| e.to_string())?;
    let d = r.document;
    let mut by = std::collections::BTreeMap::new();
    for e in d.model_space_entities() {
        let name = format!("{e:?}").split(['(', ' ', '{']).next().unwrap_or_default().to_string();
        *by.entry(name).or_insert(0usize) += 1;
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
    Ok(())
}
