//! A drawing written into an acadrust document and out as DWG. A drawing read from a DWG is
//! written into that file's document, read again from its bytes; every other drawing into an empty
//! one. See the crate documentation for how objects are matched.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use acadrust::entities::EntityType;
use acadrust::tables::{self as at, BlockRecord};
use acadrust::types::Handle as AHandle;
use acadrust::{CadDocument, DxfVersion};
use cadcraft_doc::{Drawing, EntityStore, HVal};

use crate::conv::{a3, ah, up};
use crate::entity_read::{self, Names};
use crate::entity_write::{Cx, build};
use crate::tables::{self as tb, Lookups};
use crate::{Limits, Unwritten};

pub fn write(d: &Drawing, v: DxfVersion, ext: &dyn crate::Extension) -> Result<(Vec<u8>, Unwritten), String> {
    let d = ext.prepare(d, v);
    let d = d.as_ref();
    let (mut doc, mut unwritten) = match &d.foreign.dwg_source {
        Some(src) => (crate::read::decode(&src.bytes, Limits::DEFAULT)?.0, Unwritten::new()),
        None => ext.document(d, v)?.unwrap_or_default(),
    };
    doc.version = v;
    for (kind, n) in into_document(d, &mut doc) {
        *unwritten.entry(kind).or_default() += n;
    }
    ext.write(d, &mut doc);
    for (kind, n) in crate::objects::drop_unwritable(&mut doc) {
        *unwritten.entry(kind).or_default() += n;
    }
    let dwg = acadrust::DwgWriter::write_to_vec(&doc).map_err(|e| format!("DWG write: {e}"))?;
    Ok((dwg, unwritten))
}

/// Write the drawing's state into `doc`, returning what it couldn't write.
pub fn into_document(d: &Drawing, doc: &mut CadDocument) -> Unwritten {
    let mut unwritten = Unwritten::new();
    doc.header.handle_seed = doc.header.handle_seed.max(d.handseed);
    crate::header::write(&d.header, &mut doc.header);
    let names = Names::of(doc);
    // Blocks first: dimension styles name their arrowhead blocks by handle.
    blocks(d, doc);
    tables(d, doc, &names);
    current_records(d, doc);
    {
        let styles = tb::handles(doc.text_styles.iter().map(|s| (s.name.as_str(), s.handle)));
        let style = |n: &str| styles.get(&up(n)).copied();
        crate::styles::write(d, doc, &names, &style);
    }
    let object_names = |pick: fn(&acadrust::objects::ObjectType) -> Option<(&str, AHandle)>| -> HashMap<String, AHandle> {
        tb::handles(doc.objects.values().filter_map(pick))
    };
    let mline_styles = object_names(|o| match o {
        acadrust::objects::ObjectType::MLineStyle(x) => Some((x.name.as_str(), x.handle)),
        _ => None,
    });
    let table_styles = object_names(|o| match o {
        acadrust::objects::ObjectType::TableStyle(x) => Some((x.name.as_str(), x.handle)),
        _ => None,
    });
    let mleader_styles = object_names(|o| match o {
        acadrust::objects::ObjectType::MultiLeaderStyle(x) => Some((x.name.as_str(), x.handle)),
        _ => None,
    });

    // Name → handle lookups, and a handle allocator above everything there is.
    let styles = tb::handles(doc.text_styles.iter().map(|s| (s.name.as_str(), s.handle)));
    let blocks = tb::handles(doc.block_records.iter().map(|b| (b.name.as_str(), b.handle)));
    let ltypes = tb::handles(doc.line_types.iter().map(|l| (l.name.as_str(), l.handle)));
    let layers = tb::handles(doc.layers.iter().map(|l| (l.name.as_str(), l.handle)));
    let next = Cell::new(doc.header.handle_seed.max(doc.next_handle()).max(d.handseed));
    let names = Names::of(doc);
    {
        let style = |n: &str| styles.get(&up(n)).copied();
        let block = |n: &str| blocks.get(&up(n)).copied();
        let ltype = |n: &str| ltypes.get(&up(n)).copied();
        let mut alloc = || {
            let h = next.get();
            next.set(h + 1);
            AHandle::new(h)
        };
        let mut cx = Cx {
            lookups: Lookups { style: &style, block: &block, ltype: &ltype },
            layers: &layers,
            alloc: &mut alloc,
            drawing: d,
            mline_styles: &mline_styles,
            table_styles: &table_styles,
            mleader_styles: &mleader_styles,
        };
        // Block definitions (an xref's objects come from its file), model space, layouts.
        for b in d.blocks.values().filter(|b| b.xref_path.is_none()) {
            if let Some(br) = blocks.get(&up(&b.name)) {
                container(doc, *br, &b.entities, (&names, d.foreign.dwg_source.as_ref()), &mut cx, &mut unwritten);
            }
        }
        let ms = doc.block_records.iter().find(|b| b.is_model_space()).map(|b| b.handle);
        if let Some(ms) = ms {
            container(doc, ms, &d.model, (&names, d.foreign.dwg_source.as_ref()), &mut cx, &mut unwritten);
        }
        for l in &d.layouts {
            let Some(br) = layout_record(doc, &l.name) else { continue };
            container(doc, br, &l.entities, (&names, d.foreign.dwg_source.as_ref()), &mut cx, &mut unwritten);
        }
    }
    doc.header.handle_seed = doc.header.handle_seed.max(next.get());
    crate::xclip::write(d, doc);
    image_definitions(d, doc);
    register_apps(doc);
    crate::objects::write_layouts(d, doc);
    crate::objects::write_groups(d, doc);
    crate::objects::write_views(d, doc);
    crate::objects::write_vport_configs(d, doc);
    crate::objects::write_active_vport(d, doc);
    crate::objects::variable_dictionary(doc);
    for l in &d.layouts {
        for o in doc.objects.values_mut() {
            if let acadrust::objects::ObjectType::Layout(x) = o
                && x.name.eq_ignore_ascii_case(&l.name)
            {
                x.tab_order = i16::try_from(l.tab_order).unwrap_or(x.tab_order);
                crate::objects::put_page(x, &l.page, l.limits);
            }
        }
    }
    unwritten
}

/// Each image's definition (an IMAGEDEF named in `ACAD_IMAGE_DICT`, with its reactor) for its
/// file and name: the file's while it names that file, else one of that name, else a new one.
fn image_definitions(d: &Drawing, doc: &mut CadDocument) {
    use acadrust::objects::{ImageDefinition, ImageDefinitionReactor, ObjectType, ResolutionUnit};
    let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
    let images: Vec<(AHandle, cadcraft_doc::Image)> = stores
        .flat_map(|s| s.iter())
        .filter_map(|e| if let cadcraft_doc::EntityKind::Image(i) = &e.kind { Some((ah(e.handle), i.clone())) } else { None })
        .filter(|(_, i)| !i.path.trim().is_empty())
        .collect();
    if images.is_empty() {
        return;
    }
    let dict = crate::styles::named_dict(doc, "ACAD_IMAGE_DICT");
    let entries = |doc: &CadDocument| match doc.objects.get(&dict) {
        Some(ObjectType::Dictionary(dc)) => dc.entries.clone(),
        _ => Vec::new(),
    };
    for (h, i) in images {
        let Some(EntityType::RasterImage(img)) = doc.get_entity(h) else { continue };
        let name = if i.name.trim().is_empty() {
            std::path::Path::new(&i.path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Image".into())
        } else {
            i.name.clone()
        };
        let current = img.definition_handle;
        let named = entries(doc).into_iter().find(|(k, _)| k.eq_ignore_ascii_case(&name)).map(|(_, x)| x);
        let ok = |x: AHandle| matches!(doc.objects.get(&x), Some(ObjectType::ImageDefinition(def)) if def.file_name == i.path);
        if current.is_some_and(|c| Some(c) == named && ok(c)) {
            continue;
        }
        let def = match named.filter(|x| ok(*x)) {
            Some(x) => x,
            None => {
                let mut def = ImageDefinition::new(i.path.clone());
                def.handle = doc.allocate_handle();
                def.owner = dict;
                def.size_in_pixels = (i.size.x.max(0.0) as u32, i.size.y.max(0.0) as u32);
                def.pixel_size = (i.pixel_size.x, i.pixel_size.y);
                def.resolution_unit = match i.resolution_units {
                    2 => ResolutionUnit::Centimeters,
                    5 => ResolutionUnit::Inches,
                    _ => ResolutionUnit::None,
                };
                let x = def.handle;
                doc.objects.insert(x, ObjectType::ImageDefinition(def));
                if let Some(ObjectType::Dictionary(dc)) = doc.objects.get_mut(&dict) {
                    dc.entries.retain(|(k, _)| !k.eq_ignore_ascii_case(&name));
                    dc.add_entry(name.clone(), x);
                }
                x
            }
        };
        let reactor = doc.allocate_handle();
        let mut r = ImageDefinitionReactor::new(h);
        r.handle = reactor;
        doc.objects.insert(reactor, ObjectType::ImageDefinitionReactor(r));
        if let Some(EntityType::RasterImage(img)) = doc.get_entity_mut(h) {
            img.definition_handle = Some(def);
            img.definition_reactor_handle = Some(reactor);
            img.common.raw_record = None;
        }
    }
}

/// Every application whose extended data the document holds registered in its APPID table: a
/// DWG reader drops extended data of an application the table doesn't name.
fn register_apps(doc: &mut CadDocument) {
    let mut names: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let mut add = |n: &str| {
        if !n.trim().is_empty() && seen.insert(up(n)) {
            names.push(n.to_string());
        }
    };
    for e in doc.entities() {
        for r in e.common().extended_data.records() {
            add(&r.application_name);
        }
    }
    for x in doc.object_xdata.values() {
        for r in x.records() {
            add(&r.application_name);
        }
    }
    for n in names {
        if doc.app_ids.iter().any(|a| a.name.eq_ignore_ascii_case(&n)) {
            continue;
        }
        let mut a = at::AppId::new(n);
        a.handle = doc.allocate_handle();
        let _ = doc.app_ids.add(a);
    }
}

/// The block record of the layout `name`, made if the document has no such layout.
fn layout_record(doc: &mut CadDocument, name: &str) -> Option<AHandle> {
    let find = |doc: &CadDocument| {
        doc.objects.values().find_map(|o| match o {
            acadrust::objects::ObjectType::Layout(l) if l.name.eq_ignore_ascii_case(name) => Some(l.block_record),
            _ => None,
        })
    };
    if let Some(h) = find(doc) {
        return Some(h);
    }
    doc.add_layout(name).ok()?;
    find(doc)
}

/// Whether the reader leaves `e` out of its container (block delimiters, a layout's overall
/// viewport): such objects stay whatever the drawing holds.
fn kept_aside(e: &EntityType, names: &Names) -> bool {
    match entity_read::entity(e, names) {
        None => true,
        Some(x) => matches!(&x.kind, cadcraft_doc::EntityKind::Viewport(v) if v.id == 1),
    }
}

/// Make one block record's entities the drawing's `store`.
fn container(
    doc: &mut CadDocument,
    br: AHandle,
    store: &EntityStore,
    (names, kept): (&Names, Option<&cadcraft_doc::foreign::DwgSource>),
    cx: &mut Cx,
    unwritten: &mut Unwritten,
) {
    let Some(record) = doc.block_records.iter().find(|b| b.handle == br) else { return };
    let before: Vec<AHandle> = record.entity_handles.clone();
    let mine: HashSet<u64> = store.iter().map(|e| e.handle.0).collect();
    // Gone from the drawing.
    let mut order = Vec::new();
    for h in &before {
        if mine.contains(&h.value()) {
            continue;
        }
        match doc.get_entity(*h) {
            Some(e) if kept_aside(e, names) => order.push(*h),
            Some(_) => {
                doc.remove_entity(*h);
            }
            None => {}
        }
    }
    let had: HashSet<u64> = before.iter().map(|h| h.value()).collect();
    for e in store.iter() {
        let h = ah(e.handle);
        match doc.get_entity(h).filter(|_| had.contains(&h.value())) {
            Some(p) => {
                order.push(h);
                if entity_read::entity(p, names).as_ref() == Some(e.as_ref()) || kept.is_some_and(|k| k.keeps(e)) {
                    continue;
                }
                let p = p.clone();
                match build(e, Some(&p), cx) {
                    Some(new) => {
                        if let Some(slot) = doc.get_entity_mut(h) {
                            *slot = new;
                        }
                    }
                    None => *unwritten.entry(e.kind.dxf_name().trim().to_ascii_uppercase()).or_default() += 1,
                }
            }
            None => {
                if doc.get_entity(h).is_some() {
                    // An object of another container under this handle (a moved one): it is
                    // there no more.
                    doc.remove_entity(h);
                    for b in doc.block_records.iter_mut() {
                        b.entity_handles.retain(|x| *x != h);
                    }
                }
                match build(e, None, cx) {
                    Some(mut new) => {
                        new.common_mut().owner_handle = br;
                        if doc.add_entity(new).is_ok() {
                            order.push(h);
                        }
                    }
                    None => *unwritten.entry(e.kind.dxf_name().trim().to_ascii_uppercase()).or_default() += 1,
                }
            }
        }
    }
    // The drawing's order (draw order), what the reader leaves out first; left as the file had
    // it when the drawing draws in the file's order.
    let mut read_order: Vec<AHandle> = before.iter().copied().filter(|h| doc.get_entity(*h).is_some()).collect();
    if let Some(t) = sort_table_of(doc, br) {
        read_order.sort_by_key(|h| {
            let k = t.effective_sort_handle(*h).value();
            (k == 0, k)
        });
    }
    let aside: HashSet<u64> = order.iter().filter(|h| !mine.contains(&h.value())).map(|h| h.value()).collect();
    let unchanged = read_order.iter().filter(|h| !aside.contains(&h.value())).eq(order.iter().filter(|h| !aside.contains(&h.value())));
    if unchanged {
        return;
    }
    if let Some(record) = doc.block_records.iter_mut().find(|b| b.handle == br) {
        record.entity_handles.clone_from(&order);
    }
    sort_table(doc, br, &order);
}

fn sort_table_of(doc: &CadDocument, br: AHandle) -> Option<&acadrust::objects::SortEntitiesTable> {
    doc.objects.values().find_map(|o| match o {
        acadrust::objects::ObjectType::SortEntitiesTable(t) if t.block_owner_handle == br => Some(t),
        _ => None,
    })
}

/// A block record's draw order table (SORTENTSTABLE), when it has one, made to give `order`: the
/// sort handles it already uses go to the entities in that order (a table that already gives it
/// stays as read).
fn sort_table(doc: &mut CadDocument, br: AHandle, order: &[AHandle]) {
    use acadrust::objects::ObjectType;
    let Some(t) = doc.objects.values_mut().find_map(|o| match o {
        ObjectType::SortEntitiesTable(t) if t.block_owner_handle == br => Some(t),
        _ => None,
    }) else {
        return;
    };
    let key = |k: u64| (k == 0, k);
    let keys: Vec<u64> = order.iter().map(|h| t.effective_sort_handle(*h).value()).collect();
    if keys.windows(2).all(|w| key(w[0]) <= key(w[1])) {
        return;
    }
    let mut sorted = keys.clone();
    sorted.sort_by_key(|k| key(*k));
    sorted.dedup();
    if sorted.len() != order.len() {
        return;
    }
    t.clear();
    for (h, k) in order.iter().zip(sorted) {
        t.add_entry(*h, AHandle::new(k));
    }
    t.raw_dxf_codes = None;
}

/// Layers, linetypes, text styles and dimension styles: records matched by name, written when
/// they differ, added when new and removed when the drawing no longer has them.
fn tables(d: &Drawing, doc: &mut CadDocument, names: &Names) {
    for l in &d.layers {
        if let Some(x) = doc.layers.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&l.name)) {
                if tb::layer(x) != *l {
                    tb::put_layer(x, l);
                }
        } else {
                let mut x = at::Layer::new(l.name.clone());
                x.handle = doc.allocate_handle();
                tb::put_layer(&mut x, l);
                let _ = doc.layers.add(x);
        }
    }
    let keep: HashSet<String> = d.layers.iter().map(|l| up(&l.name)).collect();
    let gone: Vec<String> =
        doc.layers.iter().filter(|l| !keep.contains(&up(&l.name)) && l.name != "0" && !l.name.contains('|')).map(|l| l.name.clone()).collect();
    for n in gone {
        doc.layers.remove(&n);
    }

    // Text styles (shape files, which have no name, stay).
    let read: HashMap<String, cadcraft_doc::TextStyle> =
        doc.text_styles.iter().map(|s| (up(&s.name), tb::text_style(s, doc.object_xdata.get(&s.handle)))).collect();
    for s in &d.text_styles {
        if let Some(x) = doc.text_styles.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&s.name)) {
                if read.get(&up(&s.name)) != Some(s) {
                    tb::put_text_style(x, s);
                }
        } else {
                let mut x = at::TextStyle::new(s.name.clone());
                x.handle = doc.allocate_handle();
                tb::put_text_style(&mut x, s);
                let _ = doc.text_styles.add(x);
        }
    }
    let keep: HashSet<String> = d.text_styles.iter().map(|s| up(&s.name)).collect();
    let gone: Vec<String> = doc
        .text_styles
        .iter()
        .filter(|s| !s.name.trim().is_empty() && !keep.contains(&up(&s.name)) && !s.name.eq_ignore_ascii_case("Standard") && !s.name.contains('|'))
        .map(|s| s.name.clone())
        .collect();
    for n in gone {
        doc.text_styles.remove(&n);
    }
    let styles: HashMap<String, AHandle> = tb::handles(doc.text_styles.iter().map(|s| (s.name.as_str(), s.handle)));
    let shapes: HashMap<String, AHandle> = doc.text_styles.iter().filter(|s| s.name.trim().is_empty()).map(|s| (up(&s.font_file), s.handle)).collect();
    let shape_files: HashMap<u64, String> =
        doc.text_styles.iter().filter(|s| s.name.trim().is_empty()).map(|s| (s.handle.value(), s.font_file.clone())).collect();

    // Linetypes.
    let style_of = |n: &str| styles.get(&up(n)).or_else(|| shapes.get(&up(n))).copied();
    for l in &d.linetypes {
        if let Some(x) = doc.line_types.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&l.name)) {
                if tb::linetype(x, names, &shape_files) != *l {
                    tb::put_linetype(x, l, &style_of);
                }
        } else {
                let mut x = at::LineType::new(l.name.clone());
                x.handle = doc.allocate_handle();
                tb::put_linetype(&mut x, l, &style_of);
                let _ = doc.line_types.add(x);
        }
    }
    let keep: HashSet<String> = d.linetypes.iter().map(|l| up(&l.name)).collect();
    let fixed = ["BYBLOCK", "BYLAYER", "CONTINUOUS"];
    let gone: Vec<String> = doc
        .line_types
        .iter()
        .filter(|l| !keep.contains(&up(&l.name)) && !fixed.contains(&up(&l.name).as_str()) && !l.name.contains('|'))
        .map(|l| l.name.clone())
        .collect();
    for n in gone {
        doc.line_types.remove(&n);
    }

    // Dimension styles.
    let blocks = tb::handles(doc.block_records.iter().map(|b| (b.name.as_str(), b.handle)));
    let ltypes = tb::handles(doc.line_types.iter().map(|l| (l.name.as_str(), l.handle)));
    let style = |n: &str| styles.get(&up(n)).copied();
    let block = |n: &str| blocks.get(&up(n)).copied();
    let ltype = |n: &str| ltypes.get(&up(n)).copied();
    let lookups = Lookups { style: &style, block: &block, ltype: &ltype };
    for s in &d.dim_styles {
        if let Some(x) = doc.dim_styles.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&s.name)) {
                if tb::dim_style(x, names) != *s {
                    tb::put_dim_style(x, s, &lookups);
                }
        } else {
                let mut x = at::DimStyle::new(s.name.clone());
                x.handle = doc.allocate_handle();
                tb::put_dim_style(&mut x, s, &lookups);
                let _ = doc.dim_styles.add(x);
        }
    }
    let keep: HashSet<String> = d.dim_styles.iter().map(|s| up(&s.name)).collect();
    let gone: Vec<String> = doc
        .dim_styles
        .iter()
        .filter(|s| !keep.contains(&up(&s.name)) && !s.name.eq_ignore_ascii_case("Standard") && !s.name.contains('|'))
        .map(|s| s.name.clone())
        .collect();
    for n in gone {
        doc.dim_styles.remove(&n);
    }
}

/// Block records: new blocks made (with their BLOCK and ENDBLK markers), blocks the drawing no
/// longer has removed with their objects, and the rest given the drawing's base point.
fn blocks(d: &Drawing, doc: &mut CadDocument) {
    let layouts: HashSet<u64> = doc
        .objects
        .values()
        .filter_map(|o| if let acadrust::objects::ObjectType::Layout(l) = o { Some(l.block_record.value()) } else { None })
        .collect();
    let is_space = |b: &BlockRecord| b.is_model_space() || b.is_paper_space() || layouts.contains(&b.handle.value());
    let keep: HashSet<String> = d.blocks.keys().map(|n| up(n)).collect();
    let gone: Vec<(String, Vec<AHandle>)> = doc
        .block_records
        .iter()
        .filter(|b| !is_space(b) && !b.name.is_empty() && !keep.contains(&up(&b.name)))
        .map(|b| (b.name.clone(), b.entity_handles.clone()))
        .collect();
    for (name, hs) in gone {
        for h in hs {
            doc.remove_entity(h);
        }
        doc.block_records.remove(&name);
    }
    for b in d.blocks.values() {
        let base = a3(b.base);
        let existing = doc.block_records.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&b.name)).map(|x| {
            let moved = x.base_point != base;
            x.base_point = base;
            (moved, x.block_entity_handle)
        });
        if let Some((moved, marker)) = existing {
            if moved && let Some(EntityType::Block(m)) = doc.get_entity_mut(marker) {
                m.base_point = base;
            }
            continue;
        }
        let mut record = BlockRecord::new(&b.name);
        record.handle = doc.allocate_handle();
        record.block_entity_handle = doc.allocate_handle();
        record.block_end_handle = doc.allocate_handle();
        record.flags.anonymous = b.anonymous && b.name.starts_with('*');
        record.base_point = base;
        record.description.clone_from(&b.description);
        record.explodable = b.explodable;
        record.units = i16::from(b.units);
        if let Some(path) = &b.xref_path {
            record.flags.is_xref = true;
            record.flags.is_xref_overlay = b.xref_overlay;
            record.xref_path.clone_from(path);
        }
        let (rh, mh, eh) = (record.handle, record.block_entity_handle, record.block_end_handle);
        if doc.block_records.add(record).is_err() {
            continue;
        }
        let mut marker = acadrust::entities::Block::new(&b.name, base);
        marker.common.handle = mh;
        marker.common.owner_handle = rh;
        if let Some(path) = &b.xref_path {
            marker.xref_path.clone_from(path);
        }
        let _ = doc.add_entity(EntityType::Block(marker));
        let mut end = acadrust::entities::BlockEnd::new();
        end.common.handle = eh;
        end.common.owner_handle = rh;
        let _ = doc.add_entity(EntityType::BlockEnd(end));
    }
}

/// The current layer, text style, dimension style and linetype by the names the header gives:
/// the file stores their handles.
fn current_records(d: &Drawing, doc: &mut CadDocument) {
    let s = |name: &str| d.header.get(name).and_then(HVal::as_str).map(str::to_string);
    if let Some(n) = s("CLAYER")
        && let Some(l) = doc.layers.iter().find(|l| l.name.eq_ignore_ascii_case(&n))
    {
        (doc.header.current_layer_handle, doc.header.current_layer_name) = (l.handle, l.name.clone());
    }
    if let Some(n) = s("TEXTSTYLE")
        && let Some(l) = doc.text_styles.iter().find(|l| l.name.eq_ignore_ascii_case(&n))
    {
        (doc.header.current_text_style_handle, doc.header.current_text_style_name) = (l.handle, l.name.clone());
    }
    if let Some(n) = s("DIMSTYLE")
        && let Some(l) = doc.dim_styles.iter().find(|l| l.name.eq_ignore_ascii_case(&n))
    {
        (doc.header.current_dimstyle_handle, doc.header.current_dimstyle_name) = (l.handle, l.name.clone());
    }
    if let Some(n) = s("CELTYPE")
        && let Some(l) = doc.line_types.iter().find(|l| l.name.eq_ignore_ascii_case(&n))
    {
        (doc.header.current_linetype_handle, doc.header.current_linetype_name) = (l.handle, l.name.clone());
    }
}
