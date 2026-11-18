//! CADCraft's drawing view for a host page: a DWG or DXF opened from its bytes and drawn by
//! CADCraft's own renderer (display lists, the GPU canvas), with no editing chrome. The page puts
//! its own controls round it and drives it from JavaScript ([`web::Viewer`]): layers shown and
//! hidden, sheets, measure, zoom and fit, the light or dark theme.
//!
//! Built with `wasm-bindgen --target web` (Arqiflow: `scripts/viewer-build.mjs`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod fonts;
pub mod scene;
pub mod snap;
pub mod view;

#[cfg(target_arch = "wasm32")]
mod web;

/// Register a firm's font file (SHX, TrueType or OpenType) by its file name, before opening the
/// drawings that use it: "shx" or "outline", or why it was refused ([`fonts::add_font`]).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn add_font(name: &str, bytes: &[u8]) -> Result<String, wasm_bindgen::JsValue> {
    match fonts::add_font(name, bytes) {
        Ok(fonts::Added::Shx) => Ok("shx".into()),
        Ok(fonts::Added::Outline) => Ok("outline".into()),
        Err(e) => Err(wasm_bindgen::JsValue::from_str(&e)),
    }
}

/// Whether a font a text style names is here (registered or built in): the page asks before
/// fetching it from its next source ([`fonts::found`]).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn has_font(name: &str) -> bool {
    fonts::found(name)
}
