//! DWG helpers for tests: DWG bytes through CADCraft's own reader and writer (`cadcraft_dwg`),
//! seen as the DXF CADCraft writes, where the tests once looked at the old bridge's DXF text.
#![allow(dead_code)]

/// A DWG as CADCraft reads it, written as CADCraft's ASCII DXF.
pub fn dwg_to_dxf(dwg: &[u8]) -> Result<Vec<u8>, String> {
    let d = crate::read(dwg, "x.dwg").map_err(|e| e.to_string())?;
    let v = crate::save_version(&d);
    crate::write_with(&d, "x.dxf", &crate::SaveOptions { version: Some(v), binary: false }).map_err(|e| e.to_string())
}

/// A DXF as CADCraft reads it, written as a DWG of the DXF's version (at least 2004).
pub fn dxf_to_dwg(dxf: &[u8]) -> Result<Vec<u8>, String> {
    let d = crate::read(dxf, "x.dxf").map_err(|e| e.to_string())?;
    crate::write(&d, "x.dwg").map_err(|e| e.to_string())
}

/// [`dxf_to_dwg`] at `version` ("AC1032").
pub fn dxf_to_dwg_as(dxf: &[u8], version: &str) -> Result<Vec<u8>, String> {
    dxf_to_dwg_noted(dxf, version).map(|(b, _)| b)
}

/// [`dxf_to_dwg_as`] with what the DWG leaves out.
pub fn dxf_to_dwg_noted(dxf: &[u8], version: &str) -> Result<(Vec<u8>, crate::Unwritten), String> {
    let d = crate::read(dxf, "x.dxf").map_err(|e| e.to_string())?;
    let v = crate::Version::for_acadver(version).ok_or("version")?;
    crate::write_noted(&d, "x.dwg", &crate::SaveOptions { version: Some(v), binary: false }).map_err(|e| e.to_string())
}

pub use cadcraft_dwg::version;
