//! CADCraft's drawing view for a host page: a DWG or DXF opened from its bytes and drawn by
//! CADCraft's own renderer (display lists, the GPU canvas), with no editing chrome. The page puts
//! its own controls round it and drives it from JavaScript ([`web::Viewer`]): layers shown and
//! hidden, sheets, measure, zoom and fit, the light or dark theme.
//!
//! Built with `wasm-bindgen --target web` (Arqiflow: `scripts/viewer-build.mjs`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod scene;
pub mod snap;
pub mod view;

#[cfg(target_arch = "wasm32")]
mod web;
