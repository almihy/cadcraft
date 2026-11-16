//! A DWG read into acadrust's document, and that document as a [`Drawing`].

use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;
use std::sync::Arc;

use acadrust::CadDocument;
use acadrust::objects::ObjectType;
use cadcraft_doc::{Block, Drawing, EntityStore, Layout, PageSetup, XrefStatus};
use cadcraft_geom::Vec2;

use crate::Limits;
use crate::conv::{up, v3};
use crate::entity_read::{Names, entity};

/// Read `bytes` into acadrust's document in failsafe mode (an object record that can't be
/// decoded is skipped, not the file), with notes on what was skipped.
pub fn decode(bytes: &[u8], limits: Limits) -> Result<(CadDocument, Vec<String>), String> {
    let outcome = acadrust::DwgReader::from_stream_with_options(Cursor::new(bytes), acadrust::DwgReadOptions::failsafe())
        .read_with_stats()
        .map_err(|e| format!("DWG: {e}"))?;
    // Failsafe skips records, not whole files: when the objects stream itself couldn't be read
    // (a damaged header or section map, or not a DWG at all), nothing was read.
    if !outcome.stats.record_stream_read {
        let why = outcome.stats.diagnostics.first().map(|d| d.message.trim_start_matches("Failsafe: ").to_string());
        return Err(format!("DWG: {}", why.unwrap_or_else(|| "the file's objects couldn't be read".to_string())));
    }
    let records = outcome.stats.output_entities.saturating_add(outcome.stats.output_objects);
    if records > limits.max_records {
        return Err(format!("DWG: the file holds {records} objects, more than the {} CADCraft opens", limits.max_records));
    }
    Ok((outcome.document, read_notes(&outcome.stats)))
}

/// The drawing of a DWG, with notes on what the read left out.
pub fn document(bytes: &[u8], limits: Limits) -> Result<(Drawing, Vec<String>), String> {
    let (doc, notes) = decode(bytes, limits)?;
    let version = crate::version(bytes).unwrap_or_else(|| "AC1032".into());
    let mut d = drawing(&doc, &version);
    d.read_notes.clone_from(&notes);
    Ok((d, notes))
}

/// Most record types named in the skipped-records note.
const NOTE_TYPES: usize = 6;

/// The note on object records a failsafe read skipped, or nothing when the read was complete.
fn read_notes(stats: &acadrust::ReadStats) -> Vec<String> {
    let skipped = stats.skipped_source_records;
    if skipped == 0 {
        return Vec::new();
    }
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    for d in stats.diagnostics.iter().filter(|d| d.record_handle.is_some()) {
        *by_type.entry(d.record_type.clone().unwrap_or_else(|| "unknown".into())).or_default() += 1;
    }
    let mut types: Vec<(String, usize)> = by_type.into_iter().collect();
    types.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut list: Vec<String> = types.iter().take(NOTE_TYPES).map(|(t, n)| format!("type {t} × {n}")).collect();
    if types.len() > NOTE_TYPES {
        list.push(format!("{} more types", types.len() - NOTE_TYPES));
    }
    let of = if stats.source_records > 0 { format!(" of {}", stats.source_records) } else { String::new() };
    let (were, them) = if skipped == 1 { ("was", "it") } else { ("were", "them") };
    let which = if list.is_empty() { String::new() } else { format!(" ({})", list.join(", ")) };
    vec![format!("DWG: {skipped}{of} object records couldn't be read and {were} left out{which}; saving over the original file would lose {them}")]
}

/// The entities of the block record `name`, in its order.
fn store(doc: &CadDocument, handles: &[acadrust::types::Handle], names: &Names) -> EntityStore {
    let mut s = EntityStore::new();
    for hd in handles {
        if let Some(e) = doc.get_entity(*hd).and_then(|e| entity(e, names)) {
            s.push(e);
        }
    }
    s
}

/// acadrust's document as a drawing of a file of `version`.
pub fn drawing(doc: &CadDocument, version: &str) -> Drawing {
    let names = Names::of(doc);
    let mut d = Drawing::new_imperial();
    d.header = crate::header::read(&doc.header, version);
    d.handseed = doc.header.handle_seed.max(doc.next_handle());
    d.header.set("HANDSEED", cadcraft_doc::HVal::Str(format!("{:X}", d.handseed)));
    if doc.header.last_saved_by.is_empty() && !doc.summary_info.last_saved_by.is_empty() {
        d.header.set("LASTSAVEDBY", cadcraft_doc::HVal::Str(doc.summary_info.last_saved_by.clone()));
    }
    let prec = doc.header.dwf_3d_precision;
    d.header.set("3DDWFPREC", cadcraft_doc::HVal::Real(if prec.is_finite() { prec.clamp(1.0, 6.0) } else { 2.0 }));
    view_tables(doc, &mut d);
    d.foreign = Arc::new(cadcraft_doc::foreign::Foreign { version: Some(version.to_string()), ..Default::default() });

    // Tables, in the file's order; the standard records CADCraft relies on stay if the file
    // lacks them.
    let layers: Vec<_> = doc.layers.iter().filter(|l| !l.name.is_empty()).map(crate::tables::layer).collect();
    if !layers.is_empty() {
        let missing_zero = !layers.iter().any(|l| l.name == "0");
        d.layers = layers;
        if missing_zero {
            d.layers.insert(0, cadcraft_doc::Layer::default());
        }
    }
    let shape_files: HashMap<u64, String> =
        doc.text_styles.iter().filter(|s| s.name.trim().is_empty()).map(|s| (s.handle.value(), s.font_file.clone())).collect();
    let mut seen = std::collections::HashSet::new();
    let linetypes: Vec<_> = doc
        .line_types
        .iter()
        .filter(|l| !l.name.is_empty() && seen.insert(up(&l.name)))
        .map(|l| crate::tables::linetype(l, &names, &shape_files))
        .collect();
    for lt in linetypes {
        match d.linetypes.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&lt.name)) {
            Some(x) => *x = lt,
            None => d.linetypes.push(lt),
        }
    }
    let styles: Vec<_> =
        doc.text_styles.iter().filter(|s| !s.name.trim().is_empty()).map(|s| crate::tables::text_style(s, doc.object_xdata.get(&s.handle))).collect();
    if !styles.is_empty() {
        d.text_styles = styles;
    }
    let dims: Vec<_> = doc.dim_styles.iter().filter(|s| !s.name.is_empty()).map(|s| crate::tables::dim_style(s, &names)).collect();
    if !dims.is_empty() {
        d.dim_styles = dims;
    }

    // Blocks (the layouts' own records are their spaces, not blocks).
    let mut layout_records: HashMap<u64, ()> = HashMap::new();
    for o in doc.objects.values() {
        if let ObjectType::Layout(l) = o {
            layout_records.insert(l.block_record.value(), ());
        }
    }
    for br in doc.block_records.iter() {
        let space = br.is_model_space() || br.is_paper_space() || layout_records.contains_key(&br.handle.value());
        if space || br.name.is_empty() {
            continue;
        }
        let xref = (br.flags.is_xref || br.flags.is_xref_overlay || !br.xref_path.is_empty()).then(|| br.xref_path.clone());
        let block = Block {
            name: br.name.clone(),
            annotative: false,
            base: v3(br.base_point),
            entities: if xref.is_some() { EntityStore::new() } else { store(doc, &br.entity_handles, &names) },
            description: br.description.clone(),
            xref_overlay: br.flags.is_xref_overlay,
            xref_status: XrefStatus::default(),
            anonymous: br.flags.anonymous || br.name.starts_with('*'),
            xref_path: xref,
            explodable: br.explodable,
            units: u8::try_from(br.units).unwrap_or(0),
            array: None,
        };
        d.blocks.insert(br.name.clone(), Arc::new(block));
    }

    // Model space and the layouts.
    if let Some(ms) = doc.block_records.iter().find(|b| b.is_model_space()) {
        d.model = store(doc, &ms.entity_handles, &names);
    }
    let mut layouts: Vec<&acadrust::objects::Layout> =
        doc.objects.values().filter_map(|o| if let ObjectType::Layout(l) = o { Some(l) } else { None }).collect();
    layouts.sort_by_key(|l| (l.tab_order, l.name.clone()));
    let mut out = Vec::new();
    for l in layouts {
        if l.name.eq_ignore_ascii_case("Model") {
            continue;
        }
        let Some(br) = doc.block_records.iter().find(|b| b.handle == l.block_record) else { continue };
        let mut lay = Layout::new(&l.name, u32::try_from(l.tab_order).unwrap_or(0));
        lay.page = page_setup(l);
        let (lo, hi) = (Vec2::new(l.min_limits.0, l.min_limits.1), Vec2::new(l.max_limits.0, l.max_limits.1));
        // A layout without limits of its own has AutoCAD's default 0,0 to 12,9.
        let default = lo == Vec2::ZERO && (hi == Vec2::ZERO || hi == Vec2::new(12.0, 9.0));
        lay.limits = (lo.is_finite() && hi.is_finite() && !default).then_some((lo, hi));
        // The overall paper-space viewport (id 1) is the layout's own view, which CADCraft keeps
        // implicit.
        let mut entities = store(doc, &br.entity_handles, &names);
        let overall = entities.iter().find(|e| matches!(&e.kind, cadcraft_doc::EntityKind::Viewport(v) if v.id == 1)).map(|e| e.handle);
        if let Some(vp) = overall {
            entities.remove(vp);
        }
        lay.entities = entities;
        if br.name.eq_ignore_ascii_case("*Paper_Space") {
            d.active_layout = Some(l.name.clone());
        }
        out.push(lay);
    }
    if !out.is_empty() {
        d.layouts = out;
    }
    // The annotation scale list, in the file's order.
    let mut scales: Vec<&acadrust::objects::Scale> =
        doc.objects.values().filter_map(|o| if let ObjectType::Scale(s) = o { Some(s) } else { None }).collect();
    scales.sort_by_key(|s| s.handle.value());
    let mut list: Vec<cadcraft_doc::AnnoScale> = Vec::new();
    for s in scales {
        let ok = !s.name.trim().is_empty() && s.paper_units.is_finite() && s.drawing_units.is_finite() && s.paper_units > 0.0 && s.drawing_units > 0.0;
        if ok && list.len() < cadcraft_doc::MAX_SCALES && !list.iter().any(|x| x.name.eq_ignore_ascii_case(&s.name)) {
            list.push(cadcraft_doc::AnnoScale { name: s.name.clone(), paper_units: s.paper_units, drawing_units: s.drawing_units });
        }
    }
    if !list.is_empty() {
        d.scales = list;
    }
    dim_associations(doc, &mut d);
    d
}

/// Dimensions' associativity from the file's DIMASSOC objects.
fn dim_associations(doc: &CadDocument, d: &mut Drawing) {
    use acadrust::objects::AssociativeData;
    for o in doc.objects.values() {
        let ObjectType::Associative(a) = o else { continue };
        let AssociativeData::DimensionAssociation(x) = &a.data else { continue };
        let refs: Vec<cadcraft_doc::dimassoc::OsnapRef> = x
            .references
            .iter()
            .filter_map(|slot| slot.first())
            .map(|r| cadcraft_doc::dimassoc::OsnapRef {
                point_ref: r.class_name == "AcDbOsnapPointRef",
                osnap: i64::from(r.osnap_type),
                main: r.xrefs.first().filter(|h| !h.is_null()).map(|h| crate::conv::h(*h)),
                other: r.intersection_objects.first().filter(|h| !h.is_null()).map(|h| crate::conv::h(*h)),
                at: v3(r.osnap_point).xy(),
                segment: (r.main_subent_type == 2 && r.main_gs_marker >= 1).then_some(i64::from(r.main_gs_marker)),
            })
            .collect();
        cadcraft_doc::dimassoc::apply(d, crate::conv::h(x.dimension), i64::from(x.associativity), &refs);
    }
}

/// The viewport configurations, the current view, snap and grid (from the first `*ACTIVE`
/// viewport record), named views and UCSs.
fn view_tables(doc: &CadDocument, d: &mut Drawing) {
    use cadcraft_doc::{HVal, NamedView, Ucs, VportConfig, VportTile};
    let mut active = false;
    for v in doc.vports.iter() {
        let name = v.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let twist = cadcraft_geom::norm_angle(v.view_twist);
        let target = v3(v.view_target).xy();
        let c = crate::conv::v2(v.view_center).rotate(-twist) + if target.is_finite() { target } else { Vec2::ZERO };
        let center = if c.is_finite() { c } else { crate::conv::v2(v.view_center) };
        let (min, max) = (crate::conv::v2(v.lower_left), crate::conv::v2(v.upper_right));
        let height = v.view_height;
        let is_active = name.eq_ignore_ascii_case(VportConfig::ACTIVE);
        let first_active = is_active && !active;
        if first_active {
            active = true;
            let (snap, grid) = (crate::conv::v2(v.snap_spacing), crate::conv::v2(v.grid_spacing));
            if snap.is_finite() && snap.x > 0.0 && snap.y > 0.0 {
                d.header.set("SNAPUNIT", HVal::Point(snap.to3(0.0)));
            }
            if grid.is_finite() && grid.x >= 0.0 && grid.y >= 0.0 {
                d.header.set("GRIDUNIT", HVal::Point(grid.to3(0.0)));
            }
            let base = crate::conv::v2(v.snap_base);
            if base.is_finite() {
                d.header.set("SNAPBASE", HVal::Point(base.to3(0.0)));
            }
            if v.snap_rotation.is_finite() {
                d.header.set_f64("SNAPANG", v.snap_rotation.to_degrees());
            }
        }
        let unit = |x: f64| x.is_finite() && (-1e-9..=1.0 + 1e-9).contains(&x);
        let view_ok = center.is_finite() && height.is_finite() && height > 0.0;
        let tile_ok = [min.x, min.y, max.x, max.y].into_iter().all(unit) && max.x > min.x + 1e-6 && max.y > min.y + 1e-6;
        if first_active && view_ok {
            d.header.set_saved_view(center, height);
        }
        if !view_ok || !tile_ok {
            continue;
        }
        let aspect = if v.aspect_ratio.is_finite() && v.aspect_ratio > 0.0 { v.aspect_ratio } else { 0.0 };
        let clamp = |p: Vec2| Vec2::new(p.x.clamp(0.0, 1.0), p.y.clamp(0.0, 1.0));
        let tile = VportTile { min: clamp(min), max: clamp(max), center, height, aspect, twist };
        let name = if is_active { VportConfig::ACTIVE.to_string() } else { name };
        match d.vports.iter_mut().find(|c| c.name.eq_ignore_ascii_case(&name)) {
            Some(c) if c.tiles.len() < 64 => c.tiles.push(tile),
            Some(_) => {}
            None => d.vports.push(VportConfig { name, tiles: vec![tile] }),
        }
    }
    d.vports.retain(|c| c.tiles.len() > usize::from(c.is_active()) || c.tiles.first().is_some_and(|t| t.twist != 0.0));
    for v in doc.views.iter() {
        let (hgt, w) = (v.height, v.width);
        if v.name.is_empty() || !(hgt.is_finite() && w.is_finite() && hgt > 0.0 && w > 0.0) {
            continue;
        }
        let twist = cadcraft_geom::norm_angle(v.twist_angle);
        let c = v3(v.center).xy().rotate(-twist) + v3(v.target).xy();
        if !c.is_finite() {
            continue;
        }
        let nv = NamedView { name: v.name.clone(), center: c, height: hgt, width: w, layer_state: None, twist, layout: v.paper_space.then(String::new) };
        match d.views.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&v.name)) {
            Some(x) => *x = nv,
            None => d.views.push(nv),
        }
    }
    for u in doc.ucss.iter() {
        let (origin, x_axis, y_axis) = (v3(u.origin), v3(u.x_axis), v3(u.y_axis));
        if u.name.is_empty() || ![origin, x_axis, y_axis].iter().all(|p| p.is_finite()) {
            continue;
        }
        let nu = Ucs { name: u.name.clone(), origin, x_axis, y_axis };
        match d.ucss.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&u.name)) {
            Some(x) => *x = nu,
            None => d.ucss.push(nu),
        }
    }
}

/// A layout's page setup, as the DXF reader takes its plot settings.
fn page_setup(l: &acadrust::objects::Layout) -> PageSetup {
    let mut p = PageSetup::default();
    let mm = |v: f64, d: f64| if v.is_finite() && (0.0..=100_000.0).contains(&v) { v } else { d };
    let (w, h) = (mm(l.paper_width, 0.0), mm(l.paper_height, 0.0));
    if w > 0.0 && h > 0.0 {
        p.width_mm = w.min(h);
        p.height_mm = w.max(h);
    }
    p.margins_mm = [
        mm(l.plot_margin_left, p.margins_mm[0]),
        mm(l.plot_margin_bottom, p.margins_mm[1]),
        mm(l.plot_margin_right, p.margins_mm[2]),
        mm(l.plot_margin_top, p.margins_mm[3]),
    ];
    p.paper_mm = l.plot_paper_units == 1;
    let rotated = matches!(l.plot_rotation, 1 | 3);
    if w > 0.0 && h > 0.0 {
        p.landscape = rotated != (w > h);
        p.file_paper = Some((w, h, i64::from(l.plot_rotation).clamp(0, 3)));
    } else {
        p.landscape = rotated;
    }
    if !l.paper_size.is_empty() {
        p.paper = l.paper_size.replace('_', " ");
    }
    let dev = l.plot_printer_name.trim();
    p.device = if dev.is_empty() || dev.eq_ignore_ascii_case("none_device") { "None".into() } else { dev.to_string() };
    let f = &l.plot_flags;
    p.show_plot_styles = f.show_plot_styles;
    p.center = f.plot_centered;
    p.lineweights = f.print_lineweights;
    p.scale_to_fit = f.use_standard_scale && l.plot_scale_type == 0;
    let pos = |v: f64| (v.is_finite() && v > 0.0).then_some(v);
    let ratio = pos(l.plot_scale_numerator).zip(pos(l.plot_scale_denominator)).map(|(n, d)| n / d);
    if let Some(s) = ratio.and_then(pos).or_else(|| pos(l.plot_scale_factor)) {
        p.scale = s;
    }
    p.plot_area = match l.plot_type {
        0 => "display",
        1 => "extents",
        2 => "limits",
        4 => "window",
        _ => "layout",
    }
    .into();
    p.plot_window = [l.plot_window_min_x, l.plot_window_min_y, l.plot_window_max_x, l.plot_window_max_y].map(|v| if v.is_finite() { v } else { 0.0 });
    p.plot_style_table.clone_from(&l.plot_style_sheet);
    let o = Vec2::new(l.paper_image_origin_x, l.paper_image_origin_y);
    p.image_origin = (o.is_finite() && o.x.abs() < 1e12 && o.y.abs() < 1e12 && o != Vec2::ZERO).then_some(o);
    p
}
