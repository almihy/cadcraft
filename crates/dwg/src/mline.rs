//! Multilines (MLINE), both ways. A multiline holds its vertices, each with the direction of the
//! segment that starts there, the miter direction and, per style element, the element's
//! distance along the miter followed by where it runs and breaks on that segment; its style
//! gives the elements' offsets, colours and linetypes, the caps and the fill.

use acadrust::entities::{self as ae};
use acadrust::types::Vector3;
use cadcraft_doc::{
    MAX_MLINE_CUTS, MAX_MLINE_ELEMENTS, MAX_MLINE_VERTICES, MLine, MLineCut, MLineElement, MLineJust, MLineStyle, MLineVertex, STANDARD_MLINE_STYLE,
};
use cadcraft_geom::{Vec2, Vec3};

use crate::conv::{a3, v3};

fn finite(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

/// Most parameters kept per element and vertex (start, then break pairs).
const MAX_PARAMS: usize = 2 * MAX_MLINE_CUTS + 2;

/// A multiline as CADCraft's: `style` its style's name, `styles` the drawing's styles (the one of
/// that name gives the elements, caps and fill when it has as many elements as the multiline).
pub fn read(m: &ae::MLine, style: &str, styles: &[MLineStyle]) -> MLine {
    let verts: Vec<&ae::MLineVertex> = m.vertices.iter().take(MAX_MLINE_VERTICES).collect();
    let params = |v: &ae::MLineVertex| -> Vec<Vec<f64>> {
        v.segments.iter().take(MAX_MLINE_ELEMENTS).map(|s| s.parameters.iter().take(MAX_PARAMS).map(|x| finite(*x)).collect()).collect()
    };
    let scale = finite(m.scale_factor);
    let nelem = verts.first().map(|v| v.segments.len()).unwrap_or(0).clamp(1, MAX_MLINE_ELEMENTS);
    // Element offsets from the first vertex: its distance along the miter, measured across the
    // first segment.
    let fin = |p: Vector3| {
        let v = v3(p);
        Vec3::new(finite(v.x), finite(v.y), finite(v.z))
    };
    let pts: Vec<Vec2> = verts.iter().map(|v| fin(v.position).xy()).collect();
    let first = verts.first();
    let dir0 = first.map(|v| fin(v.direction).xy().normalized()).filter(|d| *d != Vec2::ZERO).unwrap_or_else(|| match (pts.first(), pts.get(1)) {
        (Some(a), Some(b)) => (*b - *a).normalized(),
        _ => Vec2::X,
    });
    let miter0 = first.map(|v| fin(v.miter).xy().normalized()).filter(|d| *d != Vec2::ZERO).unwrap_or(dir0.perp());
    let across = miter0.dot(dir0.perp());
    let s = if scale.abs() > 1e-12 { scale } else { 1.0 };
    let first_params = first.map(|v| params(v)).unwrap_or_default();
    let elements: Vec<MLineElement> = (0..nelem)
        .map(|e| {
            let d = first_params.get(e).and_then(|l| l.first()).copied().unwrap_or(0.0) * across;
            MLineElement { offset: finite(d / s), ..MLineElement::default() }
        })
        .collect();
    let name = if style.trim().is_empty() { STANDARD_MLINE_STYLE.to_string() } else { style.to_string() };
    let def = match styles.iter().find(|x| x.name.eq_ignore_ascii_case(name.trim())) {
        Some(st) if st.elements.len() == elements.len() => st.clone(),
        _ => MLineStyle { name: name.clone(), elements, ..MLineStyle::default() },
    };
    let vertices = verts
        .iter()
        .map(|v| {
            let mut cuts = Vec::new();
            for (e, list) in params(v).iter().enumerate() {
                cuts.extend(cadcraft_doc::mline_breaks(list).into_iter().map(|(start, end)| MLineCut { element: e, start, end }));
            }
            cuts.truncate(MAX_MLINE_CUTS);
            MLineVertex { p: fin(v.position), cuts }
        })
        .collect();
    let flags = m.flags;
    MLine {
        style: def.name.clone(),
        def,
        scale,
        justification: match m.justification {
            ae::MLineJustification::Top => MLineJust::Top,
            ae::MLineJustification::Zero => MLineJust::Zero,
            ae::MLineJustification::Bottom => MLineJust::Bottom,
        },
        closed: flags.contains(ae::MLineFlags::CLOSED),
        no_start_caps: flags.contains(ae::MLineFlags::NO_START_CAPS),
        no_end_caps: flags.contains(ae::MLineFlags::NO_END_CAPS),
        vertices,
    }
}

/// Write CADCraft's multiline onto acadrust's: its vertices with their directions, miters and
/// element parameters, its style (`style_handle`), scale, justification and flags.
pub fn put(x: &mut ae::MLine, m: &MLine, style_handle: Option<acadrust::types::Handle>) {
    x.style_name = m.style.chars().take(32).collect();
    x.style_handle = style_handle;
    x.scale_factor = finite(m.scale);
    x.justification = match m.justification {
        MLineJust::Top => ae::MLineJustification::Top,
        MLineJust::Zero => ae::MLineJustification::Zero,
        MLineJust::Bottom => ae::MLineJustification::Bottom,
    };
    let mut flags = ae::MLineFlags::empty();
    flags.set(ae::MLineFlags::HAS_VERTICES, !m.vertices.is_empty());
    flags.set(ae::MLineFlags::CLOSED, m.closed);
    flags.set(ae::MLineFlags::NO_START_CAPS, m.no_start_caps);
    flags.set(ae::MLineFlags::NO_END_CAPS, m.no_end_caps);
    x.flags = flags;
    x.style_element_count = m.def.elements.len();
    x.normal = Vector3::new(0.0, 0.0, 1.0);
    x.start_point = a3(m.vertices.first().map(|v| v.p).unwrap_or_default());
    let pts = m.points();
    let n = pts.len();
    let dirs = m.directions();
    let miters = m.miters();
    let offs = m.offsets();
    let nseg = m.segment_count();
    x.vertices = m
        .vertices
        .iter()
        .enumerate()
        .map(|(k, v)| {
            let dir = dirs.get(k.min(nseg.saturating_sub(1))).copied().unwrap_or(Vec2::X);
            let mv = miters.get(k).copied().unwrap_or(Vec2::Y);
            let stretch = mv.len();
            let has_seg = k < nseg;
            let segments = offs
                .iter()
                .enumerate()
                .map(|(e, d)| {
                    let along = d * stretch;
                    let a = pts.get(k).copied().unwrap_or_default() + mv * *d;
                    let j = (k + 1) % n.max(1);
                    let b = pts.get(j).copied().unwrap_or_default() + miters.get(j).copied().unwrap_or_default() * *d;
                    let len = if has_seg { a.dist(b) } else { 0.0 };
                    let on = cadcraft_doc::mline_drawn(len, v.cuts.iter().filter(|c| c.element == e).map(|c| (c.start, c.end)));
                    let parameters = if has_seg { cadcraft_doc::mline_params(along, &on, len) } else { vec![along, 0.0] };
                    ae::MLineSegment { parameters, area_fill_parameters: Vec::new() }
                })
                .collect();
            ae::MLineVertex { position: a3(v.p), direction: a3(dir.to3(0.0)), miter: a3(mv.normalized().to3(0.0)), segments }
        })
        .collect();
}
