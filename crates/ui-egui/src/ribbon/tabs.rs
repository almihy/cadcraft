//! What is on the ribbon: its tabs, their panels and each panel's buttons, in the order AutoCAD's
//! Drafting & Annotation workspace shows them. Data only; `ribbon.rs` draws it.

use super::{Cell, Control, Item, PanelDef, TabDef, Tool};
use crate::icons::Icon;

const CIRCLES: &[Tool] = &[
    (Icon::Circle, "circle", "Center, Radius"),
    (Icon::Circle, "circle.cd", "Center, Diameter"),
    (Icon::Circle, "circle.2p", "2-Point"),
    (Icon::Circle, "circle.3p", "3-Point"),
    (Icon::Circle, "circle.ttr", "Tan, Tan, Radius"),
    (Icon::Circle, "circle.ttt", "Tan, Tan, Tan"),
];
const ARCS: &[Tool] = &[
    (Icon::Arc, "arc", "3-Point"),
    (Icon::Arc, "arc.sce", "Start, Center, End"),
    (Icon::Arc, "arc.sca", "Start, Center, Angle"),
    (Icon::Arc, "arc.scl", "Start, Center, Length"),
    (Icon::Arc, "arc.sea", "Start, End, Angle"),
    (Icon::Arc, "arc.sed", "Start, End, Direction"),
    (Icon::Arc, "arc.ser", "Start, End, Radius"),
    (Icon::Arc, "arc.cse", "Center, Start, End"),
    (Icon::Arc, "arc.csa", "Center, Start, Angle"),
    (Icon::Arc, "arc.csl", "Center, Start, Length"),
    (Icon::Arc, "arc.continue", "Continue"),
];
const RECTANGLES: &[Tool] = &[(Icon::Rectangle, "rectang", "Rectangle"), (Icon::Polygon, "polygon", "Polygon")];
const ELLIPSES: &[Tool] = &[(Icon::Ellipse, "ellipse", "Center"), (Icon::Ellipse, "ellipse.axis", "Axis, End"), (Icon::Ellipse, "ellipse.arc", "Elliptical Arc")];
const HATCHES: &[Tool] = &[(Icon::Hatch, "hatch", "Hatch"), (Icon::Gradient, "gradient", "Gradient"), (Icon::Boundary, "boundary", "Boundary")];
const TRIMS: &[Tool] = &[(Icon::Trim, "trim", "Trim"), (Icon::Extend, "extend", "Extend")];
const FILLETS: &[Tool] = &[(Icon::Fillet, "fillet", "Fillet"), (Icon::Chamfer, "chamfer", "Chamfer"), (Icon::Spline, "blend", "Blend Curves")];
const ARRAYS: &[Tool] =
    &[(Icon::ArrayRect, "arrayrect", "Rectangular Array"), (Icon::ArrayRect, "arraypath", "Path Array"), (Icon::ArrayPolar, "arraypolar", "Polar Array")];
const TEXTS: &[Tool] = &[(Icon::MText, "mtext", "Multiline Text"), (Icon::Text, "text", "Single Line")];
const DIMENSIONS: &[Tool] = &[
    (Icon::DimLinear, "dimlinear", "Linear"),
    (Icon::DimAligned, "dimaligned", "Aligned"),
    (Icon::DimAngular, "dimangular", "Angular"),
    (Icon::DimArc, "dimarc", "Arc Length"),
    (Icon::DimRadius, "dimradius", "Radius"),
    (Icon::DimDiameter, "dimdiameter", "Diameter"),
    (Icon::DimRadius, "dimjogged", "Jogged"),
    (Icon::DimOrdinate, "dimordinate", "Ordinate"),
];
const LEADERS: &[Tool] = &[
    (Icon::MLeader, "mleader", "Leader"),
    (Icon::AddLeader, "mleaderedit.add", "Add Leader"),
    (Icon::RemoveLeader, "mleaderedit.remove", "Remove Leader"),
    (Icon::AlignLeaders, "mleaderalign", "Align"),
    (Icon::AlignLeaders, "mleadercollect", "Collect"),
];
const MEASURES: &[Tool] = &[(Icon::Distance, "dist", "Distance"), (Icon::Area, "area", "Area"), (Icon::Measure, "measuregeom", "Quick")];
const PASTES: &[Tool] = &[(Icon::Import, "pasteclip", "Paste"), (Icon::Blocks, "pasteblock", "Paste as Block"), (Icon::Import, "pasteorig", "Paste to Original Coordinates")];
const REVCLOUDS: &[Tool] =
    &[(Icon::RevCloud, "revcloud", "Rectangular"), (Icon::RevCloud, "revcloud.polygonal", "Polygonal"), (Icon::RevCloud, "revcloud.freehand", "Freehand")];
const CENTERS: &[Tool] = &[(Icon::Point, "centermark", "Center Mark"), (Icon::XLine, "centerline", "Centerline")];
const ZOOMS: &[Tool] = &[
    (Icon::ZoomExtents, "zoom.extents", "Extents"),
    (Icon::ZoomWindow, "zoom.window", "Window"),
    (Icon::ZoomFit, "zoom.previous", "Previous"),
    (Icon::ZoomIn, "zoom.in", "In"),
    (Icon::ZoomOut, "zoom.out", "Out"),
];

const DRAW: PanelDef = PanelDef {
    name: "Draw",
    items: &[
        Item::Large((Icon::Line, "line", "Line")),
        Item::Large((Icon::Polyline, "pline", "Polyline")),
        Item::LargeSplit("Circle", CIRCLES),
        Item::LargeSplit("Arc", ARCS),
        Item::Col(&[Cell::Split(RECTANGLES), Cell::Split(ELLIPSES), Cell::Split(HATCHES)]),
    ],
    more: &[
        &[
            (Icon::Spline, "spline", "Spline Fit"),
            (Icon::Spline, "spline.cv", "Spline CV"),
            (Icon::XLine, "xline", "Construction Line"),
            (Icon::Ray, "ray", "Ray"),
            (Icon::Point, "point.multiple", "Multiple Points"),
            (Icon::Point, "divide", "Divide"),
            (Icon::Measure, "measure", "Measure"),
        ],
        &[
            (Icon::Region, "region", "Region"),
            (Icon::Wipeout, "wipeout", "Wipeout"),
            (Icon::Polyline, "3dpoly", "3D Polyline"),
            (Icon::Spline, "helix", "Helix"),
            (Icon::Donut, "donut", "Donut"),
            (Icon::RevCloud, "revcloud", "Revision Cloud"),
        ],
    ],
};

const MODIFY: PanelDef = PanelDef {
    name: "Modify",
    items: &[
        Item::Col(&[Cell::Text((Icon::Move, "move", "Move")), Cell::Text((Icon::Copy, "copy", "Copy")), Cell::Text((Icon::Stretch, "stretch", "Stretch"))]),
        Item::Col(&[Cell::Text((Icon::Rotate, "rotate", "Rotate")), Cell::Text((Icon::Mirror, "mirror", "Mirror")), Cell::Text((Icon::Scale, "scale", "Scale"))]),
        Item::Col(&[Cell::SplitText(TRIMS), Cell::SplitText(FILLETS), Cell::SplitText(ARRAYS)]),
        Item::Col(&[Cell::Icon((Icon::Erase, "erase", "Erase")), Cell::Icon((Icon::Explode, "explode", "Explode")), Cell::Icon((Icon::Offset, "offset", "Offset"))]),
    ],
    more: &[
        &[
            (Icon::Lengthen, "lengthen", "Lengthen"),
            (Icon::Polyline, "pedit", "Edit Polyline"),
            (Icon::Spline, "splinedit", "Edit Spline"),
            (Icon::Hatch, "hatchedit", "Edit Hatch"),
            (Icon::ArrayRect, "arrayedit", "Edit Array"),
        ],
        &[
            (Icon::MoveTop, "align", "Align"),
            (Icon::Break, "break", "Break"),
            (Icon::Break, "breakatpoint", "Break at Point"),
            (Icon::Join, "join", "Join"),
            (Icon::Mirror, "reverse", "Reverse"),
        ],
        &[(Icon::Erase, "overkill", "Delete Duplicate Objects"), (Icon::MoveTop, "draworder", "Draw Order"), (Icon::Layers, "chspace", "Change Space")],
    ],
};

const ANNOTATION: PanelDef = PanelDef {
    name: "Annotation",
    items: &[
        Item::LargeSplit("Text", TEXTS),
        Item::LargeSplit("Dimension", DIMENSIONS),
        Item::Col(&[Cell::SplitText(LEADERS), Cell::Text((Icon::Table, "table", "Table"))]),
    ],
    more: &[&[
        (Icon::TextStyle, "style", "Text Style"),
        (Icon::DimStyle, "dimstyle", "Dimension Style"),
        (Icon::MLeader, "mleaderstyle", "Multileader Style"),
        (Icon::Table, "tablestyle", "Table Style"),
    ]],
};

const LAYERS: PanelDef = PanelDef {
    name: "Layers",
    items: &[
        Item::Large((Icon::LayerProps, "layer", "Layer Properties")),
        Item::Col(&[
            Cell::Control(Control::Layer),
            Cell::Row(&[
                Cell::Icon((Icon::LayerOff, "layoff", "Off")),
                Cell::Icon((Icon::LayerIso, "layiso", "Isolate")),
                Cell::Icon((Icon::LayerFreeze, "layfrz", "Freeze")),
                Cell::Icon((Icon::LayerLock, "laylck", "Lock")),
                Cell::Icon((Icon::MakeCurrent, "laymcur", "Make Current")),
            ]),
            Cell::Icon((Icon::LayerMatch, "laymch", "Match Layer")),
            Cell::Icon((Icon::LayerPrev, "layerp", "Previous")),
        ]),
    ],
    more: &[&[
        (Icon::Bulb, "layon", "Turn All Layers On"),
        (Icon::LayerIso, "layuniso", "Unisolate"),
        (Icon::Sun, "laythw", "Thaw All Layers"),
        (Icon::Unlock, "layulk", "Unlock"),
        (Icon::MakeCurrent, "laycur", "Change to Current Layer"),
        (Icon::Layers, "laymrg", "Merge"),
        (Icon::Erase, "laydel", "Delete"),
    ]],
};

const BLOCK: PanelDef = PanelDef {
    name: "Block",
    items: &[
        Item::Large((Icon::Insert, "insert", "Insert")),
        Item::Col(&[
            Cell::Text((Icon::BlockCreate, "block", "Create")),
            Cell::Text((Icon::BlockEdit, "bedit", "Edit")),
            Cell::Text((Icon::AttDef, "attedit", "Edit Attributes")),
        ]),
    ],
    more: &[&[(Icon::AttDef, "attdef", "Define Attributes"), (Icon::Blocks, "battman", "Manage Attributes"), (Icon::Redo, "attsync", "Synchronize")]],
};

const PROPERTIES: PanelDef = PanelDef {
    name: "Properties",
    items: &[
        Item::Large((Icon::MatchProp, "matchprop", "Match Properties")),
        Item::Col(&[Cell::Control(Control::Color), Cell::Control(Control::Lineweight), Cell::Control(Control::Linetype)]),
    ],
    more: &[&[(Icon::List, "list", "List"), (Icon::Properties, "properties", "Properties")]],
};

const GROUPS: PanelDef = PanelDef {
    name: "Groups",
    items: &[
        Item::Large((Icon::SelectObjects, "group", "Group")),
        Item::Col(&[Cell::Icon((Icon::Explode, "ungroup", "Ungroup")), Cell::Icon((Icon::Gear, "groupedit", "Group Edit"))]),
    ],
    more: &[],
};

const UTILITIES: PanelDef = PanelDef {
    name: "Utilities",
    items: &[
        Item::LargeSplit("Measure", MEASURES),
        Item::Col(&[
            Cell::Icon((Icon::QuickSelect, "qselect", "Quick Select")),
            Cell::Icon((Icon::SelectObjects, "selectall", "Select All")),
            Cell::Icon((Icon::Point, "id", "ID Point")),
        ]),
    ],
    more: &[],
};

const CLIPBOARD: PanelDef = PanelDef {
    name: "Clipboard",
    items: &[
        Item::LargeSplit("Paste", PASTES),
        Item::Col(&[
            Cell::Icon((Icon::Erase, "cutclip", "Cut")),
            Cell::Icon((Icon::Copy, "copyclip", "Copy Clip")),
            Cell::Icon((Icon::Point, "copybase", "Copy with Base Point")),
        ]),
    ],
    more: &[],
};

const INSERT_BLOCK: PanelDef = PanelDef {
    name: "Block",
    items: &[
        Item::Large((Icon::Insert, "insert", "Insert")),
        Item::Col(&[Cell::Text((Icon::BlockEdit, "bedit", "Edit")), Cell::Text((Icon::AttDef, "attedit", "Edit Attribute"))]),
    ],
    more: &[],
};

const BLOCK_DEFINITION: PanelDef = PanelDef {
    name: "Block Definition",
    items: &[
        Item::Large((Icon::BlockCreate, "block", "Create Block")),
        Item::Large((Icon::AttDef, "attdef", "Define Attributes")),
        Item::Col(&[
            Cell::Text((Icon::Blocks, "battman", "Manage Attributes")),
            Cell::Text((Icon::BlockEdit, "bedit", "Block Editor")),
            Cell::Text((Icon::Export, "wblock", "Write Block")),
        ]),
    ],
    more: &[],
};

const REFERENCE: PanelDef = PanelDef {
    name: "Reference",
    items: &[
        Item::Large((Icon::Attach, "attach", "Attach")),
        Item::Col(&[
            Cell::Text((Icon::Attach, "xattach", "Attach DWG")),
            Cell::Text((Icon::Attach, "imageattach", "Attach Image")),
            Cell::Text((Icon::Blocks, "xref", "External References")),
        ]),
    ],
    more: &[],
};

const IMPORT: PanelDef = PanelDef { name: "Import", items: &[Item::Large((Icon::Import, "import", "Import"))], more: &[] };

const DATA: PanelDef = PanelDef {
    name: "Linking & Extraction",
    items: &[Item::Large((Icon::Table, "datalink", "Data Link")), Item::Large((Icon::Table, "dataextraction", "Extract Data"))],
    more: &[],
};

const ANNOTATE_TEXT: PanelDef = PanelDef {
    name: "Text",
    items: &[
        Item::LargeSplit("Text", TEXTS),
        Item::Col(&[Cell::Text((Icon::Search, "find", "Find Text")), Cell::Text((Icon::TextStyle, "style", "Text Style"))]),
    ],
    more: &[],
};

const ANNOTATE_DIMENSIONS: PanelDef = PanelDef {
    name: "Dimensions",
    items: &[
        Item::LargeSplit("Dimension", DIMENSIONS),
        Item::Col(&[
            Cell::Text((Icon::QDim, "qdim", "Quick")),
            Cell::Text((Icon::DimContinue, "dimcontinue", "Continue")),
            Cell::Text((Icon::DimBaseline, "dimbaseline", "Baseline")),
        ]),
        Item::Col(&[
            Cell::Icon((Icon::Break, "dimbreak", "Break")),
            Cell::Icon((Icon::DimLinear, "dimspace", "Adjust Space")),
            Cell::Icon((Icon::DimStyle, "dimstyle", "Dimension Style")),
        ]),
    ],
    more: &[&[
        (Icon::DimLinear, "dimjogline", "Jog Line"),
        (Icon::DimLinear, "diminspect", "Inspect"),
        (Icon::DimAligned, "dimedit.oblique", "Oblique"),
        (Icon::Text, "dimtedit", "Text Angle"),
        (Icon::Redo, "dimreassociate", "Reassociate"),
        (Icon::Fix, "tolerance", "Tolerance"),
    ]],
};

const CENTERLINES: PanelDef = PanelDef { name: "Centerlines", items: &[Item::LargeSplit("Center Mark", CENTERS)], more: &[] };

const ANNOTATE_LEADERS: PanelDef = PanelDef {
    name: "Leaders",
    items: &[
        Item::Large((Icon::MLeader, "mleader", "Multileader")),
        Item::Col(&[
            Cell::Text((Icon::AddLeader, "mleaderedit.add", "Add Leader")),
            Cell::Text((Icon::RemoveLeader, "mleaderedit.remove", "Remove Leader")),
            Cell::Text((Icon::AlignLeaders, "mleaderalign", "Align")),
        ]),
    ],
    more: &[&[(Icon::AlignLeaders, "mleadercollect", "Collect"), (Icon::MLeader, "mleaderstyle", "Multileader Style")]],
};

const ANNOTATE_TABLES: PanelDef = PanelDef {
    name: "Tables",
    items: &[
        Item::Large((Icon::Table, "table", "Table")),
        Item::Col(&[Cell::Text((Icon::Table, "tablestyle", "Table Style")), Cell::Text((Icon::Table, "datalink", "Link Data"))]),
    ],
    more: &[],
};

const MARKUP: PanelDef = PanelDef {
    name: "Markup",
    items: &[Item::Large((Icon::Wipeout, "wipeout", "Wipeout")), Item::LargeSplit("Revision Cloud", REVCLOUDS)],
    more: &[],
};

const ANNOTATION_SCALING: PanelDef =
    PanelDef { name: "Annotation Scaling", items: &[Item::Large((Icon::Annotation, "objectscale", "Add/Delete Scales")), Item::Large((Icon::Annotation, "scalelistedit", "Scale List"))], more: &[] };

const GEOMETRIC: PanelDef = PanelDef {
    name: "Geometric",
    items: &[
        Item::Large((Icon::Constraint, "autoconstrain", "Auto Constrain")),
        Item::Col(&[
            Cell::Row(&[
                Cell::Icon((Icon::Coincident, "gccoincident", "Coincident")),
                Cell::Icon((Icon::Collinear, "gccollinear", "Collinear")),
                Cell::Icon((Icon::Concentric, "gcconcentric", "Concentric")),
                Cell::Icon((Icon::Fix, "gcfix", "Fix")),
            ]),
            Cell::Row(&[
                Cell::Icon((Icon::Parallel, "gcparallel", "Parallel")),
                Cell::Icon((Icon::Perpendicular, "gcperpendicular", "Perpendicular")),
                Cell::Icon((Icon::Horizontal, "gchorizontal", "Horizontal")),
                Cell::Icon((Icon::Vertical, "gcvertical", "Vertical")),
            ]),
            Cell::Row(&[
                Cell::Icon((Icon::Tangent, "gctangent", "Tangent")),
                Cell::Icon((Icon::Smooth, "gcsmooth", "Smooth")),
                Cell::Icon((Icon::Symmetric, "gcsymmetric", "Symmetric")),
                Cell::Icon((Icon::Equal, "gcequal", "Equal")),
            ]),
        ]),
    ],
    more: &[],
};

const DIMENSIONAL: PanelDef = PanelDef {
    name: "Dimensional",
    items: &[
        Item::Large((Icon::DimLinear, "dclinear", "Linear")),
        Item::Large((Icon::DimAligned, "dcaligned", "Aligned")),
        Item::Col(&[
            Cell::Icon((Icon::DimRadius, "dcradius", "Radius")),
            Cell::Icon((Icon::DimDiameter, "dcdiameter", "Diameter")),
            Cell::Icon((Icon::DimAngular, "dcangular", "Angular")),
        ]),
    ],
    more: &[],
};

const PARAMETRIC_MANAGE: PanelDef = PanelDef {
    name: "Manage",
    items: &[Item::Large((Icon::DelConstraint, "delconstraint", "Delete Constraints")), Item::Large((Icon::Parameters, "parameters", "Parameters Manager"))],
    more: &[],
};

const NAVIGATE: PanelDef = PanelDef {
    name: "Navigate",
    items: &[Item::Large((Icon::Pan, "pan", "Pan")), Item::LargeSplit("Zoom", ZOOMS), Item::Large((Icon::Redo, "regen", "Regen"))],
    more: &[],
};

const VIEWS: PanelDef =
    PanelDef { name: "Named Views", items: &[Item::Large((Icon::Preview, "view", "View Manager"))], more: &[] };

const VIEWPORTS: PanelDef = PanelDef {
    name: "Model Viewports",
    items: &[
        Item::Large((Icon::Grid, "vports", "Viewport Configuration")),
        Item::Col(&[Cell::Text((Icon::Grid, "vports.named", "Named")), Cell::Text((Icon::Plus, "vports.2", "Two")), Cell::Text((Icon::Rectangle, "vports.1", "Single"))]),
    ],
    more: &[],
};

const PALETTES: PanelDef = PanelDef {
    name: "Palettes",
    items: &[
        Item::Large((Icon::Properties, "properties", "Properties")),
        Item::Large((Icon::LayerProps, "layer", "Layer Properties")),
        Item::Large((Icon::Blocks, "blockspalette", "Blocks")),
        Item::Large((Icon::Attach, "xref", "External References")),
    ],
    more: &[],
};

const LAYOUT_PANEL: PanelDef = PanelDef {
    name: "Layout",
    items: &[Item::Large((Icon::Plus, "layout", "New Layout")), Item::Large((Icon::PageSetup, "pagesetup", "Page Setup"))],
    more: &[],
};

const LAYOUT_VIEWPORTS: PanelDef = PanelDef {
    name: "Layout Viewports",
    items: &[
        Item::Large((Icon::Rectangle, "mview", "Rectangular")),
        Item::Col(&[
            Cell::Text((Icon::Polygon, "mview.polygonal", "Polygonal")),
            Cell::Text((Icon::Boundary, "vpclip", "Clip")),
            Cell::Text((Icon::Annotation, "scalelistedit", "Scale List")),
        ]),
    ],
    more: &[],
};

const CLEANUP: PanelDef = PanelDef {
    name: "Cleanup",
    items: &[
        Item::Large((Icon::Erase, "purge", "Purge")),
        Item::Col(&[Cell::Text((Icon::Erase, "overkill", "Overkill")), Cell::Text((Icon::Search, "audit", "Audit")), Cell::Text((Icon::Open, "recover", "Recover"))]),
    ],
    more: &[],
};

const APPLICATIONS: PanelDef = PanelDef {
    name: "Applications",
    items: &[Item::Large((Icon::Import, "appload", "Load Application")), Item::Large((Icon::List, "script", "Run Script"))],
    more: &[],
};

const CUSTOMIZATION: PanelDef = PanelDef {
    name: "Customization",
    items: &[Item::Large((Icon::Gear, "options", "Options")), Item::Large((Icon::Text, "aliasedit", "Edit Aliases"))],
    more: &[],
};

const PLOT: PanelDef = PanelDef {
    name: "Plot",
    items: &[
        Item::Large((Icon::Plot, "plot", "Plot")),
        Item::Large((Icon::Publish, "publish", "Batch Plot")),
        Item::Large((Icon::Preview, "preview", "Preview")),
        Item::Col(&[
            Cell::Text((Icon::PageSetup, "pagesetup", "Page Setup Manager")),
            Cell::Text((Icon::Plot, "plottermanager", "Plotter Manager")),
            Cell::Text((Icon::Lineweight, "stylesmanager", "Plot Style Manager")),
        ]),
    ],
    more: &[],
};

const EXPORT: PanelDef = PanelDef {
    name: "Export to PDF",
    items: &[Item::Large((Icon::Export, "exportpdf", "Export PDF")), Item::Large((Icon::Export, "export", "Export"))],
    more: &[],
};

/// The tabs, in AutoCAD's order.
pub const TABS: &[TabDef] = &[
    TabDef { name: "Home", panels: &[DRAW, MODIFY, ANNOTATION, LAYERS, BLOCK, PROPERTIES, GROUPS, UTILITIES, CLIPBOARD] },
    TabDef { name: "Insert", panels: &[INSERT_BLOCK, BLOCK_DEFINITION, REFERENCE, IMPORT, DATA] },
    TabDef { name: "Annotate", panels: &[ANNOTATE_TEXT, ANNOTATE_DIMENSIONS, CENTERLINES, ANNOTATE_LEADERS, ANNOTATE_TABLES, MARKUP, ANNOTATION_SCALING] },
    TabDef { name: "Parametric", panels: &[GEOMETRIC, DIMENSIONAL, PARAMETRIC_MANAGE] },
    TabDef { name: "View", panels: &[NAVIGATE, VIEWS, VIEWPORTS, PALETTES] },
    TabDef { name: "Manage", panels: &[CUSTOMIZATION, APPLICATIONS, CLEANUP] },
    TabDef { name: "Output", panels: &[PLOT, EXPORT] },
    TabDef { name: "Layout", panels: &[LAYOUT_PANEL, LAYOUT_VIEWPORTS] },
];
