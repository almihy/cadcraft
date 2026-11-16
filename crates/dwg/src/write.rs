//! A drawing written into an acadrust document.
pub fn write(_d: &cadcraft_doc::Drawing, _v: acadrust::DxfVersion) -> Result<(Vec<u8>, crate::Unwritten), String> {
    Err("not yet".into())
}
