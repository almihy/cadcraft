//! Object coordinate systems (OCS): 2D objects stored in the plane of their extrusion, as a DXF or
//! DWG file holds them, turned into what CADCraft draws (plan coordinates seen from above). Shared
//! by the DXF and DWG readers, and the inverse for the DWG writer.

use crate::geom::{PolyVertex, Vec2, Vec3};
use crate::{HAlign, Text, VAlign};

/// A point of the object coordinate system of extrusion `n` in the WCS (DXF Reference, arbitrary
/// axis algorithm).
pub fn to_wcs(n: Vec3, p: Vec3) -> Vec3 {
    if (n.z - 1.0).abs() < 1e-12 && n.x.abs() < 1e-12 && n.y.abs() < 1e-12 { p } else { crate::geom::Mat4::ocs(n).apply(p) }
}

/// Whether an object with OCS points `pts` (x, y and elevation) lies in a plane tilted off the
/// XY plane: drawing its points as plan coordinates (x flipped for a mirrored plane, as the
/// readers do) would put some of them off their place seen from above by more than a
/// billionth of the object's size and distance from the origin. A normal that is the Z axis
/// or the Z axis turned over moves nothing, and noise in one moves points by a negligible
/// amount, so those objects keep their arcs; however small the tilt, an object far enough from
/// the origin to be moved visibly is projected.
pub fn tilted(n: Vec3, pts: impl IntoIterator<Item = Vec3>) -> bool {
    if !n.is_finite() || n.len() < 1e-12 {
        return false;
    }
    let m = crate::geom::Mat4::ocs(n);
    let flip = if n.z < 0.0 { -1.0 } else { 1.0 };
    let (mut moved, mut reach) = (0.0f64, 1.0f64);
    for p in pts.into_iter().filter(|p| p.is_finite()) {
        let shown = Vec2::new(flip * p.x, p.y);
        moved = moved.max((m.apply(p).xy() - shown).len());
        reach = reach.max(p.x.abs()).max(p.y.abs()).max(p.z.abs());
    }
    moved > 1e-9 * reach
}

/// Points along a run of OCS vertices at `elevation`, for [`tilted`]: its vertices and, on its
/// arcs, points between them (an arc can bulge where its ends don't move).
pub fn run_points(vertices: &[PolyVertex], elevation: f64, closed: bool) -> Vec<Vec3> {
    let mut pts = Vec::with_capacity(vertices.len());
    for (i, v) in vertices.iter().enumerate() {
        pts.push(v.p);
        let next = if i + 1 < vertices.len() {
            vertices.get(i + 1)
        } else if closed {
            vertices.first()
        } else {
            None
        };
        if let Some(w) = next.filter(|_| v.bulge != 0.0) {
            // Each arc sampled from its own size, so a small one next to a long edge counts too.
            let arc = crate::geom::Polyline { vertices: vec![*v, PolyVertex::new(w.p)], closed: false };
            let b = arc.bounds();
            let size = (b.max - b.min).len();
            pts.extend(arc.tessellate(if size.is_finite() && size > 0.0 { size * 0.05 } else { 1e-6 }));
        }
    }
    pts.into_iter().map(|p| p.to3(elevation)).collect()
}

/// The most vertices one tilted run is flattened to: past it, bulged edges are kept as their
/// chords, so a file full of arcs can't ask for gigabytes (one arc adds at most 4,096 more).
const MAX_TILTED_VERTICES: usize = 1_000_000;

/// A run of OCS vertices at `elevation` in the plane of `n` as seen from above in the WCS. A
/// tilted arc is an elliptical arc there, so each bulged edge is flattened (within a thousandth
/// of the edge's own size, up to [`MAX_TILTED_VERTICES`] in all), its width spread evenly along
/// it; vertices that start straight edges keep their widths. Also gives the mean WCS Z, the
/// elevation the flattened run takes.
pub fn plan_of_tilted(n: Vec3, elevation: f64, vertices: &[PolyVertex], closed: bool) -> (Vec<PolyVertex>, f64) {
    let m = crate::geom::Mat4::ocs(n);
    let mut local: Vec<PolyVertex> = Vec::with_capacity(vertices.len());
    for (i, v) in vertices.iter().enumerate() {
        let next = if i + 1 < vertices.len() {
            vertices.get(i + 1)
        } else if closed {
            vertices.first()
        } else {
            None
        };
        match next {
            Some(w) if v.bulge != 0.0 && local.len() < MAX_TILTED_VERTICES => {
                let arc = crate::geom::Polyline { vertices: vec![*v, PolyVertex::new(w.p)], closed: false };
                // From the edge's own size, so a long edge elsewhere can't flatten a small arc away.
                let b = arc.bounds();
                let size = (b.max - b.min).len();
                let edge = arc.tessellate(if size.is_finite() && size > 0.0 { size * 1e-3 } else { 1e-6 });
                // The edge runs from this vertex to the next one in equal turns of its arc; its
                // width changes evenly along it, as a tapered segment's does.
                let steps = edge.len().saturating_sub(1).max(1);
                let width = |k: usize| v.start_width + (v.end_width - v.start_width) * k as f64 / steps as f64;
                local.extend(edge.iter().take(steps).enumerate().map(|(k, p)| PolyVertex {
                    p: *p,
                    bulge: 0.0,
                    start_width: width(k),
                    end_width: width(k + 1),
                }));
            }
            _ => local.push(PolyVertex { bulge: 0.0, ..*v }),
        }
    }
    let to_plan = |p: Vec2| m.apply(Vec3::new(p.x, p.y, elevation));
    let z = if local.is_empty() { elevation } else { local.iter().map(|v| to_plan(v.p).z).sum::<f64>() / local.len() as f64 };
    (local.into_iter().map(|v| PolyVertex { p: to_plan(v.p).xy(), ..v }).collect(), z)
}

/// A point of the WCS in the object coordinate system of extrusion `n`: the inverse of
/// [`to_wcs`] (the OCS axes are orthonormal).
pub fn to_ocs(n: Vec3, p: Vec3) -> Vec3 {
    if (n.z - 1.0).abs() < 1e-12 && n.x.abs() < 1e-12 && n.y.abs() < 1e-12 {
        return p;
    }
    let m = crate::geom::Mat4::ocs(n);
    let o = m.apply(Vec3::ZERO);
    let ax = m.apply(Vec3::new(1.0, 0.0, 0.0)) - o;
    let ay = m.apply(Vec3::new(0.0, 1.0, 0.0)) - o;
    let az = m.apply(Vec3::new(0.0, 0.0, 1.0)) - o;
    let q = p - o;
    Vec3::new(q.dot(ax), q.dot(ay), q.dot(az))
}

/// TEXT / ATTRIB / ATTDEF with extrusion (0,0,-1): the points are OCS and the text runs from its
/// insertion point towards -x in the WCS, with mirrored glyphs. CADCraft draws text readable
/// (MIRRTEXT = 0), so it keeps the mirrored footprint: the direction turns to -rotation and
/// left/right justification swap ends (start ↔ end point).
pub fn mirror_text(t: &mut Text, align: Option<Vec3>, insert: Vec3) {
    t.rotation = cadcraft_geom::norm_angle(-t.rotation);
    let baseline_left = |t: &Text, p: Vec3| if t.valign == VAlign::Baseline { None } else { Some(p) };
    match t.halign {
        HAlign::Left => {
            // TL/ML/BL are placed by their alignment point, baseline-left text by its start point.
            t.halign = HAlign::Right;
            t.insert = insert;
            t.align_pt = Some(if t.valign == VAlign::Baseline { insert } else { align.unwrap_or(insert) });
        }
        HAlign::Right => {
            t.halign = HAlign::Left;
            t.insert = align.unwrap_or(insert);
            t.align_pt = baseline_left(t, t.insert);
        }
        HAlign::Aligned | HAlign::Fit => {
            t.insert = align.unwrap_or(insert);
            t.align_pt = Some(insert);
        }
        HAlign::Center | HAlign::Middle => {
            t.insert = insert;
            t.align_pt = align;
        }
    }
}

