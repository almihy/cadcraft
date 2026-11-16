//! acadrust's entities as CADCraft's. Each kind maps as CADCraft's DXF reader maps the groups
//! acadrust's DXF writer would write for it, without the text in between: angles stay radians,
//! points of an object coordinate system come into the WCS the same way, and a kind CADCraft
//! doesn't model becomes [`EntityKind::Unknown`] under its DXF name (the object itself stays in the
//! file's document, see the crate documentation).

use std::collections::HashMap;

use acadrust::entities::{self as ae, Dimension as ADim, EntityType};
use acadrust::types::Vector3;
use cadcraft_doc::ocs::{mirror_text, plan_of_tilted, run_points, tilted, to_wcs};
use cadcraft_doc::{
    Arc, Attrib, AttribModes, AttribProps, Circle, Common, DimKind, Dimension, Ellipse, Entity, EntityKind, Face3d, HAlign, Handle, Hatch, HatchLoop, Helix,
    HelixConstrain, Image, Insert, Leader, Line, LwPolyline, MText, MTextColumnType, MTextColumns, MultilineAttrib, Point, PolyfaceMesh, Polyline3d,
    RayLine, Solid, Text, Tolerance, Unknown, VAlign, Viewport, Wipeout,
};
use cadcraft_geom::{FitOptions, PolyVertex, Spline, Vec2, Vec3};

use crate::conv::{color, common, h, lineweight, up, v2, v3};

/// Names behind the handles entities refer to.
#[derive(Default)]
pub struct Names {
    pub layers: HashMap<u64, String>,
    pub blocks: HashMap<u64, String>,
    pub ltypes: HashMap<u64, String>,
    pub styles: HashMap<u64, String>,
    /// Image definitions: file path and pixel size, by handle.
    pub images: HashMap<u64, (String, Vec2, u8)>,
}

impl Names {
    pub fn of(doc: &acadrust::CadDocument) -> Names {
        let mut n = Names::default();
        for l in doc.layers.iter() {
            n.layers.insert(l.handle.value(), l.name.clone());
        }
        for b in doc.block_records.iter() {
            n.blocks.insert(b.handle.value(), b.name.clone());
        }
        for l in doc.line_types.iter() {
            n.ltypes.insert(l.handle.value(), l.name.clone());
        }
        for s in doc.text_styles.iter() {
            n.styles.insert(s.handle.value(), s.name.clone());
        }
        for o in doc.objects.values() {
            if let acadrust::objects::ObjectType::ImageDefinition(d) = o {
                let units = match d.resolution_unit {
                    acadrust::objects::ResolutionUnit::Centimeters => 2,
                    acadrust::objects::ResolutionUnit::Inches => 5,
                    _ => 0,
                };
                n.images.insert(d.handle.value(), (d.file_name.clone(), Vec2::new(d.pixel_size.0, d.pixel_size.1), units));
            }
        }
        n
    }
}

/// One entity of a block, model space or a layout as CADCraft's, `None` for what isn't an entity
/// of its own there (block delimiters, attributes and vertices, which belong to their owner).
pub fn entity(e: &EntityType, names: &Names) -> Option<Entity> {
    let (common, kind) = kind(e, names)?;
    Some(Entity { handle: h(e.common().handle), common, kind })
}

fn unknown(e: &EntityType, name: &str) -> (Common, EntityKind) {
    let c = e.common();
    (common(c, 0.0, Vector3::new(0.0, 0.0, 1.0), false), EntityKind::Unknown(Unknown { dxf_type: name.to_string(), tags: Vec::new() }))
}

/// The DXF type name of a kind CADCraft keeps as [`EntityKind::Unknown`].
pub fn unknown_name(e: &EntityType) -> String {
    match e {
        EntityType::Dimension(d) => match d.as_ref() {
            ADim::Arc(_) => "ARC_DIMENSION".into(),
            ADim::LargeRadial(_) => "LARGE_RADIAL_DIMENSION".into(),
            _ => "DIMENSION".into(),
        },
        EntityType::MultiLeader(_) => "MULTILEADER".into(),
        EntityType::MLine(_) => "MLINE".into(),
        EntityType::Mesh(_) => "MESH".into(),
        EntityType::Solid3D(_) => "3DSOLID".into(),
        EntityType::Region(_) => "REGION".into(),
        EntityType::Body(_) => "BODY".into(),
        EntityType::Surface(_) => "SURFACE".into(),
        EntityType::Table(_) => "ACAD_TABLE".into(),
        EntityType::Shape(_) => "SHAPE".into(),
        EntityType::Underlay(_) => "UNDERLAY".into(),
        EntityType::Ole2Frame(_) => "OLE2FRAME".into(),
        EntityType::PolygonMesh(_) => "POLYLINE".into(),
        EntityType::Light(_) => "LIGHT".into(),
        EntityType::SectionSymbol(_) => "SECTIONSYMBOL".into(),
        EntityType::ViewBorder(_) => "VIEWBORDER".into(),
        EntityType::Extended(x) => x.class_name().to_string(),
        EntityType::Unknown(u) => u.dxf_name.clone(),
        _ => "UNKNOWN".into(),
    }
}

fn kind(e: &EntityType, names: &Names) -> Option<(Common, EntityKind)> {
    let z = Vector3::new(0.0, 0.0, 1.0);
    Some(match e {
        EntityType::Block(_) | EntityType::BlockEnd(_) | EntityType::Seqend(_) | EntityType::AttributeEntity(_) => return None,
        EntityType::Line(l) => (common(&l.common, l.thickness, l.normal, false), EntityKind::Line(Line { a: v3(l.start), b: v3(l.end) })),
        EntityType::Point(p) => {
            (common(&p.common, p.thickness, p.normal, false), EntityKind::Point(Point { p: v3(p.location), angle: p.x_axis_angle }))
        }
        EntityType::Circle(c) => {
            let cm = common(&c.common, c.thickness, c.normal, false);
            let center = to_wcs(cm.extrusion, v3(c.center));
            (cm, EntityKind::Circle(Circle { center, radius: c.radius.abs() }))
        }
        EntityType::Arc(a) => {
            let cm = common(&a.common, a.thickness, a.normal, false);
            let mut arc = Arc { center: to_wcs(cm.extrusion, v3(a.center)), radius: a.radius.abs(), start: a.start_angle, end: a.end_angle };
            if cm.extrusion.z < 0.0 {
                let (s, e) = (std::f64::consts::PI - arc.end, std::f64::consts::PI - arc.start);
                arc.start = cadcraft_geom::norm_angle(s);
                arc.end = cadcraft_geom::norm_angle(e);
            }
            (cm, EntityKind::Arc(arc))
        }
        EntityType::Ellipse(el) => {
            let cm = common(&el.common, 0.0, el.normal, false);
            let mut e2 = Ellipse {
                center: v3(el.center),
                major: v3(el.major_axis),
                ratio: el.minor_axis_ratio.clamp(1e-9, 1.0),
                start: el.start_parameter,
                end: el.end_parameter,
            };
            if cm.extrusion.z < 0.0 && (cadcraft_geom::ccw_sweep(e2.start, e2.end) - std::f64::consts::TAU).abs() > 1e-9 {
                let (s, e) = (-e2.end, -e2.start);
                e2.start = cadcraft_geom::norm_angle(s);
                e2.end = cadcraft_geom::norm_angle(e);
            }
            (cm, EntityKind::Ellipse(e2))
        }
        EntityType::LwPolyline(p) => {
            let mut cm = common(&p.common, p.thickness, p.normal, false);
            let verts = p
                .vertices
                .iter()
                .map(|v| PolyVertex { p: v2(v.location), bulge: v.bulge, start_width: v.start_width, end_width: v.end_width })
                .collect();
            let k = plan_polyline(&mut cm, verts, p.is_closed, p.constant_width, p.elevation, p.plinegen);
            (cm, k)
        }
        EntityType::Polyline2D(p) => {
            let cm = common(&p.common, p.thickness, p.normal, false);
            let m = if cm.extrusion.z < 0.0 { -1.0 } else { 1.0 };
            let vertices = p
                .vertices
                .iter()
                .filter(|v| v.flags.bits() & 16 == 0)
                .map(|v| PolyVertex { p: Vec2::new(m * v.location.x, v.location.y), bulge: m * v.bulge, start_width: v.start_width, end_width: v.end_width })
                .collect();
            let flags = p.flags.bits();
            let k = EntityKind::LwPolyline(LwPolyline {
                vertices,
                closed: flags & 1 != 0,
                const_width: p.start_width.min(p.end_width).max(0.0),
                elevation: p.elevation,
                plinegen: flags & 128 != 0,
            });
            (cm, k)
        }
        EntityType::Polyline(p) => {
            let cm = common(&p.common, 0.0, z, false);
            let points = p.vertices.iter().filter(|v| v.flags.bits() & 16 == 0).map(|v| v3(v.location)).collect();
            (cm, EntityKind::Polyline3d(Polyline3d { points, closed: p.flags.is_closed() }))
        }
        EntityType::Polyline3D(p) => {
            let cm = common(&p.common, 0.0, z, false);
            let points = p.vertices.iter().filter(|v| v.flags & 16 == 0).map(|v| v3(v.position)).collect();
            (cm, EntityKind::Polyline3d(Polyline3d { points, closed: p.flags.closed }))
        }
        EntityType::PolyfaceMesh(m) => {
            let cm = common(&m.common, m.thickness, z, false);
            let mut mesh = PolyfaceMesh { vertices: m.vertices.iter().map(|v| v3(v.location)).collect(), faces: Vec::new() };
            let faces: Vec<[i32; 4]> =
                m.faces.iter().map(|f| [f.index1, f.index2, f.index3, f.index4].map(i32::from)).filter(|f| mesh.face3d(f).is_some()).collect();
            if faces.is_empty() {
                return Some(unknown(e, "POLYLINE"));
            }
            mesh.faces = faces;
            (cm, EntityKind::PolyfaceMesh(mesh))
        }
        EntityType::Spline(s) => (common(&s.common, 0.0, s.normal, false), EntityKind::Spline(spline(s))),
        EntityType::Helix(x) => (common(&x.common, 0.0, z, false), EntityKind::Helix(helix(x))),
        EntityType::Ray(r) => (common(&r.common, 0.0, z, false), EntityKind::Ray(RayLine { base: v3(r.base_point), dir: v3(r.direction) })),
        EntityType::XLine(r) => (common(&r.common, 0.0, z, false), EntityKind::XLine(RayLine { base: v3(r.base_point), dir: v3(r.direction) })),
        EntityType::Text(t) => {
            let cm = common(&t.common, t.thickness, t.normal, false);
            let text = text(&TextParts {
                insert: t.insertion_point,
                align: t.alignment_point,
                height: t.height,
                value: &t.value,
                rotation: t.rotation,
                width_factor: t.width_factor,
                oblique: t.oblique_angle,
                style: &t.style,
                halign: t.horizontal_alignment as i16,
                valign: t.vertical_alignment as i16,
                normal: t.normal,
            });
            (cm, EntityKind::Text(text))
        }
        EntityType::MText(t) => {
            let cm = common(&t.common, 0.0, t.normal, false);
            let m = mtext(t, &cm);
            (cm, EntityKind::MText(m))
        }
        EntityType::AttributeDefinition(a) => {
            let cm = common(&a.common, 0.0, a.normal, false);
            let att = attdef(a, &cm);
            (cm, EntityKind::AttDef(att))
        }
        EntityType::Insert(i) => {
            let cm = common(&i.common, 0.0, i.normal, false);
            let ins = insert(i, &cm);
            (cm, EntityKind::Insert(ins))
        }
        EntityType::Dimension(d) => match d.as_ref() {
            ADim::Arc(_) | ADim::LargeRadial(_) => unknown(e, &unknown_name(e)),
            dim => {
                let b = dim.base();
                let cm = common(&b.common, 0.0, b.normal, true);
                let k = dimension(dim, &cm, names);
                (cm, EntityKind::Dimension(k))
            }
        },
        EntityType::Leader(l) => (
            common(&l.common, 0.0, l.normal, false),
            EntityKind::Leader(Leader {
                vertices: l.vertices.iter().map(|v| v3(*v)).collect(),
                arrow: l.arrow_enabled,
                spline: l.path_type == ae::LeaderPathType::Spline,
                style: if l.dimension_style.is_empty() { "Standard".into() } else { l.dimension_style.clone() },
            }),
        ),
        EntityType::Hatch(hx) if !hx.is_mpolygon => {
            let mut cm = common(&hx.common, 0.0, hx.normal, false);
            let k = crate::entity_read::hatch(hx, &mut cm);
            (cm, k)
        }
        EntityType::Solid(s) => {
            let cm = common(&s.common, s.thickness, s.normal, false);
            let corners = [s.first_corner, s.second_corner, s.third_corner, s.fourth_corner].map(|p| to_wcs(cm.extrusion, v3(p)));
            let k = if s.is_trace { EntityKind::Trace(Solid { corners }) } else { EntityKind::Solid(Solid { corners }) };
            (cm, k)
        }
        EntityType::Face3D(f) => (
            common(&f.common, 0.0, z, false),
            EntityKind::Face3d(Face3d {
                corners: [f.first_corner, f.second_corner, f.third_corner, f.fourth_corner].map(v3),
                hidden_edges: f.invisible_edges.bits(),
            }),
        ),
        EntityType::Viewport(v) => (common(&v.common, 0.0, z, false), EntityKind::Viewport(viewport(v, names))),
        EntityType::RasterImage(img) => (common(&img.common, 0.0, z, false), EntityKind::Image(image(img, names))),
        EntityType::Wipeout(w) => {
            let (o, u, v) = (v3(w.insertion_point).xy(), v3(w.u_vector).xy(), v3(w.v_vector).xy());
            let boundary = w.clip_boundary_vertices.iter().map(|q| o + u * (q.x + 0.5) + v * (0.5 - q.y)).collect();
            (common(&w.common, 0.0, z, false), EntityKind::Wipeout(Wipeout { boundary }))
        }
        EntityType::Tolerance(t) => {
            let dir = v3(t.direction);
            let dir = if dir.xy().is_finite() && dir.xy().len() > 1e-12 { dir } else { Vec3::new(1.0, 0.0, 0.0) };
            (
                common(&t.common, 0.0, t.normal, false),
                EntityKind::Tolerance(Tolerance {
                    insert: v3(t.insertion_point),
                    dir,
                    style: if t.dimension_style_name.is_empty() { "Standard".into() } else { t.dimension_style_name.clone() },
                    text: t.text.chars().take(cadcraft_doc::MAX_FRAME_CHARS).collect(),
                }),
            )
        }
        other => unknown(other, &unknown_name(other)),
    })
}

/// A lightweight polyline as drawn from above: in a tilted plane its arcs are flattened into
/// plan coordinates; under a mirrored OCS its x and bulges turn over (as the DXF reader does).
fn plan_polyline(cm: &mut Common, mut verts: Vec<PolyVertex>, closed: bool, const_width: f64, mut elevation: f64, plinegen: bool) -> EntityKind {
    if tilted(cm.extrusion, run_points(&verts, elevation, closed)) {
        (verts, elevation) = plan_of_tilted(cm.extrusion, elevation, &verts, closed);
        cm.extrusion = Vec3::Z;
    } else if cm.extrusion.z < 0.0 {
        for v in &mut verts {
            v.p.x = -v.p.x;
            v.bulge = -v.bulge;
        }
    }
    EntityKind::LwPolyline(LwPolyline { vertices: verts, closed, const_width, elevation, plinegen })
}

pub fn spline(s: &ae::Spline) -> Spline {
    let degree = usize::try_from(s.degree.clamp(1, 10)).unwrap_or(3);
    let control: Vec<Vec2> = s.control_points.iter().map(|p| v3(*p).xy()).collect();
    let fit: Vec<Vec2> = s.fit_points.iter().map(|p| v3(*p).xy()).collect();
    let tangent = |t: Vector3| (t.x != 0.0 || t.y != 0.0 || t.z != 0.0).then(|| v3(t).xy());
    let fit_opts = FitOptions {
        knots: Default::default(),
        start_tangent: tangent(s.begin_tangent),
        end_tangent: tangent(s.end_tangent),
        tolerance: s.fit_tolerance,
    }
    .sanitized();
    let weights = if s.flags.rational || s.weights.iter().any(|w| (*w - 1.0).abs() > 1e-12) { s.weights.clone() } else { Vec::new() };
    let mut sp = Spline { degree, knots: s.knots.clone(), control, weights, fit: fit.clone(), closed: s.flags.closed, fit_opts };
    if !sp.is_valid() && fit.len() >= 2 {
        sp = Spline::fit_with(&fit, sp.closed, fit_opts);
    } else if !sp.is_valid() && sp.control.len() >= 2 {
        sp = Spline::from_control(sp.control.clone(), degree);
    } else if fit.len() >= 3 {
        sp.fit_opts.knots = sp.infer_knot_param();
    }
    sp
}

fn helix(x: &ae::Helix) -> Helix {
    let center = v3(x.axis_base_point);
    let start = v3(x.start_point);
    let axis = v3(x.axis_vector);
    let axis = if axis.is_finite() && axis.len() > 1e-12 { axis.normalized() } else { Vec3::Z };
    let radius = |p: Vec3| {
        let v = p - center;
        let along = v.dot(axis);
        (v - axis * along).len()
    };
    let base_radius = if x.radius.is_finite() && x.radius >= 0.0 { x.radius } else { radius(start) };
    let end = x.spline.control_points.last().or(x.spline.fit_points.last()).map(|p| v3(*p));
    let height = if x.turn_height.is_finite() {
        x.turn_height * x.turns
    } else {
        end.map_or(0.0, |e| e.z - center.z)
    };
    Helix {
        center,
        base_radius,
        top_radius: end.map(radius).unwrap_or(base_radius),
        height,
        turns: x.turns,
        start_angle: center.xy().angle_to(start.xy()),
        ccw: x.handedness,
        constrain: HelixConstrain::from_dxf(match x.constraint {
            ae::HelixConstraint::TurnHeight => 0,
            ae::HelixConstraint::Turns => 1,
            ae::HelixConstraint::Height => 2,
        }),
    }
    .sanitized()
}

/// The values of a TEXT, ATTRIB or ATTDEF.
struct TextParts<'a> {
    insert: Vector3,
    align: Option<Vector3>,
    height: f64,
    value: &'a str,
    rotation: f64,
    width_factor: f64,
    oblique: f64,
    style: &'a str,
    halign: i16,
    valign: i16,
    normal: Vector3,
}

fn text(t: &TextParts) -> Text {
    let h = match t.halign {
        1 => HAlign::Center,
        2 => HAlign::Right,
        3 => HAlign::Aligned,
        4 => HAlign::Middle,
        5 => HAlign::Fit,
        _ => HAlign::Left,
    };
    let v = match t.valign {
        1 => VAlign::Bottom,
        2 => VAlign::Middle,
        3 => VAlign::Top,
        _ => VAlign::Baseline,
    };
    let align = t.align.map(v3);
    let mut text = Text {
        insert: v3(t.insert),
        align_pt: if h == HAlign::Left && v == VAlign::Baseline { None } else { align },
        height: t.height.max(1e-9),
        value: t.value.to_string(),
        rotation: t.rotation,
        width_factor: t.width_factor,
        oblique: t.oblique,
        style: if t.style.is_empty() { "Standard".into() } else { t.style.to_string() },
        halign: h,
        valign: v,
    };
    let n = v3(t.normal);
    if n.z < 0.0 {
        mirror_text(&mut text, align.map(|a| to_wcs(n, a)), to_wcs(n, v3(t.insert)));
    }
    text
}

pub fn mtext(t: &ae::MText, cm: &Common) -> MText {
    let cols = &t.column_data;
    let columns = (cols.column_type != 0).then(|| {
        let kind = if cols.column_type == 2 { MTextColumnType::Dynamic } else { MTextColumnType::Static };
        let count = if kind == MTextColumnType::Dynamic && !cols.auto_height { cols.heights.len() } else { usize::try_from(cols.column_count).unwrap_or(1) };
        (
            MTextColumns {
                kind,
                count: u16::try_from(count.clamp(1, 1000)).unwrap_or(1),
                gutter: cols.gutter,
                height: t.rectangle_height.unwrap_or(0.0),
                auto_height: cols.auto_height,
                heights: if kind == MTextColumnType::Dynamic && !cols.auto_height { cols.heights.clone() } else { Vec::new() },
                flow_reversed: cols.flow_reversed,
            },
            cols.width,
        )
    });
    let mut rotation = match t.dwg_x_direction {
        Some(d) if d.x != 0.0 || d.y != 0.0 => Vec2::new(d.x, d.y).angle(),
        _ => t.rotation,
    };
    let mut attach = (t.attachment_point as u8).clamp(1, 9);
    if cm.extrusion.z < 0.0 {
        rotation = cadcraft_geom::norm_angle(rotation + std::f64::consts::PI);
        attach = match attach {
            1 | 4 | 7 => attach + 2,
            3 | 6 | 9 => attach - 2,
            a => a,
        };
    }
    MText {
        insert: v3(t.insertion_point),
        height: t.height.max(1e-9),
        width: match &columns {
            Some((_, w)) if *w > 0.0 => *w,
            _ => t.rectangle_width.max(0.0),
        },
        attach,
        rotation,
        style: if t.style.is_empty() { "Standard".into() } else { t.style.clone() },
        contents: t.value.clone(),
        line_spacing: t.line_spacing_factor,
        line_spacing_exact: t.line_spacing_style as i16 == 2,
        columns: columns.map(|(c, _)| c),
    }
}

fn multiline(m: Option<&ae::MText>, cm: &Common) -> Option<MultilineAttrib> {
    let mt = mtext(m?, cm);
    let read_as = cadcraft_fonts::plain_mtext(&mt.contents);
    Some(MultilineAttrib { mtext: mt, read_as })
}

fn modes(flags: &ae::AttributeFlags, lock: bool) -> AttribModes {
    AttribModes { verify: flags.verify, preset: flags.preset, lock_position: lock }
}

/// A TEXT's alignment point as the DXF writer has it: none for baseline-left text.
fn align_point(p: Vector3, h: i16, v: i16) -> Option<Vector3> {
    (h != 0 || v != 0).then_some(p)
}

fn attdef(a: &ae::AttributeDefinition, cm: &Common) -> Attrib {
    let multiline = if a.is_multiline || a.mtext_flag != ae::MTextFlag::SingleLine { multiline(a.embedded_mtext.as_deref(), cm) } else { None };
    let mut t = text(&TextParts {
        insert: a.insertion_point,
        align: align_point(a.alignment_point, a.horizontal_alignment as i16, a.vertical_alignment as i16),
        height: a.height,
        value: &a.default_value,
        rotation: a.rotation,
        width_factor: a.width_factor,
        oblique: a.oblique_angle,
        style: &a.text_style,
        halign: a.horizontal_alignment as i16,
        valign: a.vertical_alignment as i16,
        normal: a.normal,
    });
    if let Some(m) = &multiline {
        t.value.clone_from(&m.read_as);
    }
    Attrib {
        tag: a.tag.clone(),
        text: t,
        invisible: a.flags.invisible,
        constant: a.flags.constant,
        prompt: a.prompt.clone(),
        props: AttribProps::default(),
        multiline,
        modes: modes(&a.flags, a.lock_position || a.flags.locked_position),
    }
}

fn attrib(a: &ae::AttributeEntity, insert_layer: &str) -> Attrib {
    let cm = common(&a.common, 0.0, a.normal, false);
    let multiline = if a.is_multiline || a.mtext_flag != ae::MTextFlag::SingleLine { multiline(a.embedded_mtext.as_deref(), &cm) } else { None };
    let mut t = text(&TextParts {
        insert: a.insertion_point,
        align: align_point(a.alignment_point, a.horizontal_alignment as i16, a.vertical_alignment as i16),
        height: a.height,
        value: &a.value,
        rotation: a.rotation,
        width_factor: a.width_factor,
        oblique: a.oblique_angle,
        style: &a.text_style,
        halign: a.horizontal_alignment as i16,
        valign: a.vertical_alignment as i16,
        normal: a.normal,
    });
    if let Some(m) = &multiline {
        t.value.clone_from(&m.read_as);
    }
    let c = &a.common;
    Attrib {
        tag: a.tag.clone(),
        text: t,
        invisible: a.flags.invisible,
        constant: a.flags.constant,
        prompt: String::new(),
        props: AttribProps {
            layer: Some(c.layer.clone()).filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case(insert_layer)),
            color: (c.color != acadrust::types::Color::ByLayer).then(|| color(&c.color)),
            linetype: Some(c.linetype.clone()).filter(|l| !l.is_empty()),
            lineweight: (c.line_weight != acadrust::types::LineWeight::ByLayer).then(|| lineweight(c.line_weight)),
        },
        multiline,
        modes: modes(&a.flags, a.lock_position || a.flags.locked_position),
    }
}

fn insert(i: &ae::Insert, cm: &Common) -> Insert {
    let n = cm.extrusion;
    let mirrored = n.z < 0.0 && n.x.hypot(n.y) <= 1e-3 * n.z.abs();
    let mut p = v3(i.insert_point);
    let mut scale = Vec3::new(i.x_scale(), i.y_scale(), i.z_scale());
    let mut rotation = i.rotation;
    let mut col_spacing = i.column_spacing;
    if mirrored {
        p = to_wcs(n, p);
        scale.x = -scale.x;
        rotation = -rotation;
        col_spacing = -col_spacing;
    }
    Insert {
        block: i.block_name.clone(),
        insert: p,
        scale,
        rotation,
        attribs: i.attributes.iter().map(|a| attrib(a, &cm.layer)).collect(),
        cols: u32::from(i.column_count.clamp(1, 10_000)),
        rows: u32::from(i.row_count.clamp(1, 10_000)),
        col_spacing,
        row_spacing: i.row_spacing,
        clip: None,
    }
}

fn dimension(d: &ADim, cm: &Common, names: &Names) -> Dimension {
    let b = d.base();
    let zero = Vec3::ZERO;
    // (kind, 10, 13, 14, 15, 16) as acadrust's DXF writer writes them.
    let (kind, defpt, p13, p14, p15, p16) = match d {
        ADim::Linear(x) => (DimKind::Linear { rotation: x.rotation }, x.definition_point, v3(x.first_point), v3(x.second_point), zero, zero),
        ADim::Aligned(x) => (DimKind::Aligned, x.definition_point, v3(x.first_point), v3(x.second_point), zero, zero),
        ADim::Radius(x) => (DimKind::Radius, x.angle_vertex, zero, zero, v3(x.definition_point), zero),
        ADim::Diameter(x) => (DimKind::Diameter, x.definition_point, zero, zero, v3(x.angle_vertex), zero),
        ADim::Angular2Ln(x) => {
            (DimKind::Angular, x.definition_point, v3(x.first_point), v3(x.second_point), v3(x.angle_vertex), v3(x.dimension_arc))
        }
        ADim::Angular3Pt(x) => (DimKind::Angular3P, x.definition_point, v3(x.first_point), v3(x.second_point), v3(x.angle_vertex), zero),
        ADim::Ordinate(x) => {
            (DimKind::Ordinate { x_type: x.is_ordinate_type_x }, x.definition_point, v3(x.feature_location), v3(x.leader_endpoint), zero, zero)
        }
        ADim::Arc(x) => (DimKind::ArcLength, x.definition_point, v3(x.first_extension_point), v3(x.second_extension_point), v3(x.center_point), zero),
        ADim::LargeRadial(x) => (DimKind::Radius, x.definition_point, zero, zero, v3(x.chord_point), zero),
    };
    let measurement = match d {
        ADim::Ordinate(x) => x.measurement(),
        _ => b.actual_measurement,
    };
    let text_mid = if cm.extrusion.z < 0.0 { to_wcs(cm.extrusion, v3(b.text_middle_point)) } else { v3(b.text_middle_point) };
    let mut dim = Dimension {
        kind,
        defpt: v3(defpt),
        text_mid,
        p13,
        p14,
        p15,
        p16,
        text: b.text_override().unwrap_or_default().to_string(),
        style: if b.style_name.is_empty() { "Standard".into() } else { b.style_name.clone() },
        measurement,
        text_rotation: b.text_rotation,
        user_text_pos: b.text_user_positioned,
        block: Some(b.block_name.clone()).filter(|s| !s.is_empty()),
        insertion_point: v3(b.insertion_point),
        overrides: crate::tables::dstyle_overrides(&b.common.extended_data, names),
        assoc: Vec::new(),
        marks: Default::default(),
    };
    let ext_rot = match d {
        ADim::Linear(x) => x.ext_line_rotation,
        ADim::Aligned(x) => x.ext_line_rotation,
        _ => 0.0,
    };
    let base = match dim.kind {
        DimKind::Linear { rotation } => Some(rotation),
        DimKind::Aligned => Some((dim.p14.xy() - dim.p13.xy()).angle()),
        _ => None,
    };
    let o = ext_rot.to_degrees();
    if let Some(base) = base
        && o.is_finite()
        && o.rem_euclid(180.0) != 0.0
    {
        dim.marks.oblique = Some(cadcraft_geom::norm_angle(base + ext_rot));
    }
    dim
}

fn viewport(v: &ae::Viewport, names: &Names) -> Viewport {
    let twist = cadcraft_geom::norm_angle(v.twist_angle);
    let c = v3(v.view_center).xy().rotate(-twist) + v3(v.view_target).xy();
    let mut seen = std::collections::HashSet::new();
    Viewport {
        center: v3(v.center),
        width: v.width,
        height: v.height,
        view_center: if c.is_finite() { c } else { v3(v.view_center).xy() },
        view_height: v.view_height,
        id: u32::try_from(v.id).ok().filter(|i| (1..=32767).contains(i)).unwrap_or(0),
        locked: v.status.locked,
        frozen_layers: v.frozen_layers.iter().filter_map(|l| names.layers.get(&l.value()).cloned()).filter(|n| seen.insert(up(n))).collect(),
        layer_colors: Vec::new(),
        clip: (v.status.non_rectangular_clipping && !v.clip_boundary_handle.is_null()).then(|| h(v.clip_boundary_handle)),
        on: !v.status.is_off,
        twist,
    }
}

fn image(i: &ae::RasterImage, names: &Names) -> Image {
    let def = Image::default();
    let mut clip: Vec<Vec2> = i.clip_boundary.vertices.iter().map(|v| v2(*v)).collect();
    if matches!(i.clip_boundary.clip_type, ae::ClipType::Rectangular) {
        clip.truncate(2);
    }
    let pct = |v: u8| v.min(100);
    let from_def = i.definition_handle.and_then(|d| names.images.get(&d.value()));
    Image {
        insert: v3(i.insertion_point),
        u: v3(i.u_vector),
        v: v3(i.v_vector),
        size: v2(i.size),
        path: from_def.map(|d| d.0.clone()).filter(|p| !p.is_empty()).unwrap_or_else(|| i.file_path.clone()),
        clip,
        clipping: i.clipping_enabled,
        display: u16::try_from(i.flags.bits()).unwrap_or(def.display),
        brightness: pct(i.brightness),
        contrast: pct(i.contrast),
        fade: pct(i.fade),
        pixel_size: from_def.map_or(def.pixel_size, |d| d.1),
        resolution_units: from_def.map_or(def.resolution_units, |d| d.2),
        ..def
    }
}

// ── Hatches ──────────────────────────────────────────────────────────────────────────────────

pub fn hatch(hx: &ae::Hatch, cm: &mut Common) -> EntityKind {
    let x = &cm.xdata;
    let own = crate::conv::xdata_tags(&hx.common.extended_data);
    let (origin, background) = hatch_xdata(&own);
    let _ = x;
    let pattern = if hx.pattern.name.is_empty() { "SOLID".to_string() } else { hx.pattern.name.clone() };
    let mut loops = Vec::new();
    for path in &hx.paths {
        let flags = path.flags.bits();
        let mut verts = Vec::new();
        for edge in &path.edges {
            match edge {
                ae::BoundaryEdge::Polyline(p) => {
                    verts.extend(p.vertices.iter().map(|v| PolyVertex::with_bulge(Vec2::new(v.x, v.y), v.z)));
                }
                ae::BoundaryEdge::Line(l) => {
                    let (s, e) = (v2(l.start), v2(l.end));
                    if verts.last().is_none_or(|last: &PolyVertex| !last.p.near(s, 1e-9)) {
                        verts.push(PolyVertex::new(s));
                    }
                    verts.push(PolyVertex::new(e));
                }
                ae::BoundaryEdge::CircularArc(a) => circular_edge(a, &mut verts),
                ae::BoundaryEdge::EllipticArc(a) => elliptic_edge(a, &mut verts),
                ae::BoundaryEdge::Spline(s) => spline_edge(s, &mut verts),
            }
        }
        if verts.len() > 1 && verts.first().map(|f| f.p).zip(verts.last().map(|l| l.p)).is_some_and(|(a, b)| a.near(b, 1e-9)) {
            verts.pop();
        }
        let sources: Vec<Handle> = path.boundary_handles.iter().filter(|b| !b.is_null()).take(4096).map(|b| h(*b)).collect();
        loops.push(HatchLoop { vertices: verts, outer: flags & 1 != 0, sources });
    }
    let solid = hx.is_solid || pattern.eq_ignore_ascii_case("SOLID");
    let (scale, angle) = (hx.pattern_scale, hx.pattern_angle);
    let pattern_lines = if solid { Vec::new() } else { pattern_definition(&hx.pattern.lines, scale, angle, origin) };
    let mut h2 = Hatch {
        pattern: pattern.to_ascii_uppercase(),
        solid,
        loops,
        scale,
        angle,
        associative: hx.is_associative,
        style: hx.style as u8,
        elevation: if hx.elevation.is_finite() { hx.elevation } else { 0.0 },
        gradient: gradient(hx, &own),
        origin,
        background,
        pattern_lines,
    };
    if tilted(cm.extrusion, h2.loops.iter().flat_map(|l| run_points(&l.vertices, h2.elevation, true))) {
        let n = cm.extrusion;
        let m = cadcraft_geom::Mat4::ocs(n);
        let (mut sum, mut count) = (0.0, 0usize);
        for l in &mut h2.loops {
            let (v, z) = plan_of_tilted(n, h2.elevation, &l.vertices, true);
            sum += z * v.len() as f64;
            count += v.len();
            l.vertices = v;
        }
        let plan_angle = |a: f64| {
            let dir = m.apply(Vec3::new(a.cos(), a.sin(), 0.0)).xy();
            if dir.len() > 1e-9 { cadcraft_geom::norm_angle(dir.y.atan2(dir.x)) } else { a }
        };
        h2.angle = plan_angle(h2.angle);
        if let Some(g) = h2.gradient.as_mut() {
            g.angle = plan_angle(g.angle);
        }
        h2.origin = m.apply(Vec3::new(h2.origin.x, h2.origin.y, h2.elevation)).xy();
        if count > 0 {
            h2.elevation = sum / count as f64;
        }
        cm.extrusion = Vec3::Z;
    } else if cm.extrusion.z < 0.0 {
        for v in h2.loops.iter_mut().flat_map(|l| l.vertices.iter_mut()) {
            v.p.x = -v.p.x;
            v.bulge = -v.bulge;
        }
        h2.angle = cadcraft_geom::norm_angle(std::f64::consts::PI - h2.angle);
    }
    EntityKind::Hatch(h2)
}

fn circular_edge(a: &ae::CircularArcEdge, vertices: &mut Vec<PolyVertex>) {
    let center = v2(a.center);
    let ccw = a.counter_clockwise;
    let (start, end) = if ccw { (a.start_angle, a.end_angle) } else { (-a.start_angle, -a.end_angle) };
    let start_point = Vec2::polar(center, a.radius, start);
    let end_point = Vec2::polar(center, a.radius, end);
    let sweep = if ccw { cadcraft_geom::ccw_sweep(start, end) } else { -cadcraft_geom::ccw_sweep(end, start) };
    if vertices.last().is_none_or(|last| !last.p.near(start_point, 1e-9)) {
        vertices.push(PolyVertex::new(start_point));
    }
    let full = (sweep.abs() - std::f64::consts::TAU).abs() < 1e-9;
    let bulge = cadcraft_geom::arc_to_bulge(if full { sweep / 2.0 } else { sweep });
    if let Some(last) = vertices.last_mut() {
        last.bulge = bulge;
    }
    if full {
        vertices.push(PolyVertex::with_bulge(Vec2::polar(center, a.radius, start + sweep / 2.0), bulge));
    }
    vertices.push(PolyVertex::new(end_point));
}

fn elliptic_edge(a: &ae::EllipticArcEdge, vertices: &mut Vec<PolyVertex>) {
    let ccw = a.counter_clockwise;
    let (start, end) = if ccw { (a.start_angle, a.end_angle) } else { (-a.end_angle, -a.start_angle) };
    let major = v2(a.major_axis_endpoint);
    let ellipse = cadcraft_geom::Ellipse { center: v2(a.center), major, ratio: a.minor_axis_ratio, start, end };
    let mut points = Vec::new();
    ellipse.tessellate(major.len() * 1e-3, &mut points);
    if !ccw {
        points.reverse();
    }
    vertices.extend(points.into_iter().map(PolyVertex::new));
}

fn spline_edge(s: &ae::SplineEdge, vertices: &mut Vec<PolyVertex>) {
    let degree = usize::try_from(s.degree.clamp(1, 10)).unwrap_or(3);
    let control: Vec<Vec2> = s.control_points.iter().map(|p| Vec2::new(p.x, p.y)).collect();
    // A rational edge's weights are the control points' third value.
    let weights: Vec<f64> = if s.rational { s.control_points.iter().map(|p| p.z).collect() } else { Vec::new() };
    let fit: Vec<Vec2> = s.fit_points.iter().map(|p| v2(*p)).collect();
    let spline = Spline { degree, knots: s.knots.clone(), control, weights, ..Spline::default() };
    let spline = if spline.is_valid() {
        spline
    } else if spline.control.is_empty() && fit.len() > 1 {
        Spline::from_fit_points(&fit)
    } else {
        Spline::from_control(spline.control, degree)
    };
    let points = spline.tessellate(1e-3);
    let skip = usize::from(vertices.last().zip(points.first()).is_some_and(|(last, point)| last.p.near(*point, 1e-9)));
    vertices.extend(points.into_iter().skip(skip).map(PolyVertex::new));
}

/// The pattern lines as the library keeps them (unit scale, no hatch rotation, base point
/// relative to the hatch origin, offset along and across the line), from the file's drawn ones.
fn pattern_definition(lines: &[ae::HatchPatternLine], scale: f64, angle: f64, origin: Vec2) -> Vec<cadcraft_doc::library::PatternLine> {
    let scale = if scale.is_finite() && scale > 1e-9 { scale } else { 1.0 };
    let angle = if angle.is_finite() { angle } else { 0.0 };
    lines
        .iter()
        .take(1024)
        .filter(|l| l.angle.is_finite() && v2(l.base_point).is_finite() && v2(l.offset).is_finite() && l.dash_lengths.iter().all(|d| d.is_finite()))
        .map(|l| {
            let local = (v2(l.base_point) - origin).rotate(-angle) * (1.0 / scale);
            let delta = v2(l.offset).rotate(-l.angle) * (1.0 / scale);
            cadcraft_doc::library::PatternLine {
                angle: l.angle.to_degrees() - angle.to_degrees(),
                origin: (local.x, local.y),
                delta: (delta.x, delta.y),
                dashes: l.dash_lengths.iter().take(256).map(|d| d / scale).collect(),
            }
        })
        .collect()
}

/// Hatch origin and background colour from `CADCRAFT` xdata, as the DXF reader takes them.
fn hatch_xdata(tags: &[cadcraft_doc::RawTag]) -> (Vec2, Option<cadcraft_color::Color>) {
    let x = crate::conv::xdata_app(tags, "CADCRAFT");
    let Some(at) = x.windows(2).position(|w| matches!(w, [m, o] if m.code == 1000 && m.value == "HATCH" && o.code == 1011)) else {
        return (Vec2::ZERO, None);
    };
    let val = |k: usize, code: i32| x.get(at + k).filter(|t| t.code == code);
    let coord = |k: usize, code: i32| val(k, code).and_then(|t| t.value.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(0.0);
    let origin = Vec2::new(coord(1, 1011), coord(2, 1021));
    (origin, val(4, 1000).and_then(|t| cadcraft_color::Color::parse(&t.value)))
}

fn gradient(hx: &ae::Hatch, own: &[cadcraft_doc::RawTag]) -> Option<cadcraft_doc::Gradient> {
    let g = &hx.gradient_color;
    let default = cadcraft_doc::Gradient {
        name: "LINEAR".into(),
        color1: cadcraft_color::Color::Index(5),
        color2: cadcraft_color::Color::Index(7),
        angle: 0.0,
        centered: true,
    };
    if g.enabled {
        let pick = |i: usize, d: cadcraft_color::Color| g.colors.get(i).map(|c| color(&c.color)).unwrap_or(d);
        return Some(cadcraft_doc::Gradient {
            name: if g.name.is_empty() { default.name.clone() } else { g.name.clone() },
            color1: pick(0, default.color1),
            color2: pick(1, default.color2),
            angle: if g.angle.is_finite() { g.angle } else { 0.0 },
            centered: g.shift.abs() < 1e-9,
        });
    }
    // CADCraft's `GRADIENT` xdata (kept by R2000 files).
    let x = crate::conv::xdata_app(own, "CADCRAFT");
    let at = x.iter().position(|t| t.code == 1000 && t.value == "GRADIENT")?;
    let o = x.get(at + 1..).unwrap_or(&[]);
    let strs: Vec<String> = o.iter().filter(|t| t.code == 1000).take(3).map(|t| t.value.clone()).collect();
    let col = |i: usize, d: cadcraft_color::Color| strs.get(i).and_then(|s| cadcraft_color::Color::parse(s)).unwrap_or(d);
    Some(cadcraft_doc::Gradient {
        name: strs.first().cloned().unwrap_or(default.name.clone()),
        color1: col(1, default.color1),
        color2: col(2, default.color2),
        angle: o.iter().find(|t| t.code == 1040).and_then(|t| t.value.trim().parse::<f64>().ok()).filter(|a| a.is_finite()).unwrap_or(0.0),
        centered: o.iter().find(|t| t.code == 1070).is_none_or(|t| t.value.trim() != "0"),
    })
}
