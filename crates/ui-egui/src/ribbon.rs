//! The ribbon: tabs of panels of command buttons across the top, the face AutoCAD has on Windows
//! (the Tool Sets are AutoCAD for Mac's). It is a second face over the same commands: every
//! button starts a command of the catalog, and nothing here knows what a command does.
//!
//! A tab is a row of panels. A panel holds large buttons (an icon over its name), small ones two
//! rows high (an icon, with or without its name), split buttons (the last variant picked, with a
//! list of the others) and a few drop-down controls (the current layer, colour, lineweight and
//! linetype). A panel's name is a button that lists every tool of the panel, those used less
//! often too. When the window is too narrow for a tab's panels, those on the right fold into one
//! button each that opens the panel below it.
//!
//! What is on it follows what AutoCAD shows on screen (observed, and Autodesk's public help). How
//! it is drawn follows BIMCraft's ribbon (large buttons 64 high, small tools in two rows, the
//! panel's name under its tools as the button to its list), so the two editors read as one
//! family in a host that has both. The icons are CADCraft's own (`icons.rs`).

use egui::{Rect, Sense, pos2, vec2};
use serde_json::json;

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

mod tabs;
pub use tabs::TABS;

/// A command button: its icon, the command it starts, and its name.
pub type Tool = (Icon, &'static str, &'static str);

/// A drop-down that shows and sets a current property of the drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Layer,
    Color,
    Lineweight,
    Linetype,
}

/// One row of a column of small things.
pub enum Cell {
    /// An icon alone.
    Icon(Tool),
    /// An icon and its name.
    Text(Tool),
    /// A split button: the variant last picked, and a list of all of them.
    Split(&'static [Tool]),
    /// A split button with the variant's name.
    SplitText(&'static [Tool]),
    /// Several cells side by side in one row.
    Row(&'static [Cell]),
    Control(Control),
}

/// What a panel holds, left to right.
pub enum Item {
    /// A large button.
    Large(Tool),
    /// A large split button: its caption, and its variants (the first is the one shown at first).
    LargeSplit(&'static str, &'static [Tool]),
    /// A column of up to three rows.
    Col(&'static [Cell]),
    Sep,
}

pub struct PanelDef {
    pub name: &'static str,
    pub items: &'static [Item],
    /// The slide-out: rows of tools used less often.
    pub more: &'static [&'static [Tool]],
}

pub struct TabDef {
    pub name: &'static str,
    pub panels: &'static [PanelDef],
}

const TAB_H: f32 = 26.0;
const CONTENT_H: f32 = 64.0;
const TITLE_H: f32 = 20.0;
/// The ribbon's whole height: its tabs, a panel's content and its title.
pub const HEIGHT: f32 = TAB_H + CONTENT_H + TITLE_H + 6.0;
const ROW_H: f32 = 29.0;
const SMALL: f32 = 30.0;
/// The rows a panel's small tools stand in.
const ROWS: usize = 2;
const ARROW_W: f32 = 12.0;
const LARGE_ICON: f32 = 24.0;
const LARGE_MIN_W: f32 = 48.0;
const CONTROL_W: f32 = 148.0;
const FOLDED_W: f32 = 62.0;
const PAD: f32 = 5.0;
const FONT: f32 = 11.5;

fn font() -> egui::FontId {
    egui::FontId::proportional(FONT)
}

fn known(id: &str) -> bool {
    cadcraft_engine::find_command(id).is_some() || crate::menus::is_ui_command(id)
}

fn text_w(ui: &egui::Ui, text: &str) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), font(), egui::Color32::WHITE).size().x
}

/// A large button's name on at most two lines, broken at the space nearest its middle.
fn two_lines(label: &str) -> (String, Option<String>) {
    let mid = label.len() / 2;
    let best = label.match_indices(' ').map(|(i, _)| i).min_by_key(|i| i.abs_diff(mid));
    match best {
        Some(i) if label.chars().count() > 8 => (label.get(..i).unwrap_or(label).to_string(), label.get(i + 1..).map(str::to_string)),
        _ => (label.to_string(), None),
    }
}

/// What the ribbon asks of the app once it is drawn.
#[derive(Default)]
struct Asked {
    start: Option<&'static str>,
    run: Option<(&'static str, serde_json::Value)>,
}

/// Where a split button's last pick is kept: by its first variant's command, the same on every tab.
fn split_id(first: &'static str) -> egui::Id {
    egui::Id::unique(("ribbon_split", first))
}

/// The variant a split button shows: the one last picked from its list.
fn picked(ui: &egui::Ui, tools: &'static [Tool]) -> Tool {
    let first = tools.first().copied().unwrap_or((Icon::Gear, "", ""));
    let i = ui.data(|d| d.get_temp::<usize>(split_id(first.1))).unwrap_or(0);
    tools.get(i).copied().unwrap_or(first)
}

fn remember(ui: &egui::Ui, tools: &'static [Tool], i: usize) {
    if let Some(first) = tools.first() {
        ui.data_mut(|d| d.insert_temp(split_id(first.1), i));
    }
}

fn describe(ui: &egui::Ui, resp: &egui::Response, label: &str) {
    icons::describe_control(ui, resp, &crate::i18n::t(label), None, false);
    crate::control::record_widget(ui, resp, label, "button", None);
}

fn hover_fill(ui: &egui::Ui, rect: Rect, resp: &egui::Response) {
    if resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, Tokens::get().control_hover.gamma_multiply(0.6));
    }
}

fn chevron(ui: &egui::Ui, center: egui::Pos2) {
    icons::paint(ui.painter(), Rect::from_center_size(center, vec2(8.0, 8.0)), Icon::ChevronDown, false);
}

/// The list a split button opens: each variant with its icon; picking one starts it and makes it
/// the one the button shows.
fn variants_menu(_ui: &egui::Ui, opener: &egui::Response, tools: &'static [Tool], asked: &mut Asked) {
    let t = Tokens::get();
    egui::Popup::menu(opener).show(|ui| {
        for (i, (icon, cmd, label)) in tools.iter().enumerate().filter(|(_, tool)| known(tool.1)) {
            let name = crate::i18n::t(label);
            let w = (text_w(ui, &name) + 44.0).max(150.0);
            let (r, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::click());
            hover_fill(ui, r, &resp);
            icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 13.0, r.center().y), vec2(16.0, 16.0)), *icon, false);
            ui.painter().text(pos2(r.left() + 28.0, r.center().y), egui::Align2::LEFT_CENTER, name, font(), t.text);
            describe(ui, &resp, label);
            if resp.clicked() {
                remember(ui, tools, i);
                asked.start = Some(cmd);
                ui.close();
            }
        }
    });
}

// ---------------------------------------------------------------------------------- small cells

fn cell_w(ui: &egui::Ui, cell: &Cell) -> f32 {
    match cell {
        Cell::Icon(_) => SMALL,
        Cell::Text((_, _, label)) => SMALL + text_w(ui, &crate::i18n::t(label)) + 6.0,
        Cell::Split(_) => SMALL + ARROW_W,
        Cell::SplitText(tools) => {
            let widest = tools.iter().map(|tool| text_w(ui, &crate::i18n::t(tool.2))).fold(0.0, f32::max);
            SMALL + widest.min(72.0) + 6.0 + ARROW_W
        }
        Cell::Row(cells) => cells.iter().map(|c| cell_w(ui, c)).sum(),
        Cell::Control(_) => CONTROL_W,
    }
}

fn small_button(ui: &mut egui::Ui, tool: Tool, with_text: bool, w: f32, asked: &mut Asked) {
    let (icon, cmd, label) = tool;
    let (r, resp) = ui.allocate_exact_size(vec2(w, ROW_H - 2.0), Sense::click());
    if !known(cmd) {
        return;
    }
    hover_fill(ui, r, &resp);
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + SMALL / 2.0, r.center().y), vec2(17.0, 17.0)), icon, false);
    if with_text {
        let name = crate::i18n::t(label);
        ui.painter().text(pos2(r.left() + SMALL, r.center().y), egui::Align2::LEFT_CENTER, name, font(), Tokens::get().text);
    }
    describe(ui, &resp, label);
    if resp.on_hover_text(crate::i18n::t(label)).clicked() {
        asked.start = Some(cmd);
    }
}

fn small_split(ui: &mut egui::Ui, tools: &'static [Tool], with_text: bool, w: f32, asked: &mut Asked) {
    let shown = picked(ui, tools);
    ui.spacing_mut().item_spacing.x = 0.0;
    small_button(ui, shown, with_text, w - ARROW_W, asked);
    let (r, resp) = ui.allocate_exact_size(vec2(ARROW_W, ROW_H - 2.0), Sense::click());
    hover_fill(ui, r, &resp);
    chevron(ui, r.center());
    describe(ui, &resp, "More");
    variants_menu(ui, &resp, tools, asked);
}

fn cell(app: &CadApp, ui: &mut egui::Ui, c: &Cell, asked: &mut Asked) {
    let w = cell_w(ui, c);
    match c {
        Cell::Icon(tool) => small_button(ui, *tool, false, w, asked),
        Cell::Text(tool) => small_button(ui, *tool, true, w, asked),
        Cell::Split(tools) => small_split(ui, tools, false, w, asked),
        Cell::SplitText(tools) => small_split(ui, tools, true, w, asked),
        Cell::Row(cells) => {
            for inner in *cells {
                cell(app, ui, inner, asked);
            }
        }
        Cell::Control(control) => controls::show(app, ui, *control, asked),
    }
}

/// Small things stand two rows high: a list of more than two goes on in the next column.
fn columns(app: &CadApp, ui: &mut egui::Ui, cells: &'static [Cell], asked: &mut Asked) {
    for pair in cells.chunks(ROWS) {
        column(app, ui, pair, asked);
    }
}

fn columns_w(ui: &egui::Ui, cells: &[Cell]) -> f32 {
    cells.chunks(ROWS).map(|pair| pair.iter().map(|c| cell_w(ui, c)).fold(0.0, f32::max) + 2.0).sum::<f32>() - 2.0
}

fn column(app: &CadApp, ui: &mut egui::Ui, cells: &'static [Cell], asked: &mut Asked) {
    let w = cells.iter().map(|c| cell_w(ui, c)).fold(0.0, f32::max);
    ui.allocate_ui_with_layout(vec2(w, CONTENT_H), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        ui.add_space((CONTENT_H - ROWS as f32 * ROW_H) / 2.0);
        for c in cells {
            ui.allocate_ui_with_layout(vec2(w, ROW_H - 2.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                cell(app, ui, c, asked);
            });
        }
    });
}

// ---------------------------------------------------------------------------------- large buttons

fn large_w(ui: &egui::Ui, label: &str) -> f32 {
    let (a, b) = two_lines(&crate::i18n::t(label));
    let w = text_w(ui, &a).max(b.as_deref().map_or(0.0, |b| text_w(ui, b)));
    (w + 10.0).max(LARGE_MIN_W)
}

/// A large button's icon and name; `arrow` draws the split button's mark after the name.
fn paint_large(ui: &egui::Ui, r: Rect, icon: Icon, label: &str, arrow: bool) {
    let t = Tokens::get();
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.center().x, r.top() + 5.0 + LARGE_ICON / 2.0), vec2(LARGE_ICON, LARGE_ICON)), icon, false);
    let (a, b) = two_lines(&crate::i18n::t(label));
    let y = r.top() + LARGE_ICON + 15.0;
    ui.painter().text(pos2(r.center().x, y), egui::Align2::CENTER_CENTER, a, font(), t.text);
    let last = match b {
        Some(b) => {
            ui.painter().text(pos2(r.center().x, y + 13.0), egui::Align2::CENTER_CENTER, b, font(), t.text);
            y + 13.0
        }
        None => y,
    };
    if arrow {
        let w = text_w(ui, &crate::i18n::t(label).rsplit(' ').next().unwrap_or_default().to_string());
        chevron(ui, pos2((r.center().x + w / 2.0 + 6.0).min(r.right() - 5.0), last + 1.0));
    }
}

fn large(ui: &mut egui::Ui, tool: Tool, asked: &mut Asked) {
    let (icon, cmd, label) = tool;
    let (r, resp) = ui.allocate_exact_size(vec2(large_w(ui, label), CONTENT_H), Sense::click());
    if !known(cmd) {
        return;
    }
    hover_fill(ui, r, &resp);
    paint_large(ui, r, icon, label, false);
    describe(ui, &resp, label);
    if resp.on_hover_text(crate::i18n::t(label)).clicked() {
        asked.start = Some(cmd);
    }
}

/// A large split button: the upper half starts the variant shown, the lower half lists them all.
fn large_split(ui: &mut egui::Ui, caption: &'static str, tools: &'static [Tool], asked: &mut Asked) {
    let shown = picked(ui, tools);
    let (r, _) = ui.allocate_exact_size(vec2(large_w(ui, caption), CONTENT_H), Sense::hover());
    let split = r.top() + LARGE_ICON + 8.0;
    let upper = Rect::from_min_max(r.min, pos2(r.right(), split));
    let lower = Rect::from_min_max(pos2(r.left(), split), r.max);
    let run = ui.interact(upper, ui.scope_id().with(("rb_run", caption)), Sense::click());
    let list = ui.interact(lower, ui.scope_id().with(("rb_list", caption)), Sense::click());
    hover_fill(ui, upper, &run);
    hover_fill(ui, lower, &list);
    paint_large(ui, r, shown.0, caption, true);
    describe(ui, &run, shown.2);
    describe(ui, &list, caption);
    if run.on_hover_text(crate::i18n::t(shown.2)).clicked() && known(shown.1) {
        asked.start = Some(shown.1);
    }
    variants_menu(ui, &list, tools, asked);
}

// ---------------------------------------------------------------------------------- panels

fn item_w(ui: &egui::Ui, item: &Item) -> f32 {
    match item {
        Item::Large((_, _, label)) => large_w(ui, label),
        Item::LargeSplit(caption, _) => large_w(ui, caption),
        Item::Col(cells) => columns_w(ui, cells),
        Item::Sep => 7.0,
    }
}

fn panel_w(ui: &egui::Ui, def: &PanelDef) -> f32 {
    let items: f32 = def.items.iter().map(|i| item_w(ui, i) + 2.0).sum();
    (items + 2.0 * PAD).max(text_w(ui, &crate::i18n::t(def.name)) + 26.0)
}

fn items(app: &CadApp, ui: &mut egui::Ui, def: &'static PanelDef, asked: &mut Asked) {
    ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
    for item in def.items {
        match item {
            Item::Large(tool) => large(ui, *tool, asked),
            Item::LargeSplit(caption, tools) => large_split(ui, caption, tools, asked),
            Item::Col(cells) => columns(app, ui, cells, asked),
            Item::Sep => {
                let (r, _) = ui.allocate_exact_size(vec2(5.0, CONTENT_H), Sense::hover());
                ui.painter().vline(r.center().x, r.y_range().shrink(6.0), egui::Stroke::new(1.0, Tokens::get().border));
            }
        }
    }
}

/// Every tool of a panel, those on it and those used less often, in order.
fn every_tool(def: &'static PanelDef) -> Vec<Tool> {
    fn of(cell: &'static Cell, out: &mut Vec<Tool>) {
        match cell {
            Cell::Icon(t) | Cell::Text(t) => out.push(*t),
            Cell::Split(ts) | Cell::SplitText(ts) => out.extend(ts.iter().copied()),
            Cell::Row(cells) => cells.iter().for_each(|c| of(c, out)),
            Cell::Control(_) => {}
        }
    }
    let mut out = Vec::new();
    for item in def.items {
        match item {
            Item::Large(t) => out.push(*t),
            Item::LargeSplit(_, ts) => out.extend(ts.iter().copied()),
            Item::Col(cells) => cells.iter().for_each(|c| of(c, &mut out)),
            Item::Sep => {}
        }
    }
    out.extend(def.more.iter().flat_map(|row| row.iter().copied()));
    out.retain(|t| known(t.1));
    out
}

/// A panel's list: every tool with its icon and name, in columns of at most ten.
fn list(ui: &mut egui::Ui, def: &'static PanelDef, asked: &mut Asked) {
    let t = Tokens::get();
    let tools = every_tool(def);
    ui.horizontal_top(|ui| {
        for chunk in tools.chunks(10) {
            ui.vertical(|ui| {
                let w = chunk.iter().map(|tool| text_w(ui, &crate::i18n::t(tool.2))).fold(110.0, f32::max) + 40.0;
                for (icon, cmd, label) in chunk {
                    let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
                    hover_fill(ui, r, &resp);
                    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(17.0, 17.0)), *icon, false);
                    ui.painter().text(pos2(r.left() + 30.0, r.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t(label), font(), t.text);
                    describe(ui, &resp, label);
                    if resp.clicked() {
                        asked.start = Some(cmd);
                        ui.close();
                    }
                }
            });
        }
    });
}

/// A panel's name, under its tools: the button to its list.
fn title(ui: &mut egui::Ui, r: Rect, def: &'static PanelDef, asked: &mut Asked) {
    let t = Tokens::get();
    let name = crate::i18n::t(def.name);
    let w = text_w(ui, &name) + 16.0;
    let hit = Rect::from_center_size(r.center(), vec2(w + 8.0, r.height()));
    let resp = ui.interact(hit, ui.scope_id().with(("rb_title", def.name)), Sense::click());
    hover_fill(ui, hit, &resp);
    ui.painter().text(pos2(r.center().x - 6.0, r.center().y), egui::Align2::CENTER_CENTER, name, font(), t.text_dim);
    chevron(ui, pos2(r.center().x + w / 2.0 - 5.0, r.center().y));
    describe(ui, &resp, def.name);
    egui::Popup::menu(&resp).show(|ui| list(ui, def, asked));
}

fn panel(app: &CadApp, ui: &mut egui::Ui, def: &'static PanelDef, w: f32, asked: &mut Asked) {
    let (r, _) = ui.allocate_exact_size(vec2(w, CONTENT_H + TITLE_H), Sense::hover());
    let content = Rect::from_min_size(pos2(r.left() + PAD, r.top()), vec2(w - 2.0 * PAD, CONTENT_H));
    ui.scope_builder(egui::UiBuilder::new().max_rect(content).layout(egui::Layout::left_to_right(egui::Align::Min)), |ui| items(app, ui, def, asked));
    title(ui, Rect::from_min_max(pos2(r.left(), r.bottom() - TITLE_H), r.max), def, asked);
    ui.painter().vline(r.right(), r.y_range().shrink(4.0), egui::Stroke::new(1.0, Tokens::get().border));
}

/// A panel folded for want of room: one button that opens the panel below it.
fn folded(app: &CadApp, ui: &mut egui::Ui, def: &'static PanelDef, asked: &mut Asked) {
    let (r, resp) = ui.allocate_exact_size(vec2(FOLDED_W, CONTENT_H + TITLE_H), Sense::click());
    hover_fill(ui, r, &resp);
    let icon = def.items.iter().find_map(|i| match i {
        Item::Large(tool) => Some(tool.0),
        Item::LargeSplit(_, tools) => tools.first().map(|tool| tool.0),
        _ => None,
    });
    paint_large(ui, r, icon.unwrap_or(Icon::Menu), def.name, true);
    describe(ui, &resp, def.name);
    ui.painter().vline(r.right(), r.y_range().shrink(4.0), egui::Stroke::new(1.0, Tokens::get().border));
    egui::Popup::menu(&resp).show(|ui| {
        ui.horizontal(|ui| items(app, ui, def, asked));
        ui.separator();
        list(ui, def, asked);
    });
}

/// How many of the panels (from the left) are drawn whole in `room`; the rest fold.
fn whole(widths: &[f32], room: f32) -> usize {
    let mut n = widths.len();
    loop {
        let used: f32 = widths.iter().take(n).sum::<f32>() + (widths.len() - n) as f32 * FOLDED_W;
        if used <= room || n == 0 {
            return n;
        }
        n -= 1;
    }
}

fn tab_row(app: &mut CadApp, ui: &mut egui::Ui, row: Rect) {
    let t = Tokens::get();
    let mut x = row.left() + 6.0;
    for tab in TABS {
        let name = crate::i18n::t(tab.name);
        let w = text_w(ui, &name) + 22.0;
        let r = Rect::from_min_size(pos2(x, row.top()), vec2(w, TAB_H));
        let active = app.ui.ribbon_tab == tab.name;
        let resp = ui.interact(r, ui.scope_id().with(("rb_tab", tab.name)), Sense::click());
        if active {
            ui.painter().rect_filled(r.shrink2(vec2(1.0, 3.0)), 4.0, t.accent.gamma_multiply(0.28));
        } else if resp.hovered() {
            ui.painter().rect_filled(r.shrink2(vec2(0.0, 3.0)), 3.0, t.control_hover.gamma_multiply(0.5));
        }
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, name.to_string(), egui::FontId::proportional(12.5), if active { t.text } else { t.text_dim });
        icons::describe_control(ui, &resp, &name, Some(active), true);
        if resp.clicked() {
            app.ui.ribbon_tab = tab.name.to_string();
        }
        x += w;
    }
}

/// The ribbon, under the title and tool bar.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    if !TABS.iter().any(|tab| tab.name == app.ui.ribbon_tab) {
        app.ui.ribbon_tab = TABS.first().map(|tab| tab.name.to_string()).unwrap_or_default();
    }
    let mut asked = Asked::default();
    egui::Panel::top("cc_ribbon").exact_size(HEIGHT).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        tab_row(app, ui, Rect::from_min_size(r.min, vec2(r.width(), TAB_H)));
        let body = Rect::from_min_max(pos2(r.left(), r.top() + TAB_H), r.max);
        ui.painter().rect_filled(body, 0.0, t.panel);
        ui.painter().hline(body.x_range(), body.bottom() - 0.5, egui::Stroke::new(1.0, t.border));
        let Some(tab) = TABS.iter().find(|tab| tab.name == app.ui.ribbon_tab) else { return };
        let inner = body.shrink2(vec2(4.0, 3.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Min)), |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let widths: Vec<f32> = tab.panels.iter().map(|p| panel_w(ui, p)).collect();
            let n = whole(&widths, inner.width());
            for (i, def) in tab.panels.iter().enumerate() {
                match widths.get(i) {
                    Some(w) if i < n => panel(app, ui, def, *w, &mut asked),
                    _ => folded(app, ui, def, &mut asked),
                }
            }
        });
    });
    if let Some((id, params)) = asked.run.take()
        && let Err(e) = app.run(id, params)
    {
        app.session.echo(e);
    }
    if let Some(cmd) = asked.start {
        app.start(cmd);
    }
}

mod controls {
    //! The ribbon's drop-downs: the drawing's current layer, colour, lineweight and linetype.
    use super::{Asked, CONTROL_W, Control, ROW_H, json};
    use crate::CadApp;
    use cadcraft_engine::doc::HVal;

    const COLORS: [(&str, i64); 9] =
        [("ByLayer", 256), ("ByBlock", 0), ("Red", 1), ("Yellow", 2), ("Green", 3), ("Cyan", 4), ("Blue", 5), ("Magenta", 6), ("White", 7)];
    /// Lineweights in hundredths of a millimetre, as `CELWEIGHT` holds them.
    const WEIGHTS: [i64; 24] = [0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];

    fn weight_name(w: i64) -> String {
        match w {
            -1 => "ByLayer".to_string(),
            -2 => "ByBlock".to_string(),
            -3 => "Default".to_string(),
            w => format!("{:.2} mm", w as f64 / 100.0),
        }
    }

    /// What the control shows now, and what it offers: (shown name, the value to set).
    fn offered(app: &CadApp, control: Control) -> (String, Vec<(String, serde_json::Value)>) {
        let Ok(st) = app.session.state() else { return (String::new(), Vec::new()) };
        let d = st.drawing();
        let text = |name: &str| d.header.get(name).and_then(HVal::as_str).unwrap_or_default().to_string();
        let int = |name: &str, or: i64| d.header.get(name).and_then(HVal::as_i64).unwrap_or(or);
        match control {
            Control::Layer => (text("CLAYER"), d.layers.iter().map(|l| (l.name.clone(), json!({"name": l.name}))).collect()),
            Control::Linetype => (text("CELTYPE"), d.linetypes.iter().map(|l| (l.name.clone(), json!({"current": l.name}))).collect()),
            Control::Color => {
                let now = int("CECOLOR", 256);
                let name = COLORS.iter().find(|(_, v)| *v == now).map_or_else(|| now.to_string(), |(n, _)| (*n).to_string());
                let list = COLORS.iter().map(|(n, v)| ((*n).to_string(), if *v == 256 || *v == 0 { json!({"color": n}) } else { json!({"color": v}) })).collect();
                (name, list)
            }
            Control::Lineweight => {
                let by_layer = std::iter::once(("ByLayer".to_string(), json!({"lineweight": "ByLayer"})));
                let list = by_layer.chain(WEIGHTS.iter().map(|w| (weight_name(*w), json!({"lineweight": *w as f64 / 100.0})))).collect();
                (weight_name(int("CELWEIGHT", -1)), list)
            }
        }
    }

    pub(super) fn show(app: &CadApp, ui: &mut egui::Ui, control: Control, asked: &mut Asked) {
        let (command, label) = match control {
            Control::Layer => ("layer.current", "Layer"),
            Control::Color => ("color", "Object Color"),
            Control::Lineweight => ("lweight", "Lineweight"),
            Control::Linetype => ("linetype", "Linetype"),
        };
        let (now, list) = offered(app, control);
        ui.spacing_mut().interact_size.y = ROW_H - 4.0;
        let resp = egui::ComboBox::from_id_salt(("ribbon_control", label)).width(CONTROL_W - 8.0).selected_text(now.clone()).show_ui(ui, |ui| {
            for (name, value) in list {
                if ui.selectable_label(name == now, &name).clicked() {
                    asked.run = Some((command, value));
                }
            }
        });
        resp.response.on_hover_text(crate::i18n::t(label));
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, Item, TABS, Tool, two_lines, whole};

    fn tools_of(cell: &'static Cell, out: &mut Vec<Tool>) {
        match cell {
            Cell::Icon(t) | Cell::Text(t) => out.push(*t),
            Cell::Split(ts) | Cell::SplitText(ts) => out.extend(ts.iter().copied()),
            Cell::Row(cells) => cells.iter().for_each(|c| tools_of(c, out)),
            Cell::Control(_) => {}
        }
    }

    fn all_tools() -> Vec<(&'static str, &'static str, Tool)> {
        let mut out = Vec::new();
        for tab in TABS {
            for panel in tab.panels {
                let mut tools = Vec::new();
                for item in panel.items {
                    match item {
                        Item::Large(t) => tools.push(*t),
                        Item::LargeSplit(_, ts) => tools.extend(ts.iter().copied()),
                        Item::Col(cells) => cells.iter().for_each(|c| tools_of(c, &mut tools)),
                        Item::Sep => {}
                    }
                }
                tools.extend(panel.more.iter().flat_map(|row| row.iter().copied()));
                out.extend(tools.into_iter().map(|t| (tab.name, panel.name, t)));
            }
        }
        out
    }

    /// Every button of the ribbon starts a command the catalog has: a button that did nothing
    /// would be left out when drawn, and the panel would differ from its definition.
    #[test]
    fn every_button_starts_a_command_that_exists() {
        let missing: Vec<String> =
            all_tools().into_iter().filter(|(_, _, t)| !super::known(t.1)).map(|(tab, panel, t)| format!("{tab} > {panel}: {} ({})", t.2, t.1)).collect();
        assert!(missing.is_empty(), "no such command:\n{}", missing.join("\n"));
    }

    #[test]
    fn a_row_of_small_tools_is_not_a_row_of_rows() {
        for tab in TABS {
            for panel in tab.panels {
                for item in panel.items {
                    if let Item::Col(cells) = item {
                        let nested = cells.iter().any(|c| matches!(c, Cell::Row(inner) if inner.iter().any(|i| matches!(i, Cell::Row(_)))));
                        assert!(!nested, "{} > {}", tab.name, panel.name);
                    }
                }
            }
        }
    }

    #[test]
    fn panels_fold_from_the_right_when_there_is_no_room() {
        let widths = [200.0, 150.0, 100.0];
        assert_eq!(whole(&widths, 500.0), 3);
        assert_eq!(whole(&widths, 440.0), 2, "the last folds to its button");
        assert_eq!(whole(&widths, 330.0), 1);
        assert_eq!(whole(&widths, 10.0), 0);
    }

    #[test]
    fn a_long_name_takes_two_lines() {
        assert_eq!(two_lines("Line"), ("Line".to_string(), None));
        assert_eq!(two_lines("Layer Properties"), ("Layer".to_string(), Some("Properties".to_string())));
        assert_eq!(two_lines("Match Properties"), ("Match".to_string(), Some("Properties".to_string())));
    }
}
