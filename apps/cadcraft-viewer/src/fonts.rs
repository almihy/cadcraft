//! A firm's own CAD fonts, given by the page before it opens a drawing: SHX shape fonts (text
//! fonts and big fonts) and TrueType/OpenType fonts. A text style that names one (`arabic.shx`,
//! `simpo.ttf`, "Simplified Arabic") draws with it; the browser has no fonts of its own, so
//! without them a style falls back to CADCraft's stroke font and its fallback fonts (built in:
//! Noto Naskh Arabic for Arabic, from the craft-fonts build input).

/// What [`add_font`] did with a file.
#[derive(Debug, PartialEq, Eq)]
pub enum Added {
    /// An SHX font, registered under its file name.
    Shx,
    /// A TrueType/OpenType font (or collection), registered under its file name; its faces'
    /// family names find it too.
    Outline,
}

/// The most a font file may be (Arial Unicode is 23 MB; a CJK collection can be larger).
pub const MAX_FONT_BYTES: usize = 64 << 20;

/// Register a font file's bytes under its file name (`romans.shx`, `Tahoma.ttf`): SHX by its
/// signature, else TrueType/OpenType when it holds a face. Drawings opened after this use it.
pub fn add_font(name: &str, bytes: &[u8]) -> Result<Added, String> {
    let name = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    if name.is_empty() {
        return Err("a font needs its file name".into());
    }
    if bytes.len() > MAX_FONT_BYTES {
        return Err(format!("{name}: larger than {} MB", MAX_FONT_BYTES >> 20));
    }
    if bytes.starts_with(b"AutoCAD-86") {
        let shx = if name.to_ascii_lowercase().ends_with(".shx") { name.to_string() } else { format!("{name}.shx") };
        return cadcraft_fonts::shx::register(&shx, bytes).map(|()| Added::Shx).map_err(|e| format!("{name}: {e}"));
    }
    let owned = std::sync::Arc::new(bytes.to_vec());
    if cadcraft_fonts::ttf::faces(&owned).is_empty() {
        return Err(format!("{name}: not an SHX, TrueType or OpenType font"));
    }
    cadcraft_fonts::ttf::register(name, bytes.to_vec());
    Ok(Added::Outline)
}

/// The font files a drawing's text styles name that are not here (neither registered nor built
/// in), each once, as the styles spell them: what the page may find among the firm's fonts and
/// give with [`add_font`] before opening the drawing again.
pub fn missing(d: &cadcraft_doc::Drawing) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for ts in &d.text_styles {
        let font = ts.font.trim();
        let builtin = font.is_empty() || font.eq_ignore_ascii_case(cadcraft_fonts::BUILTIN_FONT);
        if !builtin && cadcraft_fonts::ttf::find(font).is_none() && cadcraft_fonts::shx::find(font).is_none() {
            out.push(font.to_string());
        }
        let big = ts.big_font.trim();
        if !big.is_empty() && cadcraft_fonts::shx::find(big).is_none() {
            out.push(big.to_string());
        }
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|n| seen.insert(n.to_ascii_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_lists_each_unfound_style_font_once() {
        let mut d = cadcraft_doc::Drawing::default();
        let style = |name: &str, font: &str, big: &str| cadcraft_doc::TextStyle {
            name: name.into(),
            font: font.into(),
            big_font: big.into(),
            ..cadcraft_doc::TextStyle::default()
        };
        d.text_styles = vec![
            style("A", "nofirmfont-a.shx", "nofirmbig.shx"),
            style("B", "NOFIRMFONT-A.SHX", ""),
            style("C", cadcraft_fonts::BUILTIN_FONT, ""),
            style("D", "", ""),
        ];
        assert_eq!(missing(&d), vec!["nofirmfont-a.shx".to_string(), "nofirmbig.shx".to_string()]);
    }

    #[test]
    fn junk_and_nameless_files_are_refused() {
        assert!(add_font("", b"AutoCAD-86 shapes 1.0\r\n\x1a").is_err());
        assert!(add_font("notes.txt", b"hello").is_err());
        assert!(add_font("broken.shx", b"AutoCAD-86 shapes 1.0\r\n\x1a\x01").is_err());
    }

    #[test]
    fn an_shx_font_is_found_by_the_name_a_style_gives() {
        let src = "*0,4,firm demo\n6,2,2,0\n*65,7,UCA\n1,064,040,06C,2,020,0\n";
        let compiled = cadcraft_fonts::shx::compile(src);
        assert!(compiled.is_ok(), "the test font compiles");
        let Ok(bytes) = compiled else { return };
        assert_eq!(add_font("C:\\Fonts\\FirmDemo.SHX", &bytes), Ok(Added::Shx));
        assert!(cadcraft_fonts::shx::find("firmdemo").is_some());
        assert!(cadcraft_fonts::shx::find("FIRMDEMO.shx").is_some());
    }
}
