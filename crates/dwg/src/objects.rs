//! What a drawing holds besides its entities and symbol tables, both ways: groups, named views,
//! UCSs, the current view, snap and grid, layouts' page setups and limits; and, before writing,
//! leaving out what the DWG can't hold at the version it is written as.

use acadrust::CadDocument;
use acadrust::objects::{Dictionary, Group as AGroup, ObjectType};
use acadrust::tables as at;
use acadrust::types::{Handle as AHandle, Vector2, Vector3};
use cadcraft_doc::{Drawing, Group, HVal, PageSetup};
use cadcraft_geom::Vec2;

use crate::Unwritten;
use crate::conv::{a3, ah, h, up, v3};

// ── Reading ──────────────────────────────────────────────────────────────────────────────────

/// The file's named groups (unnamed ones, `*A…`, too), members in their order.
pub fn read_groups(doc: &CadDocument, d: &mut Drawing) {
    let names = group_names(doc);
    let mut gs: Vec<&AGroup> = doc.objects.values().filter_map(|o| if let ObjectType::Group(g) = o { Some(g) } else { None }).collect();
    gs.sort_by_key(|g| g.handle.value());
    for g in gs {
        let name = names.get(&g.handle.value()).cloned().unwrap_or_else(|| g.name.clone());
        if name.is_empty() {
            continue;
        }
        let members = g.entities.iter().filter(|e| !e.is_null()).map(|e| h(*e)).filter(|m| d.entity(*m).is_some()).collect();
        d.groups.push(Group { name, description: g.description.clone(), selectable: g.selectable, members });
    }
}

/// Group names from the ACAD_GROUP dictionary's entries (a GROUP holds no name of its own in DWG).
fn group_names(doc: &CadDocument) -> std::collections::HashMap<u64, String> {
    let mut m = std::collections::HashMap::new();
    if let Some(ObjectType::Dictionary(dict)) = doc.objects.get(&doc.header.acad_group_dict_handle) {
        for (k, v) in &dict.entries {
            m.insert(v.value(), k.clone());
        }
    }
    m
}

// ── Writing ──────────────────────────────────────────────────────────────────────────────────

/// The group dictionary, made (and named in the named object dictionary) if the file has none.
fn group_dict(doc: &mut CadDocument) -> Option<AHandle> {
    let gd = doc.header.acad_group_dict_handle;
    if !gd.is_null() && matches!(doc.objects.get(&gd), Some(ObjectType::Dictionary(_))) {
        return Some(gd);
    }
    let nod = doc.header.named_objects_dict_handle;
    if !matches!(doc.objects.get(&nod), Some(ObjectType::Dictionary(_))) {
        return None;
    }
    let handle = doc.allocate_handle();
    let mut dict = Dictionary::new();
    dict.handle = handle;
    dict.owner = nod;
    doc.objects.insert(handle, ObjectType::Dictionary(dict));
    if let Some(ObjectType::Dictionary(n)) = doc.objects.get_mut(&nod) {
        n.add_entry("ACAD_GROUP", handle);
    }
    doc.header.acad_group_dict_handle = handle;
    Some(handle)
}

/// The drawing's groups: matched by name, written when they differ, added and removed.
pub fn write_groups(d: &Drawing, doc: &mut CadDocument) {
    let names = group_names(doc);
    let existing: Vec<(u64, String)> = doc
        .objects
        .values()
        .filter_map(|o| if let ObjectType::Group(g) = o { Some((g.handle.value(), names.get(&g.handle.value()).cloned().unwrap_or_else(|| g.name.clone()))) } else { None })
        .collect();
    if d.groups.is_empty() && existing.is_empty() {
        return;
    }
    let Some(gd) = group_dict(doc) else { return };
    let want: std::collections::HashSet<String> = d.groups.iter().map(|g| up(&g.name)).collect();
    for (hd, name) in &existing {
        if !want.contains(&up(name)) {
            doc.objects.remove(&AHandle::new(*hd));
            if let Some(ObjectType::Dictionary(dict)) = doc.objects.get_mut(&gd) {
                dict.entries.retain(|(_, v)| v.value() != *hd);
            }
        }
    }
    for g in &d.groups {
        let members: Vec<AHandle> = g.members.iter().map(|m| ah(*m)).collect();
        let at = existing.iter().find(|(_, n)| n.eq_ignore_ascii_case(&g.name)).map(|(hd, _)| AHandle::new(*hd));
        match at.and_then(|hd| doc.objects.get_mut(&hd)) {
            Some(ObjectType::Group(x)) => {
                x.description.clone_from(&g.description);
                x.selectable = g.selectable;
                x.entities = members;
            }
            _ => {
                let handle = doc.allocate_handle();
                let mut x = AGroup::new(g.name.clone());
                x.handle = handle;
                x.owner = gd;
                x.description.clone_from(&g.description);
                x.selectable = g.selectable;
                x.unnamed = g.name.starts_with('*');
                x.entities = members;
                doc.objects.insert(handle, ObjectType::Group(x));
                if let Some(ObjectType::Dictionary(dict)) = doc.objects.get_mut(&gd) {
                    dict.add_entry(g.name.clone(), handle);
                }
            }
        }
    }
}

/// Named views and UCSs, matched by name.
pub fn write_views(d: &Drawing, doc: &mut CadDocument) {
    for v in &d.views {
        let dcs = v.center.rotate(v.twist);
        let found = doc.views.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&v.name));
        let x = match found {
            Some(x) => x,
            None => {
                let mut x = at::View::new(v.name.clone());
                x.handle = doc.allocate_handle();
                let _ = doc.views.add(x);
                let Some(x) = doc.views.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&v.name)) else { continue };
                x
            }
        };
        let read = Vec2::new(x.center.x, x.center.y).rotate(-v.twist) + v3(x.target).xy();
        if !read.near(v.center, 1e-9) || x.height != v.height || x.width != v.width || x.twist_angle != v.twist {
            x.center = Vector3::new(dcs.x, dcs.y, 0.0);
            x.target = Vector3::new(0.0, 0.0, 0.0);
            x.height = v.height;
            x.width = v.width;
            x.twist_angle = v.twist;
        }
        x.paper_space = v.layout.is_some();
    }
    for u in &d.ucss {
        let found = doc.ucss.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&u.name));
        let x = match found {
            Some(x) => x,
            None => {
                let mut x = at::Ucs::new(u.name.clone());
                x.handle = doc.allocate_handle();
                let _ = doc.ucss.add(x);
                let Some(x) = doc.ucss.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&u.name)) else { continue };
                x
            }
        };
        x.origin = a3(u.origin);
        x.x_axis = a3(u.x_axis);
        x.y_axis = a3(u.y_axis);
    }
}

/// The current view, snap and grid in the first `*ACTIVE` viewport record.
pub fn write_active_vport(d: &Drawing, doc: &mut CadDocument) {
    let hp = |n: &str| match d.header.get(n) {
        Some(HVal::Point(p)) => Some(*p),
        _ => None,
    };
    if !doc.vports.iter().any(|v| v.name.eq_ignore_ascii_case("*Active")) {
        let mut x = at::VPort::new("*Active");
        x.handle = doc.allocate_handle();
        let _ = doc.vports.add(x);
    }
    let Some(x) = doc.vports.iter_mut().find(|v| v.name.eq_ignore_ascii_case("*Active")) else { return };
    if let Some(p) = hp("SNAPUNIT") {
        x.snap_spacing = Vector2::new(p.x, p.y);
    }
    if let Some(p) = hp("GRIDUNIT") {
        x.grid_spacing = Vector2::new(p.x, p.y);
    }
    if let Some(p) = hp("SNAPBASE") {
        x.snap_base = Vector2::new(p.x, p.y);
    }
    if let Some(a) = d.header.get("SNAPANG").and_then(HVal::as_f64) {
        x.snap_rotation = a.to_radians();
    }
    if let Some((c, height)) = d.header.saved_view() {
        let twist = x.view_twist;
        let read = Vec2::new(x.view_center.x, x.view_center.y).rotate(-twist) + v3(x.view_target).xy();
        if !read.near(c, 1e-9) || (x.view_height - height).abs() > 1e-12 {
            let dcs = (c - v3(x.view_target).xy()).rotate(twist);
            x.view_center = Vector2::new(dcs.x, dcs.y);
            x.view_height = height;
        }
    }
}

/// A layout's page setup and limits onto its LAYOUT object, as `read::page_setup` reads them.
pub fn put_page(l: &mut acadrust::objects::Layout, p: &PageSetup, limits: Option<(Vec2, Vec2)>) {
    if crate::read::page_setup(l) == *p && read_limits(l) == limits {
        return;
    }
    let (w, hgt, r) = p.plot_paper();
    (l.paper_width, l.paper_height, l.plot_rotation) = (w, hgt, i16::try_from(r).unwrap_or(0));
    [l.plot_margin_left, l.plot_margin_bottom, l.plot_margin_right, l.plot_margin_top] = p.margins_mm;
    l.plot_paper_units = if p.paper_mm { 1 } else { 0 };
    l.paper_size = p.paper.replace(' ', "_");
    l.plot_printer_name = if p.device == "None" { "none_device".into() } else { p.device.clone() };
    l.plot_flags.show_plot_styles = p.show_plot_styles;
    l.plot_flags.plot_centered = p.center;
    l.plot_flags.print_lineweights = p.lineweights;
    l.plot_flags.use_standard_scale = p.scale_to_fit;
    l.plot_scale_type = if p.scale_to_fit { 0 } else { 16 };
    (l.plot_scale_numerator, l.plot_scale_denominator, l.plot_scale_factor) = (p.scale, 1.0, p.scale);
    l.plot_type = match p.plot_area.as_str() {
        "display" => 0,
        "extents" => 1,
        "limits" => 2,
        "window" => 4,
        _ => 5,
    };
    [l.plot_window_min_x, l.plot_window_min_y, l.plot_window_max_x, l.plot_window_max_y] = p.plot_window;
    l.plot_style_sheet.clone_from(&p.plot_style_table);
    let o = p.image_origin.unwrap_or(Vec2::ZERO);
    (l.paper_image_origin_x, l.paper_image_origin_y) = (o.x, o.y);
    let (lo, hi) = limits.unwrap_or((Vec2::ZERO, Vec2::new(12.0, 9.0)));
    l.min_limits = (lo.x, lo.y);
    l.max_limits = (hi.x, hi.y);
    l.raw_plot_settings_codes = None;
}

fn read_limits(l: &acadrust::objects::Layout) -> Option<(Vec2, Vec2)> {
    let (lo, hi) = (Vec2::new(l.min_limits.0, l.min_limits.1), Vec2::new(l.max_limits.0, l.max_limits.1));
    let default = lo == Vec2::ZERO && (hi == Vec2::ZERO || hi == Vec2::new(12.0, 9.0));
    (lo.is_finite() && hi.is_finite() && !default).then_some((lo, hi))
}

/// Leave out what acadrust can't write as this DWG version: other programs' entities and objects
/// kept only as raw records of another version (or none). acadrust refuses the whole file for
/// one of them; the rest of the drawing is written and the note says what was left out.
pub fn drop_unwritable(doc: &mut CadDocument) -> Unwritten {
    use acadrust::entities::ExtendedEntityData as X;
    let version = doc.version;
    // Before 2013 acadrust writes no class for a proxy, whose class number would then name
    // another class: proxies are left out.
    let legacy = version < acadrust::DxfVersion::AC1027;
    let mut dropped = Unwritten::new();
    let mut count = |name: &str| *dropped.entry(name.trim().to_ascii_uppercase()).or_insert(0) += 1;
    let entities: Vec<AHandle> = doc
        .entities()
        .filter_map(|e| match e {
            acadrust::EntityType::Unknown(u) if u.raw_dwg_data.is_none() || u.dwg_source_version.is_some_and(|v| v != version) => {
                count(&u.dxf_name);
                Some(u.common.handle)
            }
            acadrust::EntityType::Extended(x) if legacy && matches!(x.data, X::Proxy(_) | X::RegisteredClass(_)) => {
                count("ACAD_PROXY_ENTITY");
                Some(x.common.handle)
            }
            _ => None,
        })
        .collect();
    for hd in &entities {
        doc.remove_entity(*hd);
    }
    if !entities.is_empty() {
        let gone: std::collections::HashSet<u64> = entities.iter().map(|x| x.value()).collect();
        for b in doc.block_records.iter_mut() {
            b.entity_handles.retain(|x| !gone.contains(&x.value()));
        }
    }
    let objects: Vec<AHandle> = doc
        .objects
        .iter()
        .filter_map(|(hd, o)| match o {
            ObjectType::Unknown { type_name, raw_dwg_data, raw_dwg_version, .. }
                if raw_dwg_data.is_none() || raw_dwg_version.is_some_and(|v| v != version) =>
            {
                count(type_name);
                Some(*hd)
            }
            ObjectType::ProxyObject(_) if legacy => {
                count("ACAD_PROXY_OBJECT");
                Some(*hd)
            }
            _ => None,
        })
        .collect();
    for hd in &objects {
        doc.objects.remove(hd);
    }
    if !objects.is_empty() {
        let gone: std::collections::HashSet<AHandle> = objects.iter().copied().collect();
        for o in doc.objects.values_mut() {
            if let ObjectType::Dictionary(dc) = o {
                dc.entries.retain(|(_, v)| !gone.contains(v));
            }
        }
    }
    // A class without a name or a C++ class name is no class of anything written; kept, it
    // stops readers reading the file.
    // The others keep their order (two of one name too) and are numbered by their place again;
    // proxies follow their class, and one whose class goes goes with it.
    let bad = |c: &acadrust::classes::DxfClass| c.dxf_name.trim().is_empty() || c.cpp_class_name.trim().is_empty();
    if doc.classes.iter().any(bad) {
        let mut kept = acadrust::classes::DxfClassCollection::new();
        let mut moved: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
        for (i, c) in doc.classes.iter().enumerate() {
            if bad(c) {
                continue;
            }
            let mut c = c.clone();
            let n = 500 + i32::try_from(kept.len()).unwrap_or(i32::MAX - 500);
            moved.insert(500 + i32::try_from(i).unwrap_or(i32::MAX - 500), n);
            c.class_number = i16::try_from(n).unwrap_or(i16::MAX);
            kept.push_preserving(c);
        }
        doc.classes = kept;
        let mut gone_entities = Vec::new();
        for e in doc.entities_mut() {
            if let acadrust::EntityType::Extended(x) = e
                && let X::Proxy(p) = &mut x.data
            {
                match moved.get(&p.class_id) {
                    Some(n) => p.class_id = *n,
                    None => gone_entities.push(x.common.handle),
                }
            }
        }
        for h in &gone_entities {
            count("ACAD_PROXY_ENTITY");
            doc.remove_entity(*h);
        }
        if !gone_entities.is_empty() {
            for b in doc.block_records.iter_mut() {
                b.entity_handles.retain(|x| !gone_entities.contains(x));
            }
        }
        let mut gone = std::collections::HashSet::new();
        for (h, o) in doc.objects.iter_mut() {
            if let ObjectType::ProxyObject(p) = o {
                match moved.get(&p.class_id) {
                    Some(n) => p.class_id = *n,
                    None => {
                        gone.insert(*h);
                    }
                }
            }
        }
        for h in &gone {
            count("ACAD_PROXY_OBJECT");
            doc.objects.remove(h);
        }
        if !gone.is_empty() {
            for o in doc.objects.values_mut() {
                if let ObjectType::Dictionary(dc) = o {
                    dc.entries.retain(|(_, v)| !gone.contains(v));
                }
            }
        }
    }
    dropped
}

/// Layouts the drawing no longer has removed (with their paper space and dictionary entry), and
/// the drawing's active layout (or its first by tab order) made the one whose block is
/// `*Paper_Space`.
pub fn write_layouts(d: &Drawing, doc: &mut CadDocument) {
    let layouts: Vec<(AHandle, String, AHandle)> = doc
        .objects
        .values()
        .filter_map(|o| if let ObjectType::Layout(l) = o { Some((l.handle, l.name.clone(), l.block_record)) } else { None })
        .collect();
    let keep = |n: &str| n.eq_ignore_ascii_case("Model") || d.layout(n).is_some();
    for (lh, name, br) in &layouts {
        if keep(name) {
            continue;
        }
        let (rname, hs) = match doc.block_records.iter().find(|b| b.handle == *br) {
            Some(b) => (b.name.clone(), b.entity_handles.clone()),
            None => continue,
        };
        for x in hs {
            doc.remove_entity(x);
        }
        doc.block_records.remove(&rname);
        doc.objects.remove(lh);
        let dict = doc.header.acad_layout_dict_handle;
        if let Some(ObjectType::Dictionary(dc)) = doc.objects.get_mut(&dict) {
            dc.entries.retain(|(_, v)| v != lh);
        }
    }
    let mut tabs: Vec<&cadcraft_doc::Layout> = d.layouts.iter().collect();
    tabs.sort_by_key(|l| l.tab_order);
    let active = d.active_layout.as_deref().and_then(|a| d.layout(a)).or(tabs.first().copied());
    let Some(active) = active else { return };
    let Some(target) = doc.objects.values().find_map(|o| match o {
        ObjectType::Layout(l) if l.name.eq_ignore_ascii_case(&active.name) => Some(l.block_record),
        _ => None,
    }) else {
        return;
    };
    let Some(tname) = doc.block_records.iter().find(|b| b.handle == target).map(|b| b.name.clone()) else { return };
    if !tname.eq_ignore_ascii_case("*Paper_Space") {
        let cur = doc.block_records.iter().find(|b| b.name.eq_ignore_ascii_case("*Paper_Space")).map(|b| b.name.clone());
        if let Some(cur) = cur {
            let tmp = "*Paper_Space_swap";
            let _ = doc.block_records.rename(&cur, tmp);
            let _ = doc.block_records.rename(&tname, "*Paper_Space");
            let _ = doc.block_records.rename(tmp, &tname);
        } else {
            let _ = doc.block_records.rename(&tname, "*Paper_Space");
        }
    }
    doc.header.paper_space_block_handle = target;
}

/// The variable dictionary (`AcDbVariableDictionary` in the named object dictionary) AutoCAD
/// always writes, added with its usual variables when the document has none, for DWGs before
/// 2018: some readers (Vectric Aspire) take a 2004 DWG without it as empty. A 2018 DWG is left as
/// it is. `DIMASSOC` and `HIDETEXT` are the drawing's own (as the header has them).
pub fn variable_dictionary(doc: &mut CadDocument) {
    if doc.version >= acadrust::DxfVersion::AC1032 {
        return;
    }
    let root = doc.header.named_objects_dict_handle;
    let has = match doc.objects.get(&root) {
        Some(ObjectType::Dictionary(r)) => r.entries.iter().any(|(k, _)| k.eq_ignore_ascii_case("AcDbVariableDictionary")),
        _ => return,
    };
    if has {
        return;
    }
    let dict_h = doc.allocate_handle();
    let mut dict = acadrust::objects::Dictionary::new();
    dict.handle = dict_h;
    dict.owner = root;
    let vars = [
        ("CANNOSCALE", "1:1".to_string()),
        ("CMLEADERSTYLE", "Standard".to_string()),
        ("CTABLESTYLE", "Standard".to_string()),
        ("DIMASSOC", doc.header.dimension_associativity.to_string()),
        ("HIDETEXT", doc.header.hide_text.to_string()),
        ("LAYEREVAL", "0".to_string()),
        ("LAYERNOTIFY", "0".to_string()),
    ];
    for (name, value) in vars {
        let h = doc.allocate_handle();
        let mut v = acadrust::objects::DictionaryVariable::new(name, &value);
        v.handle = h;
        v.owner_handle = dict_h;
        doc.objects.insert(h, ObjectType::DictionaryVariable(v));
        dict.add_entry(name, h);
    }
    doc.objects.insert(dict_h, ObjectType::Dictionary(dict));
    if let Some(ObjectType::Dictionary(r)) = doc.objects.get_mut(&root) {
        r.add_entry("AcDbVariableDictionary", dict_h);
    }
}

/// The drawing's tiled viewport configurations as VPORT records (each tile one record of its
/// configuration's name), where they differ from what the file holds. The `*Active`
/// configuration's first record also holds the current view, snap and grid
/// ([`write_active_vport`]), so it stays and only its tile is set.
pub fn write_vport_configs(d: &Drawing, doc: &mut CadDocument) {
    use cadcraft_doc::VportConfig;
    let tile_of = |v: &at::VPort| {
        let twist = cadcraft_geom::norm_angle(v.view_twist);
        let center = Vec2::new(v.view_center.x, v.view_center.y).rotate(-twist) + v3(v.view_target).xy();
        (Vec2::new(v.lower_left.x, v.lower_left.y), Vec2::new(v.upper_right.x, v.upper_right.y), center, v.view_height, v.aspect_ratio, twist)
    };
    let same = |a: &cadcraft_doc::VportTile, b: &(Vec2, Vec2, Vec2, f64, f64, f64)| {
        a.min.near(b.0, 1e-9) && a.max.near(b.1, 1e-9) && a.center.near(b.2, 1e-9) && (a.height - b.3).abs() <= 1e-9 && (a.twist - b.5).abs() <= 1e-9
    };
    let put = |x: &mut at::VPort, t: &cadcraft_doc::VportTile| {
        x.lower_left = Vector2::new(t.min.x, t.min.y);
        x.upper_right = Vector2::new(t.max.x, t.max.y);
        let dcs = (t.center - v3(x.view_target).xy()).rotate(t.twist);
        x.view_center = Vector2::new(dcs.x, dcs.y);
        x.view_height = t.height;
        if t.aspect > 0.0 {
            x.aspect_ratio = t.aspect;
        }
        x.view_twist = t.twist;
    };
    for c in &d.vports {
        let active = c.name.eq_ignore_ascii_case(VportConfig::ACTIVE);
        let records: Vec<(Vec2, Vec2, Vec2, f64, f64, f64)> = doc.vports.iter().filter(|v| v.name.trim().eq_ignore_ascii_case(&c.name)).map(tile_of).collect();
        if records.len() == c.tiles.len() && c.tiles.iter().zip(&records).all(|(a, b)| same(a, b)) {
            continue;
        }
        // The records of this configuration, the active one's first kept.
        let mut kept: Option<at::VPort> = None;
        let mut rest: Vec<at::VPort> = Vec::new();
        let all: Vec<at::VPort> = doc.vports.iter().cloned().collect();
        doc.vports.clear();
        for v in all {
            if v.name.trim().eq_ignore_ascii_case(&c.name) {
                if active && kept.is_none() {
                    kept = Some(v);
                }
            } else {
                rest.push(v);
            }
        }
        let mut mine = Vec::new();
        for (i, t) in c.tiles.iter().enumerate() {
            let mut x = match (i, kept.take()) {
                (0, Some(k)) => k,
                _ => {
                    let mut x = at::VPort::new(&c.name);
                    x.handle = doc.allocate_handle();
                    x
                }
            };
            put(&mut x, t);
            mine.push(x);
        }
        // `*Active` first, as AutoCAD lists it.
        let list = if active { mine.into_iter().chain(rest).collect::<Vec<_>>() } else { rest.into_iter().chain(mine).collect() };
        for v in list {
            doc.vports.add_allow_duplicate(v);
        }
    }
}
