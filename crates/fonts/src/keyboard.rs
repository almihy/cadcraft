//! Arabic SHX fonts laid out on the keyboard: old Arabic SHX fonts (`xarab.shx`, `xarb.shx`)
//! draw an Arabic letter for each Latin key, as the Arabic keyboard places them, so a drawing
//! stores "Hu]H] Hgtw,G" and shows «اعداد الفصول». The letters such a text shows are read here
//! from tables made from the fonts' own glyphs (each code drawn and read), for those fonts only:
//! a font is known by its glyph data, not its name, since the same name is given to fonts laid
//! out differently. A text in another font, or a font not known here, is not read.

use crate::shx;

/// FNV-1a of a font's glyph data: which font it is, whatever it is called.
fn fingerprint(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// The keys of the `xarab.shx` family (xarab, xarb, xarabc, arabx…), each with the letter its
/// glyph draws. Capitals draw a letter's end form, small letters its other forms: both read as
/// the letter. Digits and the rest draw as the key reads (the digits in their Arabic shapes).
const XARAB: &[(char, char)] = &[
    ('a', 'ش'),
    ('b', 'ل'),
    ('c', 'ؤ'),
    ('d', 'ي'),
    ('e', 'ث'),
    ('f', 'ب'),
    ('g', 'ل'),
    ('h', 'ا'),
    ('i', 'ه'),
    ('j', 'ت'),
    ('k', 'ن'),
    ('l', 'م'),
    ('m', 'ة'),
    ('n', 'آ'),
    ('o', 'خ'),
    ('p', 'ح'),
    ('q', 'ض'),
    ('r', 'ق'),
    ('s', 'س'),
    ('t', 'ف'),
    ('u', 'ع'),
    ('v', 'ر'),
    ('w', 'ص'),
    ('x', 'أ'),
    ('y', 'غ'),
    ('z', 'ئ'),
    ('A', 'ش'),
    ('B', 'ل'),
    ('C', 'ؤ'),
    ('D', 'ى'),
    ('E', 'ث'),
    ('F', 'ب'),
    ('G', 'ل'),
    ('H', 'ا'),
    ('I', 'ه'),
    ('J', 'ت'),
    ('K', 'ن'),
    ('L', 'م'),
    ('M', 'ة'),
    ('N', 'آ'),
    ('O', 'خ'),
    ('P', 'ح'),
    ('Q', 'ض'),
    ('R', 'ق'),
    ('S', 'س'),
    ('T', 'ف'),
    ('U', 'ع'),
    ('V', 'ر'),
    ('W', 'ص'),
    ('X', 'أ'),
    ('Y', 'غ'),
    ('Z', 'ئ'),
    ('\'', 'ط'),
    (',', 'و'),
    ('/', 'ظ'),
    (':', 'ك'),
    (';', 'ك'),
    ('>', 'ز'),
    ('`', 'ذ'),
    ('[', 'ج'),
    ('{', 'ج'),
    (']', 'د'),
];

/// The fonts known, by [`fingerprint`] of their glyph data, with their keys.
const KNOWN: &[(u64, &[(char, char)])] = &[(0x71ff_5db0_8c43_0a53, XARAB)];

/// Lam-alef: the key draws both letters.
const LAM_ALEF: [char; 2] = ['b', 'B'];

/// The keys of the SHX font `font` names, when it is one laid out on the keyboard.
fn keys_of(font: &str) -> Option<&'static [(char, char)]> {
    let f = shx::find(font)?;
    let print = fingerprint(f.data());
    KNOWN.iter().find(|(p, _)| *p == print).map(|(_, keys)| *keys)
}

/// The Arabic a text in font `font` shows, when the font draws Arabic letters for Latin keys and
/// the text has letters on them: "Hu]H] Hgtw,G" in `xarab.shx` shows «اعداد الفصول». None for
/// any other font, or a text with no letters (digits draw as they read).
pub fn shown(font: &str, text: &str) -> Option<String> {
    if !text.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let keys = keys_of(font)?;
    let mut out = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        match keys.iter().find(|(k, _)| *k == c) {
            Some((k, l)) if LAM_ALEF.contains(k) => {
                out.push(*l);
                out.push('ا');
            }
            Some((_, l)) => out.push(*l),
            None => out.push(c),
        }
    }
    Some(out)
}

/// The fingerprint of an SHX font's bytes, as [`shown`] knows fonts by (for a table of a new font).
pub fn font_print(bytes: &[u8]) -> Option<u64> {
    shx::ShxFont::parse(bytes).ok().map(|f| fingerprint(f.data()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_font_not_known_is_not_read() {
        let src = "*0,4,firm demo\n6,2,2,0\n*72,7,UCH\n1,064,040,06C,2,020,0\n";
        let Ok(bytes) = shx::compile(src) else { return };
        assert!(shx::register("kbdemo.shx", &bytes).is_ok());
        assert_eq!(shown("kbdemo.shx", "Hgl"), None);
        assert_eq!(shown("nosuchfont.shx", "Hgl"), None);
        assert_eq!(shown("kbdemo.shx", "120"), None);
    }
}
