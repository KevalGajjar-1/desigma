//! Prototype interactions: Figma REST `reactions` / `flowStartingPoints` /
//! `overflowDirection` / `scrollBehavior` <-> `desigma_doc` prototype model.
//!
//! Importing is two-phase: [`collect`] copies the small prototype-related
//! part of a node's JSON while the tree is built (destinations may not exist
//! yet), and [`resolve`] maps source ids to [`NodeId`]s at the end.
//! `.fig` files store the same data under `prototypeInteractions`;
//! [`convert_fig`] rewrites it into the REST shape (tolerantly) so a single
//! importer handles both.

use crate::kiwi::clean_f32;
use desigma_doc::*;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}
fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}
fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}

// ----------------------------------------------------------------- key codes

const KEY_NAMES: &[(u32, &str)] = &[
    (8, "Backspace"),
    (9, "Tab"),
    (13, "Enter"),
    (16, "Shift"),
    (17, "Ctrl"),
    (18, "Alt"),
    (27, "Escape"),
    (32, "Space"),
    (37, "ArrowLeft"),
    (38, "ArrowUp"),
    (39, "ArrowRight"),
    (40, "ArrowDown"),
    (91, "Cmd"),
];

/// Human readable name of a DOM key code ("A", "7", "Space", "#112" otherwise).
pub fn key_name(code: u32) -> String {
    if let Some((_, n)) = KEY_NAMES.iter().find(|(c, _)| *c == code) {
        return (*n).into();
    }
    match code {
        48..=57 | 65..=90 => char::from_u32(code).map(String::from).unwrap_or_default(),
        _ => format!("#{code}"),
    }
}

/// Inverse of [`key_name`].
pub fn key_code(name: &str) -> Option<u32> {
    if let Some((c, _)) = KEY_NAMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)) {
        return Some(*c);
    }
    if let Some(n) = name.strip_prefix('#') {
        return n.parse().ok();
    }
    let mut it = name.chars();
    match (it.next(), it.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase() as u32),
        _ => None,
    }
}

// ------------------------------------------------------------------- import

/// Subset of a node's JSON that carries prototype data (None if there is none).
pub(crate) fn collect(v: &Value) -> Option<Value> {
    let mut m = Map::new();
    for k in ["reactions", "transitionNodeID", "transitionDuration", "transitionEasing"] {
        if let Some(x) = v.get(k).filter(|x| !x.is_null()) {
            m.insert(k.into(), x.clone());
        }
    }
    (!m.is_empty()).then_some(Value::Object(m))
}

/// Overlay presentation lives on the destination node in REST.
pub(crate) fn collect_overlay(v: &Value) -> Option<Value> {
    let mut m = Map::new();
    for k in ["overlayPositionType", "overlayBackground", "overlayBackgroundInteraction"] {
        if let Some(x) = v.get(k).filter(|x| !x.is_null()) {
            m.insert(k.into(), x.clone());
        }
    }
    (!m.is_empty()).then_some(Value::Object(m))
}

/// Scroll-related node fields, applied straight to the node while it is built.
pub(crate) fn apply_scroll(n: &mut Node, v: &Value) {
    n.prototype.overflow = match s(v, "overflowDirection") {
        Some("HORIZONTAL_SCROLLING") => Overflow::Horizontal,
        Some("VERTICAL_SCROLLING") => Overflow::Vertical,
        Some("HORIZONTAL_AND_VERTICAL_SCROLLING") => Overflow::Both,
        _ => Overflow::None,
    };
}

/// Whether a child stays put while its parent scrolls.
pub(crate) fn is_fixed(v: &Value) -> bool {
    matches!(s(v, "scrollBehavior"), Some("FIXED" | "STICKY_SCROLLS")) || b(v, "isFixed") == Some(true)
}

fn trigger(t: &Value) -> Option<Trigger> {
    let delay = || f(t, "delay").or_else(|| f(t, "timeoutMs")).unwrap_or(0.0).max(0.0) as u32;
    Some(match s(t, "type")? {
        "ON_CLICK" => Trigger::OnClick,
        "ON_HOVER" => Trigger::WhileHovering,
        "ON_PRESS" => Trigger::WhilePressing,
        "ON_DRAG" => Trigger::OnDrag,
        "AFTER_TIMEOUT" => Trigger::AfterDelay { delay_ms: f(t, "timeout").or_else(|| f(t, "timeoutMs")).unwrap_or(0.0).max(0.0) as u32 },
        "MOUSE_ENTER" => Trigger::MouseEnter { delay_ms: delay() },
        "MOUSE_LEAVE" => Trigger::MouseLeave { delay_ms: delay() },
        "MOUSE_DOWN" => Trigger::MouseDown { delay_ms: delay() },
        "MOUSE_UP" => Trigger::MouseUp { delay_ms: delay() },
        "ON_KEY_DOWN" => {
            let mut keys: Vec<String> = t.get("keyCodes").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|c| key_name(c as u32)).collect()).unwrap_or_default();
            if keys.is_empty() {
                if let Some(k) = s(t, "keys") {
                    keys.push(k.into());
                }
            }
            Trigger::KeyPress { keys: keys.join("+") }
        }
        _ => return None,
    })
}

fn direction(t: &Value) -> Direction {
    match s(t, "direction") {
        Some("RIGHT") => Direction::Right,
        Some("TOP") => Direction::Top,
        Some("BOTTOM") => Direction::Bottom,
        _ => Direction::Left,
    }
}

fn easing(e: Option<&Value>) -> Easing {
    let Some(e) = e else { return Easing::EaseOut };
    let spring = |mass, stiffness, damping| Easing::Spring { mass, stiffness, damping };
    match s(e, "type").unwrap_or("EASE_OUT") {
        "LINEAR" => Easing::Linear,
        "EASE_IN" => Easing::EaseIn,
        "EASE_IN_AND_OUT" | "EASE_IN_OUT" => Easing::EaseInOut,
        "EASE_IN_BACK" => Easing::EaseInBack,
        "EASE_OUT_BACK" => Easing::EaseOutBack,
        "EASE_IN_AND_OUT_BACK" | "EASE_IN_OUT_BACK" => Easing::EaseInOutBack,
        "CUSTOM_CUBIC_BEZIER" => {
            let c = e.get("easingFunctionCubicBezier").unwrap_or(&Value::Null);
            Easing::Bezier([f(c, "x1").unwrap_or(0.0) as f32, f(c, "y1").unwrap_or(0.0) as f32, f(c, "x2").unwrap_or(1.0) as f32, f(c, "y2").unwrap_or(1.0) as f32])
        }
        "CUSTOM_SPRING" => {
            let c = e.get("easingFunctionSpring").unwrap_or(&Value::Null);
            spring(f(c, "mass").unwrap_or(1.0) as f32, f(c, "stiffness").unwrap_or(100.0) as f32, f(c, "damping").unwrap_or(15.0) as f32)
        }
        "GENTLE" => spring(1.0, 100.0, 15.0),
        "QUICK" => spring(1.0, 300.0, 20.0),
        "BOUNCY" => spring(1.0, 600.0, 15.0),
        "SLOW" => spring(1.0, 80.0, 20.0),
        _ => Easing::EaseOut,
    }
}

fn transition(t: &Value) -> Option<Transition> {
    let dir = direction(t);
    let kind = match s(t, "type")? {
        "DISSOLVE" => TransitionKind::Dissolve,
        "SMART_ANIMATE" | "SCROLL_ANIMATE" => TransitionKind::SmartAnimate,
        "MOVE_IN" => TransitionKind::MoveIn(dir),
        "MOVE_OUT" => TransitionKind::MoveOut(dir),
        "PUSH" => TransitionKind::Push(dir),
        "SLIDE_IN" => TransitionKind::SlideIn(dir),
        "SLIDE_OUT" => TransitionKind::SlideOut(dir),
        _ => return None,
    };
    Some(Transition { kind, easing: easing(t.get("easing")), duration_ms: f(t, "duration").unwrap_or(300.0).clamp(0.0, 600_000.0) as u32 })
}

fn overlay_position(name: Option<&str>) -> OverlayPosition {
    match name {
        Some("TOP_LEFT") => OverlayPosition::TopLeft,
        Some("TOP_CENTER") => OverlayPosition::TopCenter,
        Some("TOP_RIGHT") => OverlayPosition::TopRight,
        Some("BOTTOM_LEFT") => OverlayPosition::BottomLeft,
        Some("BOTTOM_CENTER") => OverlayPosition::BottomCenter,
        Some("BOTTOM_RIGHT") => OverlayPosition::BottomRight,
        Some("MANUAL") => OverlayPosition::Manual,
        _ => OverlayPosition::Center,
    }
}

/// Maps source ids to ids, looks up destination presentation.
pub(crate) struct Resolver<'a> {
    pub ids: &'a BTreeMap<String, NodeId>,
    pub overlay: &'a BTreeMap<String, Value>,
}

impl Resolver<'_> {
    fn dest(&self, a: &Value) -> Option<(NodeId, String)> {
        let sid = s(a, "destinationId").or_else(|| s(a, "transitionNodeID"))?;
        self.ids.get(sid).map(|id| (*id, sid.to_string()))
    }

    fn action(&self, a: &Value) -> Option<Action> {
        match s(a, "type")? {
            "NODE" => {
                let (dest, sid) = self.dest(a)?;
                Some(match s(a, "navigation").unwrap_or("NAVIGATE") {
                    "NAVIGATE" => Action::Navigate { dest, preserve_scroll: b(a, "preserveScrollPosition").unwrap_or(false) },
                    "SWAP" => Action::SwapOverlay { dest },
                    "OVERLAY" => {
                        let o = self.overlay.get(&sid).unwrap_or(&Value::Null);
                        let offset = a.get("overlayRelativePosition").map(|p| Vec2::new(f(p, "x").unwrap_or(0.0) as f32, f(p, "y").unwrap_or(0.0) as f32)).unwrap_or(Vec2::ZERO);
                        let background = o
                            .get("overlayBackground")
                            .filter(|bg| s(bg, "type") == Some("COLOR"))
                            .and_then(|bg| bg.get("color"))
                            .and_then(crate::rest_import::color);
                        Action::OpenOverlay {
                            dest,
                            position: overlay_position(s(o, "overlayPositionType")),
                            offset,
                            close_on_click_outside: s(o, "overlayBackgroundInteraction") == Some("CLOSE_ON_CLICK_OUTSIDE"),
                            background,
                        }
                    }
                    "SCROLL_TO" => Action::ScrollTo { dest, offset: a.get("scrollOffset").map(|p| Vec2::new(f(p, "x").unwrap_or(0.0) as f32, f(p, "y").unwrap_or(0.0) as f32)).unwrap_or(Vec2::ZERO) },
                    "CHANGE_TO" => Action::ChangeTo { dest },
                    _ => return None,
                })
            }
            "BACK" => Some(Action::Back),
            "CLOSE" => Some(Action::CloseOverlay),
            "URL" => Some(Action::OpenUrl { url: s(a, "url")?.to_string(), new_tab: b(a, "openInNewTab").unwrap_or(true) }),
            _ => None,
        }
    }

    fn reaction(&self, r: &Value) -> Option<Reaction> {
        let trig = trigger(r.get("trigger")?)?;
        let list: Vec<&Value> = match r.get("actions").and_then(Value::as_array) {
            Some(a) => a.iter().collect(),
            None => r.get("action").into_iter().collect(),
        };
        let mut actions = Vec::new();
        let mut tr = None;
        for a in list {
            let Some(act) = self.action(a) else { continue };
            if tr.is_none() {
                tr = a.get("transition").and_then(transition);
            }
            actions.push(act);
        }
        (!actions.is_empty()).then_some(Reaction { trigger: trig, actions, transition: tr })
    }

    /// Reactions from a collected node subset, including the legacy
    /// `transitionNodeID` form.
    pub(crate) fn reactions(&self, raw: &Value) -> Vec<Reaction> {
        let mut out: Vec<Reaction> = raw.get("reactions").and_then(Value::as_array).map(|a| a.iter().filter_map(|r| self.reaction(r)).collect()).unwrap_or_default();
        if out.is_empty() {
            if let Some((dest, _)) = self.dest(raw) {
                let dur = f(raw, "transitionDuration").unwrap_or(0.0).max(0.0) as u32;
                let e = raw.get("transitionEasing").map(|e| json!({"type": e}));
                out.push(Reaction {
                    trigger: Trigger::OnClick,
                    actions: vec![Action::Navigate { dest, preserve_scroll: false }],
                    transition: (dur > 0).then(|| Transition { kind: TransitionKind::Dissolve, easing: easing(e.as_ref()), duration_ms: dur }),
                });
            }
        }
        out
    }

    /// `flowStartingPoints` of a canvas.
    pub(crate) fn flows(&self, page: NodeId, canvas: &Value) -> Vec<Flow> {
        canvas
            .get("flowStartingPoints")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let start = *self.ids.get(s(p, "nodeId")?)?;
                        Some(Flow { name: s(p, "name").unwrap_or("Flow").to_string(), page, start, description: s(p, "description").unwrap_or("").to_string() })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ------------------------------------------------------------------- export

fn num(x: f32) -> Value {
    clean_f32(x)
}

fn trigger_json(t: &Trigger) -> Value {
    match t {
        Trigger::OnClick => json!({"type": "ON_CLICK"}),
        Trigger::OnDrag => json!({"type": "ON_DRAG"}),
        Trigger::WhileHovering => json!({"type": "ON_HOVER"}),
        Trigger::WhilePressing => json!({"type": "ON_PRESS"}),
        Trigger::MouseEnter { delay_ms } => json!({"type": "MOUSE_ENTER", "delay": delay_ms}),
        Trigger::MouseLeave { delay_ms } => json!({"type": "MOUSE_LEAVE", "delay": delay_ms}),
        Trigger::MouseDown { delay_ms } => json!({"type": "MOUSE_DOWN", "delay": delay_ms}),
        Trigger::MouseUp { delay_ms } => json!({"type": "MOUSE_UP", "delay": delay_ms}),
        Trigger::AfterDelay { delay_ms } => json!({"type": "AFTER_TIMEOUT", "timeout": delay_ms}),
        Trigger::KeyPress { keys } => {
            let codes: Vec<u32> = keys.split('+').filter_map(|k| key_code(k.trim())).collect();
            json!({"type": "ON_KEY_DOWN", "device": "KEYBOARD", "keyCodes": codes})
        }
    }
}

fn dir_name(d: Direction) -> &'static str {
    match d {
        Direction::Left => "LEFT",
        Direction::Right => "RIGHT",
        Direction::Top => "TOP",
        Direction::Bottom => "BOTTOM",
    }
}

fn easing_json(e: &Easing) -> Value {
    match e {
        Easing::Linear => json!({"type": "LINEAR"}),
        Easing::EaseIn => json!({"type": "EASE_IN"}),
        Easing::EaseOut => json!({"type": "EASE_OUT"}),
        Easing::EaseInOut => json!({"type": "EASE_IN_AND_OUT"}),
        Easing::EaseInBack => json!({"type": "EASE_IN_BACK"}),
        Easing::EaseOutBack => json!({"type": "EASE_OUT_BACK"}),
        Easing::EaseInOutBack => json!({"type": "EASE_IN_AND_OUT_BACK"}),
        Easing::Bezier(c) => json!({"type": "CUSTOM_CUBIC_BEZIER", "easingFunctionCubicBezier": {"x1": num(c[0]), "y1": num(c[1]), "x2": num(c[2]), "y2": num(c[3])}}),
        Easing::Spring { mass, stiffness, damping } => {
            json!({"type": "CUSTOM_SPRING", "easingFunctionSpring": {"mass": num(*mass), "stiffness": num(*stiffness), "damping": num(*damping)}})
        }
    }
}

fn transition_json(t: &Transition) -> Value {
    let (ty, dir) = match t.kind {
        TransitionKind::Dissolve => ("DISSOLVE", None),
        TransitionKind::SmartAnimate => ("SMART_ANIMATE", None),
        TransitionKind::MoveIn(d) => ("MOVE_IN", Some(d)),
        TransitionKind::MoveOut(d) => ("MOVE_OUT", Some(d)),
        TransitionKind::Push(d) => ("PUSH", Some(d)),
        TransitionKind::SlideIn(d) => ("SLIDE_IN", Some(d)),
        TransitionKind::SlideOut(d) => ("SLIDE_OUT", Some(d)),
    };
    let mut m = Map::new();
    m.insert("type".into(), ty.into());
    if let Some(d) = dir {
        m.insert("direction".into(), dir_name(d).into());
        m.insert("matchLayers".into(), false.into());
    }
    m.insert("duration".into(), t.duration_ms.into());
    m.insert("easing".into(), easing_json(&t.easing));
    Value::Object(m)
}

fn pos_name(p: OverlayPosition) -> &'static str {
    match p {
        OverlayPosition::Center => "CENTER",
        OverlayPosition::TopLeft => "TOP_LEFT",
        OverlayPosition::TopCenter => "TOP_CENTER",
        OverlayPosition::TopRight => "TOP_RIGHT",
        OverlayPosition::BottomLeft => "BOTTOM_LEFT",
        OverlayPosition::BottomCenter => "BOTTOM_CENTER",
        OverlayPosition::BottomRight => "BOTTOM_RIGHT",
        OverlayPosition::Manual => "MANUAL",
    }
}

fn action_json(doc: &Document, a: &Action, tr: Option<&Transition>) -> Value {
    let id = |n: &NodeId| super::export::figma_id(doc, *n);
    let mut m = Map::new();
    let node = |m: &mut Map<String, Value>, nav: &str, dest: &NodeId| {
        m.insert("type".into(), "NODE".into());
        m.insert("destinationId".into(), id(dest).into());
        m.insert("navigation".into(), nav.into());
        if let Some(t) = tr {
            m.insert("transition".into(), transition_json(t));
        }
    };
    match a {
        Action::Navigate { dest, preserve_scroll } => {
            node(&mut m, "NAVIGATE", dest);
            m.insert("preserveScrollPosition".into(), (*preserve_scroll).into());
        }
        Action::OpenOverlay { dest, offset, position, .. } => {
            node(&mut m, "OVERLAY", dest);
            if *position == OverlayPosition::Manual {
                m.insert("overlayRelativePosition".into(), json!({"x": num(offset.x), "y": num(offset.y)}));
            }
        }
        Action::SwapOverlay { dest } => node(&mut m, "SWAP", dest),
        Action::ScrollTo { dest, offset } => {
            node(&mut m, "SCROLL_TO", dest);
            m.insert("scrollOffset".into(), json!({"x": num(offset.x), "y": num(offset.y)}));
        }
        Action::ChangeTo { dest } => node(&mut m, "CHANGE_TO", dest),
        Action::CloseOverlay => {
            m.insert("type".into(), "CLOSE".into());
        }
        Action::Back => {
            m.insert("type".into(), "BACK".into());
        }
        Action::OpenUrl { url, new_tab } => {
            m.insert("type".into(), "URL".into());
            m.insert("url".into(), url.clone().into());
            m.insert("openInNewTab".into(), (*new_tab).into());
        }
    }
    Value::Object(m)
}

fn bg_json(c: &Color) -> Value {
    json!({"type": "COLOR", "color": {"r": num(c.r), "g": num(c.g), "b": num(c.b), "a": num(c.a)}})
}

/// Adds `reactions`, scroll fields and flow starting points to the already
/// exported page values (each node's `id` is its Figma id).
pub(crate) fn export_into(doc: &Document, pages: &mut [Value]) {
    // Overlay presentation is stored on the destination node (first user wins).
    let mut overlay: BTreeMap<String, Value> = BTreeMap::new();
    for n in doc.nodes.values() {
        for r in &n.prototype.reactions {
            for a in &r.actions {
                if let Action::OpenOverlay { dest, position, close_on_click_outside, background, .. } = a {
                    let mut m = Map::new();
                    m.insert("overlayPositionType".into(), pos_name(*position).into());
                    m.insert("overlayBackgroundInteraction".into(), if *close_on_click_outside { "CLOSE_ON_CLICK_OUTSIDE" } else { "NONE" }.into());
                    if let Some(c) = background {
                        m.insert("overlayBackground".into(), bg_json(c));
                    }
                    overlay.entry(super::export::figma_id(doc, *dest)).or_insert(Value::Object(m));
                }
            }
        }
    }
    let by_fig: BTreeMap<String, NodeId> = doc.nodes.keys().map(|k| (super::export::figma_id(doc, *k), *k)).collect();
    for page in pages.iter_mut() {
        walk(doc, &by_fig, &overlay, page, None);
        if let Some(pid) = page.get("id").and_then(Value::as_str).and_then(|i| by_fig.get(i)) {
            let flows: Vec<Value> = doc
                .flows
                .iter()
                .filter(|fl| fl.page == *pid)
                .map(|fl| json!({"nodeId": super::export::figma_id(doc, fl.start), "name": fl.name, "description": fl.description}))
                .collect();
            if !flows.is_empty() {
                page["flowStartingPoints"] = Value::Array(flows);
            }
        }
    }
}

fn walk(doc: &Document, by_fig: &BTreeMap<String, NodeId>, overlay: &BTreeMap<String, Value>, v: &mut Value, parent: Option<NodeId>) {
    let Some(obj) = v.as_object_mut() else { return };
    let this = obj.get("id").and_then(Value::as_str).and_then(|i| by_fig.get(i)).copied();
    if let (Some(id), Some(n)) = (this, this.and_then(|i| doc.get(i))) {
        let p = &n.prototype;
        if !p.reactions.is_empty() {
            let list: Vec<Value> = p
                .reactions
                .iter()
                .map(|r| {
                    let actions: Vec<Value> = r.actions.iter().map(|a| action_json(doc, a, r.transition.as_ref())).collect();
                    json!({"trigger": trigger_json(&r.trigger), "actions": actions})
                })
                .collect();
            obj.insert("reactions".into(), Value::Array(list));
        }
        if p.overflow != Overflow::None {
            let name = match p.overflow {
                Overflow::Horizontal => "HORIZONTAL_SCROLLING",
                Overflow::Vertical => "VERTICAL_SCROLLING",
                _ => "HORIZONTAL_AND_VERTICAL_SCROLLING",
            };
            obj.insert("overflowDirection".into(), name.into());
        }
        if parent.and_then(|pp| doc.get(pp)).is_some_and(|pn| pn.prototype.fixed_children.contains(&id)) {
            obj.insert("scrollBehavior".into(), "FIXED".into());
        }
        if let Some(o) = obj.get("id").and_then(Value::as_str).and_then(|i| overlay.get(i)).and_then(Value::as_object) {
            for (k, x) in o {
                obj.insert(k.clone(), x.clone());
            }
        }
    }
    if let Some(kids) = obj.get_mut("children").and_then(Value::as_array_mut) {
        for k in kids {
            walk(doc, by_fig, overlay, k, this);
        }
    }
}

// --------------------------------------------------------------------- .fig

fn fig_trigger(event: &Value) -> Option<Value> {
    let ty = s(event, "interactionType")?;
    // Timeouts are stored in seconds as floats; tolerate milliseconds too.
    let t = f(event, "transitionTimeout").unwrap_or(0.0);
    let ms = if t <= 60.0 { (t * 1000.0).round() } else { t };
    let mut m = Map::new();
    m.insert("type".into(), ty.into());
    m.insert("timeout".into(), ms.into());
    m.insert("delay".into(), ms.into());
    if let Some(codes) = event.get("keyTrigger").and_then(|k| k.get("keyCodes")).and_then(Value::as_array) {
        m.insert("keyCodes".into(), Value::Array(codes.clone()));
    }
    Some(Value::Object(m))
}

fn fig_guid(g: &Value) -> Option<String> {
    Some(format!("{}:{}", g.get("sessionID")?.as_u64()?, g.get("localID")?.as_u64()?))
}

fn fig_action(a: &Value) -> Option<Value> {
    let conn = s(a, "connectionType").unwrap_or("INTERNAL_NODE");
    let nav = s(a, "navigationType");
    let mut m = Map::new();
    match (conn, nav) {
        ("BACK", _) | (_, Some("BACK")) => {
            m.insert("type".into(), "BACK".into());
        }
        ("CLOSE", _) | (_, Some("CLOSE")) => {
            m.insert("type".into(), "CLOSE".into());
        }
        ("URL", _) | ("EXTERNAL_URL", _) => {
            m.insert("type".into(), "URL".into());
            m.insert("url".into(), s(a, "connectionURL")?.into());
            m.insert("openInNewTab".into(), b(a, "openUrlInNewTab").unwrap_or(true).into());
        }
        _ => {
            let dest = a.get("transitionNodeID").and_then(fig_guid)?;
            m.insert("type".into(), "NODE".into());
            m.insert("destinationId".into(), dest.into());
            m.insert("navigation".into(), nav.unwrap_or("NAVIGATE").into());
            if let Some(tt) = s(a, "transitionType") {
                if tt != "INSTANT" && tt != "NONE" {
                    let d = f(a, "transitionDuration").unwrap_or(0.3);
                    let ms = if d <= 10.0 { (d * 1000.0).round() } else { d };
                    let mut tr = Map::new();
                    tr.insert("type".into(), tt.into());
                    tr.insert("duration".into(), ms.into());
                    if let Some(dir) = s(a, "transitionDirection").or_else(|| s(a, "direction")) {
                        tr.insert("direction".into(), dir.into());
                    }
                    if let Some(e) = s(a, "easingType") {
                        tr.insert("easing".into(), json!({"type": e}));
                    }
                    m.insert("transition".into(), Value::Object(tr));
                }
            }
        }
    }
    Some(Value::Object(m))
}

/// Rewrites a `.fig` node change's prototype fields into REST fields.
pub(crate) fn convert_fig(m: &mut Map<String, Value>, nc: &Value) {
    if let Some(list) = nc.get("prototypeInteractions").and_then(Value::as_array) {
        let mut reactions = Vec::new();
        for it in list {
            if b(it, "isDeleted") == Some(true) {
                continue;
            }
            let Some(trigger) = it.get("event").and_then(fig_trigger) else { continue };
            let actions: Vec<Value> = it.get("actions").and_then(Value::as_array).map(|a| a.iter().filter_map(fig_action).collect()).unwrap_or_default();
            if !actions.is_empty() {
                reactions.push(json!({"trigger": trigger, "actions": actions}));
            }
        }
        if !reactions.is_empty() {
            m.insert("reactions".into(), Value::Array(reactions));
        }
    }
    for k in ["overflowDirection", "scrollBehavior", "overlayPositionType", "overlayBackgroundInteraction"] {
        if let Some(x) = nc.get(k).filter(|x| x.is_string()) {
            m.insert(k.into(), x.clone());
        }
    }
    if let Some(x) = nc.get("overlayBackgroundAppearance").and_then(|a| a.get("backgroundColor")) {
        m.insert("overlayBackground".into(), json!({"type": "COLOR", "color": x}));
    }
    if let Some(fl) = nc.get("flowStartingPoint").or_else(|| nc.get("isFlowStartingPoint")) {
        let name = fl.as_str().map(String::from).or_else(|| s(fl, "name").map(String::from)).unwrap_or_default();
        if fl.as_bool() != Some(false) && !fl.is_null() {
            m.insert("_flowStart".into(), name.into());
        }
    }
}

/// Moves `_flowStart` markers on frames into the canvas' `flowStartingPoints`.
pub(crate) fn lift_flow_starts(page: &mut Value) {
    fn rec(v: &mut Value, out: &mut Vec<Value>) {
        let Some(obj) = v.as_object_mut() else { return };
        if let Some(name) = obj.remove("_flowStart") {
            let id = obj.get("id").cloned().unwrap_or(Value::Null);
            let nm = name.as_str().filter(|n| !n.is_empty()).map(String::from).or_else(|| obj.get("name").and_then(Value::as_str).map(String::from)).unwrap_or_default();
            out.push(json!({"nodeId": id, "name": nm}));
        }
        if let Some(kids) = obj.get_mut("children").and_then(Value::as_array_mut) {
            for k in kids {
                rec(k, out);
            }
        }
    }
    let mut out = Vec::new();
    rec(page, &mut out);
    if !out.is_empty() {
        page["flowStartingPoints"] = Value::Array(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_roundtrip() {
        for c in [13u32, 27, 32, 37, 65, 90, 48, 112] {
            assert_eq!(key_code(&key_name(c)), Some(c), "{c}");
        }
        assert_eq!(key_code("space"), Some(32));
        assert_eq!(key_code("???"), None);
    }

    #[test]
    fn fig_interactions_become_reactions() {
        let nc = json!({
            "prototypeInteractions": [
                {"event": {"interactionType": "ON_CLICK"}, "actions": [{"navigationType": "NAVIGATE", "connectionType": "INTERNAL_NODE", "transitionNodeID": {"sessionID": 1, "localID": 9}, "transitionType": "DISSOLVE", "transitionDuration": 0.4, "easingType": "EASE_IN"}]},
                {"event": {"interactionType": "AFTER_TIMEOUT", "transitionTimeout": 1.5}, "actions": [{"connectionType": "BACK"}]},
                {"isDeleted": true, "event": {"interactionType": "ON_CLICK"}, "actions": [{"connectionType": "CLOSE"}]},
                {"event": {"interactionType": "ON_CLICK"}, "actions": []}
            ],
            "overflowDirection": "VERTICAL_SCROLLING",
            "flowStartingPoint": {"name": "Onboarding"}
        });
        let mut m = Map::new();
        convert_fig(&mut m, &nc);
        let r = m["reactions"].as_array().unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0]["actions"][0]["destinationId"], "1:9");
        assert_eq!(r[0]["actions"][0]["transition"]["duration"], 400.0);
        assert_eq!(r[1]["trigger"]["timeout"], 1500.0);
        assert_eq!(r[1]["actions"][0]["type"], "BACK");
        assert_eq!(m["overflowDirection"], "VERTICAL_SCROLLING");
        assert_eq!(m["_flowStart"], "Onboarding");
    }

    #[test]
    fn tolerant_of_garbage() {
        let mut m = Map::new();
        convert_fig(&mut m, &json!({"prototypeInteractions": [1, "x", {"event": 3}, {"event": {}, "actions": 5}]}));
        assert!(!m.contains_key("reactions"));
        convert_fig(&mut m, &json!({"prototypeInteractions": "nope", "flowStartingPoint": false}));
        assert!(m.is_empty());
    }
}
