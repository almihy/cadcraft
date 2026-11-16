//! What a drawing read from a DWG changed of the file's objects it keeps without modelling them,
//! written back into the file's document on a DWG save: the dictionaries, records and dynamic
//! block representation data CADCraft adds or changes (a dynamic block reference given new
//! values, its history), the extension dictionaries of entities, and block records' extended
//! data. Each is made from its groups, as the drawing holds them, into acadrust's object; what
//! the drawing didn't change stays as the file had it.

use std::collections::HashMap;

use cadcraft_doc::foreign::{ForeignExtra, ForeignObject};
use cadcraft_doc::{Drawing, Handle, RawTag};
use cadcraft_dwg::acadrust::CadDocument;
use cadcraft_dwg::acadrust::objects::{
    BlockRepresentationData, Dictionary, DictionaryCloningFlags, DynamicBlockData, DynamicBlockObject, ObjectType, XRecord, XRecordEntry, XRecordValue,
    XRecordValueType,
};
use cadcraft_dwg::acadrust::types::Handle as AHandle;

/// The kinds of object written back from their groups.
const WRITTEN: [&str; 3] = ["DICTIONARY", "XRECORD", "ACDB_BLOCKREPRESENTATION_DATA"];

fn ah(h: Handle) -> AHandle {
    AHandle::new(h.0)
}

fn handle(v: &str) -> Option<AHandle> {
    Handle::parse_hex(v.trim()).map(ah)
}

fn written(kind: &str) -> bool {
    WRITTEN.iter().any(|k| kind.trim().eq_ignore_ascii_case(k))
}

/// An object's own groups: its reactors, extension dictionary and owner, then the rest (after
/// its first subclass marker).
struct Head<'a> {
    reactors: Vec<AHandle>,
    xdict: Option<AHandle>,
    owner: AHandle,
    rest: &'a [RawTag],
}

fn head(tags: &[RawTag]) -> Head<'_> {
    let mut h = Head { reactors: Vec::new(), xdict: None, owner: AHandle::NULL, rest: &[] };
    let mut group: Option<String> = None;
    let mut owner_seen = false;
    for (i, t) in tags.iter().enumerate() {
        match t.code {
            102 if t.value.trim().starts_with('{') => group = Some(t.value.trim().to_ascii_uppercase()),
            102 => group = None,
            330 if group.as_deref() == Some("{ACAD_REACTORS") => h.reactors.extend(handle(&t.value)),
            360 if group.as_deref() == Some("{ACAD_XDICTIONARY") => h.xdict = handle(&t.value),
            _ if group.is_some() => {}
            330 if !owner_seen => {
                owner_seen = true;
                h.owner = handle(&t.value).unwrap_or(AHandle::NULL);
            }
            100 => {
                h.rest = tags.get(i + 1..).unwrap_or(&[]);
                return h;
            }
            _ => {}
        }
    }
    h
}

/// A dictionary's entries (3, then 350 or 360), with whether each is hard-owned (360).
fn entries(tags: &[RawTag]) -> Vec<(String, AHandle, bool)> {
    let mut out = Vec::new();
    let mut key: Option<&str> = None;
    for t in tags {
        match t.code {
            3 => key = Some(t.value.as_str()),
            350 | 360 => {
                if let (Some(k), Some(h)) = (key.take(), handle(&t.value)) {
                    out.push((k.to_string(), h, t.code == 360));
                }
            }
            _ => {}
        }
    }
    out
}

fn int(tags: &[RawTag], code: i32) -> Option<i64> {
    tags.iter().find(|t| t.code == code).and_then(|t| t.value.trim().parse().ok())
}

/// Dictionary `o` (the drawing's), `old` its groups as read: the file's dictionary takes the
/// entries the drawing added, changed or took out; entries the drawing doesn't hold (CADCraft's
/// modelled ones) stay.
fn dictionary(doc: &CadDocument, o: &ForeignObject, old: Option<&ForeignObject>) -> Dictionary {
    let h = head(&o.tags);
    let mut d = match doc.objects.get(&ah(o.handle)) {
        Some(ObjectType::Dictionary(x)) => x.clone(),
        _ => {
            let mut x = Dictionary::new();
            x.handle = ah(o.handle);
            x
        }
    };
    d.owner = h.owner;
    d.reactors = h.reactors;
    d.xdictionary_handle = h.xdict;
    d.hard_owner = int(h.rest, 280).is_some_and(|v| v != 0);
    if let Some(c) = int(h.rest, 281) {
        d.duplicate_cloning = i16::try_from(c).unwrap_or(1);
    }
    let now = entries(h.rest);
    let before = old.map(|x| entries(head(&x.tags).rest)).unwrap_or_default();
    for (k, _, _) in &before {
        if !now.iter().any(|(n, _, _)| n.eq_ignore_ascii_case(k)) {
            d.entries.retain(|(x, _)| !x.eq_ignore_ascii_case(k));
            d.hard_owner_entries.retain(|x| !x.eq_ignore_ascii_case(k));
        }
    }
    for (k, v, hard) in now {
        match d.entries.iter_mut().find(|(x, _)| x.eq_ignore_ascii_case(&k)) {
            Some(e) => e.1 = v,
            None => d.entries.push((k.clone(), v)),
        }
        d.hard_owner_entries.retain(|x| !x.eq_ignore_ascii_case(&k));
        if hard && !d.hard_owner {
            d.hard_owner_entries.push(k);
        }
    }
    d
}

/// Whether group `code` is the first of a point (its Y and Z follow at +10 and +20).
fn point_start(code: i32) -> bool {
    matches!(code, 10..=18 | 110..=118 | 210..=218 | 1010..=1018)
}

/// The values of a record's groups, by the type each group code holds.
fn record_entries(tags: &[RawTag]) -> Vec<XRecordEntry> {
    let mut out = Vec::new();
    let mut i = 0;
    let real = |t: Option<&RawTag>| t.and_then(|t| t.value.trim().parse::<f64>().ok()).unwrap_or(0.0);
    while let Some(t) = tags.get(i) {
        i += 1;
        let v = t.value.as_str();
        let n = || v.trim().parse::<i64>().unwrap_or(0);
        let value = match XRecordValueType::from_code(t.code) {
            XRecordValueType::Point3D if point_start(t.code) => {
                let y = tags.get(i).filter(|y| y.code == t.code + 10);
                let z = y.and_then(|_| tags.get(i + 1)).filter(|z| z.code == t.code + 20);
                i += usize::from(y.is_some()) + usize::from(z.is_some());
                XRecordValue::Point3D(real(Some(t)), real(y), real(z))
            }
            XRecordValueType::Point3D | XRecordValueType::Double => XRecordValue::Double(real(Some(t))),
            XRecordValueType::Byte => XRecordValue::Byte(u8::try_from(n()).unwrap_or(0)),
            XRecordValueType::Int16 => XRecordValue::Int16(i16::try_from(n()).unwrap_or(0)),
            XRecordValueType::Int32 => XRecordValue::Int32(i32::try_from(n()).unwrap_or(0)),
            XRecordValueType::Int64 => XRecordValue::Int64(n()),
            XRecordValueType::Bool => XRecordValue::Bool(n() != 0),
            XRecordValueType::Handle | XRecordValueType::ObjectId => XRecordValue::Handle(handle(v).unwrap_or(AHandle::NULL)),
            XRecordValueType::Chunk => {
                let hex = v.trim();
                let bytes = (0..hex.len() / 2).filter_map(|k| hex.get(2 * k..2 * k + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect();
                XRecordValue::Chunk(bytes)
            }
            _ => XRecordValue::String(v.to_string()),
        };
        out.push(XRecordEntry::new(t.code, value));
    }
    out
}

fn record(doc: &CadDocument, o: &ForeignObject) -> XRecord {
    let h = head(&o.tags);
    let mut x = match doc.objects.get(&ah(o.handle)) {
        Some(ObjectType::XRecord(x)) => x.clone(),
        _ => {
            let mut x = XRecord::new();
            x.handle = ah(o.handle);
            x
        }
    };
    x.owner = h.owner;
    x.reactors = h.reactors;
    x.xdictionary_handle = h.xdict;
    // The cloning flag (280) comes first; the rest are the record's values.
    let (flag, rest) = match h.rest.first() {
        Some(t) if t.code == 280 => (t.value.trim().parse::<i16>().ok(), h.rest.get(1..).unwrap_or(&[])),
        _ => (None, h.rest),
    };
    if let Some(f) = flag {
        x.cloning_flags = DictionaryCloningFlags::from_value(f);
    }
    x.entries = record_entries(rest);
    x
}

fn representation(doc: &CadDocument, o: &ForeignObject) -> DynamicBlockObject {
    let h = head(&o.tags);
    let mut x = match doc.objects.get(&ah(o.handle)) {
        Some(ObjectType::DynamicBlock(x)) => x.clone(),
        _ => DynamicBlockObject {
            handle: ah(o.handle),
            dxf_name: "ACDB_BLOCKREPRESENTATION_DATA".into(),
            cpp_class_name: "AcDbBlockRepresentationData".into(),
            ..DynamicBlockObject::default()
        },
    };
    x.owner = h.owner;
    x.reactors = h.reactors;
    x.xdictionary_handle = h.xdict;
    let data = if h.rest.is_empty() { o.tags.as_slice() } else { h.rest };
    x.data = DynamicBlockData::Representation(BlockRepresentationData {
        flags: int(data, 70).and_then(|v| i16::try_from(v).ok()).unwrap_or(1),
        block: data.iter().find(|t| t.code == 340).and_then(|t| handle(&t.value)).unwrap_or(AHandle::NULL),
    });
    x
}

/// Write into `doc` what drawing `d` changed of the objects, extension dictionaries and block
/// records' extended data it kept from the DWG it was read from.
pub(crate) fn write(d: &Drawing, doc: &mut CadDocument) {
    let Some(src) = &d.foreign.dwg_source else { return };
    let (base, f) = (&src.as_read, &d.foreign);
    if f.objects == base.objects && f.entities == base.entities && f.records == base.records {
        return;
    }
    let before: HashMap<Handle, &ForeignObject> = base.objects.iter().map(|o| (o.handle, o)).collect();
    let now: HashMap<Handle, &ForeignObject> = f.objects.iter().map(|o| (o.handle, o)).collect();
    for o in &f.objects {
        let old = before.get(&o.handle).copied();
        if old == Some(o) || !written(&o.kind) {
            continue;
        }
        let made = match o.kind.trim().to_ascii_uppercase().as_str() {
            "DICTIONARY" => ObjectType::Dictionary(dictionary(doc, o, old)),
            "XRECORD" => ObjectType::XRecord(record(doc, o)),
            _ => ObjectType::DynamicBlock(representation(doc, o)),
        };
        doc.objects.insert(ah(o.handle), made);
    }
    // Objects the drawing took out (a reference's representation it no longer has).
    let gone: Vec<AHandle> = base.objects.iter().filter(|o| written(&o.kind) && !now.contains_key(&o.handle)).map(|o| ah(o.handle)).collect();
    if !gone.is_empty() {
        for h in &gone {
            doc.objects.remove(h);
        }
        for o in doc.objects.values_mut() {
            if let ObjectType::Dictionary(dc) = o {
                dc.entries.retain(|(_, v)| !gone.contains(v));
            }
        }
    }
    // Entities' extension dictionaries and reactors.
    let extras = |m: &std::collections::BTreeMap<Handle, ForeignExtra>, h: &Handle| m.get(h).map(|x| (x.xdict, x.reactors.clone()));
    let mut changed: Vec<Handle> = f.entities.keys().filter(|h| extras(&f.entities, h) != extras(&base.entities, h)).copied().collect();
    changed.extend(base.entities.keys().filter(|h| !f.entities.contains_key(h)));
    for h in changed {
        let (xdict, reactors) = extras(&f.entities, &h).unwrap_or_default();
        if let Some(e) = doc.get_entity_mut(ah(h)) {
            let c = e.common_mut();
            c.xdictionary_handle = xdict.map(ah);
            c.reactors = reactors.into_iter().map(ah).collect();
            c.raw_record = None;
        }
    }
    // Block records' extended data (a reference's anonymous block names its dynamic block).
    for ((kind, name), x) in &f.records {
        if !kind.eq_ignore_ascii_case("BLOCK_RECORD") || base.records.get(&(kind.clone(), name.clone())).map(|b| &b.xdata) == Some(&x.xdata) {
            continue;
        }
        let Some(br) = doc.block_records.iter().find(|b| b.name.eq_ignore_ascii_case(name)).map(|b| b.handle) else { continue };
        for r in cadcraft_dwg::xdata_from_groups(&x.xdata).records() {
            doc.remove_object_xdata_record(br, &r.application_name);
            doc.set_object_xdata_record(br, r.clone());
        }
    }
}
