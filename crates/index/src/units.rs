//! The drawing's unit: its name, and millimetres to one of it.

use cadcraft_doc::Drawing;

/// The unit `$INSUNITS` names and millimetres to one of it. A unitless drawing, or one in a unit no
/// building is drawn in, is taken as millimetres (as Arqiflow's engine imports it).
pub(crate) fn of(d: &Drawing) -> (&'static str, f64) {
    match d.header.i64("INSUNITS", 0) {
        1 => ("in", 25.4),
        2 => ("ft", 304.8),
        3 => ("miles", 1_609_344.0),
        4 => ("mm", 1.0),
        5 => ("cm", 10.0),
        6 => ("m", 1000.0),
        7 => ("km", 1e6),
        10 => ("yards", 914.4),
        14 => ("dm", 100.0),
        21 => ("US survey feet", 304.800_609_601_2),
        _ => ("unitless", 1.0),
    }
}
