//! CADCraft's own data in DWG files, both ways ([`Own`], the `cadcraft-dwg` extension): what
//! CADCraft keeps in extended data and dictionary records, as its DXF reader and writer keep it.
//! And, for a DXF save of a drawing read from a DWG, the DXF groups of what CADCraft keeps of the
//! file without modelling it ([`for_dxf`]).

use std::borrow::Cow;
use std::sync::Arc;

use cadcraft_doc::{DimKind, Drawing, EntityKind, HVal, RawTag};
use cadcraft_dwg::acadrust::CadDocument;
use cadcraft_dwg::acadrust::objects::{ObjectType, XRecord, XRecordEntry};
use cadcraft_dwg::acadrust::xdata::ExtendedData;
use craft_dxf::Tag;

use crate::{IoError, Result, dxf_ext, dxf_read};

/// CADCraft's extension of the DWG translation.
pub(crate) struct Own;

impl cadcraft_dwg::Extension for Own {
    fn read(&self, doc: &CadDocument, d: &mut Drawing) {
        read_records(doc, d);
        read_payloads(doc, d);
    }

    fn prepare<'a>(&self, d: &'a Drawing, version: cadcraft_dwg::acadrust::DxfVersion) -> Cow<'a, Drawing> {
        prepare(d, version < cadcraft_dwg::acadrust::DxfVersion::AC1021)
    }

    fn document(&self, d: &Drawing, version: cadcraft_dwg::acadrust::DxfVersion) -> std::result::Result<Option<(CadDocument, crate::Unwritten)>, String> {
        foreign_document(d, version)
    }

    fn write(&self, d: &Drawing, doc: &mut CadDocument) {
        write_records(d, doc);
    }
}

/// Extended data groups as the DXF reader's tags.
fn tags_of(x: &ExtendedData) -> Vec<Tag> {
    cadcraft_dwg::xdata_groups(x).into_iter().map(|t| Tag::s(t.code, t.value)).collect()
}

/// CADCraft's extended data (`1001 CADCRAFT` and its values) of `x`, empty without any.
fn own_tags(x: &ExtendedData) -> Vec<Tag> {
    let Some(r) = x.records().iter().find(|r| r.application_name.eq_ignore_ascii_case(dxf_ext::APP)) else { return Vec::new() };
    let mut one = ExtendedData::new();
    one.add_record(r.clone());
    tags_of(&one)
}

/// The text of CADCraft's XRECORD `key` in the named object dictionary (its strings, joined).
fn record_text(doc: &CadDocument, key: &str) -> Option<String> {
    let ObjectType::Dictionary(root) = doc.objects.get(&doc.header.named_objects_dict_handle)? else { return None };
    let h = root.entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, h)| *h)?;
    let ObjectType::XRecord(x) = doc.objects.get(&h)? else { return None };
    let mut text = String::new();
    for e in x.entries.iter().filter(|e| e.code == 1 || e.code == 3) {
        if let Some(s) = e.value.as_string() {
            if text.len() + s.len() > dxf_ext::MAX_PAYLOAD {
                break;
            }
            text.push_str(s);
        }
    }
    Some(text)
}

/// CADCraft's records of the named object dictionary, as its DXF reader takes them.
fn read_records(doc: &CadDocument, d: &mut Drawing) {
    if let Some(style) = record_text(doc, dxf_ext::MLEADER_STYLE_KEY).filter(|s| !s.trim().is_empty()) {
        d.header.set_str("CMLEADERSTYLE", &style);
    }
    if let Some((c, p)) = record_text(doc, dxf_ext::CONSTRAINTS_KEY).and_then(|t| dxf_ext::parse_constraints(&t)) {
        d.constraints = c;
        d.parametric = p;
    }
    if let Some(p) = record_text(doc, dxf_ext::ANNOTATION_KEY).and_then(|t| dxf_ext::parse_anno(&t)) {
        dxf_ext::apply_anno(d, p);
    }
    if let Some(states) = record_text(doc, dxf_ext::LAYER_STATES_KEY).and_then(|t| dxf_ext::parse_layer_states(&t)) {
        d.layer_states = states;
    }
    if let Some(tables) = record_text(doc, dxf_ext::PLOT_STYLES_KEY).and_then(|t| dxf_ext::parse_plot_styles(&t)) {
        d.plot_style_tables = tables;
    }
    // Layouts' saved paper-space views.
    for o in doc.objects.values() {
        let ObjectType::Layout(l) = o else { continue };
        let Some(x) = doc.object_xdata(l.handle) else { continue };
        let tags = own_tags(x);
        let list = dxf_ext::xdata_list(dxf_ext::xdata(&tags, dxf_ext::APP), "PSVIEW");
        let g = |c: i32| list.iter().find(|t| t.code == c).map(Tag::f64).filter(|v| v.is_finite());
        if let (Some(x), Some(y), Some(h)) = (g(1010), g(1020), g(1040))
            && let Some(lay) = d.layouts.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&l.name))
        {
            lay.view = Some((cadcraft_geom::Vec2::new(x, y), h));
        }
    }
    // Table styles' complete cell styles, block arrays and annotative blocks.
    for o in doc.objects.values() {
        let ObjectType::TableStyle(ts) = o else { continue };
        let Some(x) = doc.object_xdata(ts.handle) else { continue };
        let tags = own_tags(x);
        if tags.is_empty() {
            continue;
        }
        if let Some(st) = d.table_styles.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&ts.name)) {
            *st = crate::dxf_table::apply_cell_styles_xdata(st.clone(), &tags);
        }
    }
    for br in doc.block_records.iter() {
        let Some(x) = doc.object_xdata(br.handle) else { continue };
        let tags = tags_of(x);
        let array = dxf_ext::read_array(&tags);
        let annotative = x.records().iter().any(|r| r.application_name.eq_ignore_ascii_case("AcadAnnotative"));
        if array.is_none() && !annotative {
            continue;
        }
        if let Some(b) = d.blocks.get_mut(&br.name) {
            let b = Arc::make_mut(b);
            if array.is_some() {
                b.array = array;
            }
            b.annotative |= annotative;
        }
    }
}

/// What CADCraft keeps in its own extended data of entities: dimensions' associativity, arc
/// lengths and marks, viewports' frozen layers and layer colours, tables' flags and cell data,
/// and regions (closed polylines that carry their region).
fn read_payloads(doc: &CadDocument, d: &mut Drawing) {
    for e in doc.entities() {
        let x = &e.common().extended_data;
        if x.get_record(dxf_ext::APP).is_none() && !x.records().iter().any(|r| r.application_name.eq_ignore_ascii_case(dxf_ext::APP)) {
            continue;
        }
        let tags = own_tags(x);
        if tags.is_empty() {
            continue;
        }
        let h = cadcraft_doc::Handle(e.common().handle.value());
        let Some(cur) = d.entity(h).map(|x| x.kind.clone()) else { continue };
        match cur {
            EntityKind::LwPolyline(pl) => match crate::dxf_region::from_polyline(&tags, pl) {
                Some(EntityKind::Region(r)) => {
                    let _ = d.modify_entity(h, |e| e.kind = EntityKind::Region(r));
                }
                Some(_) => {}
                None => {
                    d.remove_entity(h);
                }
            },
            _ => {
                let _ = d.modify_entity(h, |e| apply_payload(&mut e.kind, &tags));
            }
        }
    }
}

fn apply_payload(k: &mut EntityKind, tags: &[Tag]) {
    match k {
        EntityKind::Dimension(dm) => {
            dm.assoc = dxf_ext::read_assoc(tags);
            if let Some(center) = dxf_ext::read_arclen(tags) {
                dm.kind = DimKind::ArcLength;
                dm.p15 = center;
            }
            let oblique = dm.marks.oblique;
            dm.marks = dxf_ext::read_marks(tags);
            if dm.marks.oblique.is_none() {
                dm.marks.oblique = oblique;
            }
        }
        EntityKind::Viewport(v) => {
            let mut seen = std::collections::HashSet::new();
            let named = dxf_ext::read_frozen(tags);
            v.frozen_layers = v.frozen_layers.iter().chain(&named).filter(|n| seen.insert(n.to_ascii_uppercase())).cloned().collect();
            let colors = dxf_ext::read_layer_colors(tags);
            if !colors.is_empty() {
                v.layer_colors = colors;
            }
        }
        EntityKind::Table(t) => {
            let x = dxf_ext::xdata(tags, dxf_ext::APP);
            if let Some(o) = x.iter().position(|t| t.code == 1000 && t.str() == "TABLE").map(|p| x.get(p + 1..).unwrap_or(&[])) {
                let flags: Vec<i64> = o.iter().filter(|t| t.code == 1070).map(Tag::i64).collect();
                t.title = flags.first().is_some_and(|v| *v != 0);
                t.header = flags.get(1).is_some_and(|v| *v != 0);
                t.text_height = o.iter().find(|t| t.code == 1040).map(Tag::f64).filter(|h| h.is_finite() && *h > 0.0).unwrap_or(0.18);
                t.style = o.iter().find(|t| t.code == 1000).map(Tag::str).unwrap_or_else(|| "Standard".into());
            }
            crate::dxf_table::read_table_xdata(tags, t);
        }
        _ => {}
    }
}

/// Kinds whose CADCraft extended data the DXF writer writes.
fn has_payload(k: &EntityKind) -> bool {
    matches!(k, EntityKind::Dimension(_) | EntityKind::Viewport(_) | EntityKind::Table(_) | EntityKind::Hatch(_))
}

/// The groups of `list` without the `1000 <name> 1002 { … 1002 }` list `name`.
fn without_list(x: &[Tag], name: &str) -> Vec<Tag> {
    let Some(at) = x.iter().position(|t| t.code == 1000 && t.str().trim() == name) else { return x.to_vec() };
    let mut out = x.get(..at).unwrap_or(&[]).to_vec();
    let mut depth = 0i32;
    let mut i = at + 1;
    while let Some(t) = x.get(i) {
        i += 1;
        if t.code == 1002 {
            depth += if t.str().trim() == "{" { 1 } else { -1 };
            if depth <= 0 {
                break;
            }
        }
    }
    out.extend(x.get(i..).unwrap_or(&[]).iter().cloned());
    out
}

/// The drawing to write as DWG: regions as the closed polylines CADCraft writes them as, and each
/// entity's CADCraft extended data (as its DXF writer writes it) among its extended data.
/// With `legacy` (a DWG before 2007, which has no multileaders), multileaders as leaders and their
/// text or block, as the DXF writer writes them for those versions.
fn prepare(d: &Drawing, legacy: bool) -> Cow<'_, Drawing> {
    let committed = d.committed();
    let base: &Drawing = &committed;
    let mut changes: Vec<(cadcraft_doc::Handle, Vec<RawTag>)> = Vec::new();
    let mut regions = false;
    let mut mleaders = false;
    for e in all(base) {
        mleaders |= legacy && matches!(e.kind, EntityKind::MLeader(_));
        if matches!(e.kind, EntityKind::Region(_)) {
            regions = true;
            continue;
        }
        if !has_payload(&e.kind) {
            continue;
        }
        let (_, tags) = crate::dxf_write::entity_groups(base, e);
        let mut own = dxf_ext::xdata(&tags, dxf_ext::APP).to_vec();
        if matches!(e.kind, EntityKind::Dimension(_)) {
            // An arc length dimension is one in DWG itself.
            own = without_list(&own, "ARCLEN");
        }
        if own.is_empty() {
            continue;
        }
        let mut x = vec![RawTag { code: 1001, value: dxf_ext::APP.into() }];
        x.extend(own.iter().map(|t| RawTag { code: t.code, value: t.str() }));
        changes.push((e.handle, x));
    }
    if changes.is_empty() && !regions && !mleaders && matches!(committed, Cow::Borrowed(_)) {
        return Cow::Borrowed(d);
    }
    let mut out = committed.into_owned();
    for (h, x) in changes {
        let _ = out.modify_entity(h, |e| {
            strip_app(&mut e.common.xdata, dxf_ext::APP);
            e.common.xdata.extend(x);
        });
    }
    if regions {
        regions_as_polylines(&mut out);
    }
    if mleaders {
        mleaders_as_leaders(&mut out);
    }
    Cow::Owned(out)
}

/// Every multileader as leaders (one per arm, ending at the landing) and its text or block, the
/// first keeping the multileader's handle.
fn mleaders_as_leaders(d: &mut Drawing) {
    let style = d.header.str("DIMSTYLE", "Standard");
    let mut next = d.handseed.max(1);
    let mut alloc = || {
        let h = cadcraft_doc::Handle(next);
        next += 1;
        h
    };
    let mut one = |store: &mut cadcraft_doc::EntityStore| {
        let found: Vec<cadcraft_doc::Entity> = store.iter().filter(|e| matches!(e.kind, EntityKind::MLeader(_))).map(|e| (**e).clone()).collect();
        for e in found {
            let EntityKind::MLeader(m) = &e.kind else { continue };
            let mut parts: Vec<EntityKind> = m
                .leaders
                .iter()
                .take(64)
                .map(|arm| {
                    let mut vertices = arm.clone();
                    vertices.push(m.landing);
                    EntityKind::Leader(cadcraft_doc::Leader { vertices, arrow: true, spline: m.spline, style: style.clone() })
                })
                .collect();
            parts.extend(m.text.clone().map(EntityKind::MText));
            parts.extend(m.block.clone().map(EntityKind::Insert));
            let mut parts = parts.into_iter();
            match parts.next() {
                Some(first) => {
                    store.modify(e.handle, |x| x.kind = first);
                }
                None => {
                    store.remove(e.handle);
                }
            }
            for k in parts {
                store.push(cadcraft_doc::Entity { handle: alloc(), common: e.common.clone(), kind: k });
            }
        }
    };
    one(&mut d.model);
    for l in &mut d.layouts {
        one(&mut l.entities);
    }
    for b in d.blocks.values_mut() {
        if b.entities.iter().any(|e| matches!(e.kind, EntityKind::MLeader(_))) {
            one(&mut Arc::make_mut(b).entities);
        }
    }
    d.handseed = d.handseed.max(next);
}

/// `xdata` without application `app`'s groups.
fn strip_app(xdata: &mut Vec<RawTag>, app: &str) {
    let mut keep = true;
    xdata.retain(|t| {
        if t.code == 1001 {
            keep = !t.value.trim().eq_ignore_ascii_case(app);
        }
        keep
    });
}

/// Every region of the drawing as CADCraft writes it: closed polylines, one per loop, the first
/// (with the region's handle) carrying the region in its extended data.
fn regions_as_polylines(d: &mut Drawing) {
    let mut next = d.handseed.max(1);
    let mut alloc = || {
        let h = cadcraft_doc::Handle(next);
        next += 1;
        h
    };
    region_store(&mut d.model, &mut alloc);
    for l in &mut d.layouts {
        region_store(&mut l.entities, &mut alloc);
    }
    for b in d.blocks.values_mut() {
        if b.entities.iter().any(|e| matches!(e.kind, EntityKind::Region(_))) {
            region_store(&mut Arc::make_mut(b).entities, &mut alloc);
        }
    }
    d.handseed = d.handseed.max(next);
}

fn region_store(store: &mut cadcraft_doc::EntityStore, alloc: &mut dyn FnMut() -> cadcraft_doc::Handle) {
    let regions: Vec<(cadcraft_doc::Handle, cadcraft_doc::Common, cadcraft_doc::Region)> =
        store.iter().filter_map(|e| if let EntityKind::Region(r) = &e.kind { Some((e.handle, e.common.clone(), r.clone())) } else { None }).collect();
    for (h, common, r) in regions {
        let parts = crate::dxf_region::polylines(&r);
        for (i, (l, tags)) in r.loops.iter().zip(parts).enumerate() {
            let pl = cadcraft_doc::LwPolyline { vertices: l.vertices.clone(), closed: true, const_width: 0.0, elevation: 0.0, plinegen: false };
            let mut c = common.clone();
            strip_app(&mut c.xdata, dxf_ext::APP);
            let at = tags.iter().position(|t| t.code == 1001).unwrap_or(tags.len());
            c.xdata.extend(tags.get(at..).unwrap_or(&[]).iter().map(|t| RawTag { code: t.code, value: t.str() }));
            if i == 0 {
                store.modify(h, |e| {
                    e.common = c;
                    e.kind = EntityKind::LwPolyline(pl);
                });
            } else {
                store.push(cadcraft_doc::Entity { handle: alloc(), common: c, kind: EntityKind::LwPolyline(pl) });
            }
        }
    }
}

/// CADCraft's records of the named object dictionary written as its DXF writer writes them.
fn write_records(d: &Drawing, doc: &mut CadDocument) {
    let mut records: Vec<(&str, Vec<String>)> = vec![(dxf_ext::HANDSEED_KEY, vec![format!("{:X}", d.handseed)])];
    if let Some(name) = d.header.get("CMLEADERSTYLE").and_then(HVal::as_str) {
        records.push((dxf_ext::MLEADER_STYLE_KEY, vec![name.to_string()]));
    }
    if let Some(chunks) = dxf_ext::constraint_chunks(&d.constraints, &d.parametric) {
        records.push((dxf_ext::CONSTRAINTS_KEY, chunks));
    }
    if let Some(chunks) = dxf_ext::layer_state_chunks(&d.layer_states) {
        records.push((dxf_ext::LAYER_STATES_KEY, chunks));
    }
    if let Some(chunks) = dxf_ext::plot_style_chunks(&d.plot_style_tables) {
        records.push((dxf_ext::PLOT_STYLES_KEY, chunks));
    }
    if let Some(chunks) = dxf_ext::anno_chunks(d) {
        records.push((dxf_ext::ANNOTATION_KEY, chunks));
    }
    // Layouts' saved paper-space views.
    let layouts: Vec<(cadcraft_dwg::acadrust::types::Handle, String)> =
        doc.objects.values().filter_map(|o| if let ObjectType::Layout(l) = o { Some((l.handle, l.name.clone())) } else { None }).collect();
    for (h, name) in layouts {
        let view = d.layouts.iter().find(|l| l.name.eq_ignore_ascii_case(&name)).and_then(|l| l.view).filter(|(c, v)| c.x.is_finite() && c.y.is_finite() && v.is_finite());
        let tags: Vec<RawTag> = match view {
            Some((c, height)) => [(1001, dxf_ext::APP.to_string()), (1000, "PSVIEW".into()), (1002, "{".into()), (1010, format!("{:?}", c.x)), (1020, format!("{:?}", c.y)), (1030, "0.0".into()), (1040, format!("{height:?}")), (1002, "}".into())]
                .into_iter()
                .map(|(code, value)| RawTag { code, value })
                .collect(),
            None => Vec::new(),
        };
        let have = doc.object_xdata(h).map(own_tags).unwrap_or_default();
        let want: Vec<Tag> = tags.iter().map(|t| Tag::s(t.code, t.value.clone())).collect();
        if have.iter().map(|t| (t.code, t.str())).eq(want.iter().map(|t| (t.code, t.str()))) {
            continue;
        }
        doc.remove_object_xdata_record(h, dxf_ext::APP);
        if let Some(r) = cadcraft_dwg::xdata_from_groups(&tags).records().first() {
            doc.set_object_xdata_record(h, r.clone());
        }
    }
    let root = doc.header.named_objects_dict_handle;
    let keys = [
        dxf_ext::HANDSEED_KEY,
        dxf_ext::MLEADER_STYLE_KEY,
        dxf_ext::CONSTRAINTS_KEY,
        dxf_ext::LAYER_STATES_KEY,
        dxf_ext::PLOT_STYLES_KEY,
        dxf_ext::ANNOTATION_KEY,
    ];
    for key in keys {
        let want = records.iter().find(|(k, _)| *k == key).map(|(_, c)| c);
        let existing = match doc.objects.get(&root) {
            Some(ObjectType::Dictionary(r)) => r.entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, h)| *h),
            _ => None,
        };
        let same = existing.is_some_and(|h| match (doc.objects.get(&h), want) {
            (Some(ObjectType::XRecord(x)), Some(chunks)) => {
                x.entries.len() == chunks.len() && x.entries.iter().zip(chunks).all(|(e, c)| e.code == 1 && e.value.as_string() == Some(c.as_str()))
            }
            _ => false,
        });
        if same {
            continue;
        }
        if let Some(h) = existing {
            doc.objects.remove(&h);
            if let Some(ObjectType::Dictionary(r)) = doc.objects.get_mut(&root) {
                r.entries.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
            }
        }
        let Some(chunks) = want else { continue };
        let mut x = XRecord::new();
        x.handle = doc.allocate_handle();
        x.owner = root;
        x.reactors = vec![root];
        x.cloning_flags = cadcraft_dwg::acadrust::objects::DictionaryCloningFlags::KeepExisting;
        for c in chunks {
            x.add_entry(XRecordEntry::string(1, c.clone()));
        }
        let h = x.handle;
        doc.objects.insert(h, ObjectType::XRecord(x));
        if let Some(ObjectType::Dictionary(r)) = doc.objects.get_mut(&root) {
            r.add_entry(key, h);
        }
    }
}

fn all(d: &Drawing) -> impl Iterator<Item = &cadcraft_doc::Entity> {
    std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities)).flat_map(|s| s.iter().map(|e| e.as_ref()))
}

/// Whether a drawing holds other programs' data CADCraft keeps as DXF groups: objects, extension
/// dictionaries, reactors and classes it doesn't model, and entities of kinds it doesn't model.
fn has_foreign_groups(d: &Drawing) -> bool {
    let f = &d.foreign;
    let unknown = |s: &cadcraft_doc::EntityStore| s.iter().any(|e| matches!(&e.kind, EntityKind::Unknown(u) if !u.tags.is_empty()));
    !(f.objects.is_empty() && f.dictionary.is_empty() && f.entities.is_empty() && f.records.is_empty() && f.classes.is_empty() && f.data_store.is_empty())
        || unknown(&d.model)
        || d.layouts.iter().any(|l| unknown(&l.entities))
        || d.blocks.values().any(|b| unknown(&b.entities))
}

/// The document a drawing with no DWG source is written into. A drawing read from a DXF file can
/// hold other programs' objects and entities that CADCraft keeps only as their DXF groups; the
/// one translator of those groups into acadrust's objects is acadrust's DXF reader, which takes a
/// DXF stream only. So such a drawing (and no other) starts from acadrust's read of its DXF,
/// with the groups declared as the version they were read in; everything CADCraft models is then
/// written into it directly. Every other drawing starts from an empty document.
fn foreign_document(d: &Drawing, version: cadcraft_dwg::acadrust::DxfVersion) -> std::result::Result<Option<(CadDocument, crate::Unwritten)>, String> {
    if !has_foreign_groups(d) {
        return Ok(None);
    }
    let from = d.foreign.version.as_deref().and_then(crate::Version::for_acadver).unwrap_or(crate::Version::R2000);
    let dwg = crate::Version::for_acadver(&format!("{version:?}")).unwrap_or(crate::Version::R2018);
    let (dxf, classes) = crate::dxf_write::foreign_bridge(d, from, dwg);
    let mut doc = std::panic::catch_unwind(move || cadcraft_dwg::acadrust::DxfReader::from_reader(std::io::Cursor::new(dxf.into_bytes())).and_then(|r| r.read()))
        .map_err(|_| "the DWG writer failed on this drawing".to_string())?
        .map_err(|e| format!("DWG: {e}"))?;
    let mut unwritten = proxy_classes(&mut doc, &classes, version);
    // A kept proxy object the DXF itself left out: its class can't be named either.
    let legacy = version < cadcraft_dwg::acadrust::DxfVersion::AC1027;
    for o in d.foreign.objects.iter().filter(|o| o.kind.trim().eq_ignore_ascii_case("ACAD_PROXY_OBJECT")) {
        if !legacy && d.foreign.proxy_class(&o.tags).is_none() {
            *unwritten.entry("ACAD_PROXY_OBJECT".into()).or_default() += 1;
        }
    }
    Ok(Some((doc, unwritten)))
}

/// Proxies name their class by number, 500 plus its place in the CLASSES of the file; acadrust
/// lists the classes it writes in its own order, so each proxy read from the DXF (whose classes
/// are `dxf_classes`) is given the number of its class in the document. One whose class the
/// document can't name, and every proxy in a DWG before 2013 (acadrust writes no class for one
/// there), is left out: kept with its number it would name another class. Left-out proxy objects
/// are counted (entities are counted where the drawing's entities are written).
fn proxy_classes(doc: &mut CadDocument, dxf_classes: &[String], version: cadcraft_dwg::acadrust::DxfVersion) -> crate::Unwritten {
    use cadcraft_dwg::acadrust::entities::{EntityType, ExtendedEntityData};
    let legacy = version < cadcraft_dwg::acadrust::DxfVersion::AC1027;
    let numbers: std::collections::HashMap<String, i32> =
        doc.classes.iter().enumerate().map(|(i, c)| (c.dxf_name.to_ascii_uppercase(), 500 + i32::try_from(i).unwrap_or(i32::MAX - 500))).collect();
    let renumber = |id: i32| -> Option<i32> {
        if legacy {
            return None;
        }
        let name = usize::try_from(id - 500).ok().and_then(|i| dxf_classes.get(i))?;
        numbers.get(name).copied()
    };
    let mut gone_entities = Vec::new();
    for e in doc.entities_mut() {
        if let EntityType::Extended(x) = e
            && let ExtendedEntityData::Proxy(p) = &mut x.data
        {
            match renumber(p.class_id) {
                Some(n) => p.class_id = n,
                None => gone_entities.push(x.common.handle),
            }
        }
    }
    for h in gone_entities {
        doc.remove_entity(h);
        for b in doc.block_records.iter_mut() {
            b.entity_handles.retain(|x| *x != h);
        }
    }
    let mut unwritten = crate::Unwritten::new();
    let mut gone = std::collections::HashSet::new();
    for (h, o) in doc.objects.iter_mut() {
        if let ObjectType::ProxyObject(p) = o {
            match renumber(p.class_id) {
                Some(n) => p.class_id = n,
                None => {
                    gone.insert(*h);
                }
            }
        }
    }
    for h in &gone {
        doc.objects.remove(h);
        *unwritten.entry("ACAD_PROXY_OBJECT".into()).or_default() += 1;
    }
    if !gone.is_empty() {
        for o in doc.objects.values_mut() {
            if let ObjectType::Dictionary(dc) = o {
                dc.entries.retain(|(_, v)| !gone.contains(v));
            }
        }
    }
    unwritten
}

/// A drawing read from a DWG, for a DXF save: with the DXF groups of the file's objects that
/// CADCraft keeps without modelling them (dictionaries, other programs' objects, dynamic block
/// data, extension dictionaries and reactors) and of its unmodelled entities, derived from the
/// file's DWG through acadrust's DXF writer (`cadcraft_dwg::kept_dxf`). `None` for a drawing not
/// read from a DWG, or one whose groups are already there.
pub(crate) fn for_dxf(d: &Drawing) -> Result<Option<Drawing>> {
    let Some(src) = &d.foreign.dwg_source else { return Ok(None) };
    if !d.foreign.objects.is_empty() {
        return Ok(None);
    }
    let dxf = cadcraft_dwg::kept_dxf(&src.bytes).map_err(IoError::Format)?;
    let k = dxf_read::read(&dxf).map_err(|e| IoError::Format(format!("{e} (in the objects of this DWG)")))?;
    let mut out = d.clone();
    let mut foreign = (*k.foreign).clone();
    foreign.dwg_source.clone_from(&d.foreign.dwg_source);
    foreign.version.clone_from(&d.foreign.version);
    out.foreign = Arc::new(foreign);
    // The unmodelled entities' groups, as the DXF reader keeps them.
    let take = |store: &mut cadcraft_doc::EntityStore, from: &cadcraft_doc::EntityStore| {
        let hs: Vec<cadcraft_doc::Handle> =
            store.iter().filter(|e| matches!(&e.kind, EntityKind::Unknown(u) if u.tags.is_empty())).map(|e| e.handle).collect();
        for h in hs {
            let Some(src) = from.get(h) else { continue };
            if let EntityKind::Unknown(u) = &src.kind {
                let u = u.clone();
                store.modify(h, |e| e.kind = EntityKind::Unknown(u));
            }
        }
    };
    take(&mut out.model, &k.model);
    for l in &mut out.layouts {
        if let Some(kl) = k.layouts.iter().find(|x| x.name.eq_ignore_ascii_case(&l.name)) {
            take(&mut l.entities, &kl.entities);
        }
    }
    let names: Vec<String> = out.blocks.keys().cloned().collect();
    for n in names {
        let Some(kb) = k.blocks.get(&n) else { continue };
        if let Some(b) = out.blocks.get_mut(&n) {
            take(&mut Arc::make_mut(b).entities, &kb.entities);
        }
    }
    Ok(Some(out))
}
