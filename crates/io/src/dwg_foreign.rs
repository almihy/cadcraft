//! What CADCraft keeps of a DWG without modelling it, in the form its model keeps it: other
//! programs' dictionaries and records (XRECORD, DICTIONARY, dictionary variables, proxy
//! objects), the extension dictionaries and reactors of entities and table records, and the
//! classes they use. The groups are made here from acadrust's objects (no file is written or
//! parsed); which of them a drawing keeps, and the roles of the handles they name, are decided by
//! the same code that decides it for a DXF file (`dxf_raw::read`).
//!
//! Objects acadrust holds in its own typed form (materials, visual styles, dynamic block data,
//! render settings…) stay in the DWG the drawing was read from and are written back from it;
//! their groups are derived only for a DXF save of the drawing (`dwg_ext::for_dxf`).

use cadcraft_doc::Drawing;
use cadcraft_dwg::acadrust::CadDocument;
use cadcraft_dwg::acadrust::objects::{ObjectType, ProxyObjectReference, ProxyReferenceKind, XRecordValue};
use cadcraft_dwg::acadrust::types::Handle as AHandle;
use craft_dxf::{Section, Tag};

fn hx(h: AHandle) -> String {
    format!("{:X}", h.value())
}

fn f(v: f64) -> String {
    format!("{v:?}")
}

/// `102 {ACAD_REACTORS … }` and `102 {ACAD_XDICTIONARY … }` of a record.
fn ownership(t: &mut Vec<Tag>, reactors: &[AHandle], xdict: Option<AHandle>) {
    if !reactors.is_empty() {
        t.push(Tag::s(102, "{ACAD_REACTORS"));
        t.extend(reactors.iter().map(|r| Tag::s(330, hx(*r))));
        t.push(Tag::s(102, "}"));
    }
    if let Some(x) = xdict.filter(|x| !x.is_null()) {
        t.push(Tag::s(102, "{ACAD_XDICTIONARY"));
        t.push(Tag::s(360, hx(x)));
        t.push(Tag::s(102, "}"));
    }
}

fn refs(t: &mut Vec<Tag>, list: &[ProxyObjectReference]) {
    for r in list {
        let code = match r.kind {
            ProxyReferenceKind::Undefined | ProxyReferenceKind::SoftPointer => 330,
            ProxyReferenceKind::HardPointer => 340,
            ProxyReferenceKind::SoftOwnership => 350,
            ProxyReferenceKind::HardOwnership => 360,
        };
        t.push(Tag::s(code, hx(r.handle)));
    }
}

fn hex(b: &[u8]) -> Vec<String> {
    b.chunks(127).map(|c| c.iter().map(|x| format!("{x:02X}")).collect()).collect()
}

/// A string of a DWG before 2007, whose characters outside its code page are `\U+XXXX` escapes
/// (one UTF-16 unit each; a character above U+FFFF is a surrogate pair of them), with the
/// escapes decoded as the DXF reader decodes them. Malformed escapes stay as they are.
pub(crate) fn unescape(s: &str) -> String {
    if !s.contains("\\U+") {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let unit = |i: usize| -> Option<u16> {
        if chars.get(i) != Some(&'\\') || chars.get(i + 1) != Some(&'U') || chars.get(i + 2) != Some(&'+') {
            return None;
        }
        let hex: String = chars.get(i + 3..i + 7)?.iter().collect();
        u16::from_str_radix(&hex, 16).ok()
    };
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(u) = unit(i) {
            if (0xD800..0xDC00).contains(&u)
                && let Some(l) = unit(i + 7).filter(|l| (0xDC00..0xE000).contains(l))
                && let Some(c) = char::decode_utf16([u, l]).next().and_then(Result::ok)
            {
                out.push(c);
                i += 14;
                continue;
            }
            if let Some(c) = char::from_u32(u32::from(u)) {
                out.push(c);
                i += 7;
                continue;
            }
        }
        if let Some(c) = chars.get(i) {
            out.push(*c);
        }
        i += 1;
    }
    out
}

/// The groups of an object of a kind made here: its type and its groups, `None` for any other.
fn object(doc: &CadDocument, h: AHandle, o: &ObjectType, legacy: bool) -> Option<(String, Vec<Tag>)> {
    let text = |s: &str| if legacy { unescape(s) } else { s.to_string() };
    let mut t = vec![Tag::s(5, hx(h))];
    let kind = match o {
        ObjectType::Dictionary(d) => {
            ownership(&mut t, &d.reactors, d.xdictionary_handle);
            t.push(Tag::s(330, hx(d.owner)));
            t.push(Tag::s(100, "AcDbDictionary"));
            if d.hard_owner {
                t.push(Tag::i(280, 1));
            }
            t.push(Tag::i(281, i64::from(d.duplicate_cloning)));
            for (k, v) in &d.entries {
                let hard = d.hard_owner || d.hard_owner_entries.iter().any(|x| x.eq_ignore_ascii_case(k));
                t.push(Tag::s(3, k.clone()));
                t.push(Tag::s(if hard { 360 } else { 350 }, hx(*v)));
            }
            "DICTIONARY"
        }
        ObjectType::DictionaryWithDefault(d) => {
            ownership(&mut t, &[], doc.extension_dictionary_handle(h));
            t.push(Tag::s(330, hx(d.owner)));
            t.push(Tag::s(100, "AcDbDictionary"));
            if d.hard_owner {
                t.push(Tag::i(280, 1));
            }
            t.push(Tag::i(281, i64::from(d.duplicate_cloning)));
            for (k, v) in &d.entries {
                t.push(Tag::s(3, k.clone()));
                t.push(Tag::s(350, hx(*v)));
            }
            t.push(Tag::s(100, "AcDbDictionaryWithDefault"));
            t.push(Tag::s(340, hx(d.default_handle)));
            "ACDBDICTIONARYWDFLT"
        }
        ObjectType::XRecord(x) => {
            ownership(&mut t, &x.reactors, x.xdictionary_handle);
            t.push(Tag::s(330, hx(x.owner)));
            t.push(Tag::s(100, "AcDbXrecord"));
            t.push(Tag::i(280, i64::from(x.cloning_flags as i16)));
            for e in &x.entries {
                match &e.value {
                    XRecordValue::String(s) => t.push(Tag::s(e.code, text(s))),
                    XRecordValue::Double(v) => t.push(Tag::s(e.code, f(*v))),
                    XRecordValue::Int16(v) => t.push(Tag::i(e.code, i64::from(*v))),
                    XRecordValue::Int32(v) => t.push(Tag::i(e.code, i64::from(*v))),
                    XRecordValue::Int64(v) => t.push(Tag::i(e.code, *v)),
                    XRecordValue::Byte(v) => t.push(Tag::i(e.code, i64::from(*v))),
                    XRecordValue::Bool(v) => t.push(Tag::i(e.code, i64::from(*v))),
                    XRecordValue::Handle(v) => t.push(Tag::s(e.code, hx(*v))),
                    XRecordValue::Point3D(x, y, z) => {
                        t.push(Tag::s(e.code, f(*x)));
                        t.push(Tag::s(e.code + 10, f(*y)));
                        t.push(Tag::s(e.code + 20, f(*z)));
                    }
                    XRecordValue::Chunk(b) => t.extend(hex(b).into_iter().map(|c| Tag::s(e.code, c))),
                }
            }
            "XRECORD"
        }
        ObjectType::DictionaryVariable(v) => {
            t.push(Tag::s(330, hx(v.owner_handle)));
            t.push(Tag::s(100, "DictionaryVariables"));
            t.push(Tag::i(280, i64::from(v.schema_number)));
            t.push(Tag::s(1, text(&v.value)));
            "DICTIONARYVAR"
        }
        ObjectType::ProxyObject(p) => {
            ownership(&mut t, &p.reactors, p.xdictionary_handle);
            t.push(Tag::s(330, hx(p.owner)));
            t.push(Tag::s(100, "AcDbProxyObject"));
            t.push(Tag::i(90, 499));
            t.push(Tag::i(91, i64::from(p.class_id)));
            t.push(Tag::i(95, i64::from((p.maintenance_version << 16) | (p.dwg_version & 0xffff))));
            t.push(Tag::i(70, i64::from(p.from_dxf)));
            t.push(Tag::i(93, i64::from(p.payload.bit_count)));
            t.extend(hex(&p.payload.data()).into_iter().map(|c| Tag::s(310, c)));
            refs(&mut t, &p.object_ids);
            t.push(Tag::i(94, 0));
            "ACAD_PROXY_OBJECT"
        }
        _ => return None,
    };
    if let Some(x) = doc.object_xdata(h) {
        t.extend(cadcraft_dwg::xdata_groups(x).into_iter().map(|r| Tag::s(r.code, r.value)));
    }
    Some((kind.to_string(), t))
}

/// The sections CADCraft's kept-data reader takes: the file's classes, table records and blocks
/// (by handle and name, with their extension dictionaries), the entities that have an extension
/// dictionary or reactors, and the objects made here (the named object dictionary first).
fn sections(doc: &CadDocument, version: &str) -> Vec<Section> {
    let legacy = matches!(version, "AC1012" | "AC1014" | "AC1015" | "AC1018");
    let sec = |name: &str, tags: Vec<Tag>| Section { name: name.to_string(), tags };
    let header = vec![Tag::s(9, "$ACADVER"), Tag::s(1, version)];
    let mut classes = Vec::new();
    for c in doc.classes.iter() {
        classes.extend([
            Tag::s(0, "CLASS"),
            Tag::s(1, c.dxf_name.clone()),
            Tag::s(2, c.cpp_class_name.clone()),
            Tag::s(3, c.application_name.clone()),
            Tag::i(90, i64::from(c.proxy_flags.0)),
            Tag::i(280, i64::from(c.was_zombie)),
            Tag::i(281, i64::from(c.is_an_entity)),
        ]);
    }
    let mut tables = Vec::new();
    let mut record = |kind: &str, h: AHandle, name: &str| {
        tables.push(Tag::s(0, kind));
        tables.push(Tag::s(5, hx(h)));
        ownership(&mut tables, &[], doc.extension_dictionary_handle(h));
        tables.push(Tag::s(2, name));
    };
    for x in doc.layers.iter() {
        record("LAYER", x.handle, &x.name);
    }
    for x in doc.line_types.iter() {
        record("LTYPE", x.handle, &x.name);
    }
    for x in doc.text_styles.iter() {
        record("STYLE", x.handle, &x.name);
    }
    for x in doc.dim_styles.iter() {
        record("DIMSTYLE", x.handle, &x.name);
    }
    for x in doc.app_ids.iter() {
        record("APPID", x.handle, &x.name);
    }
    for x in doc.views.iter() {
        record("VIEW", x.handle, &x.name);
    }
    for x in doc.ucss.iter() {
        record("UCS", x.handle, &x.name);
    }
    for x in doc.block_records.iter() {
        record("BLOCK_RECORD", x.handle, &x.name);
    }
    let mut blocks = Vec::new();
    for b in doc.block_records.iter() {
        blocks.extend([Tag::s(0, "BLOCK"), Tag::s(5, hx(b.block_entity_handle)), Tag::s(2, b.name.clone())]);
        blocks.extend([Tag::s(0, "ENDBLK"), Tag::s(5, hx(b.block_end_handle))]);
    }
    let mut entities = Vec::new();
    for e in doc.entities() {
        let c = e.common();
        if c.reactors.is_empty() && c.xdictionary_handle.is_none() {
            continue;
        }
        // The record's type matters only to which extended data CADCraft reads itself (a
        // dimension's `ACAD` data is its overrides); the entity's own data is the model's.
        let kind = if matches!(e, cadcraft_dwg::acadrust::EntityType::Dimension(_)) { "DIMENSION" } else { "ENTITY" };
        entities.push(Tag::s(0, kind));
        entities.push(Tag::s(5, hx(c.handle)));
        ownership(&mut entities, &c.reactors, c.xdictionary_handle);
        entities.push(Tag::s(330, hx(c.owner_handle)));
        entities.push(Tag::s(100, "AcDbEntity"));
        entities.push(Tag::s(8, if c.layer.is_empty() { "0".into() } else { c.layer.clone() }));
    }
    let mut objects = Vec::new();
    let root = doc.header.named_objects_dict_handle;
    let mut handles: Vec<AHandle> = doc.objects.keys().copied().filter(|h| *h != root).collect();
    handles.sort_by_key(|h| h.value());
    for h in std::iter::once(root).chain(handles) {
        let Some(o) = doc.objects.get(&h) else { continue };
        if let Some((kind, t)) = object(doc, h, o, legacy) {
            objects.push(Tag::s(0, kind));
            objects.extend(t);
        }
    }
    vec![sec("HEADER", header), sec("CLASSES", classes), sec("TABLES", tables), sec("BLOCKS", blocks), sec("ENTITIES", entities), sec("OBJECTS", objects)]
}

/// Give a drawing read from `doc` what CADCraft keeps of the file without modelling it (see the
/// module documentation).
pub(crate) fn read(doc: &CadDocument, d: &mut Drawing) {
    let version = d.foreign.version.clone().unwrap_or_else(|| "AC1032".into());
    let class_order = d.foreign.class_order.clone();
    let secs = sections(doc, &version);
    crate::dxf_raw::read(&secs, d);
    let f = std::sync::Arc::make_mut(&mut d.foreign);
    if f.version.is_none() {
        f.version = Some(version);
    }
    if f.class_order.is_empty() {
        f.class_order = class_order;
    }
}
