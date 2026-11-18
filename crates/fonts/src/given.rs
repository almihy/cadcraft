//! Fonts a host page gives at run time, for a build that has no fonts folders (the browser):
//! a firm's own SHX fonts (text fonts and big fonts) and TrueType/OpenType fonts, registered by
//! file name before a drawing is drawn. A text style that names one (`arabic.shx`, `simpo.ttf`,
//! "Simplified Arabic") draws with it; without them a style falls back to the stroke font and the
//! fallback fonts built in. Arqiflow's drawing viewer and its agent's drawing reader both take
//! fonts this way, so a drawing reads the same in each.

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
/// signature, else TrueType/OpenType when it holds a face. Text drawn after this uses it.
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
        return crate::shx::register(&shx, bytes).map(|()| Added::Shx).map_err(|e| format!("{name}: {e}"));
    }
    let owned = std::sync::Arc::new(bytes.to_vec());
    if crate::ttf::faces(&owned).is_empty() {
        return Err(format!("{name}: not an SHX, TrueType or OpenType font"));
    }
    crate::ttf::register(name, bytes.to_vec());
    Ok(Added::Outline)
}

/// Whether a font a text style names is here: registered, built in, or the built-in stroke font
/// (an empty name). A host asks this between its sources of fonts (the firm's, the computer's,
/// a package of stand-ins) so it fetches each missing font once, from the first that has it.
pub fn found(name: &str) -> bool {
    let font = name.trim();
    font.is_empty() || font.eq_ignore_ascii_case(crate::BUILTIN_FONT) || crate::ttf::find(font).is_some() || crate::shx::find(font).is_some()
}

/// The fonts text styles name (each `(font, big font)`) that are not here, each once, as the
/// styles spell them: what a host may find and give with [`add_font`].
pub fn missing<'a>(styles: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (font, big) in styles {
        let font = font.trim();
        if !found(font) {
            out.push(font.to_string());
        }
        let big = big.trim();
        if !big.is_empty() && crate::shx::find(big).is_none() {
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
        let styles = [("nofirmfont-a.shx", "nofirmbig.shx"), ("NOFIRMFONT-A.SHX", ""), (crate::BUILTIN_FONT, ""), ("", "")];
        assert_eq!(missing(styles), vec!["nofirmfont-a.shx".to_string(), "nofirmbig.shx".to_string()]);
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
        let compiled = crate::shx::compile(src);
        assert!(compiled.is_ok(), "the test font compiles");
        let Ok(bytes) = compiled else { return };
        assert_eq!(add_font("C:\\Fonts\\FirmDemo.SHX", &bytes), Ok(Added::Shx));
        assert!(crate::shx::find("firmdemo").is_some());
        assert!(crate::shx::find("FIRMDEMO.shx").is_some());
        assert!(found(r"C:\Fonts\firmdemo.shx") && found("FirmDemo") && found("") && !found("nofirmfont-b.shx"));
    }
}
