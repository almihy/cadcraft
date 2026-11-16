//! acadrust's own read-back of a DWG: model-space entity count by type, layers, block records.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let r = acadrust::DwgReader::from_stream_with_options(std::io::Cursor::new(&bytes[..]), acadrust::DwgReadOptions::failsafe()).read_with_stats().unwrap();
    let d = r.document;
    let mut by = std::collections::BTreeMap::new();
    for e in d.model_space_entities() {
        *by.entry(format!("{:?}", std::mem::discriminant(e)).len().to_string() + &crate_name(e)).or_insert(0) += 1;
    }
    println!("model space {} layers {} block records {} skipped {}", d.model_space_entities().count(), d.layers.len(), d.block_records.len(), r.stats.skipped_source_records);
    for (k, v) in by { println!("  {k}: {v}"); }
}
fn crate_name(e: &acadrust::EntityType) -> String {
    let s = format!("{e:?}");
    s.split(['(', ' ', '{']).next().unwrap_or("").to_string()
}
