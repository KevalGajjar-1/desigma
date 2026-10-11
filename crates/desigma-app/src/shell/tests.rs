//! End-to-end UI tests: drive the whole window with synthetic egui input.

use super::*;
use desigma_doc::{NodeId, NodeKind, Vec2 as DV};
use egui::{Modifiers, PointerButton, RawInput};

struct Sim {
    ctx: Context,
    shell: Shell,
    t: f64,
    pos: Pos2,
    mods: Modifiers,
}

impl Sim {
    fn new() -> Self {
        Self::with(Options::default())
    }
    fn with(opts: Options) -> Self {
        let ctx = Context::default();
        let shell = Shell::build(&ctx, opts);
        let mut s = Sim { ctx, shell, t: 0.0, pos: Pos2::new(700.0, 500.0), mods: Modifiers::NONE };
        s.frame(vec![]);
        s.frame(vec![]);
        s
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.t += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1088.0))),
            time: Some(self.t),
            events,
            modifiers: self.mods,
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let shell = &mut self.shell;
        let _ = ctx.run_ui(raw, |ui| shell.draw(ui));
    }
    fn app(&self) -> &App {
        &self.shell.app
    }
    fn screen(&self, x: f32, y: f32) -> Pos2 {
        self.app().view.to_screen(self.app().canvas_rect.min, DV::new(x, y))
    }
    fn mv(&mut self, p: Pos2) {
        self.pos = p;
        self.frame(vec![Event::PointerMoved(p)]);
    }
    fn button(&mut self, pressed: bool) {
        let ev = Event::PointerButton { pos: self.pos, button: PointerButton::Primary, pressed, modifiers: self.mods };
        self.frame(vec![ev]);
    }
    fn click(&mut self, p: Pos2) {
        self.mv(p);
        self.button(true);
        self.button(false);
        self.frame(vec![]);
    }
    fn drag(&mut self, a: Pos2, b: Pos2) {
        self.mv(a);
        self.button(true);
        for i in 1..=4 {
            let t = i as f32 / 4.0;
            self.mv(a + (b - a) * t);
        }
        self.button(false);
        self.frame(vec![]);
    }
    fn key(&mut self, key: Key) {
        let m = self.mods;
        self.frame(vec![Event::Key { key, physical_key: Some(key), pressed: true, repeat: false, modifiers: m }]);
        self.frame(vec![Event::Key { key, physical_key: Some(key), pressed: false, repeat: false, modifiers: m }]);
    }
    fn cmd_key(&mut self, key: Key) {
        self.mods = Modifiers::COMMAND;
        self.key(key);
        self.mods = Modifiers::NONE;
    }
    fn type_text(&mut self, s: &str) {
        self.frame(vec![Event::Text(s.into())]);
    }
    fn by_name(&self, name: &str) -> NodeId {
        self.app().doc.nodes.values().find(|n| n.name == name).map(|n| n.id).unwrap_or_else(|| panic!("no node {name}"))
    }
    fn count(&self) -> usize {
        self.app().doc.nodes.len()
    }
}

#[test]
fn first_frames_render_without_panic() {
    let s = Sim::new();
    assert!(s.app().canvas_rect.width() > 500.0);
    assert!(s.app().view.zoom > 0.1);
}

#[test]
fn rectangle_tool_draws_and_undoes() {
    let mut s = Sim::new();
    let before = s.count();
    s.key(Key::R);
    assert_eq!(s.app().tool, Tool::Rectangle);
    let (a, b) = (s.screen(300.0, 800.0), s.screen(380.0, 860.0));
    s.drag(a, b);
    assert_eq!(s.count(), before + 1);
    assert_eq!(s.app().tool, Tool::Move);
    let id = s.app().selection[0];
    let n = s.app().doc.get(id).unwrap();
    assert!(matches!(n.kind, NodeKind::Rectangle));
    assert!((n.size.x - 80.0).abs() < 4.0 && (n.size.y - 60.0).abs() < 4.0, "size {:?}", n.size);
    s.cmd_key(Key::Z);
    assert_eq!(s.count(), before);
}

#[test]
fn click_selects_child_and_drag_moves_it() {
    let mut s = Sim::new();
    let hero = s.by_name("Hero");
    let p = s.screen(300.0, 150.0);
    s.click(p);
    assert_eq!(s.app().selection, vec![hero]);
    let t0 = s.app().doc.get(hero).unwrap().transform.tx;
    let a = s.screen(300.0, 150.0);
    let b = s.screen(330.0, 150.0);
    s.drag(a, b);
    let t1 = s.app().doc.get(hero).unwrap().transform.tx;
    assert!((t1 - t0 - 30.0).abs() < 8.0, "moved by {}", t1 - t0);
    s.cmd_key(Key::Z);
    assert_eq!(s.app().doc.get(hero).unwrap().transform.tx, t0);
}

#[test]
fn resize_with_corner_handle() {
    let mut s = Sim::with(Options { select: Some("Button".into()), ..Default::default() });
    let id = s.by_name("Button");
    let (corners, _) = interact::selection_box(s.app()).unwrap();
    let br = s.app().view.to_screen(s.app().canvas_rect.min, corners[2]);
    let w0 = s.app().doc.get(id).unwrap().size.x;
    s.drag(br, br + Vec2::new(-20.0, 0.0));
    let w1 = s.app().doc.get(id).unwrap().size.x;
    assert!(w1 < w0 - 10.0, "{w0} -> {w1}");
}

#[test]
fn pen_tool_creates_vector_path() {
    let mut s = Sim::new();
    s.key(Key::P);
    for (x, y) in [(300.0, 800.0), (370.0, 840.0), (300.0, 880.0)] {
        let p = s.screen(x, y);
        s.click(p);
    }
    s.key(Key::Enter);
    let id = s.app().selection[0];
    match &s.app().doc.get(id).unwrap().kind {
        NodeKind::Vector { paths } => assert_eq!(paths[0].cmds.len(), 3),
        k => panic!("unexpected {k:?}"),
    }
}

#[test]
fn text_tool_types_and_commits() {
    let mut s = Sim::new();
    s.key(Key::T);
    let p = s.screen(300.0, 820.0);
    s.click(p);
    assert!(s.app().canvas.text.is_some());
    s.type_text("Hi");
    s.key(Key::Escape);
    assert!(s.app().canvas.text.is_none());
    let id = s.app().selection[0];
    match &s.app().doc.get(id).unwrap().kind {
        NodeKind::Text(t) => assert_eq!(t.content, "Hi"),
        k => panic!("unexpected {k:?}"),
    }
    s.cmd_key(Key::Z);
    assert!(s.app().doc.get(id).is_none(), "one undo removes the created text");
}

#[test]
fn empty_text_is_discarded() {
    let mut s = Sim::new();
    let before = s.count();
    s.key(Key::T);
    let p = s.screen(300.0, 820.0);
    s.click(p);
    s.key(Key::Escape);
    assert_eq!(s.count(), before);
    assert!(!s.app().hist.can_undo());
}

#[test]
fn shortcuts_duplicate_group_delete() {
    let mut s = Sim::with(Options { select: Some("Hero".into()), ..Default::default() });
    let n0 = s.count();
    s.cmd_key(Key::D);
    assert_eq!(s.count(), n0 + 1);
    s.cmd_key(Key::G);
    assert_eq!(s.count(), n0 + 2);
    s.key(Key::Delete);
    assert_eq!(s.count(), n0, "group + its duplicate removed");
    s.cmd_key(Key::Z);
    s.cmd_key(Key::Z);
    s.cmd_key(Key::Z);
    assert_eq!(s.count(), n0);
    assert!(!s.app().hist.can_undo());
}

#[test]
fn auto_layout_shortcut_wraps() {
    let mut s = Sim::with(Options { select: Some("Tile A".into()), ..Default::default() });
    s.mods = Modifiers::SHIFT;
    s.key(Key::A);
    s.mods = Modifiers::NONE;
    let id = s.app().selection[0];
    let n = s.app().doc.get(id).unwrap();
    assert!(n.kind.frame_props().and_then(|f| f.auto_layout).is_some());
}

#[test]
fn wheel_pans_and_pinch_zooms() {
    let mut s = Sim::new();
    let p = s.screen(600.0, 400.0);
    s.mv(p);
    let off0 = s.app().view.offset;
    s.frame(vec![Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: Vec2::new(0.0, -60.0), phase: egui::TouchPhase::Move, modifiers: Modifiers::NONE }]);
    s.frame(vec![]);
    assert!(s.app().view.offset.y > off0.y, "scrolling down moves the view down the page");
    let z0 = s.app().view.zoom;
    s.frame(vec![Event::Zoom(1.5)]);
    assert!((s.app().view.zoom / z0 - 1.5).abs() < 0.01);
    s.mods = Modifiers::SHIFT;
    s.key(Key::Num0);
    assert!((s.app().view.zoom - 1.0).abs() < 1e-4);
    s.key(Key::Num1);
    s.mods = Modifiers::NONE;
    assert!(s.app().view.zoom < 1.0);
}

#[test]
fn hand_tool_pans() {
    let mut s = Sim::new();
    s.key(Key::H);
    let off0 = s.app().view.offset;
    let a = s.screen(600.0, 400.0);
    s.drag(a, a + Vec2::new(100.0, 0.0));
    assert!(s.app().view.offset.x < off0.x - 50.0);
    assert_eq!(s.app().tool, Tool::Hand);
}

#[test]
fn layers_panel_click_and_visibility_toggle() {
    let mut s = Sim::new();
    // first row (top-most layer = "Dashboard") of the layers list
    s.click(Pos2::new(180.0, 241.0));
    let dash = s.by_name("Dashboard");
    assert_eq!(s.app().selection, vec![dash]);
    // hover reveals the eye button at the row's right edge
    s.mv(Pos2::new(180.0, 241.0));
    s.click(Pos2::new(297.0 - 8.0 - 12.0 - 1.0, 241.0));
    assert!(!s.app().doc.get(dash).unwrap().visible);
    s.cmd_key(Key::Z);
    assert!(s.app().doc.get(dash).unwrap().visible);
}

#[test]
fn layers_drag_reparents_into_frame() {
    let mut s = Sim::new();
    let welcome = s.by_name("Welcome");
    let card = s.by_name("Card");
    // rows: Dashboard (241), Card (273), Welcome (305). Drag Card onto the middle of Welcome.
    s.drag(Pos2::new(180.0, 273.0), Pos2::new(180.0, 305.0));
    assert_eq!(s.app().doc.get(card).unwrap().parent, Some(welcome));
}

#[test]
fn page_add_and_switch() {
    let mut s = Sim::new();
    let n = s.app().doc.pages.len();
    s.click(Pos2::new(277.0, 85.0));
    assert_eq!(s.app().doc.pages.len(), n + 1);
    assert_eq!(s.app().page, *s.app().doc.pages.last().unwrap());
    s.click(Pos2::new(150.0, 118.0));
    assert_eq!(s.app().page, s.app().doc.pages[0]);
}

#[test]
fn inspector_x_field_edits_position() {
    let mut s = Sim::with(Options { select: Some("Card".into()), ..Default::default() });
    let id = s.by_name("Card");
    s.click(Pos2::new(1410.0, 247.0));
    s.type_text("700");
    s.key(Key::Enter);
    assert_eq!(s.app().doc.get(id).unwrap().transform.tx, 700.0);
    s.cmd_key(Key::Z);
    assert_eq!(s.app().doc.get(id).unwrap().transform.tx, 560.0);
}

#[test]
fn rail_menu_opens_and_closes() {
    let mut s = Sim::new();
    s.click(Pos2::new(28.0, 24.0));
    assert_eq!(s.app().ui.menu, Some(MenuKind::Main));
    s.key(Key::Escape);
    assert_eq!(s.app().ui.menu, None);
}

#[test]
fn marquee_selects_multiple_and_nudges() {
    let mut s = Sim::new();
    let a = s.screen(560.0, 290.0);
    let b = s.screen(1250.0, 790.0);
    s.drag(a, b);
    assert!(!s.app().selection.is_empty());
    let id = s.app().selection[0];
    let x0 = s.app().doc.get(id).unwrap().transform.tx;
    s.key(Key::ArrowRight);
    assert_eq!(s.app().doc.get(id).unwrap().transform.tx, x0 + 1.0);
    s.mods = Modifiers::SHIFT;
    s.key(Key::ArrowRight);
    s.mods = Modifiers::NONE;
    assert_eq!(s.app().doc.get(id).unwrap().transform.tx, x0 + 11.0);
}

#[test]
fn alt_drag_duplicates() {
    let mut s = Sim::with(Options { select: Some("Hero".into()), ..Default::default() });
    let n0 = s.count();
    s.mods = Modifiers::ALT;
    let a = s.screen(300.0, 150.0);
    s.drag(a, a + Vec2::new(40.0, 40.0));
    s.mods = Modifiers::NONE;
    assert_eq!(s.count(), n0 + 1);
}

#[test]
fn dropped_into_frame_on_move() {
    let mut s = Sim::new();
    let tile = s.by_name("Tile A");
    let welcome = s.by_name("Welcome");
    // drag the tile (inside Dashboard) onto the Welcome frame
    let a = s.screen(560.0 + 24.0 + 90.0, 360.0 + 24.0 + 60.0);
    let b = s.screen(300.0, 400.0);
    s.drag(a, b);
    assert_eq!(s.app().doc.get(tile).unwrap().parent, Some(welcome));
}

#[test]
fn color_swatch_opens_picker_without_panic() {
    let mut s = Sim::with(Options { select: Some("Card".into()), ..Default::default() });
    s.click(Pos2::new(1384.0, 733.0));
    assert!(s.app().props.picker.is_some());
    s.frame(vec![]);
    s.key(Key::Escape);
    assert!(s.app().props.picker.is_none());
}
