fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let h = u64::from_str_radix(&a[2], 16).unwrap();
    let r = acadrust::DwgReader::from_stream_with_options(std::io::Cursor::new(&bytes[..]), acadrust::DwgReadOptions::failsafe()).read_with_stats().unwrap();
    if let Some(acadrust::EntityType::Hatch(x)) = r.document.get_entity(acadrust::types::Handle::new(h)) {
        for p in &x.paths { println!("{:?}", p.edges); }
    }
}
