//! Conversions of the values both models share: points, colours, lineweights, transparency,
//! extended data and the properties every entity has.

use acadrust::entities::EntityCommon;
use acadrust::types::{Color as AColor, Handle as AHandle, LineWeight, Transparency as ATransparency, Vector2, Vector3};
use acadrust::xdata::{ExtendedData, ExtendedDataRecord, XDataValue};
use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{Common, Handle, Lineweight, RawTag, Transparency};
use cadcraft_geom::{Vec2, Vec3};

pub fn v3(v: Vector3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

pub fn v2(v: Vector2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

pub fn a3(v: Vec3) -> Vector3 {
    Vector3::new(v.x, v.y, v.z)
}

pub fn a2(v: Vec2) -> Vector2 {
    Vector2::new(v.x, v.y)
}

pub fn h(h: AHandle) -> Handle {
    Handle(h.value())
}

pub fn ah(h: Handle) -> AHandle {
    AHandle::new(h.0)
}

/// Upper-case key for case-insensitive name lookups.
pub fn up(s: &str) -> String {
    s.to_ascii_uppercase()
}

pub fn color(c: &AColor) -> Color {
    match *c {
        AColor::ByLayer | AColor::None => Color::ByLayer,
        AColor::ByBlock => Color::ByBlock,
        AColor::Index(0) => Color::ByBlock,
        AColor::Index(i) => Color::Index(i),
        AColor::Rgb { r, g, b } => Color::True(Rgb(r, g, b)),
    }
}

pub fn acolor(c: Color) -> AColor {
    match c {
        Color::ByLayer => AColor::ByLayer,
        Color::ByBlock => AColor::ByBlock,
        Color::Index(i) => AColor::Index(i),
        Color::True(Rgb(r, g, b)) => AColor::Rgb { r, g, b },
    }
}

/// An ACI number (DXF group 62 values: 0 ByBlock, 256 ByLayer) as a colour.
pub fn aci(n: i16) -> Color {
    Color::from_aci(n)
}

pub fn lineweight(l: LineWeight) -> Lineweight {
    Lineweight::from_dxf(l.value())
}

pub fn alineweight(l: Lineweight) -> LineWeight {
    LineWeight::from_value(l.to_dxf())
}

/// An entity's transparency: an explicit one as the percentage CADCraft shows (0 to 90), as the
/// DXF reader maps group 440.
pub fn transparency(t: ATransparency) -> Transparency {
    match t {
        ATransparency::ByLayer => Transparency::ByLayer,
        ATransparency::ByBlock => Transparency::ByBlock,
        ATransparency::Explicit(amount) => Transparency::Percent(percent(255 - u32::from(amount))),
    }
}

/// The transparency percentage (0 to 90) of an opacity byte (255 opaque).
pub fn percent(alpha: u32) -> u8 {
    let opaque = (alpha.min(255) * 100 + 127) / 255;
    u8::try_from(100u32.saturating_sub(opaque).min(90)).unwrap_or(90)
}

/// The opacity byte of a transparency percentage, the inverse of [`percent`].
pub fn alpha(percent: u8) -> u8 {
    let opaque = 100u32.saturating_sub(u32::from(percent.min(90)));
    u8::try_from((opaque * 255 + 50) / 100).unwrap_or(255)
}

pub fn atransparency(t: Transparency) -> ATransparency {
    match t {
        Transparency::ByLayer => ATransparency::ByLayer,
        Transparency::ByBlock => ATransparency::ByBlock,
        Transparency::Percent(p) => ATransparency::Explicit(255 - alpha(p)),
    }
}

/// Applications whose extended data CADCraft reads into its own model, kept out of
/// [`Common::xdata`] as the DXF reader keeps them out.
const OWN_APPS: &[&str] = &["CADCRAFT", "ACADANNOTATIVE", "ACCMTRANSPARENCY", "ACAECLAYERSTANDARD"];

/// Every application's extended data as DXF groups (`1001` name, then its values).
pub fn xdata_tags(x: &ExtendedData) -> Vec<RawTag> {
    let mut out = Vec::new();
    for r in x.records() {
        out.push(RawTag { code: 1001, value: r.application_name.clone() });
        for v in &r.values {
            push_value(&mut out, v);
        }
    }
    out
}

/// The extended data an entity keeps in [`Common::xdata`]: other applications' (and `ACAD`'s,
/// except on dimensions, whose `ACAD` data is their style overrides).
pub fn entity_xdata(x: &ExtendedData, dimension: bool) -> Vec<RawTag> {
    let mut out = Vec::new();
    for r in x.records() {
        let app = up(&r.application_name);
        if app.is_empty() || OWN_APPS.contains(&app.as_str()) || (dimension && app == "ACAD") {
            continue;
        }
        out.push(RawTag { code: 1001, value: r.application_name.clone() });
        for v in &r.values {
            push_value(&mut out, v);
        }
    }
    out
}

/// A real as the DXF text of acadrust's writer has it (`0.0`, `1.5`).
fn f(v: f64) -> String {
    format!("{v:?}")
}

fn push_point(out: &mut Vec<RawTag>, code: i32, p: Vector3) {
    out.push(RawTag { code, value: f(p.x) });
    out.push(RawTag { code: code + 10, value: f(p.y) });
    out.push(RawTag { code: code + 20, value: f(p.z) });
}

fn push_value(out: &mut Vec<RawTag>, v: &XDataValue) {
    let mut s = |code: i32, value: String| out.push(RawTag { code, value });
    match v {
        XDataValue::String(t) => s(1000, t.clone()),
        XDataValue::ControlString(t) => s(1002, t.clone()),
        XDataValue::LayerName(t) => s(1003, t.clone()),
        XDataValue::BinaryData(b) => s(1004, b.iter().map(|x| format!("{x:02X}")).collect()),
        XDataValue::Handle(hd) => s(1005, format!("{:X}", hd.value())),
        XDataValue::Real(x) => s(1040, f(*x)),
        XDataValue::Distance(x) => s(1041, f(*x)),
        XDataValue::ScaleFactor(x) => s(1042, f(*x)),
        XDataValue::Integer16(n) => s(1070, n.to_string()),
        XDataValue::Integer32(n) => s(1071, n.to_string()),
        XDataValue::Point3D(p) => push_point(out, 1010, *p),
        XDataValue::Position3D(p) => push_point(out, 1011, *p),
        XDataValue::Displacement3D(p) => push_point(out, 1012, *p),
        XDataValue::Direction3D(p) => push_point(out, 1013, *p),
    }
}

/// DXF extended data groups (from `1001` on) as acadrust's records. Groups that don't parse are
/// left out.
pub fn xdata_from_tags(tags: &[RawTag]) -> ExtendedData {
    let mut x = ExtendedData::new();
    let mut rec: Option<ExtendedDataRecord> = None;
    let num = |t: &RawTag| t.value.trim().parse::<f64>().ok();
    let mut i = 0;
    while let Some(t) = tags.get(i) {
        i += 1;
        if t.code == 1001 {
            if let Some(r) = rec.take() {
                x.add_record(r);
            }
            rec = Some(ExtendedDataRecord::new(t.value.clone()));
            continue;
        }
        let Some(r) = rec.as_mut() else { continue };
        let point = |i: &mut usize| {
            let y = tags.get(*i).and_then(num).unwrap_or(0.0);
            let z = tags.get(*i + 1).and_then(num).unwrap_or(0.0);
            *i += 2;
            Vector3::new(num(t).unwrap_or(0.0), y, z)
        };
        let v = match t.code {
            1000 => XDataValue::String(t.value.clone()),
            1002 => XDataValue::ControlString(t.value.clone()),
            1003 => XDataValue::LayerName(t.value.clone()),
            1004 => XDataValue::BinaryData(
                (0..t.value.len() / 2).filter_map(|k| t.value.get(2 * k..2 * k + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect(),
            ),
            1005 => XDataValue::Handle(AHandle::new(u64::from_str_radix(t.value.trim(), 16).unwrap_or(0))),
            1010 => XDataValue::Point3D(point(&mut i)),
            1011 => XDataValue::Position3D(point(&mut i)),
            1012 => XDataValue::Displacement3D(point(&mut i)),
            1013 => XDataValue::Direction3D(point(&mut i)),
            1040 => XDataValue::Real(num(t).unwrap_or(0.0)),
            1041 => XDataValue::Distance(num(t).unwrap_or(0.0)),
            1042 => XDataValue::ScaleFactor(num(t).unwrap_or(0.0)),
            1070 => XDataValue::Integer16(t.value.trim().parse().unwrap_or(0)),
            1071 => XDataValue::Integer32(t.value.trim().parse().unwrap_or(0)),
            _ => continue,
        };
        r.add_value(v);
    }
    if let Some(r) = rec {
        x.add_record(r);
    }
    x
}

/// The groups of application `app` in DXF extended data groups (after its `1001`).
pub fn xdata_app<'a>(tags: &'a [RawTag], app: &str) -> &'a [RawTag] {
    let Some(start) = tags.iter().position(|t| t.code == 1001 && t.value.trim().eq_ignore_ascii_case(app)) else { return &[] };
    let rest = tags.get(start + 1..).unwrap_or(&[]);
    let end = rest.iter().position(|t| t.code == 1001).unwrap_or(rest.len());
    rest.get(..end).unwrap_or(&[])
}

/// The groups inside `1000 <name>` `1002 {` … `1002 }` of one application's groups.
pub fn xdata_list<'a>(x: &'a [RawTag], name: &str) -> &'a [RawTag] {
    let Some(at) = x.iter().position(|t| t.code == 1000 && t.value.trim().eq_ignore_ascii_case(name)) else { return &[] };
    if !x.get(at + 1).is_some_and(|t| t.code == 1002 && t.value.trim() == "{") {
        return &[];
    }
    let body = x.get(at + 2..).unwrap_or(&[]);
    let mut depth = 0usize;
    for (i, t) in body.iter().enumerate() {
        if t.code == 1002 {
            match t.value.trim() {
                "{" => depth += 1,
                "}" if depth == 0 => return body.get(..i).unwrap_or(&[]),
                "}" => depth -= 1,
                _ => {}
            }
        }
    }
    body
}

/// The properties every entity has, from acadrust's, with the entity's own thickness and
/// extrusion (`normal`).
pub fn common(c: &EntityCommon, thickness: f64, normal: Vector3, dimension: bool) -> Common {
    Common {
        layer: if c.layer.is_empty() { "0".into() } else { c.layer.clone() },
        color: color(&c.color),
        linetype: if c.linetype.is_empty() { "ByLayer".into() } else { c.linetype.clone() },
        lineweight: lineweight(c.line_weight),
        ltscale: if c.linetype_scale > 0.0 && c.linetype_scale.is_finite() { c.linetype_scale } else { 1.0 },
        transparency: transparency(c.transparency),
        visible: !c.invisible,
        thickness,
        extrusion: normal3(normal),
        xdata: entity_xdata(&c.extended_data, dimension),
        ..Common::default()
    }
}

/// An extrusion direction, (0,0,1) for a zero one.
pub fn normal3(n: Vector3) -> Vec3 {
    let v = v3(n);
    if v.x == 0.0 && v.y == 0.0 && v.z == 0.0 { Vec3::Z } else { v }
}

/// Write CADCraft's common properties onto acadrust's (what CADCraft doesn't model stays).
pub fn put_common(dst: &mut EntityCommon, c: &Common) {
    if dst.layer != c.layer {
        dst.layer.clone_from(&c.layer);
        dst.layer_handle = None;
    }
    let col = acolor(c.color);
    if dst.color != col {
        dst.color = col;
        dst.color_name = None;
        dst.color_book_handle = None;
    }
    let lt = if c.linetype.eq_ignore_ascii_case("ByLayer") { String::new() } else { c.linetype.clone() };
    if dst.linetype != lt {
        dst.linetype = lt;
        dst.linetype_handle = None;
    }
    dst.line_weight = alineweight(c.lineweight);
    dst.linetype_scale = c.ltscale;
    dst.transparency = atransparency(c.transparency);
    dst.invisible = !c.visible;
}

/// The extended data of a new entity: the drawing's own groups (other applications').
pub fn put_xdata(dst: &mut EntityCommon, c: &Common) {
    if !c.xdata.is_empty() {
        dst.extended_data = xdata_from_tags(&c.xdata);
    }
}
