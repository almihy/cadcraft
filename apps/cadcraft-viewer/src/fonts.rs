//! A firm's own CAD fonts, given by the page before it opens a drawing: SHX shape fonts (text
//! fonts and big fonts) and TrueType/OpenType fonts. A text style that names one (`arabic.shx`,
//! `simpo.ttf`, "Simplified Arabic") draws with it; the browser has no fonts of its own, so
//! without them a style falls back to CADCraft's stroke font and its fallback fonts (built in:
//! Noto Naskh Arabic for Arabic, from the craft-fonts build input). The fonts crate's
//! [`cadcraft_fonts::given`] does the work, as for Arqiflow's agent's drawing reader.

pub use cadcraft_fonts::given::{Added, MAX_FONT_BYTES, add_font, found};

/// The font files a drawing's text styles name that are not here (neither registered nor built
/// in), each once, as the styles spell them: what the page may find among the firm's fonts and
/// give with [`add_font`] before opening the drawing again.
pub fn missing(d: &cadcraft_doc::Drawing) -> Vec<String> {
    cadcraft_fonts::given::missing(d.text_styles.iter().map(|ts| (ts.font.as_str(), ts.big_font.as_str())))
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
}
