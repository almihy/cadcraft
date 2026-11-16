//! CADCraft's entities written onto acadrust's: the inverse of `entity_read`. An entity the file
//! had is written onto its original object (what CADCraft doesn't model stays); a new one is made
//! from acadrust's defaults.

use std::collections::HashMap;

use acadrust::entities::{self as ae, Dimension as ADim, EntityType};
use acadrust::types::{Handle as AHandle, Vector2, Vector3};
use cadcraft_doc::ocs::to_ocs;
use cadcraft_doc::{DimKind, Entity, EntityKind, HAlign, VAlign};
use cadcraft_geom::{Vec2, Vec3};

use crate::conv::{a2, a3, acolor, ah, alineweight, put_common, put_xdata, up};
use crate::tables::Lookups;

/// Name lookups and a handle allocator for the writer.
pub struct Cx<'a> {
    pub lookups: Lookups<'a>,
    pub layers: &'a HashMap<String, AHandle>,
    pub alloc: &'a mut dyn FnMut() -> AHandle,
}

fn z() -> Vector3 {
    Vector3::new(0.0, 0.0, 1.0)
}

/// `e` as acadrust's entity: written onto `prev` (the file's object of the same handle) when it
/// is of a kind that can take it, else made new keeping `prev`'s identity (owner, extension
/// dictionary, reactors, extended data). `None` when CADCraft can't write the kind (an
/// [`EntityKind::Unknown`] it didn't read from the file).
pub fn build(e: &Entity, prev: Option<&EntityType>, cx: &mut Cx) -> Option<EntityType> {
    let c = &e.common;
    let n = c.extrusion;
    let same = |pick: fn(&EntityType) -> bool| prev.filter(|p| pick(p)).cloned();
    let mut out = match &e.kind {
        EntityKind::Unknown(_) => return prev.cloned(),
        EntityKind::Line(l) => {
            let mut x = match same(|p| matches!(p, EntityType::Line(_))) { Some(EntityType::Line(x)) => x, _ => ae::Line::default() };
            x.start = a3(l.a);
            x.end = a3(l.b);
            x.thickness = c.thickness;
            x.normal = a3(n);
            EntityType::Line(x)
        }
        EntityKind::Point(p) => {
            let mut x = match same(|p| matches!(p, EntityType::Point(_))) { Some(EntityType::Point(x)) => x, _ => ae::Point::default() };
            x.location = a3(p.p);
            x.x_axis_angle = p.angle;
            x.thickness = c.thickness;
            x.normal = a3(n);
            EntityType::Point(x)
        }
        EntityKind::Circle(ci) => {
            let mut x = match same(|p| matches!(p, EntityType::Circle(_))) { Some(EntityType::Circle(x)) => x, _ => ae::Circle::default() };
            x.center = a3(to_ocs(n, ci.center));
            x.radius = ci.radius;
            x.thickness = c.thickness;
            x.normal = a3(n);
            EntityType::Circle(x)
        }
        EntityKind::Arc(a) => {
            let mut x = match same(|p| matches!(p, EntityType::Arc(_))) { Some(EntityType::Arc(x)) => x, _ => ae::Arc::default() };
            x.center = a3(to_ocs(n, a.center));
            x.radius = a.radius;
            (x.start_angle, x.end_angle) = if n.z < 0.0 {
                (cadcraft_geom::norm_angle(std::f64::consts::PI - a.end), cadcraft_geom::norm_angle(std::f64::consts::PI - a.start))
            } else {
                (a.start, a.end)
            };
            x.thickness = c.thickness;
            x.normal = a3(n);
            EntityType::Arc(x)
        }
        EntityKind::Ellipse(el) => {
            let mut x = match same(|p| matches!(p, EntityType::Ellipse(_))) { Some(EntityType::Ellipse(x)) => x, _ => ae::Ellipse::default() };
            x.center = a3(el.center);
            x.major_axis = a3(el.major);
            x.minor_axis_ratio = el.ratio;
            let full = (cadcraft_geom::ccw_sweep(el.start, el.end) - std::f64::consts::TAU).abs() <= 1e-9;
            (x.start_parameter, x.end_parameter) = if n.z < 0.0 && !full {
                (cadcraft_geom::norm_angle(-el.end), cadcraft_geom::norm_angle(-el.start))
            } else {
                (el.start, el.end)
            };
            x.normal = a3(n);
            EntityType::Ellipse(x)
        }
        EntityKind::LwPolyline(p) => {
            let mut x =
                match same(|p| matches!(p, EntityType::LwPolyline(_))) { Some(EntityType::LwPolyline(x)) => x, _ => ae::LwPolyline::default() };
            let m = if n.z < 0.0 { -1.0 } else { 1.0 };
            x.vertices = p
                .vertices
                .iter()
                .map(|v| ae::LwVertex {
                    location: Vector2::new(m * v.p.x, v.p.y),
                    bulge: m * v.bulge,
                    start_width: v.start_width,
                    end_width: v.end_width,
                    vertex_id: 0,
                })
                .collect();
            x.is_closed = p.closed;
            x.plinegen = p.plinegen;
            x.constant_width = p.const_width;
            x.elevation = p.elevation;
            x.thickness = c.thickness;
            x.normal = a3(n);
            EntityType::LwPolyline(x)
        }
        EntityKind::Polyline3d(p) => {
            let mut x =
                match same(|p| matches!(p, EntityType::Polyline3D(_))) { Some(EntityType::Polyline3D(x)) => x, _ => ae::Polyline3D::default() };
            // The file's vertices keep their handles where there are as many.
            let old: Vec<AHandle> = x.vertices.iter().map(|v| v.handle).collect();
            let layer = c.layer.clone();
            x.vertices = p
                .points
                .iter()
                .enumerate()
                .map(|(i, q)| {
                    let handle = old.get(i).copied().filter(|h| !h.is_null()).unwrap_or_else(|| (cx.alloc)());
                    ae::Vertex3DPolyline { handle, layer: layer.clone(), position: a3(*q), flags: 32 }
                })
                .collect();
            x.flags.closed = p.closed;
            x.flags.is_3d = true;
            EntityType::Polyline3D(x)
        }
        EntityKind::PolyfaceMesh(m) => {
            let mut x =
                match same(|p| matches!(p, EntityType::PolyfaceMesh(_))) { Some(EntityType::PolyfaceMesh(x)) => x, _ => ae::PolyfaceMesh::default() };
            let mut verts = std::mem::take(&mut x.vertices);
            verts.resize_with(m.vertices.len(), ae::PolyfaceVertex::default);
            for (v, q) in verts.iter_mut().zip(&m.vertices) {
                v.location = a3(*q);
                v.common.raw_record = None;
                if v.common.handle.is_null() {
                    v.common.handle = (cx.alloc)();
                }
            }
            x.vertices = verts;
            let mut faces = std::mem::take(&mut x.faces);
            faces.resize_with(m.faces.len(), ae::PolyfaceFace::default);
            for (f, q) in faces.iter_mut().zip(&m.faces) {
                let ix = |i: i32| i16::try_from(i).unwrap_or(0);
                (f.index1, f.index2, f.index3, f.index4) = (ix(q[0]), ix(q[1]), ix(q[2]), ix(q[3]));
                f.common.raw_record = None;
                if f.common.handle.is_null() {
                    f.common.handle = (cx.alloc)();
                }
            }
            x.faces = faces;
            EntityType::PolyfaceMesh(x)
        }
        EntityKind::Spline(s) => {
            let mut x = match same(|p| matches!(p, EntityType::Spline(_))) { Some(EntityType::Spline(x)) => x, _ => ae::Spline::default() };
            x.degree = i32::try_from(s.degree).unwrap_or(3);
            x.knots = s.knots.clone();
            x.control_points = s.control.iter().map(|p| Vector3::new(p.x, p.y, 0.0)).collect();
            x.weights = if s.weights.len() == s.control.len() { s.weights.clone() } else { vec![1.0; s.control.len()] };
            x.flags.rational = s.weights.iter().any(|w| (*w - 1.0).abs() > 1e-12);
            x.fit_points = s.fit.iter().map(|p| Vector3::new(p.x, p.y, 0.0)).collect();
            x.flags.closed = s.closed;
            let t = |v: Option<Vec2>| v.map_or(Vector3::new(0.0, 0.0, 0.0), |v| Vector3::new(v.x, v.y, 0.0));
            x.begin_tangent = t(s.fit_opts.start_tangent);
            x.end_tangent = t(s.fit_opts.end_tangent);
            x.fit_tolerance = s.fit_opts.tolerance;
            x.normal = a3(n);
            EntityType::Spline(x)
        }
        EntityKind::Ray(r) => {
            let mut x = match same(|p| matches!(p, EntityType::Ray(_))) { Some(EntityType::Ray(x)) => x, _ => ae::Ray::default() };
            x.base_point = a3(r.base);
            x.direction = a3(r.dir);
            EntityType::Ray(x)
        }
        EntityKind::XLine(r) => {
            let mut x = match same(|p| matches!(p, EntityType::XLine(_))) { Some(EntityType::XLine(x)) => x, _ => ae::XLine::default() };
            x.base_point = a3(r.base);
            x.direction = a3(r.dir);
            EntityType::XLine(x)
        }
        EntityKind::Text(t) => {
            let mut x = match same(|p| matches!(p, EntityType::Text(_))) { Some(EntityType::Text(x)) => x, _ => ae::Text::default() };
            // Written as drawn: in the WCS, readable (a mirrored file text is kept the way
            // CADCraft shows it).
            x.insertion_point = a3(t.insert);
            x.alignment_point = t.align_pt.map(a3).or_else(|| (t.halign != HAlign::Left || t.valign != VAlign::Baseline).then(|| a3(t.insert)));
            x.height = t.height;
            x.value.clone_from(&t.value);
            x.rotation = t.rotation;
            x.width_factor = t.width_factor;
            x.oblique_angle = t.oblique;
            x.style.clone_from(&t.style);
            x.horizontal_alignment = halign_t(t.halign);
            x.vertical_alignment = valign_t(t.valign);
            x.thickness = c.thickness;
            x.normal = z();
            EntityType::Text(x)
        }
        EntityKind::MText(t) => {
            let mut x = match same(|p| matches!(p, EntityType::MText(_))) { Some(EntityType::MText(x)) => *x, _ => ae::MText::default() };
            put_mtext(&mut x, t);
            EntityType::MText(Box::new(x))
        }
        EntityKind::AttDef(a) => {
            let mut x = match same(|p| matches!(p, EntityType::AttributeDefinition(_))) {
                Some(EntityType::AttributeDefinition(x)) => x,
                _ => ae::AttributeDefinition::default(),
            };
            x.tag.clone_from(&a.tag);
            x.prompt.clone_from(&a.prompt);
            x.default_value.clone_from(&a.text.value);
            put_attr_text(&a.text, &mut x.insertion_point, &mut x.alignment_point, &mut x.height, &mut x.rotation, &mut x.width_factor);
            x.oblique_angle = a.text.oblique;
            x.text_style.clone_from(&a.text.style);
            x.horizontal_alignment = halign_a(a.text.halign);
            x.vertical_alignment = valign_a(a.text.valign);
            x.flags.invisible = a.invisible;
            x.flags.constant = a.constant;
            x.flags.verify = a.modes.verify;
            x.flags.preset = a.modes.preset;
            x.lock_position = a.modes.lock_position;
            x.normal = z();
            put_multiline(&a.multiline, &mut x.embedded_mtext, &mut x.is_multiline, &mut x.mtext_flag, ae::MTextFlag::ConstantMultiLine);
            EntityType::AttributeDefinition(x)
        }
        EntityKind::Insert(i) => {
            let mut x = match same(|p| matches!(p, EntityType::Insert(_))) {
                Some(EntityType::Insert(x)) => x,
                _ => ae::Insert::new(i.block.clone(), a3(i.insert)),
            };
            x.block_name.clone_from(&i.block);
            let mirrored = n.z < 0.0 && n.x.hypot(n.y) <= 1e-3 * n.z.abs();
            let (mut p, mut sx, mut rot, mut cs) = (i.insert, i.scale.x, i.rotation, i.col_spacing);
            if mirrored {
                p = to_ocs(n, p);
                sx = -sx;
                rot = -rot;
                cs = -cs;
            }
            x.insert_point = a3(p);
            x.set_x_scale(sx);
            x.set_y_scale(i.scale.y);
            x.set_z_scale(i.scale.z);
            x.rotation = rot;
            x.normal = a3(if mirrored { n } else { Vec3::Z });
            x.column_count = u16::try_from(i.cols).unwrap_or(1);
            x.row_count = u16::try_from(i.rows).unwrap_or(1);
            x.column_spacing = cs;
            x.row_spacing = i.row_spacing;
            let mut attrs = std::mem::take(&mut x.attributes);
            attrs.resize_with(i.attribs.len(), ae::AttributeEntity::default);
            for (dst, a) in attrs.iter_mut().zip(&i.attribs) {
                put_attrib(dst, a, &c.layer, cx);
            }
            x.attributes = attrs;
            EntityType::Insert(x)
        }
        EntityKind::Dimension(dm) => {
            let prev_dim = match prev {
                Some(EntityType::Dimension(p)) if same_dim_kind(p, &dm.kind) => Some((**p).clone()),
                _ => None,
            };
            let mut x = prev_dim.unwrap_or_else(|| new_dim(&dm.kind));
            put_dim(&mut x, dm, n, cx);
            EntityType::Dimension(Box::new(x))
        }
        EntityKind::Leader(l) => {
            let mut x = match same(|p| matches!(p, EntityType::Leader(_))) { Some(EntityType::Leader(x)) => x, _ => ae::Leader::default() };
            x.vertices = l.vertices.iter().map(|v| a3(*v)).collect();
            x.arrow_enabled = l.arrow;
            x.path_type = if l.spline { ae::LeaderPathType::Spline } else { ae::LeaderPathType::StraightLine };
            x.dimension_style.clone_from(&l.style);
            x.normal = a3(n);
            EntityType::Leader(x)
        }
        EntityKind::Hatch(h) => {
            let mut x = match same(|p| matches!(p, EntityType::Hatch(x) if !x.is_mpolygon)) {
                Some(EntityType::Hatch(x)) => *x,
                _ => ae::Hatch::default(),
            };
            put_hatch(&mut x, h, n);
            EntityType::Hatch(Box::new(x))
        }
        EntityKind::Solid(s) | EntityKind::Trace(s) => {
            let mut x = match same(|p| matches!(p, EntityType::Solid(_))) {
                Some(EntityType::Solid(x)) => x,
                _ => ae::Solid::new(Vector3::default(), Vector3::default(), Vector3::default(), Vector3::default()),
            };
            let k = s.corners.map(|p| a3(to_ocs(n, p)));
            (x.first_corner, x.second_corner, x.third_corner, x.fourth_corner) = (k[0], k[1], k[2], k[3]);
            x.is_trace = matches!(e.kind, EntityKind::Trace(_));
            x.thickness = c.thickness;
            x.normal = a3(n);
            EntityType::Solid(x)
        }
        EntityKind::Face3d(f) => {
            let k = f.corners.map(a3);
            let mut x =
                match same(|p| matches!(p, EntityType::Face3D(_))) { Some(EntityType::Face3D(x)) => x, _ => ae::Face3D::new(k[0], k[1], k[2], k[3]) };
            (x.first_corner, x.second_corner, x.third_corner, x.fourth_corner) = (k[0], k[1], k[2], k[3]);
            x.invisible_edges = ae::InvisibleEdgeFlags::from_bits(f.hidden_edges);
            EntityType::Face3D(x)
        }
        EntityKind::Viewport(v) => {
            let mut x = match same(|p| matches!(p, EntityType::Viewport(_))) { Some(EntityType::Viewport(x)) => *x, _ => ae::Viewport::default() };
            x.center = a3(v.center);
            x.width = v.width;
            x.height = v.height;
            let target = Vec2::new(x.view_target.x, x.view_target.y);
            let dcs = (v.view_center - target).rotate(v.twist);
            x.view_center = Vector3::new(dcs.x, dcs.y, 0.0);
            x.view_height = v.view_height;
            x.id = i16::try_from(v.id).unwrap_or(0);
            x.status.locked = v.locked;
            x.status.is_off = !v.on;
            x.status.is_on = v.on;
            x.twist_angle = v.twist;
            x.frozen_layers = v.frozen_layers.iter().filter_map(|l| cx.layers.get(&up(l)).copied()).collect();
            x.status.non_rectangular_clipping = v.clip.is_some();
            x.clip_boundary_handle = v.clip.map_or(AHandle::NULL, ah);
            EntityType::Viewport(Box::new(x))
        }
        EntityKind::Image(i) => {
            let mut x =
                match same(|p| matches!(p, EntityType::RasterImage(_))) { Some(EntityType::RasterImage(x)) => x, _ => ae::RasterImage::default() };
            x.insertion_point = a3(i.insert);
            x.u_vector = a3(i.u);
            x.v_vector = a3(i.v);
            x.size = a2(i.size);
            x.clipping_enabled = i.clipping;
            x.brightness = i.brightness;
            x.contrast = i.contrast;
            x.fade = i.fade;
            x.clip_boundary.vertices = i.clip.iter().map(|p| a2(*p)).collect();
            x.clip_boundary.clip_type = if i.clip.len() == 2 { ae::ClipType::Rectangular } else { ae::ClipType::Polygonal };
            if x.definition_handle.is_none() {
                x.file_path.clone_from(&i.path);
            }
            EntityType::RasterImage(x)
        }
        EntityKind::Wipeout(w) => {
            let mut x = match same(|p| matches!(p, EntityType::Wipeout(_))) { Some(EntityType::Wipeout(x)) => x, _ => ae::Wipeout::default() };
            put_wipeout(&mut x, &w.boundary);
            EntityType::Wipeout(x)
        }
        EntityKind::Tolerance(t) => {
            let mut x = match same(|p| matches!(p, EntityType::Tolerance(_))) { Some(EntityType::Tolerance(x)) => x, _ => ae::Tolerance::default() };
            x.insertion_point = a3(t.insert);
            x.direction = a3(t.dir);
            x.text.clone_from(&t.text);
            x.dimension_style_name.clone_from(&t.style);
            x.dimension_style_handle = None;
            EntityType::Tolerance(x)
        }
        // Not written from CADCraft's model yet: the file's object stays as it was read.
        EntityKind::Helix(_) | EntityKind::MLine(_) | EntityKind::MLeader(_) | EntityKind::Table(_) | EntityKind::Region(_) => return prev.cloned(),
    };
    let common = out.common_mut();
    if let Some(p) = prev {
        // The identity of the file's object: owner, extension dictionary, reactors, xdata.
        let pc = p.common();
        if common.handle != pc.handle {
            *common = pc.clone();
        }
    } else {
        put_xdata(common, c);
    }
    common.handle = ah(e.handle);
    common.raw_record = None;
    put_common(common, c);
    Some(out)
}

fn halign_t(h: HAlign) -> ae::TextHorizontalAlignment {
    match h {
        HAlign::Left => ae::TextHorizontalAlignment::Left,
        HAlign::Center => ae::TextHorizontalAlignment::Center,
        HAlign::Right => ae::TextHorizontalAlignment::Right,
        HAlign::Aligned => ae::TextHorizontalAlignment::Aligned,
        HAlign::Middle => ae::TextHorizontalAlignment::Middle,
        HAlign::Fit => ae::TextHorizontalAlignment::Fit,
    }
}

fn valign_t(v: VAlign) -> ae::TextVerticalAlignment {
    match v {
        VAlign::Baseline => ae::TextVerticalAlignment::Baseline,
        VAlign::Bottom => ae::TextVerticalAlignment::Bottom,
        VAlign::Middle => ae::TextVerticalAlignment::Middle,
        VAlign::Top => ae::TextVerticalAlignment::Top,
    }
}

fn halign_a(h: HAlign) -> ae::HorizontalAlignment {
    match h {
        HAlign::Left => ae::HorizontalAlignment::Left,
        HAlign::Center => ae::HorizontalAlignment::Center,
        HAlign::Right => ae::HorizontalAlignment::Right,
        HAlign::Aligned => ae::HorizontalAlignment::Aligned,
        HAlign::Middle => ae::HorizontalAlignment::Middle,
        HAlign::Fit => ae::HorizontalAlignment::Fit,
    }
}

fn valign_a(v: VAlign) -> ae::VerticalAlignment {
    match v {
        VAlign::Baseline => ae::VerticalAlignment::Baseline,
        VAlign::Bottom => ae::VerticalAlignment::Bottom,
        VAlign::Middle => ae::VerticalAlignment::Middle,
        VAlign::Top => ae::VerticalAlignment::Top,
    }
}

/// An attribute's text placement (insertion, alignment point, height, rotation, width).
fn put_attr_text(t: &cadcraft_doc::Text, ins: &mut Vector3, align: &mut Vector3, height: &mut f64, rot: &mut f64, width: &mut f64) {
    *ins = a3(t.insert);
    *align = a3(t.align_pt.unwrap_or(t.insert));
    *height = t.height;
    *rot = t.rotation;
    *width = t.width_factor;
}

fn put_mtext(x: &mut ae::MText, t: &cadcraft_doc::MText) {
    x.insertion_point = a3(t.insert);
    x.height = t.height;
    x.rectangle_width = t.width;
    x.rotation = t.rotation;
    x.dwg_x_direction = None;
    x.style.clone_from(&t.style);
    x.value.clone_from(&t.contents);
    x.attachment_point = attachment(t.attach);
    x.line_spacing_factor = t.line_spacing;
    x.line_spacing_style = if t.line_spacing_exact { ae::LineSpacingStyle::Exactly } else { ae::LineSpacingStyle::AtLeast };
    x.normal = z();
    match &t.columns {
        None => x.column_data = ae::MTextColumnData::default(),
        Some(c) => {
            let dynamic = c.kind == cadcraft_doc::MTextColumnType::Dynamic;
            x.column_data.column_type = if dynamic { 2 } else { 1 };
            x.column_data.column_count = i32::from(c.count);
            x.column_data.flow_reversed = c.flow_reversed;
            x.column_data.auto_height = c.auto_height;
            x.column_data.width = t.width;
            x.column_data.gutter = c.gutter;
            x.column_data.heights.clone_from(&c.heights);
            x.rectangle_height = Some(c.height);
        }
    }
}

fn attachment(a: u8) -> ae::AttachmentPoint {
    use ae::AttachmentPoint as P;
    match a {
        2 => P::TopCenter,
        3 => P::TopRight,
        4 => P::MiddleLeft,
        5 => P::MiddleCenter,
        6 => P::MiddleRight,
        7 => P::BottomLeft,
        8 => P::BottomCenter,
        9 => P::BottomRight,
        _ => P::TopLeft,
    }
}

fn put_multiline(
    m: &Option<cadcraft_doc::MultilineAttrib>,
    dst: &mut Option<Box<ae::MText>>,
    is_multiline: &mut bool,
    flag: &mut ae::MTextFlag,
    multi: ae::MTextFlag,
) {
    match m {
        Some(ml) => {
            let mut x = dst.take().map(|b| *b).unwrap_or_default();
            put_mtext(&mut x, &ml.mtext);
            *dst = Some(Box::new(x));
            *is_multiline = true;
            *flag = multi;
        }
        None => {
            *dst = None;
            *is_multiline = false;
            *flag = ae::MTextFlag::SingleLine;
        }
    }
}

fn put_attrib(dst: &mut ae::AttributeEntity, a: &cadcraft_doc::Attrib, insert_layer: &str, cx: &mut Cx) {
    dst.common.raw_record = None;
    if dst.common.handle.is_null() {
        dst.common.handle = (cx.alloc)();
    }
    dst.tag.clone_from(&a.tag);
    dst.value.clone_from(&a.text.value);
    put_attr_text(&a.text, &mut dst.insertion_point, &mut dst.alignment_point, &mut dst.height, &mut dst.rotation, &mut dst.width_factor);
    dst.oblique_angle = a.text.oblique;
    dst.text_style.clone_from(&a.text.style);
    dst.horizontal_alignment = halign_a(a.text.halign);
    dst.vertical_alignment = valign_a(a.text.valign);
    dst.flags.invisible = a.invisible;
    dst.flags.constant = a.constant;
    dst.flags.verify = a.modes.verify;
    dst.flags.preset = a.modes.preset;
    dst.lock_position = a.modes.lock_position;
    dst.normal = z();
    put_multiline(&a.multiline, &mut dst.embedded_mtext, &mut dst.is_multiline, &mut dst.mtext_flag, ae::MTextFlag::MultiLine);
    let p = &a.props;
    dst.common.layer = p.layer.clone().unwrap_or_else(|| insert_layer.to_string());
    dst.common.layer_handle = None;
    dst.common.color = p.color.map_or(acadrust::types::Color::ByLayer, acolor);
    dst.common.linetype = p.linetype.clone().unwrap_or_default();
    dst.common.linetype_handle = None;
    dst.common.line_weight = p.lineweight.map_or(acadrust::types::LineWeight::ByLayer, alineweight);
}

fn same_dim_kind(d: &ADim, k: &DimKind) -> bool {
    matches!(
        (d, k),
        (ADim::Linear(_), DimKind::Linear { .. })
            | (ADim::Aligned(_), DimKind::Aligned)
            | (ADim::Radius(_), DimKind::Radius)
            | (ADim::Diameter(_), DimKind::Diameter)
            | (ADim::Angular2Ln(_), DimKind::Angular)
            | (ADim::Angular3Pt(_), DimKind::Angular3P)
            | (ADim::Ordinate(_), DimKind::Ordinate { .. })
            | (ADim::Arc(_), DimKind::ArcLength)
    )
}

fn new_dim(k: &DimKind) -> ADim {
    match k {
        DimKind::Linear { .. } => ADim::Linear(ae::DimensionLinear::default()),
        DimKind::Aligned => ADim::Aligned(ae::DimensionAligned::default()),
        DimKind::Radius => ADim::Radius(ae::DimensionRadius::default()),
        DimKind::Diameter => ADim::Diameter(ae::DimensionDiameter::default()),
        DimKind::Angular => ADim::Angular2Ln(ae::DimensionAngular2Ln::default()),
        DimKind::Angular3P => ADim::Angular3Pt(ae::DimensionAngular3Pt::default()),
        DimKind::Ordinate { .. } => ADim::Ordinate(ae::DimensionOrdinate::default()),
        DimKind::ArcLength => ADim::Arc(ae::DimensionArc::default()),
    }
}

fn put_dim(x: &mut ADim, dm: &cadcraft_doc::Dimension, n: Vec3, cx: &mut Cx) {
    let (d10, p13, p14, p15, p16) = (a3(dm.defpt), a3(dm.p13), a3(dm.p14), a3(dm.p15), a3(dm.p16));
    let oblique = |base: f64| dm.marks.oblique.map_or(0.0, |o| cadcraft_geom::norm_angle(o - base));
    match x {
        ADim::Linear(d) => {
            let rotation = if let DimKind::Linear { rotation } = dm.kind { rotation } else { 0.0 };
            (d.definition_point, d.first_point, d.second_point, d.rotation) = (d10, p13, p14, rotation);
            d.ext_line_rotation = oblique(rotation);
            d.base.definition_point = d10;
        }
        ADim::Aligned(d) => {
            (d.definition_point, d.first_point, d.second_point) = (d10, p13, p14);
            d.ext_line_rotation = oblique((dm.p14.xy() - dm.p13.xy()).angle());
            d.base.definition_point = d10;
        }
        ADim::Radius(d) => {
            (d.angle_vertex, d.definition_point) = (d10, p15);
            d.base.definition_point = d10;
        }
        ADim::Diameter(d) => {
            (d.definition_point, d.angle_vertex) = (d10, p15);
            d.base.definition_point = d10;
        }
        ADim::Angular2Ln(d) => {
            (d.definition_point, d.first_point, d.second_point, d.angle_vertex, d.dimension_arc) = (d10, p13, p14, p15, p16);
            d.base.definition_point = d10;
        }
        ADim::Angular3Pt(d) => {
            (d.definition_point, d.first_point, d.second_point, d.angle_vertex) = (d10, p13, p14, p15);
            d.base.definition_point = d10;
        }
        ADim::Ordinate(d) => {
            (d.definition_point, d.feature_location, d.leader_endpoint) = (d10, p13, p14);
            d.is_ordinate_type_x = matches!(dm.kind, DimKind::Ordinate { x_type: true });
            d.base.definition_point = d10;
        }
        ADim::Arc(d) => {
            (d.definition_point, d.first_extension_point, d.second_extension_point, d.center_point) = (d10, p13, p14, p15);
            d.base.definition_point = d10;
        }
        ADim::LargeRadial(d) => {
            (d.definition_point, d.chord_point) = (d10, p15);
        }
    }
    let b = x.base_mut();
    b.text_middle_point = a3(if n.z < 0.0 { to_ocs(n, dm.text_mid) } else { dm.text_mid });
    b.normal = a3(n);
    b.insertion_point = a3(dm.insertion_point);
    b.style_name.clone_from(&dm.style);
    b.actual_measurement = dm.measurement;
    b.text_rotation = dm.text_rotation;
    b.text_user_positioned = dm.user_text_pos;
    b.block_name = dm.block.clone().unwrap_or_default();
    b.set_text_override(Some(dm.text.clone()).filter(|t| !t.is_empty()));
    crate::tables::put_dstyle(&mut b.common.extended_data, &dm.overrides, &cx.lookups);
}

fn put_hatch(x: &mut ae::Hatch, h: &cadcraft_doc::Hatch, n: Vec3) {
    let m = if n.z < 0.0 { -1.0 } else { 1.0 };
    x.paths = h
        .loops
        .iter()
        .map(|l| {
            let mut flags = ae::BoundaryPathFlags::new();
            flags.set_polyline(true);
            flags.set_external(l.outer);
            ae::BoundaryPath {
                flags,
                edges: vec![ae::BoundaryEdge::Polyline(ae::PolylineEdge {
                    vertices: l.vertices.iter().map(|v| Vector3::new(m * v.p.x, v.p.y, m * v.bulge)).collect(),
                    is_closed: true,
                })],
                boundary_handles: l.sources.iter().map(|s| ah(*s)).collect(),
            }
        })
        .collect();
    x.pattern.name.clone_from(&h.pattern);
    x.is_solid = h.solid;
    x.is_associative = h.associative;
    x.pattern_scale = h.scale;
    x.pattern_angle = if n.z < 0.0 { cadcraft_geom::norm_angle(std::f64::consts::PI - h.angle) } else { h.angle };
    x.style = match h.style {
        1 => ae::HatchStyleType::Outer,
        2 => ae::HatchStyleType::Ignore,
        _ => ae::HatchStyleType::Normal,
    };
    x.elevation = h.elevation;
    x.normal = a3(n);
    if !h.solid {
        // The library's lines back as drawn: scaled, turned with the hatch, from its origin.
        let scale = if h.scale.is_finite() && h.scale > 1e-9 { h.scale } else { 1.0 };
        x.pattern.lines = h
            .pattern_lines
            .iter()
            .map(|pl| {
                let a = (pl.angle + h.angle.to_degrees()).to_radians();
                let base = Vec2::new(pl.origin.0, pl.origin.1).rotate(h.angle) * scale + h.origin;
                let offset = Vec2::new(pl.delta.0, pl.delta.1).rotate(a) * scale;
                ae::HatchPatternLine { angle: a, base_point: a2(base), offset: a2(offset), dash_lengths: pl.dashes.iter().map(|d| d * scale).collect() }
            })
            .collect();
    }
    match &h.gradient {
        Some(g) => {
            x.gradient_color.enabled = true;
            x.gradient_color.name.clone_from(&g.name);
            x.gradient_color.angle = g.angle;
            x.gradient_color.shift = if g.centered { 0.0 } else { 1.0 };
            let entry = |c: cadcraft_color::Color, v: f64| ae::GradientColorEntry { value: v, color: acolor(c), color_name: None, book_name: None };
            x.gradient_color.colors = vec![entry(g.color1, 0.0), entry(g.color2, 1.0)];
        }
        None => x.gradient_color.enabled = false,
    }
}

/// A wipeout's frame: its boundary in a unit image over the boundary's extent (pixel space has
/// its origin at the image's top-left corner, y down).
fn put_wipeout(x: &mut ae::Wipeout, boundary: &[Vec2]) {
    let b = cadcraft_geom::Bounds2::from_points(boundary.iter().copied());
    let (w, h) = ((b.max.x - b.min.x).max(1e-9), (b.max.y - b.min.y).max(1e-9));
    x.insertion_point = Vector3::new(b.min.x, b.min.y, 0.0);
    x.u_vector = Vector3::new(w, 0.0, 0.0);
    x.v_vector = Vector3::new(0.0, h, 0.0);
    x.size = Vector2::new(1.0, 1.0);
    x.clip_type = ae::WipeoutClipType::Polygonal;
    x.clipping_enabled = true;
    let mut pts: Vec<Vector2> = boundary.iter().map(|p| Vector2::new((p.x - b.min.x) / w - 0.5, 0.5 - (p.y - b.min.y) / h)).collect();
    if let Some(first) = pts.first().copied() {
        pts.push(first);
    }
    x.clip_boundary_vertices = pts;
}
