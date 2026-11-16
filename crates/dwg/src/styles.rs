//! Table, multiline and multileader styles, both ways. Each is an object named by its entry in
//! the named object dictionary's `ACAD_TABLESTYLE`, `ACAD_MLINESTYLE` or `ACAD_MLEADERSTYLE`
//! dictionary; CADCraft keeps what each style's documented properties say, as its DXF reader
//! takes them from the groups of those properties.

use acadrust::CadDocument;
use acadrust::objects::{self as ao, ObjectType};
use acadrust::types::Handle as AHandle;
use cadcraft_color::Color;
use cadcraft_doc::{CellBorder, CellStyle, Drawing, MAX_MLINE_ELEMENTS, MLeaderStyle, MLineCaps, MLineElement, MLineStyle, TableStyle};

use crate::conv::{acolor, alineweight, color, lineweight};
use crate::entity_read::Names;

/// The cell types a table style documents, in their order.
const CELL_TYPES: [&str; 3] = [CellStyle::DATA, CellStyle::HEADER, CellStyle::TITLE];

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// The drawing's table, multiline and multileader styles from the file's.
pub fn read(doc: &CadDocument, names: &Names, d: &mut Drawing) {
    let mut table_styles = Vec::new();
    let mut mline_styles = Vec::new();
    let mut mleader_styles = Vec::new();
    let mut objects: Vec<&ObjectType> = doc.objects.values().collect();
    objects.sort_by_key(|o| handle_of(o));
    for o in objects {
        match o {
            ObjectType::TableStyle(s) if !s.name.trim().is_empty() => table_styles.push(table_style(s, names)),
            ObjectType::MLineStyle(s) => {
                if let Some(s) = mline_style(s) {
                    mline_styles.push(s);
                }
            }
            ObjectType::MultiLeaderStyle(s) if !s.name.trim().is_empty() => mleader_styles.push(mleader_style(s, names)),
            _ => {}
        }
    }
    put(&mut d.table_styles, table_styles, |s: &TableStyle| &s.name);
    put(&mut d.mline_styles, mline_styles, |s: &MLineStyle| &s.name);
    put(&mut d.mleader_styles, mleader_styles, |s: &MLeaderStyle| &s.name);
}

/// The styles `new` in `list`, each replacing the one of its name.
fn put<S>(list: &mut Vec<S>, new: Vec<S>, name: fn(&S) -> &str) {
    for s in new {
        match list.iter_mut().find(|x| name(x).eq_ignore_ascii_case(name(&s))) {
            Some(x) => *x = s,
            None => list.push(s),
        }
    }
}

fn handle_of(o: &ObjectType) -> u64 {
    match o {
        ObjectType::TableStyle(s) => s.handle.value(),
        ObjectType::MLineStyle(s) => s.handle.value(),
        ObjectType::MultiLeaderStyle(s) => s.handle.value(),
        _ => 0,
    }
}

fn row_text_style(r: &ao::RowCellStyle, names: &Names) -> String {
    let n = r.text_style_name.trim();
    if !n.is_empty() {
        return n.to_string();
    }
    r.text_style_handle.and_then(|h| names.styles.get(&h.value()).cloned()).unwrap_or_default()
}

/// A table style as CADCraft's: margins, title and header, and the data, header and title cell
/// styles where they differ from the ones those imply.
pub fn table_style(s: &ao::TableStyle, names: &Names) -> TableStyle {
    let def = TableStyle::default();
    let data_h = Some(s.data_row_style.text_height).filter(|h| h.is_finite() && *h > 0.0);
    let mut st = TableStyle {
        name: s.name.clone(),
        text_height: data_h.unwrap_or(def.text_height),
        margin: Some(s.horizontal_margin).filter(|m| m.is_finite() && *m >= 0.0).unwrap_or(def.margin),
        title: !s.title_suppressed,
        header: !s.header_suppressed,
        cell_styles: Vec::new(),
    };
    let margin_v = Some(s.vertical_margin).filter(|m| m.is_finite() && *m >= 0.0);
    for (kind, row) in CELL_TYPES.iter().zip([&s.data_row_style, &s.header_row_style, &s.title_row_style]) {
        let Some(base) = st.base_cell_style(kind) else { continue };
        let mut cs = base.clone();
        if let Some(v) = margin_v {
            cs.margin_v = v;
        }
        let ts = row_text_style(row, names);
        if !ts.is_empty() {
            cs.text_style = ts;
        }
        let h = row.text_height;
        if h.is_finite() && h > 0.0 && (h - base.text_height).abs() > 1e-9 * base.text_height.max(1.0) {
            cs.text_height = h;
        }
        cs.align = u8::try_from(row.alignment as i32).unwrap_or(5).clamp(1, 9);
        cs.text_color = color(&row.text_color);
        cs.fill = row.fill_enabled.then(|| color(&row.fill_color));
        let borders = [
            &row.top_border,
            &row.horizontal_inside_border,
            &row.bottom_border,
            &row.left_border,
            &row.vertical_inside_border,
            &row.right_border,
        ];
        for (dst, b) in cs.borders.as_array_mut().into_iter().zip(borders) {
            *dst = CellBorder { visible: !b.is_invisible, color: color(&b.color), lineweight: lineweight(b.line_weight) };
        }
        st.set_cell_style(cs);
    }
    st
}

fn alignment(a: u8) -> ao::CellAlignment {
    use ao::CellAlignment as A;
    match a {
        1 => A::TopLeft,
        2 => A::TopCenter,
        3 => A::TopRight,
        4 => A::MiddleLeft,
        6 => A::MiddleRight,
        7 => A::BottomLeft,
        8 => A::BottomCenter,
        9 => A::BottomRight,
        _ => A::MiddleCenter,
    }
}

/// Write CADCraft's table style onto the file's (what CADCraft doesn't model stays).
pub fn put_table_style(x: &mut ao::TableStyle, s: &TableStyle, style: &dyn Fn(&str) -> Option<AHandle>) {
    x.name.clone_from(&s.name);
    x.horizontal_margin = s.margin;
    x.vertical_margin = s.cell_style(CellStyle::DATA).margin_v;
    x.title_suppressed = !s.title;
    x.header_suppressed = !s.header;
    let rows = [&mut x.data_row_style, &mut x.header_row_style, &mut x.title_row_style];
    for (kind, row) in CELL_TYPES.iter().zip(rows) {
        let cs = s.cell_style(kind);
        let text_style = if style(&cs.text_style).is_some() { cs.text_style.clone() } else { "Standard".to_string() };
        row.text_style_handle = style(&text_style);
        row.text_style_name = text_style;
        row.text_height = cs.text_height;
        row.alignment = alignment(cs.align);
        row.text_color = acolor(cs.text_color);
        row.fill_enabled = cs.fill.is_some();
        row.fill_color = acolor(cs.fill.unwrap_or(Color::Index(7)));
        let b = cs.borders.as_array();
        let dst = [
            &mut row.top_border,
            &mut row.horizontal_inside_border,
            &mut row.bottom_border,
            &mut row.left_border,
            &mut row.vertical_inside_border,
            &mut row.right_border,
        ];
        for (dst, b) in dst.into_iter().zip(b) {
            dst.is_invisible = !b.visible;
            dst.color = acolor(b.color);
            dst.line_weight = alineweight(b.lineweight);
        }
    }
}

/// A multiline style as CADCraft's (`None` for one without a name).
pub fn mline_style(s: &ao::MLineStyle) -> Option<MLineStyle> {
    if s.name.trim().is_empty() {
        return None;
    }
    let linetype = |n: &str| {
        if n.is_empty() || n.eq_ignore_ascii_case("bylayer") {
            "ByLayer".to_string()
        } else if n.eq_ignore_ascii_case("byblock") {
            "ByBlock".to_string()
        } else {
            n.to_string()
        }
    };
    let elements = s
        .elements
        .iter()
        .take(MAX_MLINE_ELEMENTS)
        .map(|e| MLineElement { offset: finite(e.offset), color: color(&e.color), linetype: linetype(&e.linetype) })
        .collect();
    let f = &s.flags;
    let st = MLineStyle {
        name: s.name.clone(),
        description: s.description.clone(),
        elements,
        start_caps: MLineCaps { line: f.start_square_cap, outer_arc: f.start_round_cap, inner_arcs: f.start_inner_arcs_cap, angle: finite(s.start_angle) },
        end_caps: MLineCaps { line: f.end_square_cap, outer_arc: f.end_round_cap, inner_arcs: f.end_inner_arcs_cap, angle: finite(s.end_angle) },
        joints: f.display_joints,
        fill: f.fill_on.then(|| color(&s.fill_color)),
    };
    Some(st.sanitized())
}

/// Write CADCraft's multiline style onto the file's.
pub fn put_mline_style(x: &mut ao::MLineStyle, s: &MLineStyle) {
    x.name.clone_from(&s.name);
    x.description.clone_from(&s.description);
    x.elements = s
        .elements
        .iter()
        .map(|e| {
            let mut a = ao::MLineStyleElement::new(e.offset);
            a.color = acolor(e.color);
            a.linetype = if e.linetype.eq_ignore_ascii_case("ByLayer") { "BYLAYER".into() } else { e.linetype.clone() };
            a
        })
        .collect();
    x.start_angle = s.start_caps.angle;
    x.end_angle = s.end_caps.angle;
    x.fill_color = acolor(s.fill.unwrap_or(Color::ByLayer));
    let f = &mut x.flags;
    f.fill_on = s.fill.is_some();
    f.display_joints = s.joints;
    f.start_square_cap = s.start_caps.line;
    f.start_inner_arcs_cap = s.start_caps.inner_arcs;
    f.start_round_cap = s.start_caps.outer_arc;
    f.end_square_cap = s.end_caps.line;
    f.end_inner_arcs_cap = s.end_caps.inner_arcs;
    f.end_round_cap = s.end_caps.outer_arc;
}

/// A multileader style as CADCraft's.
pub fn mleader_style(s: &ao::MultiLeaderStyle, names: &Names) -> MLeaderStyle {
    let def = MLeaderStyle::default();
    let num = |v: f64, d: f64| if v.is_finite() && v >= 0.0 { v } else { d };
    MLeaderStyle {
        name: s.name.clone(),
        arrow_size: num(s.arrowhead_size, def.arrow_size),
        text_height: Some(s.text_height).filter(|v| v.is_finite() && *v > 0.0).unwrap_or(def.text_height),
        landing_gap: num(s.landing_gap, def.landing_gap),
        dogleg: num(s.landing_distance, def.dogleg),
        text_style: s.text_style_handle.and_then(|h| names.styles.get(&h.value()).cloned()).filter(|n| !n.is_empty()).unwrap_or(def.text_style),
        annotative: s.is_annotative,
    }
}

/// Write CADCraft's multileader style onto the file's.
pub fn put_mleader_style(x: &mut ao::MultiLeaderStyle, s: &MLeaderStyle, style: &dyn Fn(&str) -> Option<AHandle>) {
    x.name.clone_from(&s.name);
    x.arrowhead_size = s.arrow_size;
    x.text_height = s.text_height;
    x.landing_gap = s.landing_gap;
    x.landing_distance = s.dogleg;
    x.text_style_handle = style(&s.text_style).or_else(|| style("Standard"));
    x.is_annotative = s.annotative;
}

/// The entry `key` of the named object dictionary.
pub fn named(doc: &CadDocument, key: &str) -> Option<AHandle> {
    match doc.objects.get(&doc.header.named_objects_dict_handle) {
        Some(ObjectType::Dictionary(root)) => root.entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, h)| *h),
        _ => None,
    }
}

/// The dictionary `key` of the named object dictionary, made when the file has none.
pub fn named_dict(doc: &mut CadDocument, key: &str) -> AHandle {
    if let Some(h) = named(doc, key).filter(|h| matches!(doc.objects.get(h), Some(ObjectType::Dictionary(_)))) {
        return h;
    }
    let root = doc.header.named_objects_dict_handle;
    let h = doc.allocate_handle();
    let mut dict = ao::Dictionary::new();
    dict.handle = h;
    dict.owner = root;
    doc.objects.insert(h, ObjectType::Dictionary(dict));
    if let Some(ObjectType::Dictionary(r)) = doc.objects.get_mut(&root) {
        r.entries.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
        r.add_entry(key, h);
    }
    h
}

/// Name `obj` `name` in the dictionary `dict`.
fn enter(doc: &mut CadDocument, dict: AHandle, name: &str, obj: AHandle) {
    if let Some(ObjectType::Dictionary(dc)) = doc.objects.get_mut(&dict) {
        dc.entries.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        dc.add_entry(name, obj);
    }
}

/// The drawing's table, multiline and multileader styles written into the file's: each matched
/// by name, written when it differs, added when new.
pub fn write(d: &Drawing, doc: &mut CadDocument, names: &Names, style: &dyn Fn(&str) -> Option<AHandle>) {
    for s in &d.table_styles {
        let found = doc.objects.values_mut().find_map(|o| match o {
            ObjectType::TableStyle(x) if x.name.eq_ignore_ascii_case(&s.name) => Some(x),
            _ => None,
        });
        match found {
            Some(x) => {
                if table_style(x, names) != *s {
                    put_table_style(x, s, style);
                }
            }
            None => {
                let mut x = ao::TableStyle::new(&s.name);
                put_table_style(&mut x, s, style);
                let dict = named_dict(doc, "ACAD_TABLESTYLE");
                x.handle = doc.allocate_handle();
                x.owner_handle = dict;
                let h = x.handle;
                doc.objects.insert(h, ObjectType::TableStyle(x));
                enter(doc, dict, &s.name, h);
            }
        }
    }
    for s in &d.mline_styles {
        let found = doc.objects.values_mut().find_map(|o| match o {
            ObjectType::MLineStyle(x) if x.name.eq_ignore_ascii_case(&s.name) => Some(x),
            _ => None,
        });
        match found {
            Some(x) => {
                if mline_style(x).as_ref() != Some(s) {
                    put_mline_style(x, s);
                }
            }
            None => {
                let mut x = ao::MLineStyle::new(s.name.clone());
                put_mline_style(&mut x, s);
                let dict = named_dict(doc, "ACAD_MLINESTYLE");
                x.handle = doc.allocate_handle();
                x.owner = dict;
                let h = x.handle;
                doc.objects.insert(h, ObjectType::MLineStyle(x));
                enter(doc, dict, &s.name, h);
            }
        }
    }
    for s in &d.mleader_styles {
        let found = doc.objects.values_mut().find_map(|o| match o {
            ObjectType::MultiLeaderStyle(x) if x.name.eq_ignore_ascii_case(&s.name) => Some(x),
            _ => None,
        });
        match found {
            Some(x) => {
                if mleader_style(x, names) != *s {
                    put_mleader_style(x, s, style);
                }
            }
            None => {
                let mut x = ao::MultiLeaderStyle::new(&s.name);
                put_mleader_style(&mut x, s, style);
                let dict = named_dict(doc, "ACAD_MLEADERSTYLE");
                x.handle = doc.allocate_handle();
                x.owner_handle = dict;
                let h = x.handle;
                doc.objects.insert(h, ObjectType::MultiLeaderStyle(x));
                enter(doc, dict, &s.name, h);
            }
        }
    }
}
