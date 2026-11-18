//! Text set in SHX fonts: a text style's main font and big font, horizontal or vertical.
//!
//! Each character is drawn by the font that has it: a double-byte character of the big font's
//! code page (its lead byte in an escape range) from the big font, others from the main font
//! (character code, or Unicode in a Unicode font; `%%d` `%%p` `%%c` from shapes 256–258 or their
//! older or Unicode places). A font draws at text height / its `above` value, and its pen's end
//! position is the advance: there is no added letter gap. Characters neither font has come from
//! the built-in stroke font (and its fallback fonts), so nothing goes missing.
//!
//! Vertical text (a style flag, for fonts with modes 2) runs each shape's vertical-only
//! commands (code 14) and follows the pen downward.

use std::sync::Arc;

use cadcraft_geom::Vec2;
use encoding_rs::Encoding;

use super::{ShxFont, ShxKind};
use crate::{Shaped, bidi};

/// A text style's SHX fonts: the main font, the big font and the orientation. Either font may
/// be missing (not found): its characters then come from the stroke font.
#[derive(Clone, Debug)]
pub struct ShxText {
    pub main: Option<Arc<ShxFont>>,
    pub big: Option<Arc<ShxFont>>,
    /// The style is vertical and the main font supports it.
    pub vertical: bool,
    /// The big font's code page (from its name and escape ranges, or the drawing's).
    big_encoding: &'static Encoding,
    /// The big font holds East Asian characters of its own code page. Otherwise (an Arabic big
    /// font: escape bytes such as 0xCA or 0x7F) a double-byte code is two of the drawing's bytes
    /// in a row, as they are stored.
    big_cjk: bool,
    /// The drawing's one-byte code page (`$DWGCODEPAGE`; Windows Latin-1 when unknown): a
    /// character's byte in it is its shape number in a one-byte font.
    code_page: &'static Encoding,
}

impl ShxText {
    /// The fonts of a style: `big_name` is the big font's file name (it picks the code page).
    pub fn new(main: Option<Arc<ShxFont>>, big: Option<Arc<ShxFont>>, big_name: &str, vertical: bool) -> ShxText {
        let big_encoding = big.as_deref().map_or(encoding_rs::GBK, |b| big_font_encoding(big_name, b.escape_ranges()));
        let big_cjk = big.as_deref().is_some_and(|b| is_cjk_big_font(big_name, b.escape_ranges()));
        let vertical = vertical && main.as_ref().is_some_and(|m| m.is_dual());
        ShxText { main, big, vertical, big_encoding, big_cjk, code_page: encoding_rs::WINDOWS_1252 }
    }
    /// The same fonts in a drawing whose code page is `code_page` (`$DWGCODEPAGE`, `ANSI_1256`):
    /// a one-byte font draws a character by its byte in that code page, as AutoCAD does, and an
    /// East Asian big font reads its double-byte codes in it. An unknown name changes nothing.
    pub fn in_code_page(mut self, code_page: &str) -> ShxText {
        match code_page_encoding(code_page) {
            Some(enc) if enc.is_single_byte() => self.code_page = enc,
            Some(enc) if self.big_cjk => self.big_encoding = enc,
            _ => {}
        }
        self
    }
    /// The code page of the big font's double-byte codes.
    pub fn big_font_encoding(&self) -> &'static str {
        self.big_encoding.name()
    }
    /// The drawing's one-byte code page the fonts are read in.
    pub fn code_page(&self) -> &'static str {
        self.code_page.name()
    }
    /// The byte of `c` in the drawing's code page, if it has one.
    fn byte(&self, c: char) -> Option<u8> {
        if c.is_ascii() {
            return u8::try_from(u32::from(c)).ok();
        }
        let mut buf = [0u8; 4];
        let (bytes, _, errors) = self.code_page.encode(c.encode_utf8(&mut buf));
        match (errors, bytes.as_ref()) {
            (false, [b]) => Some(*b),
            _ => None,
        }
    }
}

/// The encoding of a drawing code page name (`ANSI_1256`, `ANSI_936`), if it is one CADCraft knows.
fn code_page_encoding(name: &str) -> Option<&'static Encoding> {
    let n = name.trim().to_ascii_uppercase();
    let number = n.strip_prefix("ANSI_")?;
    match number {
        "874" | "1250" | "1251" | "1252" | "1253" | "1254" | "1255" | "1256" | "1257" | "1258" => {
            Encoding::for_label(format!("windows-{number}").as_bytes())
        }
        "932" => Some(encoding_rs::SHIFT_JIS),
        "936" => Some(encoding_rs::GBK),
        "949" => Some(encoding_rs::EUC_KR),
        "950" => Some(encoding_rs::BIG5),
        _ => None,
    }
}

/// Whether a big font holds East Asian characters: a known Japanese, Chinese or Korean name, or
/// escape ranges that span an East Asian code page's lead bytes (from 0x81 or 0xA1 on). Arabic
/// big fonts escape with a few single bytes (0xCA, 0xDC, 0x7F, `|`).
fn is_cjk_big_font(name: &str, ranges: &[(u16, u16)]) -> bool {
    let stem = name.trim().rsplit(['/', '\\']).next().unwrap_or_default().to_ascii_lowercase();
    let stem = stem.trim_end_matches(".shx").trim_start_matches('@');
    let known = ["whg", "wht", "chineset", "cht", "gb", "hz", "bigfont", "extfont"].iter().any(|p| stem.starts_with(p))
        || ["kor", "ksc", "big5", "chs", "jis", "jpn"].iter().any(|p| stem.contains(p));
    known || ranges.iter().any(|(lo, hi)| (*lo == 0x81 || *lo == 0xA1) && hi.saturating_sub(*lo) >= 0x1E)
}

/// The code page a big font's double-byte codes are in: known Japanese, Chinese and Korean
/// big font names, else its escape ranges (Shift-JIS lead bytes from 0x81; the others start at
/// 0xA1 or above, read as GBK).
fn big_font_encoding(name: &str, ranges: &[(u16, u16)]) -> &'static Encoding {
    let stem = name.trim().rsplit(['/', '\\']).next().unwrap_or_default().to_ascii_lowercase();
    let stem = stem.trim_end_matches(".shx").trim_start_matches('@');
    if stem.starts_with("whg") || stem.starts_with("wht") || stem.contains("kor") || stem.contains("ksc") {
        return encoding_rs::EUC_KR;
    }
    if stem.starts_with("chineset") || stem.contains("big5") || stem.starts_with("cht") {
        return encoding_rs::BIG5;
    }
    if stem.starts_with("gb") || stem.starts_with("hz") || stem.contains("chs") {
        return encoding_rs::GBK;
    }
    if stem.starts_with("bigfont") || stem.starts_with("extfont") || stem.contains("jis") || stem.contains("jpn") {
        return encoding_rs::SHIFT_JIS;
    }
    if ranges.iter().any(|(lo, _)| (0x81..0xA1).contains(lo)) { encoding_rs::SHIFT_JIS } else { encoding_rs::GBK }
}

/// The double-byte code of `c` in `enc`, if it has one.
fn double_byte(c: char, enc: &'static Encoding) -> Option<u16> {
    let mut buf = [0u8; 4];
    let (bytes, _, errors) = enc.encode(c.encode_utf8(&mut buf));
    match (errors, bytes.as_ref()) {
        (false, [a, b]) => Some(u16::from_be_bytes([*a, *b])),
        _ => None,
    }
}

/// The shape numbers that may draw `c` in `font`, best first.
fn candidates(c: char, font: &ShxFont, byte: Option<u8>) -> [Option<u16>; 4] {
    let u = u32::from(c);
    let unicode = font.kind() == ShxKind::Unifont && font.encoding() == 0;
    let code = |x: u32| u16::try_from(x).ok();
    // The special characters of `%%d` `%%p` `%%c`: shapes 256–258, the Latin-1 or Unicode code,
    // and the oldest fonts' 127–129.
    let special = match c {
        '°' => Some((256, 0xB0, 127)),
        '±' => Some((257, 0xB1, 128)),
        '⌀' | '∅' => Some((258, u, 129)),
        _ => None,
    };
    if let Some((s, latin, old)) = special {
        // In a DOS 862 Hebrew font 128–129 are letters and 0xB0.. box drawing: shape 256–258,
        // and the old degree sign 127 (not a letter there).
        if !unicode && font.kind() == ShxKind::Shapes && font.hebrew_places().1 {
            return [Some(s), (old == 127).then_some(old), None, None];
        }
        return if unicode {
            [code(u), code(if c == '⌀' { 0x2205 } else { latin }), Some(s), Some(old)]
        } else {
            [Some(s), code(latin), Some(old), None]
        };
    }
    if unicode || u < 0x80 {
        return [code(u), None, None, None];
    }
    // A Hebrew letter in an Israeli one-byte font: its Windows-1255 or DOS 862 place.
    if (0x05D0..=0x05EA).contains(&u) && font.kind() == ShxKind::Shapes {
        let (win, dos) = font.hebrew_places();
        return [win.then(|| code(u - 0x05D0 + 0xE0)).flatten(), dos.then(|| code(u - 0x05D0 + 0x80)).flatten(), None, None];
    }
    // A one-byte font: the character's byte in the drawing's code page (not the old special
    // codes 127–129).
    match byte {
        Some(b) if !(127..=129).contains(&b) => [Some(u16::from(b)), None, None, None],
        _ => [None, None, None, None],
    }
}

/// Whether `font` draws by bytes of a code page rather than by Unicode.
fn by_bytes(font: &ShxFont) -> bool {
    !(font.kind() == ShxKind::Unifont && font.encoding() == 0)
}

/// The font and shape that draw `c`.
fn pick(t: &ShxText, c: char) -> Option<(&ShxFont, u16)> {
    let from_main = || {
        let m = t.main.as_deref()?;
        candidates(c, m, t.byte(c)).into_iter().flatten().find(|n| m.has_shape(*n)).map(|n| (m, n))
    };
    let from_big = || {
        let b = t.big.as_deref()?;
        let code = double_byte(c, t.big_encoding)?;
        let [lead, _] = code.to_be_bytes();
        (t.big_cjk && b.is_lead_byte(lead) && b.has_shape(code)).then_some((b, code))
    };
    if u32::from(c) < 0x100 || matches!(c, '⌀' | '∅') || t.byte(c).is_some() {
        from_main().or_else(from_big)
    } else {
        from_big().or_else(from_main)
    }
}

/// A character of a line as the fonts draw it: the character (one of `chars`, the first of a
/// pair), and the font and shape that draw it, if one does. A pair is a big font's double-byte
/// code made of two of the drawing's bytes in a row (an Arabic big font's escape byte and the
/// next): it draws as one shape and spans two characters.
struct Unit<'a> {
    at: usize,
    len: usize,
    shape: Option<(&'a ShxFont, u16)>,
}

/// The units of a line: each character, or a pair of them for a big font that pairs the drawing's
/// bytes (as AutoCAD reads a lead byte and the byte after it).
fn units<'a>(t: &'a ShxText, chars: &[(char, bool, bool)]) -> Vec<Unit<'a>> {
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while let Some(&(c, _, _)) = chars.get(i) {
        let pair = t.big.as_deref().filter(|_| !t.big_cjk).and_then(|b| {
            let lead = t.byte(c).filter(|l| b.is_lead_byte(*l))?;
            let trail = chars.get(i + 1).and_then(|v| t.byte(v.0))?;
            let code = u16::from_be_bytes([lead, trail]);
            b.has_shape(code).then_some((b, code))
        });
        match pair {
            Some(shape) => {
                out.push(Unit { at: i, len: 2, shape: Some(shape) });
                i += 2;
            }
            None => {
                out.push(Unit { at: i, len: 1, shape: pick(t, c) });
                i += 1;
            }
        }
    }
    out
}

/// Whether unit `u` of `chars` is drawn as stored, left to right, as AutoCAD draws it: a shape of
/// a font read by bytes for a character outside ASCII (an Arabic letter's byte in an Arabic SHX
/// font, already in the order and form the font wants). It takes no part in bidi reordering.
fn as_stored(u: &Unit<'_>, chars: &[(char, bool, bool)]) -> bool {
    u.shape.is_some_and(|(f, _)| by_bytes(f)) && (u.len > 1 || chars.get(u.at).is_some_and(|v| !v.0.is_ascii()))
}

/// A piece of a bidi run: one unit (an SHX font's shape, or the stroke font's), or a stretch of
/// complex-script characters neither SHX font has, shaped as one piece (unit indices).
enum Segment {
    Unit(usize),
    Shaped(std::ops::Range<usize>),
}

/// Whether unit `u` joins a shaped piece: a right-to-left or Thai character, or a combining mark
/// or joiner inside one, that neither SHX font draws.
fn joins(u: &Unit<'_>, chars: &[(char, bool, bool)], inside: bool) -> bool {
    let c = chars.get(u.at).map_or(' ', |v| v.0);
    let part = crate::complex::needed_for(c) || (inside && (bidi::is_mark(c) || matches!(c, '\u{200C}' | '\u{200D}')));
    part && u.len == 1 && u.shape.is_none()
}

/// The pieces of the run `run` of `units`, in display order (reversed when `rtl`). Arabic the
/// SHX fonts lack (a style in a Latin SHX font, or one whose fonts are not found) is shaped a
/// word at a time, so its letters join, rather than drawn one isolated letter after another.
fn segments(t: &ShxText, units: &[Unit<'_>], chars: &[(char, bool, bool)], run: std::ops::Range<usize>, rtl: bool) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut i = run.start;
    while i < run.end {
        if !t.vertical && units.get(i).is_some_and(|u| joins(u, chars, false)) {
            let mut j = i + 1;
            while j < run.end && units.get(j).is_some_and(|u| joins(u, chars, true)) {
                j += 1;
            }
            out.push(Segment::Shaped(i..j));
            i = j;
        } else {
            out.push(Segment::Unit(i));
            i += 1;
        }
    }
    if rtl {
        out.reverse();
    }
    out
}

fn finite_or(v: f64, d: f64) -> f64 {
    if v.is_finite() { v } else { d }
}

/// Shape one line (baseline at y = 0, x from 0). A font whose shapes move the pen leftward (an
/// Arabic SHX font drawing right to left) ends left of where it starts: the line's box is then
/// `[start, start + width]` with `start` < 0, the pen's start staying at 0 as AutoCAD keeps it at
/// the insertion point.
pub(crate) fn shape(t: &ShxText, s: &str, height: f64, width_factor: f64, oblique: f64) -> Shaped {
    let h = if height.is_finite() && height > 0.0 { height } else { 1.0 };
    let wf = if width_factor.is_finite() && width_factor.abs() > 1e-6 { width_factor } else { 1.0 };
    let shear = finite_or(oblique, 0.0).tan().clamp(-10.0, 10.0);
    let chars = crate::decode_controls(s);
    let units = units(t, &chars);
    // Bidi sees each unit by its character; one drawn as stored counts as a left-to-right letter.
    let logical: Vec<char> = units.iter().map(|u| if as_stored(u, &chars) { 'a' } else { chars.get(u.at).map_or(' ', |v| v.0) }).collect();
    let mut out = Shaped::default();
    let mut spans: Vec<(f64, f64, bool, bool)> = Vec::new();
    let mut pen = Vec2::ZERO;
    let mut max_x: f64 = 0.0;
    let runs = if t.vertical { vec![(0..logical.len(), false)] } else { bidi::visual_runs(&logical, None) };
    for (run, rtl) in runs {
        for seg in segments(t, &units, &chars, run, rtl) {
            let i = match seg {
                Segment::Unit(i) => i,
                Segment::Shaped(r) => {
                    // A word of joined letters neither SHX font has, shaped together by the
                    // fallback font in the run's direction, as the stroke font's own text is.
                    let (Some(first), Some(last)) = (units.get(r.start), r.end.checked_sub(1).and_then(|e| units.get(e))) else { continue };
                    let Some(&(_, under, over)) = chars.get(first.at) else { continue };
                    let piece: Vec<(char, bool, bool)> =
                        chars.get(first.at..last.at + last.len).unwrap_or_default().iter().map(|&(c, _, _)| (c, false, false)).collect();
                    let x0 = pen.x;
                    if let Some(mut fb) = crate::complex::shape_decoded(None, &piece, h, wf, oblique, Some(rtl)) {
                        let at = pen;
                        fb.map(|p| p + at);
                        max_x = max_x.max(at.x + fb.width);
                        pen.x += fb.width;
                        out.extend(fb);
                    }
                    spans.push((x0, pen.x, under, over));
                    continue;
                }
            };
            let Some(u) = units.get(i) else { continue };
            let Some(&(c, under, over)) = chars.get(u.at) else { continue };
            if u.shape.is_none() && bidi::is_format_control(c) {
                continue;
            }
            let x0 = pen.x;
            // A character drawn as stored keeps its shape; another in a right-to-left run is
            // mirrored (brackets) and looked up again.
            let shape = if rtl && !as_stored(u, &chars) {
                let m = bidi::mirror(c);
                if m == c { u.shape } else { pick(t, m) }
            } else {
                u.shape
            };
            let c = if rtl && !as_stored(u, &chars) { bidi::mirror(c) } else { c };
            match shape.and_then(|(f, n)| Some((f, f.glyph(n, t.vertical)?))) {
                Some((f, g)) => {
                    let k = h / f.units_per_height();
                    let place = |p: Vec2| Vec2::new(pen.x + p.x * k * wf + p.y * k * shear, pen.y + p.y * k);
                    for st in &g.strokes {
                        let pts: Vec<Vec2> = st.iter().map(|p| place(*p)).collect();
                        max_x = pts.iter().fold(max_x, |m, p| m.max(p.x));
                        out.strokes.push(pts);
                    }
                    let adv = Vec2::new(g.advance.x * k * wf, g.advance.y * k);
                    pen = if t.vertical { pen + adv } else { Vec2::new(pen.x + adv.x, pen.y) };
                }
                None => {
                    // The stroke font draws it, at the pen.
                    let mut fb = crate::shape_stroke(&c.to_string(), h, wf, oblique);
                    let at = pen;
                    fb.map(|p| p + at);
                    max_x = max_x.max(at.x + fb.width);
                    out.extend(fb);
                    if t.vertical {
                        pen.y -= h * 1.5;
                    } else {
                        pen.x += crate::stroke_advance(c, h, wf);
                    }
                }
            }
            if !t.vertical {
                spans.push((x0.min(pen.x), x0.max(pen.x), under, over));
            }
        }
    }
    out.strokes.extend(crate::decorations(&spans, h));
    if t.vertical {
        out.width = max_x.max(0.0);
    } else if pen.x < 0.0 {
        // The pen ran leftward: the box runs from where it ended to where it started.
        out.start = pen.x;
        out.width = -pen.x;
    } else {
        out.width = pen.x;
    }
    out
}

/// The width of one line: the sum of the advances (the box's width, [`shape`]).
pub(crate) fn width(t: &ShxText, s: &str, height: f64, width_factor: f64) -> f64 {
    if t.vertical || bidi::has_rtl(s) || (t.big.is_some() && !t.big_cjk) {
        return shape(t, s, height, width_factor, 0.0).width;
    }
    let h = if height.is_finite() && height > 0.0 { height } else { 1.0 };
    let wf = if width_factor.is_finite() && width_factor.abs() > 1e-6 { width_factor } else { 1.0 };
    crate::decode_controls(s)
        .into_iter()
        .filter(|(c, _, _)| !bidi::is_format_control(*c))
        .map(|(c, _, _)| match pick(t, c).and_then(|(f, n)| Some((f, f.glyph(n, false)?))) {
            Some((f, g)) => g.advance.x * h / f.units_per_height() * wf,
            None => crate::stroke_advance(c, h, wf),
        })
        .sum::<f64>()
        .abs()
}
