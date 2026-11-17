//! The viewer in a page: eframe's web runner on the page's canvas (wgpu: WebGPU where the browser
//! has it, WebGL2 otherwise), the drawing drawn by CADCraft's GPU canvas code, and the page's
//! JavaScript driving it through [`Viewer`].
//!
//! The page draws its own controls (its bar, its layers panel); the canvas only shows the drawing,
//! takes the mouse (drag to pan, the wheel to zoom about the pointer, a double click of the wheel
//! to fit, clicks to measure) and tells the page what it measured through the function given to
//! [`Viewer::on_event`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use cadcraft_color::Rgb;
use cadcraft_doc::Space;
use cadcraft_geom::{Bounds2, Vec2};
use cadcraft_render::Kind;
use cadcraft_ui_egui::gpu::{CanvasCallback, GpuFaults, IDENTITY_XFORM, LAYER_DRAG, LAYER_DRAWING, MeshSlot, Stencil};
use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Sense, Stroke, pos2, vec2};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use crate::scene::{self, Scene, now_ms};
use crate::snap::Snaps;
use crate::view::View;

/// The detail is built only once zooming and panning have stopped this long, in milliseconds.
const SETTLE_MS: f64 = 250.0;
/// How far from the pointer measure snaps, in points.
const SNAP_PX: f64 = 10.0;

fn c32(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Vertex data on the GPU: the whole list's, or the detail's.
struct Mesh {
    slot: MeshSlot,
    key: u64,
    origin: Vec2,
    /// Made and handed to the GPU.
    ready: bool,
    /// To be made again (the layers shown, the theme, a new list).
    dirty: bool,
    /// Which of the canvas's GPU layers holds it.
    layer: usize,
}

impl Mesh {
    fn new(layer: usize) -> Mesh {
        Mesh { slot: Arc::new(Mutex::new(None)), key: 0, origin: Vec2::ZERO, ready: false, dirty: false, layer }
    }
}

struct State {
    scene: Option<Scene>,
    view: View,
    /// The view each space was left at, by its name ("" model space).
    views: HashMap<String, View>,
    dark: bool,
    whole: Mesh,
    part: Mesh,
    /// The detail is drawn this frame (the view is inside it).
    use_detail: bool,
    list_dirty: bool,
    fit_pending: bool,
    /// When the view last zoomed (ms).
    zoomed_at: f64,
    measuring: bool,
    points: Vec<Vec2>,
    hover: Option<Vec2>,
    snapped: bool,
    /// The snap points, and whether they are the detail's.
    snaps: Option<(Snaps, bool)>,
    told: Option<Value>,
    events: Vec<Value>,
    rect: Rect,
    gpu: bool,
    opened_at: Option<f64>,
    /// Frames painted and their time in Rust, for the page's measuring.
    frames: u64,
    frame_ms_total: f64,
    frame_ms_most: f64,
}

impl State {
    fn new(dark: bool) -> State {
        State {
            scene: None,
            view: View::default(),
            views: HashMap::new(),
            dark,
            whole: Mesh::new(LAYER_DRAWING),
            part: Mesh::new(LAYER_DRAG),
            use_detail: false,
            list_dirty: false,
            fit_pending: false,
            zoomed_at: 0.0,
            measuring: false,
            points: Vec::new(),
            hover: None,
            snapped: false,
            snaps: None,
            told: None,
            events: Vec::new(),
            rect: Rect::NOTHING,
            gpu: false,
            opened_at: None,
            frames: 0,
            frame_ms_total: 0.0,
            frame_ms_most: 0.0,
        }
    }

    fn sheet(&self) -> bool {
        self.scene.as_ref().is_some_and(|s| s.space.is_paper())
    }

    /// The colour behind the drawing: the paper on a sheet, else the theme's.
    fn background(&self) -> Rgb {
        if self.sheet() {
            scene::PAPER
        } else if self.dark {
            scene::DARK
        } else {
            scene::LIGHT
        }
    }

    fn accent(&self) -> Color32 {
        if self.dark && !self.sheet() { Color32::from_rgb(0x60, 0xa5, 0xfa) } else { Color32::from_rgb(0x25, 0x63, 0xeb) }
    }

    fn fit(&mut self) {
        let Some(scene) = &self.scene else { return };
        if self.rect.width() < 2.0 || self.rect.height() < 2.0 {
            self.fit_pending = true;
            return;
        }
        self.view = View::fit(&scene.extents(), f64::from(self.rect.width()), f64::from(self.rect.height()));
        self.fit_pending = false;
        self.zoomed_at = now_ms();
    }

    fn to_world(&self, p: Pos2) -> Vec2 {
        let c = self.rect.center();
        self.view.to_world((f64::from(p.x - c.x), f64::from(p.y - c.y)))
    }

    fn to_screen(&self, p: Vec2) -> Pos2 {
        let c = self.rect.center();
        let (x, y) = self.view.to_screen(p);
        pos2(c.x + x as f32, c.y + y as f32)
    }

    /// The drawing's box on the canvas.
    fn seen(&self) -> Bounds2 {
        Bounds2::from_points([self.to_world(self.rect.left_top()), self.to_world(self.rect.right_bottom())])
    }

    /// The pointer's drawing point, snapped to the nearest end or corner when one is near.
    fn snap(&mut self, p: Vec2) -> (Vec2, bool) {
        if self.snaps.as_ref().is_none_or(|(_, d)| *d != self.use_detail)
            && let Some(scene) = &self.scene
        {
            let (list, prims) = scene.shown(self.use_detail);
            self.snaps = Some((Snaps::new(list, prims), self.use_detail));
        }
        match self.snaps.as_ref().and_then(|(s, _)| s.nearest(p, SNAP_PX / self.view.scale)) {
            Some(q) => (q, true),
            None => (p, false),
        }
    }

    /// Tell the page what is measured, when it changed.
    fn tell_measure(&mut self) {
        let ends: Vec<Vec2> = match (self.points.as_slice(), self.hover) {
            ([a], Some(h)) => vec![*a, h],
            ([a, b], _) => vec![*a, *b],
            _ => Vec::new(),
        };
        let m = match ends.as_slice() {
            [a, b] => json!({ "points": self.points.len(), "distance": a.dist(*b), "dx": (b.x - a.x).abs(), "dy": (b.y - a.y).abs() }),
            _ => json!({ "points": self.points.len(), "distance": null }),
        };
        if self.told.as_ref() != Some(&m) {
            self.told = Some(m.clone());
            self.events.push(json!({ "kind": "measure", "measure": m }));
        }
    }

    fn input(&mut self, ui: &egui::Ui, resp: &egui::Response) {
        let (scroll, zoom, touch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.multi_touch()));
        let hover = resp.hover_pos();
        let c = self.rect.center();
        let at = |p: Option<Pos2>| p.map_or((0.0, 0.0), |p| (f64::from(p.x - c.x), f64::from(p.y - c.y)));
        // The wheel zooms about the pointer (as CAD programs do), a pinch or Ctrl+wheel too.
        let mut factor = 1.0f64;
        if resp.hovered() && scroll.y != 0.0 {
            factor *= 1.0015f64.powf(f64::from(scroll.y));
        }
        if resp.hovered() && (zoom - 1.0).abs() > 1e-6 && touch.is_none() {
            factor *= f64::from(zoom);
        }
        if let Some(t) = touch {
            factor *= f64::from(t.zoom_delta);
            self.view.pan((f64::from(t.translation_delta.x), f64::from(t.translation_delta.y)));
        }
        if (factor - 1.0).abs() > 1e-9 {
            self.view.zoom(factor, at(hover.or(touch.map(|t| t.center_pos))));
            self.zoomed_at = now_ms();
        }
        // Drag to pan: the left button (not while measuring), the wheel, the right button.
        let panning = touch.is_none()
            && (resp.dragged_by(PointerButton::Middle)
                || resp.dragged_by(PointerButton::Secondary)
                || (resp.dragged_by(PointerButton::Primary) && !self.measuring));
        if panning {
            let d = resp.drag_delta();
            self.view.pan((f64::from(d.x), f64::from(d.y)));
        }
        if resp.double_clicked_by(PointerButton::Middle) {
            self.fit();
        }
        if self.measuring {
            let was = self.hover;
            self.hover = None;
            self.snapped = false;
            if let Some(p) = hover {
                let (q, snapped) = self.snap(self.to_world(p));
                self.snapped = snapped;
                self.hover = Some(q);
                if resp.clicked_by(PointerButton::Primary) {
                    if self.points.len() >= 2 {
                        self.points.clear();
                    }
                    self.points.push(q);
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.points.clear();
            }
            if was != self.hover || resp.clicked() {
                self.tell_measure();
            }
            ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        } else if panning {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }
    }

    /// Build the list, the detail or the vertex data again when the drawing, the layers shown,
    /// the theme or the zoom asks for it.
    fn prepare(&mut self, ctx: &egui::Context) {
        let seen = self.seen();
        let scale = self.view.scale;
        let dark = self.dark;
        let Some(scene) = &mut self.scene else { return };
        if self.list_dirty {
            scene.build(scale);
            self.list_dirty = false;
            self.whole.dirty = true;
            self.snaps = None;
        }
        if scene.wants_detail(&seen, scale) {
            if now_ms() - self.zoomed_at >= SETTLE_MS {
                scene.build_detail(&seen, scale);
                self.part.dirty = true;
                self.snaps = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(SETTLE_MS as u64));
            }
        }
        self.use_detail = scene.detail_serves(&seen, scale);
        let bg = if scene.space.is_paper() {
            scene::PAPER
        } else if dark {
            scene::DARK
        } else {
            scene::LIGHT
        };
        for (mesh, detail) in [(&mut self.whole, false), (&mut self.part, true)] {
            // The detail's vertex data is made when it is drawn; the whole's always.
            if !mesh.dirty || (detail && !self.use_detail) {
                continue;
            }
            let origin = match (detail, &scene.detail) {
                (true, Some(d)) => d.window.center(),
                _ => scene.list.bounds.center(),
            };
            let made = scene.mesh(detail, origin, bg);
            if let Ok(mut s) = mesh.slot.lock() {
                *s = Some(made);
            }
            mesh.origin = origin;
            mesh.key += 1;
            mesh.dirty = false;
            mesh.ready = true;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let t0 = now_ms();
        let rect = ui.max_rect();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let resized = self.rect != rect;
        self.rect = rect;
        if self.fit_pending || (resized && self.scene.is_some() && self.view.scale == self.view.fitted && self.whole.ready) {
            self.fit();
        }
        self.input(ui, &resp);
        self.prepare(ui.ctx());
        self.paint(ui);
        if let (Some(at), true) = (self.opened_at, self.whole.ready) {
            self.opened_at = None;
            let ms = now_ms() - at;
            let took = self.scene.as_ref().map(|s| s.took).unwrap_or_default();
            self.events.push(json!({ "kind": "drawn", "ms": ms.round(), "mesh_ms": took.mesh_ms.round() }));
        }
        let ms = now_ms() - t0;
        self.frames += 1;
        self.frame_ms_total += ms;
        self.frame_ms_most = self.frame_ms_most.max(ms);
    }

    fn paint(&mut self, ui: &egui::Ui) {
        let painter = ui.painter_at(self.rect);
        let Some(scene) = &self.scene else {
            painter.rect_filled(self.rect, 0.0, c32(if self.dark { scene::DARK } else { scene::LIGHT }));
            return;
        };
        match scene.list.sheet {
            Some(sheet) => {
                painter.rect_filled(self.rect, 0.0, c32(if self.dark { scene::SURROUND_DARK } else { scene::SURROUND_LIGHT }));
                let paper = Rect::from_two_pos(self.to_screen(Vec2::new(0.0, sheet.size.y)), self.to_screen(Vec2::new(sheet.size.x, 0.0)));
                painter.rect_filled(paper.translate(vec2(0.0, 2.0)), 2.0, Color32::from_black_alpha(if self.dark { 120 } else { 40 }));
                painter.rect_filled(paper, 0.0, c32(scene::PAPER));
            }
            None => {
                painter.rect_filled(self.rect, 0.0, c32(self.background()));
            }
        }
        let mesh = if self.use_detail && self.part.ready { &self.part } else { &self.whole };
        if self.gpu && mesh.ready {
            let center = self.rect.center();
            painter.add(egui_wgpu::Callback::new_paint_callback(
                self.rect,
                CanvasCallback {
                    key: mesh.key,
                    slot: mesh.slot.clone(),
                    offset: [(mesh.origin.x - self.view.center.x) as f32, (mesh.origin.y - self.view.center.y) as f32],
                    scale: self.view.scale as f32,
                    center: [center.x, center.y],
                    layer: mesh.layer,
                    xform: IDENTITY_XFORM,
                    dash: 0.0,
                    stencil: Stencil::Off,
                },
            ));
        }
        // Endless lines (xlines, rays) are drawn across the view.
        let reach = f64::from(self.rect.width() + self.rect.height()) / self.view.scale;
        let bg = self.background();
        let (list, prims) = scene.shown(self.use_detail && self.part.ready);
        for p in prims {
            let Kind::Infinite { ray } = p.kind else { continue };
            let v = list.points(p);
            let (Some(base), Some(dir)) = (v.first().copied(), v.get(1).copied()) else { continue };
            let far = base.dist(self.view.center) + reach;
            let a = if ray { base } else { base - dir * far };
            let b = base + dir * far;
            let rgb = p.display_rgb(bg);
            painter.line_segment([self.to_screen(a), self.to_screen(b)], Stroke::new(1.0, c32(rgb)));
        }
        if self.measuring {
            let accent = self.accent();
            let ends: Vec<Vec2> = match (self.points.as_slice(), self.hover) {
                ([a], Some(h)) => vec![*a, h],
                (ps, _) => ps.to_vec(),
            };
            if let [a, b] = ends.as_slice() {
                painter.line_segment([self.to_screen(*a), self.to_screen(*b)], Stroke::new(1.5, accent));
            }
            for p in &self.points {
                painter.circle_filled(self.to_screen(*p), 3.5, accent);
            }
            if let (Some(h), true) = (self.hover, self.snapped) {
                let r = Rect::from_center_size(self.to_screen(h), vec2(10.0, 10.0));
                painter.rect_stroke(r, 0.0, Stroke::new(1.5, accent), egui::StrokeKind::Middle);
            }
        }
    }
}

struct App {
    state: Rc<RefCell<State>>,
    listener: Rc<RefCell<Option<js_sys::Function>>>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let events = {
            let mut s = self.state.borrow_mut();
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| s.ui(ui));
            std::mem::take(&mut s.events)
        };
        // Told after the state is let go: the page's function may call the viewer back.
        if let Some(f) = self.listener.borrow().as_ref() {
            for e in events {
                let _ = f.call1(&JsValue::NULL, &JsValue::from_str(&e.to_string()));
            }
        }
    }
}

/// A drawing view on a page's canvas.
#[wasm_bindgen]
pub struct Viewer {
    state: Rc<RefCell<State>>,
    listener: Rc<RefCell<Option<js_sys::Function>>>,
    ctx: Rc<RefCell<Option<egui::Context>>>,
    /// The GPU device, destroyed with the viewer: its buffers are let go at once rather than
    /// when the page's garbage collector gets to them (hundreds of MB for a large drawing).
    device: Rc<RefCell<Option<egui_wgpu::wgpu::Device>>>,
    canvas: web_sys::HtmlCanvasElement,
    runner: eframe::WebRunner,
}

/// Lose the canvas's WebGL2 context, if it has one (a WebGPU canvas has none: `getContext`
/// answers null).
fn lose_webgl(canvas: &web_sys::HtmlCanvasElement) {
    use js_sys::{Function, Reflect};
    let Ok(Some(gl)) = canvas.get_context("webgl2") else { return };
    let ext = Reflect::get(&gl, &"getExtension".into())
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
        .and_then(|f| f.call1(&gl, &"WEBGL_lose_context".into()).ok())
        .filter(|e| e.is_object());
    if let Some(ext) = ext
        && let Some(lose) = Reflect::get(&ext, &"loseContext".into()).ok().and_then(|f| f.dyn_into::<Function>().ok())
    {
        let _ = lose.call0(&ext);
    }
}

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// A JavaScript error as text: its message (`Error`, `DOMException`), else the value as a string.
fn js_text(e: &JsValue) -> String {
    if let Some(s) = e.as_string() {
        return s;
    }
    if let Some(m) = js_sys::Reflect::get(e, &"message".into()).ok().and_then(|m| m.as_string()) {
        let name = js_sys::Reflect::get(e, &"name".into()).ok().and_then(|n| n.as_string());
        return match name {
            Some(n) if !n.is_empty() && n != "Error" => format!("{n}: {m}"),
            _ => m,
        };
    }
    format!("{e:?}")
}

/// Start the viewer on `canvas` (sized by the page's CSS), in the dark theme or the light one;
/// `webgl` keeps it to WebGL2 even where the browser has WebGPU.
///
/// It fails with the reason as text. A browser can offer WebGPU and then not give it (no adapter
/// for the page's GPU, the device refused): the canvas then holds a WebGPU context and cannot get
/// a WebGL2 one, so the page starts again on a fresh canvas with `webgl`.
#[wasm_bindgen]
pub async fn start(canvas: web_sys::HtmlCanvasElement, dark: bool, webgl: bool) -> Result<Viewer, JsValue> {
    let _ = eframe::WebLogger::init(log::LevelFilter::Warn);
    let state = Rc::new(RefCell::new(State::new(dark)));
    let listener = Rc::new(RefCell::new(None));
    let ctx_cell: Rc<RefCell<Option<egui::Context>>> = Rc::new(RefCell::new(None));
    let device: Rc<RefCell<Option<egui_wgpu::wgpu::Device>>> = Rc::new(RefCell::new(None));
    let mut options = eframe::WebOptions::default();
    if webgl && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
        create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
    }
    let runner = eframe::WebRunner::new();
    let (s, l, c, d) = (state.clone(), listener.clone(), ctx_cell.clone(), device.clone());
    let started = runner
        .start(
            canvas.clone(),
            options,
            Box::new(move |cc| {
                *c.borrow_mut() = Some(cc.egui_ctx.clone());
                if let Some(rs) = &cc.wgpu_render_state {
                    *d.borrow_mut() = Some(rs.device.clone());
                    let faults = Arc::new(GpuFaults::default());
                    cadcraft_ui_egui::gpu::install(rs, &faults, None);
                    s.borrow_mut().gpu = true;
                }
                Ok(Box::new(App { state: s, listener: l }))
            }),
        )
        .await;
    let api = if webgl { "WebGL2" } else { "WebGPU or WebGL2" };
    if let Err(e) = started {
        // The runner listens to the canvas from before the GPU is asked for: let go of it.
        runner.destroy();
        return Err(err(format!("{api} could not start: {}", js_text(&e))));
    }
    if !state.borrow().gpu {
        runner.destroy();
        return Err(err(format!("{api} could not start: no GPU renderer")));
    }
    Ok(Viewer { state, listener, ctx: ctx_cell, device, canvas, runner })
}

#[wasm_bindgen]
impl Viewer {
    fn repaint(&self) {
        if let Some(c) = self.ctx.borrow().as_ref() {
            c.request_repaint();
        }
    }

    /// Open a drawing from its bytes (a DWG, a DXF, or one packed by cadcraft-index): what it is,
    /// as JSON (`name`, `unit`, `items`, `sheets`, `layers`, `took`). The picture follows on the
    /// next frame; the `drawn` event says when.
    pub fn open(&self, bytes: &[u8], name: &str) -> Result<String, JsValue> {
        let t0 = now_ms();
        let (drawing, read_ms) = Scene::read(bytes, name).map_err(err)?;
        let px = f64::from(self.state.borrow().rect.width().max(800.0));
        let scene = Scene::new(drawing, name, read_ms, px);
        let about = scene.about();
        {
            let mut s = self.state.borrow_mut();
            s.scene = Some(scene);
            s.views.clear();
            s.points.clear();
            s.snaps = None;
            s.whole = Mesh::new(LAYER_DRAWING);
            s.part = Mesh::new(LAYER_DRAG);
            s.whole.dirty = true;
            s.opened_at = Some(t0);
            s.fit();
        }
        self.repaint();
        Ok(about.to_string())
    }

    /// The layers, as JSON: `[{name, color, visible}]`.
    pub fn layers(&self) -> String {
        self.state.borrow().scene.as_ref().map_or_else(|| "[]".into(), |s| s.layers_json().to_string())
    }

    /// Show or hide the layers named (a JSON array of names).
    pub fn set_layers(&self, names: &str, visible: bool) -> Result<(), JsValue> {
        let names: Vec<String> = serde_json::from_str(names).map_err(err)?;
        {
            let mut s = self.state.borrow_mut();
            let Some(scene) = &mut s.scene else { return Ok(()) };
            let rebuild = scene.set_visible(&names, visible);
            s.list_dirty |= rebuild;
            s.whole.dirty = true;
            s.part.dirty = true;
            s.snaps = None;
        }
        self.repaint();
        Ok(())
    }

    /// Show a sheet (layout) by name, or model space for "".
    pub fn set_sheet(&self, name: &str) {
        {
            let mut s = self.state.borrow_mut();
            let space = if name.is_empty() { Space::Model } else { Space::Paper(name.to_string()) };
            let Some(scene) = &s.scene else { return };
            if scene.space == space {
                return;
            }
            let was = scene.space.tab_name().to_string();
            // A sheet opens fitted to its paper; a space looked at before, as it was left.
            let (w, h) = (f64::from(s.rect.width().max(100.0)), f64::from(s.rect.height().max(100.0)));
            let paper = match &space {
                Space::Paper(n) => cadcraft_render::sheet(&scene.drawing, n).map(|sh| sh.bounds()),
                Space::Model => None,
            };
            let view =
                s.views.get(space.tab_name()).copied().unwrap_or_else(|| View::fit(&paper.unwrap_or_else(|| scene.drawing.extents(&space)), w, h));
            let left = s.view;
            s.views.insert(was, left);
            if let Some(scene) = &mut s.scene {
                scene.show(space, view.fitted);
            }
            s.view = view;
            s.zoomed_at = now_ms();
            s.whole.dirty = true;
            s.part.dirty = true;
            s.snaps = None;
            s.points.clear();
        }
        self.repaint();
    }

    /// See the whole drawing (or the whole sheet).
    pub fn fit(&self) {
        self.state.borrow_mut().fit();
        self.repaint();
    }

    /// Zoom about the middle of the view.
    pub fn zoom(&self, factor: f64) {
        {
            let mut s = self.state.borrow_mut();
            s.view.zoom(factor, (0.0, 0.0));
            s.zoomed_at = now_ms();
        }
        self.repaint();
    }

    /// The page's theme: model space dark or light (a sheet's paper stays white).
    pub fn set_dark(&self, dark: bool) {
        {
            let mut s = self.state.borrow_mut();
            if s.dark == dark {
                return;
            }
            s.dark = dark;
            s.whole.dirty = true;
            s.part.dirty = true;
        }
        self.repaint();
    }

    /// Measure on or off: clicks then pick points (snapped to ends and corners).
    pub fn set_measure(&self, on: bool) {
        {
            let mut s = self.state.borrow_mut();
            s.measuring = on;
            s.points.clear();
            s.hover = None;
            s.tell_measure();
        }
        self.repaint();
    }

    /// The function the viewer tells what happens, with a JSON string: `{kind: "measure",
    /// measure: {points, distance, dx, dy}}` and `{kind: "drawn", ms}` (the first picture of a
    /// drawing opened).
    pub fn on_event(&self, f: js_sys::Function) {
        *self.listener.borrow_mut() = Some(f);
    }

    /// How the frames went, as JSON (for measuring smoothness): frames painted, their mean and
    /// longest time in the viewer's own code, and the vertex data's last build.
    pub fn stats(&self) -> String {
        let s = self.state.borrow();
        let took = s.scene.as_ref().map(|s| s.took).unwrap_or_default();
        json!({
            "frames": s.frames,
            "frame_ms_mean": if s.frames > 0 { s.frame_ms_total / s.frames as f64 } else { 0.0 },
            "frame_ms_most": s.frame_ms_most,
            "list_ms": took.list_ms, "detail_ms": took.detail_ms, "mesh_ms": took.mesh_ms, "read_ms": took.read_ms,
            "prims": s.scene.as_ref().map_or(0, |s| s.list.prims.len()),
            "detail_prims": s.scene.as_ref().and_then(|s| s.detail.as_ref()).map_or(0, |d| d.list.prims.len()),
            "detail": s.use_detail,
            "scale": s.view.scale, "built_scale": s.scene.as_ref().map_or(0.0, |s| s.built_scale),
        })
        .to_string()
    }

    /// Reset the frame counts of [`Viewer::stats`].
    pub fn reset_stats(&self) {
        let mut s = self.state.borrow_mut();
        s.frames = 0;
        s.frame_ms_total = 0.0;
        s.frame_ms_most = 0.0;
    }

    /// Stop drawing and let go of the drawing and the GPU: the device is destroyed (WebGPU frees
    /// its memory at once) and a WebGL2 context is lost (the browser keeps only a few).
    pub fn destroy(&self) {
        self.listener.borrow_mut().take();
        if let Ok(mut s) = self.state.try_borrow_mut() {
            s.scene = None;
            s.snaps = None;
            s.whole = Mesh::new(LAYER_DRAWING);
            s.part = Mesh::new(LAYER_DRAG);
        }
        self.runner.destroy();
        if let Some(device) = self.device.borrow_mut().take() {
            device.destroy();
        }
        lose_webgl(&self.canvas);
    }
}
