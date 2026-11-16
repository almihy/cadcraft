//! Symbol tables both ways: layers, linetypes, text styles and dimension styles, and a
//! dimension's own style overrides (the `ACAD` "DSTYLE" extended data).

use acadrust::tables::{self as at};
use acadrust::types::{Color as AColor, Handle as AHandle};
use acadrust::xdata::{ExtendedData, ExtendedDataRecord, XDataValue};
use cadcraft_doc::dimcodes::{DIM_CODES, K};
use cadcraft_doc::{DashElement, DimStyle, Layer, Linetype, TextStyle};
use serde_json::{Map, Value, json};

use crate::conv::{acolor, alineweight, color, lineweight, percent, up};
use crate::entity_read::Names;

// ── Layers ───────────────────────────────────────────────────────────────────────────────────

pub fn layer(l: &at::Layer) -> Layer {
    let mut out = Layer {
        name: l.name.clone(),
        color: match l.color {
            AColor::Index(i) => cadcraft_color::Color::Index(i.max(1)),
            AColor::Rgb { .. } => color(&l.color),
            _ => cadcraft_color::Color::Index(7),
        },
        linetype: if l.line_type.is_empty() { "Continuous".into() } else { l.line_type.clone() },
        lineweight: lineweight(l.line_weight),
        on: !l.flags.off,
        frozen: l.flags.frozen,
        locked: l.flags.locked,
        vp_freeze_new: l.flags.frozen_in_new_viewport,
        plot: l.is_plottable,
        description: l.description.clone(),
        ..Layer::default()
    };
    if let acadrust::types::Transparency::Explicit(t) = l.transparency {
        out.transparency = percent(255 - u32::from(t));
    }
    out
}

pub fn put_layer(dst: &mut at::Layer, l: &Layer) {
    dst.name.clone_from(&l.name);
    let c = acolor(l.color);
    if dst.color != c {
        dst.color = c;
        dst.color_name = None;
        dst.book_name = None;
    }
    dst.line_type.clone_from(&l.linetype);
    dst.line_weight = alineweight(l.lineweight);
    dst.flags.off = !l.on;
    dst.flags.frozen = l.frozen;
    dst.flags.locked = l.locked;
    dst.flags.frozen_in_new_viewport = l.vp_freeze_new;
    dst.is_plottable = l.plot;
    dst.description.clone_from(&l.description);
    dst.transparency = acadrust::types::Transparency::Explicit(255 - crate::conv::alpha(l.transparency));
}

// ── Linetypes ────────────────────────────────────────────────────────────────────────────────

pub fn linetype(l: &at::LineType, names: &Names, shape_files: &std::collections::HashMap<u64, String>) -> Linetype {
    let pattern = l
        .elements
        .iter()
        .map(|e| {
            let mut d = DashElement::dash(e.length);
            if let Some(c) = &e.complex {
                match &c.content {
                    at::LineTypeComplexContent::Text { text } => d.text = Some(text.clone()),
                    at::LineTypeComplexContent::Shape { shape_number } => d.shape = Some(u16::try_from(*shape_number).unwrap_or(0)),
                }
                d.absolute = c.absolute_rotation;
                d.style = names.styles.get(&c.style_handle.value()).filter(|n| !n.trim().is_empty()).cloned().or_else(|| shape_files.get(&c.style_handle.value()).cloned());
                if c.scale.is_finite() {
                    d.scale = c.scale;
                }
                if c.rotation.is_finite() {
                    d.rotation = c.rotation.to_degrees();
                }
                d.offset = cadcraft_geom::Vec2::new(c.offset[0], c.offset[1]);
            }
            d
        })
        .collect();
    Linetype { name: l.name.clone(), description: l.description.clone(), pattern }
}

pub fn put_linetype(dst: &mut at::LineType, l: &Linetype, style: &impl Fn(&str) -> Option<AHandle>) {
    dst.name.clone_from(&l.name);
    dst.description.clone_from(&l.description);
    dst.elements = l
        .pattern
        .iter()
        .map(|d| at::LineTypeElement {
            length: d.length,
            complex: if d.text.is_some() || d.shape.is_some() {
                Some(at::LineTypeComplexData {
                    content: match (&d.text, d.shape) {
                        (Some(t), _) => at::LineTypeComplexContent::Text { text: t.clone() },
                        (None, s) => at::LineTypeComplexContent::Shape { shape_number: i16::try_from(s.unwrap_or(0)).unwrap_or(0) },
                    },
                    style_handle: d.style.as_deref().and_then(style).unwrap_or(AHandle::NULL),
                    scale: d.scale,
                    rotation: d.rotation.to_radians(),
                    absolute_rotation: d.absolute,
                    upright: false,
                    source_flags: 0,
                    offset: [d.offset.x, d.offset.y],
                })
            } else {
                None
            },
        })
        .collect();
    dst.pattern_length = l.pattern.iter().map(|d| d.length.abs()).sum();
}

// ── Text styles ──────────────────────────────────────────────────────────────────────────────

pub fn text_style(s: &at::TextStyle, xdata: Option<&ExtendedData>) -> TextStyle {
    let file = s.font_file.clone();
    let font = if file.trim().is_empty() && !s.true_type_font.trim().is_empty() {
        s.true_type_font.trim().to_string()
    } else {
        // CADCraft's own font name, kept while the file is the `txt` or Windows font written
        // for other readers (as the DXF reader takes it).
        let own = xdata.map(crate::conv::xdata_tags).and_then(|x| {
            let x = crate::conv::xdata_app(&x, "CADCRAFT").to_vec();
            x.windows(2).find_map(|w| match w {
                [m, n] if m.code == 1000 && m.value == "FONT" && n.code == 1000 => Some(n.value.clone()),
                _ => None,
            })
        });
        let stem = file.trim().to_ascii_lowercase();
        let base = stem.rsplit(['/', '\\']).next().unwrap_or_default().to_string();
        match own {
            Some(name) if !name.trim().is_empty() && (stem == "txt" || stem == "txt.shx" || cadcraft_fonts::windows::windows_font(&name) == Some(base.as_str())) => {
                name
            }
            _ => file,
        }
    };
    TextStyle {
        name: s.name.clone(),
        font,
        big_font: s.big_font_file.clone(),
        height: s.height,
        width_factor: s.width_factor,
        oblique: s.oblique_angle,
        backwards: s.flags.backward,
        upside_down: s.flags.upside_down,
        vertical: s.is_vertical,
        annotative: s.annotative,
    }
}

pub fn put_text_style(dst: &mut at::TextStyle, s: &TextStyle) {
    dst.name.clone_from(&s.name);
    let lower = s.font.to_ascii_lowercase();
    if lower.ends_with(".shx") || lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".ttc") || lower == "txt" {
        dst.font_file.clone_from(&s.font);
        dst.true_type_font.clear();
    } else if !s.font.eq_ignore_ascii_case(&dst.true_type_font) && !s.font.eq_ignore_ascii_case(&dst.font_file) {
        // A family name (how AutoCAD stores a TrueType style without a file).
        dst.font_file.clear();
        dst.true_type_font.clone_from(&s.font);
    }
    dst.big_font_file.clone_from(&s.big_font);
    dst.height = s.height;
    dst.width_factor = s.width_factor;
    dst.oblique_angle = s.oblique;
    dst.flags.backward = s.backwards;
    dst.flags.upside_down = s.upside_down;
    dst.is_vertical = s.vertical;
    dst.annotative = s.annotative;
}

// ── Dimension styles ─────────────────────────────────────────────────────────────────────────

/// A dimension variable's value in the style's JSON form, by its [`K`].
fn dim_json(kind: K, v: &XVal, names: &Names) -> Option<Value> {
    Some(match (kind, v) {
        (K::Real, XVal::Real(x)) => json!(x),
        (K::Real, XVal::Int(x)) => json!(*x as f64),
        (K::Int, XVal::Int(x)) => json!(x),
        (K::Bool, XVal::Int(x)) => Value::Bool(*x != 0),
        (K::Str, XVal::Str(s)) => Value::String(s.clone()),
        (K::Color, XVal::Int(x)) => serde_json::to_value(crate::conv::aci(i16::try_from(*x).ok()?)).ok()?,
        (K::Char, XVal::Int(x)) => Value::String(char::from_u32(u32::try_from(*x).ok()?).filter(|c| !c.is_control())?.to_string()),
        (K::TextStyle, XVal::Handle(h)) => Value::String(names.styles.get(h)?.clone()),
        (K::Block, XVal::Handle(h)) => Value::String(if *h == 0 { String::new() } else { names.blocks.get(h)?.clone() }),
        (K::Linetype, XVal::Handle(h)) => Value::String(if *h == 0 { String::new() } else { names.ltypes.get(h)?.clone() }),
        (K::Angle, XVal::Real(x)) => json!((x.to_degrees() * 1e9).round() / 1e9),
        (K::Str, XVal::Name(s)) | (K::TextStyle | K::Block | K::Linetype, XVal::Name(s)) => Value::String(s.clone()),
        _ => return None,
    })
}

/// A dimension variable as a file holds it.
#[derive(Clone, Debug, PartialEq)]
enum XVal {
    Real(f64),
    Int(i64),
    Str(String),
    Handle(u64),
    /// A name where the file holds a handle: what acadrust resolved it to.
    Name(String),
}

/// The style's variables by their DXF group codes (see [`DIM_CODES`]).
fn style_values(s: &at::DimStyle) -> Vec<(i32, XVal)> {
    use XVal::{Handle as H, Int as I, Name as N, Real as R, Str as S};
    let b = |v: bool| I(i64::from(v));
    let i = |v: i16| I(i64::from(v));
    let col = |aci: i16, tc: &Option<AColor>| I(i64::from(tc.as_ref().and_then(|c| c.index()).map_or(aci, |x| i16::try_from(x).unwrap_or(aci))));
    let blk = |name: &str, hd: AHandle| if !name.is_empty() { N(name.to_string()) } else { H(hd.value()) };
    vec![
        (3, S(s.dimpost.clone())),
        (4, S(s.dimapost.clone())),
        (40, R(s.dimscale)),
        (41, R(s.dimasz)),
        (42, R(s.dimexo)),
        (43, R(s.dimdli)),
        (44, R(s.dimexe)),
        (45, R(s.dimrnd)),
        (46, R(s.dimdle)),
        (47, R(s.dimtp)),
        (48, R(s.dimtm)),
        (140, R(s.dimtxt)),
        (141, R(s.dimcen)),
        (142, R(s.dimtsz)),
        (143, R(s.dimaltf)),
        (144, R(s.dimlfac)),
        (146, R(s.dimtfac)),
        (147, R(s.dimgap)),
        (148, R(s.dimaltrnd)),
        (71, b(s.dimtol)),
        (72, b(s.dimlim)),
        (73, b(s.dimtih)),
        (74, b(s.dimtoh)),
        (75, b(s.dimse1)),
        (76, b(s.dimse2)),
        (77, i(s.dimtad)),
        (78, i(s.dimzin)),
        (170, b(s.dimalt)),
        (171, i(s.dimaltd)),
        (176, col(s.dimclrd, &s.dimclrd_true_color)),
        (177, col(s.dimclre, &s.dimclre_true_color)),
        (178, col(s.dimclrt, &s.dimclrt_true_color)),
        (179, i(s.dimadec)),
        (271, i(s.dimdec)),
        (272, i(s.dimtdec)),
        (273, i(s.dimaltu)),
        (275, i(s.dimaunit)),
        (276, i(s.dimfrac)),
        (277, i(s.dimlunit)),
        (278, i(s.dimdsep)),
        (280, i(s.dimjust)),
        (284, i(s.dimtzin)),
        (285, i(s.dimaltz)),
        (340, if s.dimtxsty.is_empty() { H(s.dimtxsty_handle.value()) } else { N(s.dimtxsty.clone()) }),
        (342, blk(&s.dimblk_name, s.dimblk)),
        (343, blk(&s.dimblk1_name, s.dimblk1)),
        (344, blk(&s.dimblk2_name, s.dimblk2)),
        (49, R(s.dimfxl)),
        (50, R(s.dimjogang)),
        (69, i(s.dimtfill)),
        (70, col(s.dimtfillclr, &s.dimtfillclr_true_color)),
        (79, i(s.dimazin)),
        (90, i(s.dimarcsym)),
        (145, R(s.dimtvp)),
        (172, b(s.dimtofl)),
        (174, b(s.dimtix)),
        (175, b(s.dimsoxd)),
        (279, i(s.dimtmove)),
        (281, b(s.dimsd1)),
        (282, b(s.dimsd2)),
        (283, i(s.dimtolj)),
        (289, i(s.dimatfit)),
        (290, b(s.dimfxlon)),
        (341, H(s.dimldrblk.value())),
        (345, H(s.dimltex_handle.value())),
        (346, H(s.dimltex1_handle.value())),
        (347, H(s.dimltex2_handle.value())),
        (371, i(s.dimlwd)),
        (372, i(s.dimlwe)),
    ]
}

/// The variables (by field) of `values`, those the style can take.
fn fields_of(values: impl IntoIterator<Item = (i32, XVal)>, names: &Names) -> Map<String, Value> {
    let mut m = Map::new();
    for (code, v) in values {
        let Some((field, kind)) = DIM_CODES.iter().find(|(_, c, _)| *c == code).map(|(f, _, k)| (*f, *k)) else { continue };
        if let Some(j) = dim_json(kind, &v, names) {
            m.insert(field.to_string(), j);
        }
    }
    m
}

pub fn dim_style(s: &at::DimStyle, names: &Names) -> DimStyle {
    let mut st = DimStyle { name: s.name.clone(), annotative: s.annotative, ..DimStyle::default() };
    st.apply_fields(&fields_of(style_values(s), names));
    st
}

/// Write a dimension style's variables onto acadrust's record.
pub fn put_dim_style(dst: &mut at::DimStyle, st: &DimStyle, lookups: &Lookups) {
    dst.name.clone_from(&st.name);
    dst.annotative = st.annotative;
    let v = serde_json::to_value(st).unwrap_or_default();
    let get = |f: &str| v.get(f);
    let r = |f: &str, d: f64| get(f).and_then(Value::as_f64).unwrap_or(d);
    let n = |f: &str, d: i16| get(f).and_then(Value::as_i64).and_then(|x| i16::try_from(x).ok()).unwrap_or(d);
    let b = |f: &str, d: bool| get(f).and_then(Value::as_bool).unwrap_or(d);
    let s = |f: &str| get(f).and_then(Value::as_str).unwrap_or_default().to_string();
    let c = |f: &str| get(f).and_then(|x| serde_json::from_value::<cadcraft_color::Color>(x.clone()).ok());
    macro_rules! real {
        ($($field:ident = $name:literal),*) => { $( dst.$field = r($name, dst.$field); )* };
    }
    macro_rules! int {
        ($($field:ident = $name:literal),*) => { $( dst.$field = n($name, dst.$field); )* };
    }
    macro_rules! flag {
        ($($field:ident = $name:literal),*) => { $( dst.$field = b($name, dst.$field); )* };
    }
    real!(
        dimscale = "scale", dimasz = "arrowSize", dimexo = "extOffset", dimdli = "baselineSpacing", dimexe = "extExtend", dimrnd = "round",
        dimdle = "dimLineExtend", dimtp = "tolPlus", dimtm = "tolMinus", dimtxt = "textHeight", dimcen = "centerMark", dimtsz = "tickSize",
        dimaltf = "altFactor", dimlfac = "linearFactor", dimtfac = "tolScale", dimgap = "textGap", dimaltrnd = "altRound", dimfxl = "fixedExtLength",
        dimtvp = "textVerticalPos"
    );
    int!(
        dimtad = "textAbove", dimzin = "zeroSuppression", dimaltd = "altDecimals", dimadec = "angularDecimals", dimdec = "decimals",
        dimtdec = "tolDecimals", dimaltu = "altUnit", dimaunit = "angularUnit", dimfrac = "fractionFormat", dimlunit = "linearUnit",
        dimjust = "textJust", dimtzin = "tolZeroSuppression", dimaltz = "altZeroSuppression", dimtfill = "textFill", dimazin = "angularZeroSuppression",
        dimarcsym = "arcSymbol", dimtmove = "textMove", dimtolj = "tolJust", dimatfit = "fit", dimlwd = "dimLineweight", dimlwe = "extLineweight"
    );
    flag!(
        dimtol = "tolerance", dimlim = "limits", dimtih = "textInsideHorizontal", dimtoh = "textOutsideHorizontal", dimse1 = "suppressExt1",
        dimse2 = "suppressExt2", dimalt = "alt", dimtofl = "forceLineInside", dimtix = "textInside", dimsoxd = "suppressOutsideArrows",
        dimsd1 = "suppressDim1", dimsd2 = "suppressDim2", dimfxlon = "fixedExt"
    );
    dst.dimpost = s("post");
    dst.dimapost = s("altPost");
    if let Some(a) = get("jogAngle").and_then(Value::as_f64) {
        dst.dimjogang = a.to_radians();
    }
    if let Some(ch) = s("decimalSeparator").chars().next() {
        dst.dimdsep = i16::try_from(u32::from(ch)).unwrap_or(46);
    }
    let put_color = |aci: &mut i16, tc: &mut Option<AColor>, f: &str| {
        if let Some(col) = c(f) {
            *aci = col.to_aci();
            *tc = matches!(col, cadcraft_color::Color::True(_)).then(|| acolor(col));
        }
    };
    put_color(&mut dst.dimclrd, &mut dst.dimclrd_true_color, "dimLineColor");
    put_color(&mut dst.dimclre, &mut dst.dimclre_true_color, "extLineColor");
    put_color(&mut dst.dimclrt, &mut dst.dimclrt_true_color, "textColor");
    put_color(&mut dst.dimtfillclr, &mut dst.dimtfillclr_true_color, "textFillColor");
    let style = s("textStyle");
    if !style.is_empty() {
        dst.dimtxsty_handle = (lookups.style)(&style).unwrap_or(dst.dimtxsty_handle);
        dst.dimtxsty = style;
    }
    let block = |name: String, handle: &mut AHandle, keep: &mut String| {
        *handle = if name.is_empty() { AHandle::NULL } else { (lookups.block)(&name).unwrap_or(AHandle::NULL) };
        *keep = name;
    };
    block(s("arrowBlock"), &mut dst.dimblk, &mut dst.dimblk_name);
    block(s("arrowBlock1"), &mut dst.dimblk1, &mut dst.dimblk1_name);
    block(s("arrowBlock2"), &mut dst.dimblk2, &mut dst.dimblk2_name);
    let leader = s("leaderArrowBlock");
    dst.dimldrblk = if leader.is_empty() { AHandle::NULL } else { (lookups.block)(&leader).unwrap_or(AHandle::NULL) };
    let lt = |f: &str| {
        let name = s(f);
        if name.is_empty() { AHandle::NULL } else { (lookups.ltype)(&name).unwrap_or(AHandle::NULL) }
    };
    dst.dimltex_handle = lt("dimLinetype");
    dst.dimltex1_handle = lt("extLinetype1");
    dst.dimltex2_handle = lt("extLinetype2");
    dst.dimsah = !st.arrow_block1.is_empty() || !st.arrow_block2.is_empty();
}

/// Name → handle lookups of the writer.
pub struct Lookups<'a> {
    pub style: &'a dyn Fn(&str) -> Option<AHandle>,
    pub block: &'a dyn Fn(&str) -> Option<AHandle>,
    pub ltype: &'a dyn Fn(&str) -> Option<AHandle>,
}

/// A dimension's style overrides from its `ACAD` "DSTYLE" extended data: `1070 <code>` then the
/// value, as the DXF reader takes them.
pub fn dstyle_overrides(x: &ExtendedData, names: &Names) -> Map<String, Value> {
    let Some(rec) = x.records().iter().find(|r| r.application_name.eq_ignore_ascii_case("ACAD")) else { return Map::new() };
    let vals = &rec.values;
    let Some(at) = vals.iter().position(|v| matches!(v, XDataValue::String(s) if s.trim().eq_ignore_ascii_case("DSTYLE"))) else {
        return Map::new();
    };
    let mut pairs = Vec::new();
    let mut it = vals.iter().skip(at + 2).take(4096 * 2);
    while let Some(v) = it.next() {
        match v {
            XDataValue::ControlString(s) if s.trim() == "}" => break,
            XDataValue::Integer16(code) => {
                let Some(val) = it.next() else { break };
                let xv = match val {
                    XDataValue::Real(r) | XDataValue::Distance(r) | XDataValue::ScaleFactor(r) => XVal::Real(*r),
                    XDataValue::Integer16(n) => XVal::Int(i64::from(*n)),
                    XDataValue::Integer32(n) => XVal::Int(i64::from(*n)),
                    XDataValue::String(s) => XVal::Str(s.clone()),
                    XDataValue::Handle(h) => XVal::Handle(h.value()),
                    _ => continue,
                };
                pairs.push((i32::from(*code), xv));
            }
            _ => {}
        }
    }
    let fields = fields_of(pairs, names);
    let mut out = Map::new();
    for (field, v) in fields {
        if let Some(c) = canonical(&field, &v) {
            out.insert(field, c);
        }
    }
    out
}

/// The value as a serialised [`DimStyle`] holds it, or `None` when the field can't take it.
fn canonical(field: &str, v: &Value) -> Option<Value> {
    let field = DimStyle::field_name(field)?;
    let mut st = DimStyle::default();
    let mut m = Map::new();
    m.insert(field.to_string(), v.clone());
    if !st.apply_fields(&m).is_empty() {
        return None;
    }
    serde_json::to_value(&st).ok()?.get(field).cloned()
}

/// A dimension's overrides as `ACAD` "DSTYLE" extended data, replacing the record's old ones;
/// other applications' data and the rest of `ACAD`'s stay.
pub fn put_dstyle(x: &mut ExtendedData, overrides: &Map<String, Value>, lookups: &Lookups) {
    let mut values = Vec::new();
    for (field, v) in overrides {
        let Some((code, kind)) = DIM_CODES.iter().find(|(f, _, _)| f == field).map(|(_, c, k)| (*c, *k)) else { continue };
        let val = match kind {
            K::Real => v.as_f64().filter(|x| x.is_finite()).map(XDataValue::Real),
            K::Int => v.as_i64().and_then(|n| i16::try_from(n).ok()).map(XDataValue::Integer16),
            K::Bool => v.as_bool().map(|b| XDataValue::Integer16(i16::from(b))),
            K::Str => v.as_str().map(|s| XDataValue::String(s.to_string())),
            K::Color => serde_json::from_value::<cadcraft_color::Color>(v.clone()).ok().map(|c| XDataValue::Integer16(c.to_aci())),
            K::Char => v.as_str().and_then(|s| s.chars().next()).and_then(|c| i16::try_from(u32::from(c)).ok()).map(XDataValue::Integer16),
            K::TextStyle => v.as_str().and_then(|n| (lookups.style)(n)).map(XDataValue::Handle),
            K::Block => v.as_str().map(|n| XDataValue::Handle(if n.is_empty() { AHandle::NULL } else { (lookups.block)(n).unwrap_or(AHandle::NULL) })),
            K::Linetype => v.as_str().filter(|n| !n.is_empty()).and_then(|n| (lookups.ltype)(n)).map(XDataValue::Handle),
            K::Angle => v.as_f64().filter(|x| x.is_finite()).map(|d| XDataValue::Real(d.to_radians())),
        };
        if let Some(val) = val {
            values.push(XDataValue::Integer16(i16::try_from(code).unwrap_or(0)));
            values.push(val);
        }
    }
    // The ACAD record without its old DSTYLE list.
    let mut acad = x.get_record("ACAD").cloned().unwrap_or_else(|| ExtendedDataRecord::new("ACAD"));
    if let Some(at) = acad.values.iter().position(|v| matches!(v, XDataValue::String(s) if s.trim().eq_ignore_ascii_case("DSTYLE"))) {
        let mut end = at + 1;
        let mut depth = 0i32;
        while let Some(v) = acad.values.get(end) {
            end += 1;
            match v {
                XDataValue::ControlString(s) if s.trim() == "{" => depth += 1,
                XDataValue::ControlString(s) if s.trim() == "}" => {
                    depth -= 1;
                    if depth <= 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
        acad.values.drain(at..end.min(acad.values.len()));
    }
    if !values.is_empty() {
        acad.values.push(XDataValue::String("DSTYLE".into()));
        acad.values.push(XDataValue::ControlString("{".into()));
        acad.values.extend(values);
        acad.values.push(XDataValue::ControlString("}".into()));
    }
    if acad.values.is_empty() {
        x.remove_record("ACAD");
    } else {
        x.upsert_record(acad);
    }
}

/// Upper-case name → handle of a table's records.
pub fn handles<'a, I: Iterator<Item = (&'a str, AHandle)>>(records: I) -> std::collections::HashMap<String, AHandle> {
    records.map(|(n, h)| (up(n), h)).collect()
}
