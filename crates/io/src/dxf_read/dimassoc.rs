use super::common::T;
use super::{Drawing, Handle, Tag};
use cadcraft_doc::dimassoc::OsnapRef;

/// Associativity from a DIMASSOC object (files from other writers): its references, given to
/// [`cadcraft_doc::dimassoc::apply`].
pub(super) fn dimassoc(d: &mut Drawing, tags: &[Tag]) {
    let start = tags.iter().position(|x| x.code == 100 && x.str() == "AcDbDimAssoc").unwrap_or(0);
    let tail = tags.get(start..).unwrap_or(&[]);
    let Some(dim_h) = tail.iter().find(|x| x.code == 330).and_then(|x| Handle::parse_hex(&x.str())) else { return };
    let flags = T(tail).i(90).unwrap_or(0);
    // Point references start at each group 1 (their class name).
    let starts: Vec<usize> = tail.iter().enumerate().filter(|(_, x)| x.code == 1).map(|(i, _)| i).take(8).collect();
    let mut refs = Vec::new();
    for (k, s) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(tail.len());
        let r = T(tail.get(s + 1..end).unwrap_or(&[]));
        refs.push(OsnapRef {
            point_ref: tail.get(*s).is_some_and(|x| x.str() == "AcDbOsnapPointRef"),
            osnap: r.i(72).unwrap_or(0),
            main: r.s(331).and_then(|h| Handle::parse_hex(&h)),
            other: r.s(332).and_then(|h| Handle::parse_hex(&h)),
            at: r.p(10).xy(),
            segment: r.i(91).filter(|k| *k >= 1 && r.i(73) == Some(2)),
        });
    }
    cadcraft_doc::dimassoc::apply(d, dim_h, flags, &refs);
}
