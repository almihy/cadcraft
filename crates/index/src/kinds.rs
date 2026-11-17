//! The kinds an item is asked for and counted by: a few plain words for CADCraft's entity kinds.

use cadcraft_doc::EntityKind;

/// The kinds, as the agent asks for them.
pub const KINDS: &[&str] = &[
    "line",
    "polyline",
    "circle",
    "arc",
    "ellipse",
    "spline",
    "point",
    "text",
    "block",
    "dimension",
    "leader",
    "hatch",
    "mline",
    "image",
    "table",
    "other",
];

/// What kind of item an entity is.
pub fn kind_of(k: &EntityKind) -> &'static str {
    match k {
        EntityKind::Line(_) => "line",
        EntityKind::LwPolyline(_) | EntityKind::Polyline3d(_) => "polyline",
        EntityKind::Circle(_) => "circle",
        EntityKind::Arc(_) => "arc",
        EntityKind::Ellipse(_) => "ellipse",
        EntityKind::Spline(_) => "spline",
        EntityKind::Point(_) => "point",
        EntityKind::Text(_) | EntityKind::MText(_) | EntityKind::AttDef(_) => "text",
        EntityKind::Insert(_) => "block",
        EntityKind::Dimension(_) => "dimension",
        EntityKind::Leader(_) | EntityKind::MLeader(_) => "leader",
        EntityKind::Hatch(_) | EntityKind::Solid(_) | EntityKind::Trace(_) | EntityKind::Wipeout(_) => "hatch",
        EntityKind::MLine(_) => "mline",
        EntityKind::Image(_) => "image",
        EntityKind::Table(_) => "table",
        EntityKind::Helix(_)
        | EntityKind::Ray(_)
        | EntityKind::XLine(_)
        | EntityKind::Face3d(_)
        | EntityKind::PolyfaceMesh(_)
        | EntityKind::Region(_)
        | EntityKind::Viewport(_)
        | EntityKind::Tolerance(_)
        | EntityKind::Unknown(_) => "other",
    }
}

/// The kind a word names (any case, "blocks" or "texts" too), if it names one.
pub(crate) fn named(word: &str) -> Option<&'static str> {
    let w = word.trim().to_ascii_lowercase();
    let ones = [Some(w.as_str()), w.strip_suffix("es"), w.strip_suffix('s')];
    ones.into_iter().flatten().find_map(|one| KINDS.iter().find(|k| **k == one).copied())
}
