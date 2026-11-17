//! The drawing being viewed: its layers as the viewer shows them, the sheet looked at, the display
//! lists built for it and the GPU vertex data made from them.
//!
//! Model space is built once whole, with each primitive's layer
//! ([`cadcraft_render::build_layered`]), so hiding or showing a layer only remakes the vertex data
//! from the list already built (no regeneration). Its curves and dashes are made fine enough for a
//! few zoom steps past the fitted view; zoomed closer, the part of the drawing round the view is
//! built again finer (the *detail*), which is quick because it is a part, and drawn instead of
//! the whole while the view stays inside it. A sheet (layout) shows model space through its
//! viewports, which the renderer clips as it builds, so a sheet is built again when a layer
//! changes.

use cadcraft_color::{Color, Rgb};
use cadcraft_doc::{BlockBoundsCache, Drawing, Space, entity_bounds_cached};
use cadcraft_geom::{Bounds2, Vec2};
use cadcraft_render::{DPrim, DisplayList, Options};
use cadcraft_ui_egui::gpu::{MeshData, build_mesh, choose_origin};
use serde_json::{Value, json};

/// The paper of a sheet: white in both themes, as CAD programs show a layout.
pub const PAPER: Rgb = Rgb(255, 255, 255);
/// Model space in the light theme.
pub const LIGHT: Rgb = Rgb(255, 255, 255);
/// Model space in the dark theme: AutoCAD's dark drawing area.
pub const DARK: Rgb = Rgb(0x21, 0x28, 0x30);
/// Round a sheet in the light and the dark theme.
pub const SURROUND_LIGHT: Rgb = Rgb(0xe4, 0xe6, 0xea);
pub const SURROUND_DARK: Rgb = Rgb(0x3b, 0x41, 0x4b);

/// A list's curves are made for a view this many times closer than the one it is built at (half
/// a pixel's chord there), its dashes for one this many times closer ([`DASH_AHEAD`]).
const CURVE_AHEAD: f64 = 8.0;
const DASH_AHEAD: f64 = 2.0;
/// Zoomed this many times past the scale a list was built at, the part round the view is built
/// again.
pub const DETAIL_PAST: f64 = 5.0;
/// The detail covers the view and this much of its size again on each side.
const DETAIL_MARGIN: f64 = 0.6;

/// A layer as the page lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerRow {
    pub name: String,
    pub rgb: Rgb,
    pub visible: bool,
}

/// What opening and building took, in milliseconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Took {
    pub read_ms: f64,
    pub list_ms: f64,
    pub detail_ms: f64,
    pub mesh_ms: f64,
}

/// A part of model space built finer, for a close view.
pub struct Detail {
    pub list: DisplayList,
    /// The part built: a view inside it is drawn from it.
    pub window: Bounds2,
    /// The view scale it was built at (points per unit).
    pub scale: f64,
}

pub struct Scene {
    pub drawing: Drawing,
    pub name: String,
    /// Shown, by index in `drawing.layers`.
    pub visible: Vec<bool>,
    pub space: Space,
    /// The whole space shown.
    pub list: DisplayList,
    /// The view scale `list` was built at (points per unit).
    pub built_scale: f64,
    pub detail: Option<Detail>,
    /// Model space's list and its scale, while a sheet is shown.
    kept: Option<(DisplayList, f64)>,
    /// Each model-space entity's box, in the store's order, once a detail needs them.
    boxes: Option<Vec<Bounds2>>,
    pub took: Took,
}

fn options(scale: f64, view: Option<Bounds2>) -> Options {
    let s = scale.max(1e-12);
    Options { tolerance: 0.5 / (s * CURVE_AHEAD), min_dash: 2.0 / (s * DASH_AHEAD), view, ..Options::default() }
}

impl Scene {
    /// A drawing read from a file's bytes (a DWG, a DXF, or a drawing packed by cadcraft-index).
    pub fn read(bytes: &[u8], name: &str) -> Result<(Drawing, f64), String> {
        let t0 = now_ms();
        let d = if bytes.starts_with(b"CCDRAW\0\0") { cadcraft_index::unpack(bytes)? } else { cadcraft_io::read(bytes, name).map_err(|e| e.to_string())? };
        Ok((d, now_ms() - t0))
    }

    /// The scene of a drawing just read, on model space, built for its whole view in a canvas
    /// `px` points wide.
    pub fn new(drawing: Drawing, name: &str, read_ms: f64, px: f64) -> Scene {
        let visible = drawing.layers.iter().map(|l| l.on && !l.frozen).collect();
        let mut s = Scene {
            drawing,
            name: name.to_string(),
            visible,
            space: Space::Model,
            list: DisplayList::default(),
            built_scale: 0.0,
            detail: None,
            kept: None,
            boxes: None,
            took: Took { read_ms, ..Took::default() },
        };
        let ext = s.drawing.extents(&Space::Model);
        let scale = if ext.is_empty() { 1.0 } else { px.max(100.0) / ext.width().max(ext.height()).max(1e-9) };
        s.build(scale);
        s
    }

    /// The layers, as the page lists them.
    pub fn layers(&self) -> Vec<LayerRow> {
        self.drawing.layers.iter().zip(&self.visible).map(|(l, v)| LayerRow { name: l.name.clone(), rgb: layer_rgb(l.color), visible: *v }).collect()
    }

    /// The sheets (layouts) by their tab order, model space left out, with whether anything is
    /// drawn on each.
    pub fn sheets(&self) -> Vec<(String, bool)> {
        let mut ls: Vec<_> = self.drawing.layouts.iter().filter(|l| !l.name.eq_ignore_ascii_case("model")).collect();
        ls.sort_by_key(|l| l.tab_order);
        ls.into_iter().map(|l| (l.name.clone(), l.entities.is_empty())).collect()
    }

    /// Show or hide layers by name. Gives whether the list itself must be built again (a sheet).
    pub fn set_visible(&mut self, names: &[String], visible: bool) -> bool {
        let mut changed = false;
        for (l, v) in self.drawing.layers.iter().zip(self.visible.iter_mut()) {
            if *v != visible && names.iter().any(|n| n.eq_ignore_ascii_case(&l.name)) {
                *v = visible;
                changed = true;
            }
        }
        changed && self.space.is_paper()
    }

    /// Model space is built with every layer on (the viewer hides them itself); a sheet with the
    /// layers shown, its viewports seeing model space through them.
    fn set_layer_flags(&mut self) {
        let paper = self.space.is_paper();
        for (l, v) in self.drawing.layers.iter_mut().zip(&self.visible) {
            l.on = !paper || *v;
            l.frozen = false;
        }
    }

    /// Build the whole space shown, for a view at `scale` points per unit.
    pub fn build(&mut self, scale: f64) {
        let t0 = now_ms();
        self.set_layer_flags();
        let opts = options(scale, None);
        self.list = match &self.space {
            Space::Model => cadcraft_render::build_layered(&self.drawing, self.drawing.model.iter().map(|e| &**e), &opts),
            space => cadcraft_render::build(&self.drawing, space, &opts),
        };
        self.built_scale = scale;
        self.detail = None;
        self.took.list_ms = now_ms() - t0;
    }

    /// Show another space, built for a view at `scale`. Model space's list is kept while a sheet
    /// is shown, so coming back to it builds nothing.
    pub fn show(&mut self, space: Space, scale: f64) {
        if space == self.space {
            return;
        }
        let leaving = std::mem::replace(&mut self.space, space);
        if leaving == Space::Model {
            self.kept = Some((std::mem::take(&mut self.list), self.built_scale));
        }
        self.detail = None;
        match (&self.space, self.kept.take()) {
            (Space::Model, Some((list, built))) => {
                self.list = list;
                self.built_scale = built;
            }
            (_, kept) => {
                self.kept = kept;
                self.build(scale);
            }
        }
    }

    /// Whether a view showing `seen` at `scale` wants a finer detail than the one there is.
    pub fn wants_detail(&self, seen: &Bounds2, scale: f64) -> bool {
        if self.space.is_paper() || scale <= self.built_scale * DETAIL_PAST {
            return false;
        }
        match &self.detail {
            Some(d) => !d.window.contains_box(seen) || scale > d.scale * DETAIL_PAST,
            None => true,
        }
    }

    /// Whether the detail is what a view showing `seen` at `scale` is drawn from.
    pub fn detail_serves(&self, seen: &Bounds2, scale: f64) -> bool {
        self.detail.as_ref().is_some_and(|d| d.window.contains_box(seen) && scale >= d.scale / DETAIL_PAST)
    }

    /// Build the part of model space round `seen` again, finer, for a view at `scale`.
    pub fn build_detail(&mut self, seen: &Bounds2, scale: f64) {
        let t0 = now_ms();
        self.set_layer_flags();
        if self.boxes.is_none() {
            let mut cache = BlockBoundsCache::default();
            let d = &self.drawing;
            self.boxes = Some(d.model.iter().map(|e| entity_bounds_cached(d, e, 0, &mut cache)).collect());
        }
        let m = Vec2::new(seen.width() * DETAIL_MARGIN, seen.height() * DETAIL_MARGIN);
        let window = Bounds2::from_points([seen.min - m, seen.max + m]);
        let boxes = self.boxes.as_deref().unwrap_or(&[]);
        let ents = self.drawing.model.iter().zip(boxes).filter(|(_, b)| b.is_empty() || b.intersects(&window)).map(|(e, _)| &**e);
        let list = cadcraft_render::build_layered(&self.drawing, ents, &options(scale, Some(window)));
        self.detail = Some(Detail { list, window, scale });
        self.took.detail_ms = now_ms() - t0;
    }

    /// The vertex data of what is shown of `list` (the whole, or the detail), at `origin`, against
    /// the background `bg`.
    pub fn mesh(&mut self, detail: bool, origin: Vec2, bg: Rgb) -> MeshData {
        let t0 = now_ms();
        let all = self.space.is_paper() || self.visible.iter().all(|v| *v);
        let visible = &self.visible;
        let list = match (detail, &mut self.detail) {
            (true, Some(d)) => &mut d.list,
            _ => &mut self.list,
        };
        let mesh = if all {
            build_mesh(list, origin, bg, false)
        } else {
            let prims = std::mem::take(&mut list.prims);
            let shown = |layer: u32| visible.get(layer as usize).copied().unwrap_or(true);
            list.prims = prims.iter().zip(list.prim_layers.iter()).filter(|(_, l)| shown(**l)).map(|(p, _)| *p).collect::<Vec<DPrim>>();
            let m = build_mesh(list, origin, bg, false);
            list.prims = prims;
            m
        };
        self.took.mesh_ms = now_ms() - t0;
        mesh
    }

    /// A list (the whole, or the detail) and its primitives shown (model space: on a layer
    /// shown).
    pub fn shown(&self, detail: bool) -> (&DisplayList, impl Iterator<Item = &DPrim>) {
        let paper = self.space.is_paper();
        let list = match (detail, &self.detail) {
            (true, Some(d)) => &d.list,
            _ => &self.list,
        };
        let prims = list
            .prims
            .iter()
            .enumerate()
            .filter(move |(i, _)| paper || list.prim_layers.get(*i).is_none_or(|l| self.visible.get(*l as usize).copied().unwrap_or(true)))
            .map(|(_, p)| p);
        (list, prims)
    }

    /// The box to fit: the sheet, or what is drawn.
    pub fn extents(&self) -> Bounds2 {
        match self.list.sheet {
            Some(sheet) => sheet.bounds(),
            None => self.list.bounds,
        }
    }

    /// Where the whole list's vertex data is made relative to, for a view at `center`, `height`
    /// units high.
    pub fn origin(&self, center: Vec2, height: f64) -> Vec2 {
        choose_origin(&self.list.bounds, center, height)
    }

    /// The drawing's unit, as `$INSUNITS` names it ("" when it names none).
    pub fn unit(&self) -> &'static str {
        match self.drawing.header.i64("INSUNITS", 0) {
            1 => "in",
            2 => "ft",
            4 => "mm",
            5 => "cm",
            6 => "m",
            7 => "km",
            10 => "yd",
            14 => "dm",
            _ => "",
        }
    }

    /// What the page is told of the drawing when it opens.
    pub fn about(&self) -> Value {
        json!({
            "name": self.name,
            "unit": self.unit(),
            "items": self.drawing.model.len(),
            "prims": self.list.prims.len(),
            "truncated": self.list.truncated,
            "sheets": self.sheets().into_iter().map(|(name, empty)| json!({ "name": name, "empty": empty })).collect::<Vec<_>>(),
            "layers": self.layers_json(),
            "took": { "read_ms": self.took.read_ms.round(), "list_ms": self.took.list_ms.round() },
        })
    }

    pub fn layers_json(&self) -> Value {
        Value::Array(
            self.layers()
                .into_iter()
                .map(|l| json!({ "name": l.name, "color": format!("#{:02x}{:02x}{:02x}", l.rgb.0, l.rgb.1, l.rgb.2), "visible": l.visible }))
                .collect(),
        )
    }
}

/// A layer's colour as its swatch shows it (colour 7 as black: the page draws a ring round the
/// swatch, so it reads on either theme).
pub fn layer_rgb(c: Color) -> Rgb {
    match c {
        Color::Index(7) => Rgb(0, 0, 0),
        Color::Index(i) => cadcraft_color::aci_rgb(i),
        Color::True(rgb) => rgb,
        _ => Rgb(0, 0, 0),
    }
}

/// Milliseconds from a monotonic clock (the page's `performance.now()` in a browser).
pub fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64() * 1000.0
    }
}
