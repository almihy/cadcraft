include!("C:/Users/al_mi/AppData/Local/Temp/claude/2dd1a4e87a9e9d81/2d3da087-bb66-4e17-a0c7-f1096a792a63/scratchpad/dwgdirect/dump_common.rs");
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let t = std::time::Instant::now();
    let (d, notes) = cadcraft_dwg::read_here(&bytes, cadcraft_dwg::Limits::DEFAULT).unwrap();
    eprintln!("read {:?}, notes {:?}", t.elapsed(), notes);
    dump(&d, &a[2]);
}
