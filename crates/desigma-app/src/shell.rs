//! Window shell: lays out the panels, routes shortcuts and drives screenshots.

use crate::app::{App, MenuKind};
use crate::canvas::tools::Tool;
use crate::canvas::{self, interact};
use crate::cli::Options;
use crate::demo;
use crate::icons::Icon;
use crate::panels::menus::{self, Action};
use crate::panels::{left, rail, right, toolbar};
use crate::theme;
use crate::widgets;
use egui::{Align2, Context, Event, Key, Pos2, Rect, Vec2};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set once an `--screenshot` run has written its PNG.
pub static SHOT_OK: AtomicBool = AtomicBool::new(false);

struct Shot {
    path: PathBuf,
    size: (u32, u32),
    frames: u32,
    requested: bool,
}

pub struct Shell {
    app: App,
    shot: Option<Shot>,
    title: String,
}

impl Shell {
    pub fn new(cc: &eframe::CreationContext<'_>, opts: Options) -> Self {
        Self::build(&cc.egui_ctx, opts)
    }

    pub fn build(ctx: &Context, opts: Options) -> Self {
        theme::install_fonts(ctx);
        theme::apply(ctx);
        let mut app = App::new(demo::demo_document());
        app.doc.name = "Untitled".into();
        if let Some(f) = &opts.file {
            app.open_path(f.clone());
        }
        if let Some(name) = &opts.select {
            if let Some(id) = app.doc.nodes.values().find(|n| &n.name == name).map(|n| n.id) {
                if let Some(pg) = app.doc.page_of(id) {
                    app.page = pg;
                }
                app.selection = vec![id];
            }
        }
        let shot = opts.screenshot.map(|path| Shot {
            path,
            size: opts.size.unwrap_or((theme::DEFAULT_WINDOW.0 as u32, theme::DEFAULT_WINDOW.1 as u32)),
            frames: 0,
            requested: false,
        });
        Self { app, shot, title: String::new() }
    }

    fn drive_screenshot(&mut self, ctx: &Context) {
        let Some(shot) = &mut self.shot else { return };
        shot.frames += 1;
        ctx.request_repaint();
        if shot.frames == 1 {
            // 1 egui point == 1 physical pixel so the PNG matches the requested size.
            ctx.set_pixels_per_point(1.0);
            let npp = ctx.input(|i| i.viewport().native_pixels_per_point).unwrap_or(1.0).max(0.1);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(shot.size.0 as f32 / npp, shot.size.1 as f32 / npp)));
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            if let Event::Screenshot { image, .. } = ev {
                let (w, h) = (image.width() as u32, image.height() as u32);
                let raw: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(buf) = image::RgbaImage::from_raw(w, h, raw) {
                    let buf = if (w, h) != shot.size { image::imageops::resize(&buf, shot.size.0, shot.size.1, image::imageops::FilterType::Lanczos3) } else { buf };
                    if let Some(dir) = shot.path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    match buf.save(&shot.path) {
                        Ok(()) => SHOT_OK.store(true, Ordering::SeqCst),
                        Err(e) => eprintln!("desigma: cannot write {}: {e}", shot.path.display()),
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        let sized = {
            let r = ctx.content_rect();
            (r.width() - shot.size.0 as f32).abs() < 1.5 && (r.height() - shot.size.1 as f32).abs() < 1.5
        };
        if !shot.requested && ((shot.frames >= 12 && sized) || shot.frames >= 60) {
            shot.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if shot.frames > 240 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn handle_keys(&mut self, ctx: &Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let editing_text = self.app.canvas.text.is_some();
        if typing && !editing_text {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                Event::Copy if !editing_text => {
                    if let Some(s) = self.app.copy_selection() {
                        ctx.copy_text(s);
                    }
                }
                Event::Cut if !editing_text => {
                    if let Some(s) = self.app.cut_selection() {
                        ctx.copy_text(s);
                    }
                }
                Event::Paste(s) if !editing_text => self.app.paste(Some(&s)),
                Event::Key { key, pressed: true, modifiers, .. } => {
                    if editing_text {
                        continue;
                    }
                    self.key(ctx, key, modifiers);
                }
                _ => {}
            }
        }
    }

    fn key(&mut self, ctx: &Context, key: Key, m: egui::Modifiers) {
        let app = &mut self.app;
        let cmd = m.command;
        if cmd {
            let act = match (key, m.shift, m.alt) {
                (Key::Z, false, _) => Some(Action::Undo),
                (Key::Z, true, _) | (Key::Y, false, _) => Some(Action::Redo),
                (Key::O, false, _) => Some(Action::Open),
                (Key::S, false, _) => Some(Action::Save),
                (Key::S, true, _) => Some(Action::SaveAs),
                (Key::D, false, _) => Some(Action::Duplicate),
                (Key::G, false, false) => Some(Action::Group),
                (Key::G, true, _) => Some(Action::Ungroup),
                (Key::G, false, true) => Some(Action::FrameSelection),
                (Key::A, false, _) => Some(Action::SelectAll),
                (Key::CloseBracket, false, _) => Some(Action::BringForward),
                (Key::OpenBracket, false, _) => Some(Action::SendBackward),
                (Key::CloseBracket, true, _) => Some(Action::BringToFront),
                (Key::OpenBracket, true, _) => Some(Action::SendToBack),
                (Key::Equals | Key::Plus, _, _) => Some(Action::ZoomIn),
                (Key::Minus, _, _) => Some(Action::ZoomOut),
                (Key::Backslash, _, _) => Some(Action::ToggleSidebar),
                _ => None,
            };
            if let Some(a) = act {
                menus::run(app, ctx, a);
            }
            return;
        }
        match key {
            Key::Delete | Key::Backspace => menus::run(app, ctx, Action::Delete),
            Key::Escape => {
                if !matches!(app.canvas.inter, canvas::Interaction::None) {
                    app.cancel_interaction();
                } else if app.canvas.pen.is_some() {
                    interact::finish_pen(app);
                    app.tool = Tool::Move;
                } else if app.ui.menu.is_some() {
                    app.ui.menu = None;
                } else if app.tool != Tool::Move {
                    app.tool = Tool::Move;
                } else {
                    app.clear_selection();
                }
            }
            Key::Enter => {
                if app.canvas.pen.is_some() {
                    interact::finish_pen(app);
                    app.tool = Tool::Move;
                } else if m.shift {
                    if let Some(p) = app.selection.first().and_then(|s| app.doc.get(*s)).and_then(|n| n.parent) {
                        if p != app.page {
                            app.select_only(p);
                        }
                    }
                } else if let [one] = app.selection.as_slice() {
                    if let Some(n) = app.doc.get(*one) {
                        if matches!(n.kind, desigma_doc::NodeKind::Text(_)) {
                            let id = *one;
                            interact::begin_text_edit(app, id);
                        } else if !n.children.is_empty() {
                            app.selection = n.children.clone();
                        }
                    }
                }
            }
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                if !app.selection.is_empty() {
                    let step = if m.shift { theme::NUDGE_BIG } else { theme::NUDGE };
                    let (dx, dy) = match key {
                        Key::ArrowLeft => (-step, 0.0),
                        Key::ArrowRight => (step, 0.0),
                        Key::ArrowUp => (0.0, -step),
                        _ => (0.0, step),
                    };
                    app.nudge(dx, dy);
                }
            }
            Key::Num1 if m.shift => menus::run(app, ctx, Action::FitAll),
            Key::Num2 if m.shift => menus::run(app, ctx, Action::FitSelection),
            Key::Num0 if m.shift => menus::run(app, ctx, Action::Zoom100),
            Key::A if m.shift => menus::run(app, ctx, Action::AutoLayout),
            k => {
                if !m.alt {
                    if let Some(t) = Tool::from_key(k, m.shift) {
                        app.set_tool(t);
                    }
                }
            }
        }
    }

    fn toast(&mut self, ui: &egui::Ui, screen: Rect) {
        let Some(t) = &self.app.toast else { return };
        if self.app.now > t.until {
            self.app.toast = None;
            return;
        }
        let p = ui.painter();
        let font = theme::font_body();
        let w = widgets::text_width(p, &t.text, &font) + 48.0;
        let r = Rect::from_center_size(Pos2::new(screen.center().x, screen.bottom() - theme::TOAST_BOTTOM - theme::TOAST_HEIGHT / 2.0), Vec2::new(w, theme::TOAST_HEIGHT));
        widgets::elevation_shadow(p, r, theme::TOAST_RADIUS);
        p.rect_filled(r, theme::cr(theme::TOAST_RADIUS), theme::BG_MENU);
        p.circle_filled(Pos2::new(r.left() + 16.0, r.center().y), 4.0, if t.error { theme::DANGER } else { theme::BG_BRAND });
        p.text(Pos2::new(r.left() + 28.0, r.center().y), Align2::LEFT_CENTER, &t.text, font, theme::TEXT);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
}

impl eframe::App for Shell {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.17, 0.17, 0.17, 1.0]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl Shell {
    /// One full UI pass over the window.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.app.now = ctx.input(|i| i.time);
        self.drive_screenshot(&ctx);
        self.handle_keys(&ctx);
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.into_iter().find_map(|f| f.path) {
            self.app.open_path(path);
        }

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, theme::BG);
        let rail_w = theme::RAIL_WIDTH + theme::BORDER_WIDTH;
        let panel_w = theme::LEFT_PANEL_WIDTH + theme::BORDER_WIDTH;
        let rail_rect = Rect::from_min_max(full.min, Pos2::new(full.left() + rail_w, full.bottom()));
        let left_rect = Rect::from_min_max(Pos2::new(rail_rect.right(), full.top()), Pos2::new(rail_rect.right() + panel_w, full.bottom()));
        let right_rect = Rect::from_min_max(Pos2::new(full.right() - theme::RIGHT_PANEL_WIDTH - theme::BORDER_WIDTH, full.top()), full.max);
        let canvas_left = if self.app.ui.left_collapsed { rail_rect.right() } else { left_rect.right() };
        let canvas_rect = Rect::from_min_max(Pos2::new(canvas_left, full.top()), Pos2::new(right_rect.left(), full.bottom()));

        canvas::show(&mut self.app, ui, canvas_rect);
        rail::show(&mut self.app, ui, rail_rect);
        if self.app.ui.left_collapsed {
            let b = Rect::from_min_size(Pos2::new(canvas_rect.left() + 8.0, canvas_rect.top() + 8.0), Vec2::splat(theme::PANEL_ACTION_BUTTON));
            ui.painter().rect_filled(b, theme::cr(theme::RADIUS_MEDIUM), theme::BG);
            if widgets::icon_button(ui, b, "show-sidebar", Icon::Sidebar, false, "Show sidebar").clicked() {
                self.app.ui.left_collapsed = false;
            }
        } else {
            left::show(&mut self.app, ui, left_rect);
        }
        right::show(&mut self.app, ui, right_rect);
        toolbar::show(&mut self.app, ui, canvas_rect_for_toolbar(full));
        menus::show(&mut self.app, &ctx);
        self.toast(ui, full);

        let title = self.app.title();
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        if self.app.ui.menu == Some(MenuKind::Main) && self.app.ui.menu_anchor.is_none() {
            self.app.ui.menu = None;
        }
    }
}

/// The toolbar is centered on the window (not the canvas).
fn canvas_rect_for_toolbar(full: Rect) -> Rect {
    full
}

#[cfg(test)]
mod tests;
