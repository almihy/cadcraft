//! Solid modeler entities (3DSOLID, REGION, BODY), which CADCraft keeps without modelling: the
//! groups its DXF reader keeps of one, and the drawing's data store records that hold the ACIS
//! geometry of those whose file keeps it there (AutoCAD 2013 and later). The objects themselves
//! stay in the file's document and are written back from it.

use acadrust::entities::{AcisData, AcisVersion, EntityCommon, EntityType};
use cadcraft_doc::{Common, EntityKind, RawTag, Unknown};

use crate::conv::common;

/// Most bytes of one body's geometry kept as groups (the file's object keeps it all).
const MAX_ACIS_BYTES: usize = 64 << 20;

fn modeler(e: &EntityType) -> Option<(&EntityCommon, &AcisData, &'static str, Option<acadrust::types::Handle>)> {
    match e {
        EntityType::Solid3D(s) => Some((&s.common, &s.acis_data, "3DSOLID", Some(s.history_handle.unwrap_or(acadrust::types::Handle::NULL)))),
        EntityType::Region(r) => Some((&r.common, &r.acis_data, "REGION", None)),
        EntityType::Body(b) => Some((&b.common, &b.acis_data, "BODY", None)),
        _ => None,
    }
}

/// Whether the geometry is the binary kind a file of 2013 or later keeps in its data store.
fn in_store(a: &AcisData) -> bool {
    a.sat_data.is_empty() && !a.sab_data.is_empty()
}

/// A solid, region or body as the groups the DXF reader keeps of it: its geometry as SAT text
/// (groups 1 and 3, the DXF cipher for version 1), or, for binary geometry, the marker of a body
/// whose geometry is in the data store ([`data_store`]). A body the file holds no geometry for has
/// neither.
pub fn entity(e: &EntityType) -> Option<(Common, EntityKind)> {
    let (c, a, name, history) = modeler(e)?;
    let mut tags = Vec::new();
    let mut put = |code: i32, value: String| tags.push(RawTag { code, value });
    put(100, "AcDbEntity".into());
    put(8, if c.layer.is_empty() { "0".into() } else { c.layer.clone() });
    put(100, "AcDbModelerGeometry".into());
    if a.is_binary || in_store(a) {
        put(290, "0".into());
        put(2, "{00000000-0000-0000-0000-000000000000}".into());
    } else {
        put(70, "1".into());
        let mut full = AcisData::strip_sat_terminator(&a.sat_data);
        if full.len() > MAX_ACIS_BYTES {
            full.clear();
        }
        // No geometry (none the file holds): no SAT text either.
        if !full.trim().is_empty() {
            full.push_str("End-of-ACIS-data\n");
            let text = if a.version == AcisVersion::Version1 { AcisData::encode_sat(&full) } else { full };
            for line in text.lines() {
                // Lines past 2049 bytes go on in group 3, cut on character boundaries.
                let mut rest = line;
                let mut code = 1;
                loop {
                    let mut end = rest.len().min(2049);
                    while !rest.is_char_boundary(end) {
                        end -= 1;
                    }
                    let (chunk, more) = rest.split_at(end);
                    put(code, chunk.to_string());
                    code = 3;
                    rest = more;
                    if rest.is_empty() {
                        break;
                    }
                }
            }
        }
    }
    if let Some(h) = history {
        put(100, "AcDb3dSolid".into());
        put(350, format!("{:X}", h.value()));
    }
    let cm = common(c, 0.0, acadrust::types::Vector3::new(0.0, 0.0, 1.0), false);
    Some((cm, EntityKind::Unknown(Unknown { dxf_type: name.into(), tags })))
}

/// The data store section's groups (after its name) for the document's bodies with binary
/// geometry, as the DXF reader keeps a file's: one `ACDSRECORD` per body, naming it (320) and
/// holding its `ASM_Data`. Empty when no body has any.
pub fn data_store(doc: &acadrust::CadDocument) -> Vec<RawTag> {
    let mut out = Vec::new();
    let mut put = |code: i32, value: String| out.push(RawTag { code, value });
    let mut any = false;
    for e in doc.entities() {
        let Some((c, a, _, _)) = modeler(e) else { continue };
        if !in_store(a) || a.sab_data.len() > MAX_ACIS_BYTES {
            continue;
        }
        if !any {
            any = true;
            for (code, v) in [(70, "2"), (71, "2"), (0, "ACDSSCHEMA"), (90, "1"), (1, "AcDb3DSolid_ASM_Data"), (2, "AcDbDs::ID"), (280, "10"), (91, "8")] {
                put(code, v.into());
            }
            for (code, v) in [(2, "ASM_Data"), (280, "15"), (91, "0")] {
                put(code, v.into());
            }
        }
        let sab = &a.sab_data;
        for (code, v) in [(0, "ACDSRECORD".to_string()), (90, "1".into()), (2, "AcDbDs::ID".into()), (280, "10".into())] {
            put(code, v);
        }
        put(320, format!("{:X}", c.handle.value()));
        put(2, "ASM_Data".into());
        put(280, "15".into());
        put(94, sab.len().to_string());
        for chunk in sab.chunks(127) {
            put(310, chunk.iter().map(|x| format!("{x:02X}")).collect());
        }
    }
    out
}
