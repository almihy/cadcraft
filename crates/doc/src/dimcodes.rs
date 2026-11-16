//! The DXF group codes of the dimension variables, as DIMSTYLE records and the DSTYLE override
//! xdata of a dimension hold them (DXF Reference, DIMSTYLE), with how each value is stored.
//! Shared by the DXF and DWG readers and writers.

/// How a dimension variable is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum K {
    Real,
    Int,
    Bool,
    Str,
    /// ACI colour number (0 ByBlock, 256 ByLayer).
    Color,
    /// A character stored as its code (DIMDSEP).
    Char,
    /// Handle of a STYLE record (DIMTXSTY).
    TextStyle,
    /// Handle of an arrowhead BLOCK_RECORD (DIMBLK, DIMBLK1, DIMBLK2, DIMLDRBLK).
    Block,
    /// Handle of an LTYPE record (DIMLTYPE, DIMLTEX1, DIMLTEX2); "" (ByBlock) isn't written.
    Linetype,
    /// An angle: degrees in the style, radians in the file (DIMJOGANG).
    Angle,
}

/// [`crate::DimStyle`] field (camelCase) → DIMSTYLE group code (DXF Reference, DIMSTYLE table). The
/// same codes identify variables in the DSTYLE override xdata.
pub const DIM_CODES: &[(&str, i32, K)] = &[
    ("post", 3, K::Str),
    ("altPost", 4, K::Str),
    ("scale", 40, K::Real),
    ("arrowSize", 41, K::Real),
    ("extOffset", 42, K::Real),
    ("baselineSpacing", 43, K::Real),
    ("extExtend", 44, K::Real),
    ("round", 45, K::Real),
    ("dimLineExtend", 46, K::Real),
    ("tolPlus", 47, K::Real),
    ("tolMinus", 48, K::Real),
    ("textHeight", 140, K::Real),
    ("centerMark", 141, K::Real),
    ("tickSize", 142, K::Real),
    ("altFactor", 143, K::Real),
    ("linearFactor", 144, K::Real),
    ("tolScale", 146, K::Real),
    ("textGap", 147, K::Real),
    ("altRound", 148, K::Real),
    ("tolerance", 71, K::Bool),
    ("limits", 72, K::Bool),
    ("textInsideHorizontal", 73, K::Bool),
    ("textOutsideHorizontal", 74, K::Bool),
    ("suppressExt1", 75, K::Bool),
    ("suppressExt2", 76, K::Bool),
    ("textAbove", 77, K::Int),
    ("zeroSuppression", 78, K::Int),
    ("alt", 170, K::Bool),
    ("altDecimals", 171, K::Int),
    ("dimLineColor", 176, K::Color),
    ("extLineColor", 177, K::Color),
    ("textColor", 178, K::Color),
    ("angularDecimals", 179, K::Int),
    ("decimals", 271, K::Int),
    ("tolDecimals", 272, K::Int),
    ("altUnit", 273, K::Int),
    ("angularUnit", 275, K::Int),
    ("fractionFormat", 276, K::Int),
    ("linearUnit", 277, K::Int),
    ("decimalSeparator", 278, K::Char),
    ("textJust", 280, K::Int),
    ("tolZeroSuppression", 284, K::Int),
    ("altZeroSuppression", 285, K::Int),
    ("textStyle", 340, K::TextStyle),
    ("arrowBlock", 342, K::Block),
    ("arrowBlock1", 343, K::Block),
    ("arrowBlock2", 344, K::Block),
    ("fixedExtLength", 49, K::Real),
    ("jogAngle", 50, K::Angle),
    // DIMTFILL and DIMTFILLCLR (Open Design Specification for .dwg files 5.4.1, DIMSTYLE). In a
    // DIMSTYLE record 70 also holds the record's flags, which come first.
    ("textFill", 69, K::Int),
    ("textFillColor", 70, K::Color),
    ("angularZeroSuppression", 79, K::Int),
    ("arcSymbol", 90, K::Int),
    ("textVerticalPos", 145, K::Real),
    ("forceLineInside", 172, K::Bool),
    ("textInside", 174, K::Bool),
    ("suppressOutsideArrows", 175, K::Bool),
    ("textMove", 279, K::Int),
    ("suppressDim1", 281, K::Bool),
    ("suppressDim2", 282, K::Bool),
    ("tolJust", 283, K::Int),
    ("fit", 289, K::Int),
    ("fixedExt", 290, K::Bool),
    ("leaderArrowBlock", 341, K::Block),
    ("dimLinetype", 345, K::Linetype),
    ("extLinetype1", 346, K::Linetype),
    ("extLinetype2", 347, K::Linetype),
    ("dimLineweight", 371, K::Int),
    ("extLineweight", 372, K::Int),
];

