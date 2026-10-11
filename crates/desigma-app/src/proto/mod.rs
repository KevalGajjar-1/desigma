//! Prototype mode: editing interactions (Prototype tab, canvas noodles) and
//! the player behind the Present view.

pub mod easing;
pub mod geom;
pub mod player;
pub mod smart;

use crate::app::App;
use desigma_doc::*;
use player::top_level_frames;

/// Editing state of the Prototype tab and canvas.
#[derive(Default)]
pub struct ProtoState {
    /// Interaction being edited in the popover: (layer, reaction index).
    pub editing: Option<(NodeId, usize)>,
    /// Screen rect of the row that opened the popover (clicks on it do not close it).
    pub editing_anchor: Option<egui::Rect>,
    /// Popover height measured last frame (the background is drawn before the content).
    pub popover_h: f32,
    /// Flow whose name is being edited in the list.
    pub scroll: f32,
    pub content_h: f32,
    /// Hotspot drag started on the canvas: (source layer, pointer position).
    pub connect: Option<(NodeId, egui::Pos2)>,
    /// Layer whose hotspot the pointer is over.
    pub hot: Option<NodeId>,
    /// Deepest layer under the pointer while in prototype mode.
    pub hover: Option<NodeId>,
    /// Device preset index into [`DEVICES`] (shown as a bezel when presenting).
    pub device: usize,
    pub present: Option<crate::present::PresentState>,
}

/// Device presets: name, width, height, corner radius.
pub const DEVICES: &[(&str, f32, f32, f32)] = &[
    ("None", 0.0, 0.0, 0.0),
    ("iPhone 15", 393.0, 852.0, 48.0),
    ("iPhone SE", 375.0, 667.0, 24.0),
    ("Pixel 8", 412.0, 915.0, 40.0),
    ("iPad mini", 744.0, 1133.0, 28.0),
    ("Desktop", 1440.0, 1024.0, 8.0),
];

// ------------------------------------------------------------------ labels

pub const TRIGGERS: &[&str] = &[
    "On click",
    "On drag",
    "While hovering",
    "While pressing",
    "Mouse enter",
    "Mouse leave",
    "Mouse down",
    "Mouse up",
    "After delay",
    "Key/Gamepad",
];

pub fn trigger_index(t: &Trigger) -> usize {
    match t {
        Trigger::OnClick => 0,
        Trigger::OnDrag => 1,
        Trigger::WhileHovering => 2,
        Trigger::WhilePressing => 3,
        Trigger::MouseEnter { .. } => 4,
        Trigger::MouseLeave { .. } => 5,
        Trigger::MouseDown { .. } => 6,
        Trigger::MouseUp { .. } => 7,
        Trigger::AfterDelay { .. } => 8,
        Trigger::KeyPress { .. } => 9,
    }
}

/// A fresh trigger of the i-th kind, keeping the delay of `prev` when both have one.
pub fn trigger_of_index(i: usize, prev: &Trigger) -> Trigger {
    let d = trigger_delay(prev).unwrap_or(match i {
        8 => 800,
        _ => 0,
    });
    match i {
        0 => Trigger::OnClick,
        1 => Trigger::OnDrag,
        2 => Trigger::WhileHovering,
        3 => Trigger::WhilePressing,
        4 => Trigger::MouseEnter { delay_ms: d },
        5 => Trigger::MouseLeave { delay_ms: d },
        6 => Trigger::MouseDown { delay_ms: d },
        7 => Trigger::MouseUp { delay_ms: d },
        8 => Trigger::AfterDelay { delay_ms: d },
        _ => Trigger::KeyPress { keys: if let Trigger::KeyPress { keys } = prev { keys.clone() } else { "Space".into() } },
    }
}

pub fn trigger_delay(t: &Trigger) -> Option<u32> {
    match t {
        Trigger::MouseEnter { delay_ms } | Trigger::MouseLeave { delay_ms } | Trigger::MouseDown { delay_ms } | Trigger::MouseUp { delay_ms } | Trigger::AfterDelay { delay_ms } => Some(*delay_ms),
        _ => None,
    }
}

pub fn with_delay(t: &Trigger, ms: u32) -> Trigger {
    match t {
        Trigger::MouseEnter { .. } => Trigger::MouseEnter { delay_ms: ms },
        Trigger::MouseLeave { .. } => Trigger::MouseLeave { delay_ms: ms },
        Trigger::MouseDown { .. } => Trigger::MouseDown { delay_ms: ms },
        Trigger::MouseUp { .. } => Trigger::MouseUp { delay_ms: ms },
        Trigger::AfterDelay { .. } => Trigger::AfterDelay { delay_ms: ms },
        other => other.clone(),
    }
}

pub fn trigger_label(t: &Trigger) -> String {
    match t {
        Trigger::KeyPress { keys } if !keys.is_empty() => format!("Key {keys}"),
        Trigger::AfterDelay { delay_ms } => format!("After delay {delay_ms}ms"),
        other => TRIGGERS[trigger_index(other)].to_string(),
    }
}

pub const ACTIONS: &[&str] = &["Navigate to", "Back", "Open overlay", "Swap overlay", "Close overlay", "Scroll to", "Open link", "Change to"];

pub fn action_index(a: &Action) -> usize {
    match a {
        Action::Navigate { .. } => 0,
        Action::Back => 1,
        Action::OpenOverlay { .. } => 2,
        Action::SwapOverlay { .. } => 3,
        Action::CloseOverlay => 4,
        Action::ScrollTo { .. } => 5,
        Action::OpenUrl { .. } => 6,
        Action::ChangeTo { .. } => 7,
    }
}

pub fn action_dest(a: &Action) -> Option<NodeId> {
    match a {
        Action::Navigate { dest, .. } | Action::OpenOverlay { dest, .. } | Action::SwapOverlay { dest } | Action::ScrollTo { dest, .. } | Action::ChangeTo { dest } => Some(*dest),
        _ => None,
    }
}

/// A new action of the i-th kind, keeping the destination of `prev` when it has one.
pub fn action_of_index(i: usize, prev: &Action, fallback_dest: NodeId) -> Action {
    let dest = action_dest(prev).unwrap_or(fallback_dest);
    match i {
        0 => Action::Navigate { dest, preserve_scroll: false },
        1 => Action::Back,
        2 => Action::OpenOverlay { dest, position: OverlayPosition::Center, offset: Vec2::ZERO, close_on_click_outside: true, background: None },
        3 => Action::SwapOverlay { dest },
        4 => Action::CloseOverlay,
        5 => Action::ScrollTo { dest, offset: Vec2::ZERO },
        6 => Action::OpenUrl { url: "https://".into(), new_tab: true },
        _ => Action::ChangeTo { dest },
    }
}

/// Whether the action plays an animation (so the editor shows the transition).
pub fn action_animates(a: &Action) -> bool {
    !matches!(a, Action::Back | Action::CloseOverlay | Action::OpenUrl { .. })
}

pub fn action_label(doc: &Document, a: &Action) -> String {
    let name = |id: &NodeId| doc.get(*id).map(|n| n.name.clone()).unwrap_or_else(|| "(missing)".into());
    match a {
        Action::Navigate { dest, .. } => format!("Navigate to {}", name(dest)),
        Action::Back => "Back".into(),
        Action::OpenOverlay { dest, .. } => format!("Open overlay {}", name(dest)),
        Action::SwapOverlay { dest } => format!("Swap overlay {}", name(dest)),
        Action::CloseOverlay => "Close overlay".into(),
        Action::ScrollTo { dest, .. } => format!("Scroll to {}", name(dest)),
        Action::OpenUrl { url, .. } => format!("Open link {url}"),
        Action::ChangeTo { dest } => format!("Change to {}", name(dest)),
    }
}

pub const ANIMATIONS: &[&str] = &["Instant", "Dissolve", "Smart animate", "Move in", "Move out", "Push", "Slide in", "Slide out"];

pub fn animation_index(t: &Option<Transition>) -> usize {
    match t.map(|t| t.kind) {
        None => 0,
        Some(TransitionKind::Dissolve) => 1,
        Some(TransitionKind::SmartAnimate) => 2,
        Some(TransitionKind::MoveIn(_)) => 3,
        Some(TransitionKind::MoveOut(_)) => 4,
        Some(TransitionKind::Push(_)) => 5,
        Some(TransitionKind::SlideIn(_)) => 6,
        Some(TransitionKind::SlideOut(_)) => 7,
    }
}

pub fn transition_direction(k: TransitionKind) -> Option<Direction> {
    match k {
        TransitionKind::MoveIn(d) | TransitionKind::MoveOut(d) | TransitionKind::Push(d) | TransitionKind::SlideIn(d) | TransitionKind::SlideOut(d) => Some(d),
        _ => None,
    }
}

pub fn with_direction(k: TransitionKind, d: Direction) -> TransitionKind {
    match k {
        TransitionKind::MoveIn(_) => TransitionKind::MoveIn(d),
        TransitionKind::MoveOut(_) => TransitionKind::MoveOut(d),
        TransitionKind::Push(_) => TransitionKind::Push(d),
        TransitionKind::SlideIn(_) => TransitionKind::SlideIn(d),
        TransitionKind::SlideOut(_) => TransitionKind::SlideOut(d),
        other => other,
    }
}

/// The transition for the i-th animation entry, keeping easing, duration and direction of `prev`.
pub fn transition_of_index(i: usize, prev: &Option<Transition>) -> Option<Transition> {
    let dir = prev.and_then(|t| transition_direction(t.kind)).unwrap_or(Direction::Left);
    let kind = match i {
        0 => return None,
        1 => TransitionKind::Dissolve,
        2 => TransitionKind::SmartAnimate,
        3 => TransitionKind::MoveIn(dir),
        4 => TransitionKind::MoveOut(dir),
        5 => TransitionKind::Push(dir),
        6 => TransitionKind::SlideIn(dir),
        _ => TransitionKind::SlideOut(dir),
    };
    Some(prev.map(|t| Transition { kind, ..t }).unwrap_or(Transition { kind, easing: Easing::EaseOut, duration_ms: 300 }))
}

pub fn direction_name(d: Direction) -> &'static str {
    match d {
        Direction::Left => "Left",
        Direction::Right => "Right",
        Direction::Top => "Top",
        Direction::Bottom => "Bottom",
    }
}

pub fn position_name(p: OverlayPosition) -> &'static str {
    match p {
        OverlayPosition::Center => "Center",
        OverlayPosition::TopLeft => "Top left",
        OverlayPosition::TopCenter => "Top center",
        OverlayPosition::TopRight => "Top right",
        OverlayPosition::BottomLeft => "Bottom left",
        OverlayPosition::BottomCenter => "Bottom center",
        OverlayPosition::BottomRight => "Bottom right",
        OverlayPosition::Manual => "Manual",
    }
}

pub const OVERFLOWS: &[(&str, Overflow)] = &[("No scrolling", Overflow::None), ("Horizontal scrolling", Overflow::Horizontal), ("Vertical scrolling", Overflow::Vertical), ("Horizontal & vertical", Overflow::Both)];

// ------------------------------------------------------------------ queries

/// Top-level frame (child of its page) that contains `id`, or `id` itself.
pub fn top_frame(doc: &Document, mut id: NodeId) -> Option<NodeId> {
    loop {
        let p = doc.get(id)?.parent?;
        if matches!(doc.get(p)?.kind, NodeKind::Page { .. }) {
            return Some(id);
        }
        id = p;
    }
}

/// Frames a reaction on `src` may target: the top-level frames of its page.
pub fn destination_frames(doc: &Document, src: NodeId) -> Vec<NodeId> {
    let Some(page) = doc.page_of(src) else { return Vec::new() };
    top_level_frames(doc, page)
}

/// (target frame, is overlay) pairs of every connection `id` draws a noodle to.
pub fn connections(doc: &Document, id: NodeId) -> Vec<NodeId> {
    let Some(n) = doc.get(id) else { return Vec::new() };
    let mut out = Vec::new();
    for r in &n.prototype.reactions {
        for a in &r.actions {
            if let Action::Navigate { dest, .. } | Action::OpenOverlay { dest, .. } | Action::SwapOverlay { dest } = a {
                if let Some(f) = top_frame(doc, *dest) {
                    if doc.page_of(f) == doc.page_of(id) && !out.contains(&f) {
                        out.push(f);
                    }
                }
            }
        }
    }
    out
}

pub fn has_interactions(doc: &Document, id: NodeId) -> bool {
    doc.get(id).is_some_and(|n| !n.prototype.reactions.is_empty())
}

/// Next flow name: "Flow N" with N one above the highest existing "Flow N".
pub fn next_flow_name(flows: &[Flow]) -> String {
    let max = flows.iter().filter_map(|f| f.name.strip_prefix("Flow ").and_then(|n| n.trim().parse::<u32>().ok())).max().unwrap_or(0);
    format!("Flow {}", max.max(flows.len() as u32) + 1)
}

// ------------------------------------------------------------------- edits

impl App {
    /// Edits the prototype settings of `ids` as one undo step.
    pub fn proto_edit(&mut self, ids: &[NodeId], mut f: impl FnMut(&mut PrototypeProps)) {
        self.edit_nodes(ids, |n| f(&mut n.prototype));
    }

    /// Replaces the flow list as one undo step.
    pub fn proto_set_flows(&mut self, flows: Vec<Flow>) {
        if flows == self.doc.flows {
            return;
        }
        if let Err(e) = self.hist.exec(&mut self.doc, Command::SetFlows { flows }) {
            self.show_toast(format!("Edit failed: {e}"), true);
        }
        self.changed();
    }

    pub fn proto_add_flow(&mut self, start: NodeId) {
        let Some(page) = self.doc.page_of(start) else { return };
        let mut flows = self.doc.flows.clone();
        if flows.iter().any(|f| f.start == start) {
            return;
        }
        let name = next_flow_name(&flows);
        flows.push(Flow { name, page, start, description: String::new() });
        self.proto_set_flows(flows);
    }

    /// Default reaction for the + button: On click -> navigate to the next frame.
    pub fn proto_default_reaction(&self, src: NodeId) -> Reaction {
        let frames = destination_frames(&self.doc, src);
        let own = top_frame(&self.doc, src);
        let dest = own
            .and_then(|o| frames.iter().position(|f| *f == o))
            .map(|i| frames[(i + 1) % frames.len()])
            .or_else(|| frames.first().copied())
            .unwrap_or(src);
        Reaction {
            trigger: Trigger::OnClick,
            actions: vec![Action::Navigate { dest, preserve_scroll: false }],
            transition: Some(Transition { kind: TransitionKind::Dissolve, easing: Easing::EaseOut, duration_ms: 300 }),
        }
    }

    /// Adds a reaction to `src` and opens it in the editor.
    pub fn proto_add_reaction(&mut self, src: NodeId) {
        let r = self.proto_default_reaction(src);
        let mut idx = 0;
        self.proto_edit(&[src], |p| {
            p.reactions.push(r.clone());
            idx = p.reactions.len() - 1;
        });
        self.proto.editing = Some((src, idx));
    }

    /// Drag-from-hotspot: On click -> Navigate to `dest_frame` (instant, Figma's default).
    pub fn proto_connect(&mut self, src: NodeId, dest_frame: NodeId) {
        let r = Reaction { trigger: Trigger::OnClick, actions: vec![Action::Navigate { dest: dest_frame, preserve_scroll: false }], transition: None };
        self.proto_edit(&[src], |p| p.reactions.push(r.clone()));
    }

    /// Opens the Present view from the selected frame, else the page's first flow, else the first frame.
    pub fn start_present(&mut self) {
        let start = self
            .selection
            .first()
            .and_then(|s| top_frame(&self.doc, *s))
            .or_else(|| self.doc.flows.iter().find(|f| f.page == self.page).map(|f| f.start))
            .or_else(|| top_level_frames(&self.doc, self.page).first().copied());
        match start {
            Some(s) => self.proto.present = Some(crate::present::PresentState::new(&self.doc, s)),
            None => self.show_toast("Nothing to present: add a frame first", true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc3() -> (Document, NodeId, NodeId, NodeId, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let mut f = Vec::new();
        for i in 0..3 {
            let id = d.alloc_id();
            f.push(d.insert(page, Node::frame(id, Rect::new(i as f32 * 500.0, 0.0, 400.0, 800.0))).unwrap());
        }
        let id = d.alloc_id();
        let b = d.insert(f[0], Node::rectangle(id, Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        (d, f[0], f[1], f[2], b)
    }

    #[test]
    fn trigger_round_trips_through_indexes() {
        for i in 0..TRIGGERS.len() {
            let t = trigger_of_index(i, &Trigger::OnClick);
            assert_eq!(trigger_index(&t), i);
        }
        let t = trigger_of_index(4, &Trigger::AfterDelay { delay_ms: 250 });
        assert_eq!(t, Trigger::MouseEnter { delay_ms: 250 }, "delay carries over");
        assert_eq!(trigger_of_index(8, &Trigger::OnClick), Trigger::AfterDelay { delay_ms: 800 });
        assert_eq!(with_delay(&t, 90), Trigger::MouseEnter { delay_ms: 90 });
        assert_eq!(with_delay(&Trigger::OnClick, 90), Trigger::OnClick);
    }

    #[test]
    fn action_round_trips_and_keeps_destination() {
        let prev = Action::Navigate { dest: NodeId(7), preserve_scroll: true };
        for i in 0..ACTIONS.len() {
            assert_eq!(action_index(&action_of_index(i, &prev, NodeId(1))), i);
        }
        assert_eq!(action_dest(&action_of_index(2, &prev, NodeId(1))), Some(NodeId(7)));
        assert_eq!(action_dest(&action_of_index(0, &Action::Back, NodeId(1))), Some(NodeId(1)));
        assert!(!action_animates(&Action::Back) && action_animates(&prev));
    }

    #[test]
    fn animation_picker_keeps_duration_and_direction() {
        let prev = Some(Transition { kind: TransitionKind::Push(Direction::Top), easing: Easing::Linear, duration_ms: 777 });
        let t = transition_of_index(3, &prev).unwrap();
        assert_eq!(t.kind, TransitionKind::MoveIn(Direction::Top));
        assert_eq!((t.duration_ms, t.easing), (777, Easing::Linear));
        assert_eq!(transition_of_index(0, &prev), None);
        let fresh = transition_of_index(1, &None).unwrap();
        assert_eq!((fresh.kind, fresh.duration_ms), (TransitionKind::Dissolve, 300));
        for i in 0..ANIMATIONS.len() {
            assert_eq!(animation_index(&transition_of_index(i, &None)), i);
        }
        assert_eq!(with_direction(TransitionKind::Dissolve, Direction::Left), TransitionKind::Dissolve);
        assert_eq!(transition_direction(TransitionKind::SlideOut(Direction::Right)), Some(Direction::Right));
    }

    #[test]
    fn labels_name_destinations() {
        let (d, a, b, _c, _) = doc3();
        assert_eq!(action_label(&d, &Action::Navigate { dest: b, preserve_scroll: false }), "Navigate to Frame");
        assert_eq!(action_label(&d, &Action::Navigate { dest: NodeId(99), preserve_scroll: false }), "Navigate to (missing)");
        assert_eq!(trigger_label(&Trigger::AfterDelay { delay_ms: 5 }), "After delay 5ms");
        assert_eq!(trigger_label(&Trigger::KeyPress { keys: "K".into() }), "Key K");
        assert_eq!(trigger_label(&Trigger::WhileHovering), "While hovering");
        let _ = a;
    }

    #[test]
    fn connections_lists_destination_frames_once() {
        let (mut d, _a, b, c, btn) = doc3();
        let r = |dest| Reaction { trigger: Trigger::OnClick, actions: vec![Action::Navigate { dest, preserve_scroll: false }], transition: None };
        d.get_mut(btn).unwrap().prototype.reactions = vec![r(b), r(b), r(c), Reaction { trigger: Trigger::OnClick, actions: vec![Action::Back], transition: None }];
        assert_eq!(connections(&d, btn), vec![b, c]);
        assert!(has_interactions(&d, btn));
        assert!(connections(&d, b).is_empty());
    }

    #[test]
    fn flow_names_count_up() {
        let mk = |n: &str| Flow { name: n.into(), page: NodeId(1), start: NodeId(2), description: String::new() };
        assert_eq!(next_flow_name(&[]), "Flow 1");
        assert_eq!(next_flow_name(&[mk("Flow 1"), mk("Flow 4")]), "Flow 5");
        assert_eq!(next_flow_name(&[mk("Onboarding")]), "Flow 2");
    }

    #[test]
    fn default_reaction_targets_the_next_frame() {
        let (d, _a, b, _c, btn) = doc3();
        let app = App::new(d);
        let r = app.proto_default_reaction(btn);
        assert_eq!(r.trigger, Trigger::OnClick);
        assert_eq!(r.actions, vec![Action::Navigate { dest: b, preserve_scroll: false }]);
    }

    #[test]
    fn edits_are_single_undo_steps() {
        let (d, _a, b, c, btn) = doc3();
        let mut app = App::new(d);
        app.proto_add_reaction(btn);
        assert_eq!(app.doc.get(btn).unwrap().prototype.reactions.len(), 1);
        assert_eq!(app.proto.editing, Some((btn, 0)));
        app.proto_edit(&[btn], |p| p.reactions[0].actions = vec![Action::Navigate { dest: c, preserve_scroll: false }]);
        app.proto_connect(btn, b);
        assert_eq!(app.doc.get(btn).unwrap().prototype.reactions.len(), 2);
        app.undo();
        assert_eq!(app.doc.get(btn).unwrap().prototype.reactions.len(), 1);
        app.undo();
        assert!(matches!(app.doc.get(btn).unwrap().prototype.reactions[0].actions[0], Action::Navigate { dest, .. } if dest == b));
        app.undo();
        assert!(app.doc.get(btn).unwrap().prototype.reactions.is_empty());
        app.redo();
        assert_eq!(app.doc.get(btn).unwrap().prototype.reactions.len(), 1);
    }

    #[test]
    fn flows_are_undoable() {
        let (d, a, b, _c, _) = doc3();
        let mut app = App::new(d);
        app.proto_add_flow(a);
        app.proto_add_flow(a);
        assert_eq!(app.doc.flows.len(), 1, "a frame starts at most one flow");
        app.proto_add_flow(b);
        assert_eq!(app.doc.flows.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Flow 1", "Flow 2"]);
        app.undo();
        assert_eq!(app.doc.flows.len(), 1);
        app.undo();
        assert!(app.doc.flows.is_empty());
        app.redo();
        assert_eq!(app.doc.flows[0].start, a);
    }
}
