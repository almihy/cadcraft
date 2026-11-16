//! XCLIP, both ways (DXF Reference, SPATIAL_FILTER): a clipped block reference has an extension
//! dictionary whose `ACAD_FILTER` dictionary's `SPATIAL` entry is a spatial filter. The filter
//! holds the boundary in the reference's coordinates, whether clipping is on, the front and back
//! clip, and the inverse of the reference's transform, which takes the boundary into the block's
//! coordinates (where CADCraft keeps it). A filter CADCraft can't hold (front or back clipping, a
//! tilted or transformed boundary) stays as the file has it.

use std::collections::HashMap;

use acadrust::CadDocument;
use acadrust::entities::EntityType;
use acadrust::objects::{Dictionary, ObjectType, SpatialFilter};
use acadrust::types::{Handle as AHandle, Matrix4, Vector2, Vector3};
use cadcraft_doc::{ClipBoundary, Drawing, EntityKind, MAX_CLIP_VERTICES};
use cadcraft_geom::{Mat3, Vec2};

fn entry(doc: &CadDocument, dict: AHandle, key: &str) -> Option<AHandle> {
    match doc.objects.get(&dict)? {
        ObjectType::Dictionary(d) => d.entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, h)| *h),
        _ => None,
    }
}

/// The spatial filter of a reference whose extension dictionary is `xdict`.
fn filter_of(doc: &CadDocument, xdict: AHandle) -> Option<(AHandle, &SpatialFilter)> {
    let fd = entry(doc, xdict, "ACAD_FILTER")?;
    let sf = entry(doc, fd, "SPATIAL")?;
    match doc.objects.get(&sf)? {
        ObjectType::SpatialFilter(f) => Some((sf, f)),
        _ => None,
    }
}

fn mat(m: &Matrix4) -> Option<Mat3> {
    let r = &m.m;
    let flat = [r[0][2], r[1][2], r[2][0], r[2][1], r[2][3]].iter().all(|v| v.abs() <= 1e-9) && (r[2][2] - 1.0).abs() <= 1e-9;
    let (a, c, e, b, d, f) = (r[0][0], r[0][1], r[0][3], r[1][0], r[1][1], r[1][3]);
    (flat && [a, b, c, d, e, f].iter().all(|v| v.is_finite())).then_some(Mat3 { a, b, c, d, e, f })
}

fn matrix(m: &Mat3) -> Matrix4 {
    Matrix4 { m: [[m.a, m.c, 0.0, m.e], [m.b, m.d, 0.0, m.f], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]] }
}

/// The boundary a planar filter holds, in the block's coordinates; `None` for one the model can't
/// hold as it is.
fn clip(f: &SpatialFilter) -> Option<ClipBoundary> {
    let n = f.boundary_points.len();
    if !(2..=MAX_CLIP_VERTICES).contains(&n) {
        return None;
    }
    let up = f.normal.x.abs() <= 1e-9 && f.normal.y.abs() <= 1e-9 && (f.normal.z - 1.0).abs() <= 1e-9;
    if !up || f.front_clip.is_some() || f.back_clip.is_some() {
        return None;
    }
    let own = mat(&f.clip_bound_transform)?;
    let identity = |m: &Mat3| (m.a - 1.0).abs() <= 1e-9 && m.b.abs() <= 1e-9 && m.c.abs() <= 1e-9 && (m.d - 1.0).abs() <= 1e-9 && m.e.abs() <= 1e-9 && m.f.abs() <= 1e-9;
    if !identity(&own) {
        return None;
    }
    let inverse = mat(&f.inverse_block_transform)?;
    let points: Vec<Vec2> = f.boundary_points.iter().map(|p| inverse.apply(Vec2::new(p.x, p.y))).collect();
    let c = ClipBoundary { points, enabled: f.display_enabled, inverted: false };
    (points_ok(&c)).then_some(c)
}

fn points_ok(c: &ClipBoundary) -> bool {
    c.points.iter().all(|p| p.is_finite()) && !c.polygon().is_empty()
}

/// The clips of the file's block references, by the reference's handle.
pub fn read(doc: &CadDocument) -> HashMap<u64, ClipBoundary> {
    let mut out = HashMap::new();
    for e in doc.entities() {
        let EntityType::Insert(_) = e else { continue };
        let c = e.common();
        let Some(xd) = c.xdictionary_handle else { continue };
        if let Some((_, f)) = filter_of(doc, xd)
            && let Some(clip) = clip(f)
        {
            out.insert(c.handle.value(), clip);
        }
    }
    out
}

/// Every reference's clip written onto its filter: made when the reference gained one, changed
/// when it changed, removed when it was taken off. Filters the model can't hold stay.
pub fn write(d: &Drawing, doc: &mut CadDocument) {
    let mut want: Vec<(AHandle, Option<(ClipBoundary, Mat3)>)> = Vec::new();
    let stores = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities));
    for s in stores {
        for e in s.iter() {
            let EntityKind::Insert(i) = &e.kind else { continue };
            let base = d.block(&i.block).map_or(Vec2::ZERO, |b| b.base.xy());
            want.push((AHandle::new(e.handle.0), i.clip.clone().map(|c| (c, i.transform(base)))));
        }
    }
    for (h, clip) in want {
        let xd = doc.get_entity(h).and_then(|e| e.common().xdictionary_handle);
        let current = xd.and_then(|x| filter_of(doc, x));
        let held = current.as_ref().map(|(_, f)| (clip_of(f), f));
        match (&clip, held) {
            // A filter the model can't hold, on a reference that has no clip of its own: kept.
            (None, Some((None, _))) | (None, None) => {}
            (None, Some((Some(_), _))) => {
                if let (Some(x), Some((sf, _))) = (xd, current) {
                    if let Some(fd) = entry(doc, x, "ACAD_FILTER")
                        && let Some(ObjectType::Dictionary(dc)) = doc.objects.get_mut(&fd)
                    {
                        dc.entries.retain(|(k, _)| !k.eq_ignore_ascii_case("SPATIAL"));
                    }
                    doc.objects.remove(&sf);
                }
            }
            (Some((c, _)), Some((Some(old), _))) if old == *c => {}
            (Some((c, to_world)), _) => put(doc, h, c, to_world),
        }
    }
}

fn clip_of(f: &SpatialFilter) -> Option<ClipBoundary> {
    clip(f)
}

/// Give reference `h` the filter of `c` (in the block's coordinates; `to_world` the reference's
/// transform), making its extension dictionary and `ACAD_FILTER` dictionary when it has none.
fn put(doc: &mut CadDocument, h: AHandle, c: &ClipBoundary, to_world: &Mat3) {
    let Some(inverse) = to_world.inverse() else { return };
    let xd = match doc.get_entity(h).and_then(|e| e.common().xdictionary_handle) {
        Some(x) => x,
        None => {
            let x = doc.allocate_handle();
            let mut dict = Dictionary::new();
            dict.handle = x;
            dict.owner = h;
            dict.hard_owner = true;
            doc.objects.insert(x, ObjectType::Dictionary(dict));
            if let Some(e) = doc.get_entity_mut(h) {
                e.common_mut().xdictionary_handle = Some(x);
                e.common_mut().raw_record = None;
            }
            x
        }
    };
    let fd = match entry(doc, xd, "ACAD_FILTER") {
        Some(f) => f,
        None => {
            let f = doc.allocate_handle();
            let mut dict = Dictionary::new();
            dict.handle = f;
            dict.owner = xd;
            doc.objects.insert(f, ObjectType::Dictionary(dict));
            if let Some(ObjectType::Dictionary(x)) = doc.objects.get_mut(&xd) {
                x.add_entry("ACAD_FILTER", f);
            }
            f
        }
    };
    let sf = entry(doc, fd, "SPATIAL").filter(|s| matches!(doc.objects.get(s), Some(ObjectType::SpatialFilter(_))));
    let mut filter = match sf.and_then(|s| doc.objects.get(&s)) {
        Some(ObjectType::SpatialFilter(f)) => f.clone(),
        _ => {
            let mut f = SpatialFilter::new();
            f.handle = doc.allocate_handle();
            f.owner = fd;
            f
        }
    };
    filter.boundary_points = c.points.iter().map(|p| to_world.apply(*p)).map(|p| Vector2::new(p.x, p.y)).collect();
    filter.normal = Vector3::new(0.0, 0.0, 1.0);
    filter.display_enabled = c.enabled;
    filter.front_clip = None;
    filter.back_clip = None;
    filter.inverse_block_transform = matrix(&inverse);
    filter.clip_bound_transform = matrix(&Mat3::IDENTITY);
    let fh = filter.handle;
    doc.objects.insert(fh, ObjectType::SpatialFilter(filter));
    if sf.is_none()
        && let Some(ObjectType::Dictionary(x)) = doc.objects.get_mut(&fd)
    {
        x.entries.retain(|(k, _)| !k.eq_ignore_ascii_case("SPATIAL"));
        x.add_entry("SPATIAL", fh);
    }
}
