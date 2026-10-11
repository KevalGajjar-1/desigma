//! The prototype player: a UI-free state machine that runs reactions against
//! a scratch copy of the document. The Present view feeds it pointer/key
//! events plus the clock and draws whatever [`Player`] says is on screen.

use super::{easing, geom};
use desigma_doc::*;
use std::collections::BTreeMap;

/// Things the host must do outside the player (the player never touches the OS).
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    OpenUrl { url: String, new_tab: bool },
}

/// Event kinds a trigger can match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Click,
    Drag,
    HoverOn,
    PressOn,
    Enter,
    Leave,
    Down,
    Up,
}

pub fn trigger_matches(t: &Trigger, w: When) -> bool {
    matches!(
        (t, w),
        (Trigger::OnClick, When::Click)
            | (Trigger::OnDrag, When::Drag)
            | (Trigger::WhileHovering, When::HoverOn)
            | (Trigger::WhilePressing, When::PressOn)
            | (Trigger::MouseEnter { .. }, When::Enter)
            | (Trigger::MouseLeave { .. }, When::Leave)
            | (Trigger::MouseDown { .. }, When::Down)
            | (Trigger::MouseUp { .. }, When::Up)
    )
}

fn trigger_delay_ms(t: &Trigger) -> u32 {
    match t {
        Trigger::MouseEnter { delay_ms }
        | Trigger::MouseLeave { delay_ms }
        | Trigger::MouseDown { delay_ms }
        | Trigger::MouseUp { delay_ms }
        | Trigger::AfterDelay { delay_ms } => *delay_ms,
        _ => 0,
    }
}

/// Order-insensitive, case-insensitive comparison of "Ctrl+K" style key strings.
pub fn keys_match(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        let mut v: Vec<String> = s.split('+').map(|k| k.trim().to_ascii_lowercase()).filter(|k| !k.is_empty()).collect();
        v.sort();
        v
    };
    let (x, y) = (norm(a), norm(b));
    !x.is_empty() && x == y
}

/// A screen kept for Back, with the transition that undoes how we left it.
#[derive(Clone, Debug, PartialEq)]
pub struct BackEntry {
    pub frame: NodeId,
    pub via: Option<Transition>,
}

/// Content of a frame as it was before a ChangeTo, kept for the transition.
pub struct Snapshot {
    pub id: u64,
    pub doc: Document,
}

/// A running frame-to-frame transition.
pub struct Anim {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: TransitionKind,
    pub easing: Easing,
    pub start: f64,
    pub dur: f64,
    /// `from` lives in this snapshot instead of the live document.
    pub snapshot: Option<Snapshot>,
}

impl Anim {
    pub fn raw(&self, now: f64) -> f32 {
        if self.dur <= 0.0 {
            1.0
        } else {
            (((now - self.start) / self.dur) as f32).clamp(0.0, 1.0)
        }
    }
    pub fn eased(&self, now: f64) -> f32 {
        easing::ease(&self.easing, self.raw(now))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayAnim {
    pub start: f64,
    pub dur: f64,
    pub opening: bool,
}

#[derive(Clone, Debug)]
pub struct Overlay {
    pub frame: NodeId,
    /// Top-left inside the frame it covers.
    pub origin: Vec2,
    pub background: Option<Color>,
    pub close_outside: bool,
    pub position: OverlayPosition,
    pub offset: Vec2,
    pub transition: Option<Transition>,
    pub anim: Option<OverlayAnim>,
}

impl Overlay {
    pub fn closing(&self) -> bool {
        self.anim.is_some_and(|a| !a.opening)
    }
    /// 0 hidden .. 1 shown (eased).
    pub fn shown(&self, now: f64) -> f32 {
        let Some(a) = self.anim else { return 1.0 };
        let raw = if a.dur <= 0.0 { 1.0 } else { (((now - a.start) / a.dur) as f32).clamp(0.0, 1.0) };
        let e = self.transition.map(|t| t.easing).unwrap_or(Easing::EaseOut);
        let v = easing::ease(&e, raw);
        if a.opening {
            v
        } else {
            1.0 - v
        }
    }
}

struct ScrollAnim {
    frame: NodeId,
    from: Vec2,
    to: Vec2,
    easing: Easing,
    start: f64,
    dur: f64,
}

struct Timer {
    at: f64,
    node: NodeId,
    reaction: Reaction,
    /// Only fires while this frame is on screen.
    frame: NodeId,
}

enum Undo {
    RestoreComponent { inst: NodeId, comp: NodeId },
    CloseOverlay { frame: NodeId },
}

struct Active {
    node: NodeId,
    when: When,
    undo: Vec<Undo>,
}

pub struct Player {
    /// Scratch copy of the document; ChangeTo edits this, never the user's file.
    pub doc: Document,
    pub current: NodeId,
    pub back: Vec<BackEntry>,
    pub overlays: Vec<Overlay>,
    pub anim: Option<Anim>,
    pub scroll: BTreeMap<NodeId, Vec2>,
    /// Bumped per top-level frame whenever its content changes (texture cache key).
    pub rev: BTreeMap<NodeId, u64>,
    pub effects: Vec<Effect>,
    pub now: f64,
    scroll_anim: Option<ScrollAnim>,
    timers: Vec<Timer>,
    hover: Vec<NodeId>,
    actives: Vec<Active>,
    press_target: Option<NodeId>,
    snapshots: u64,
}

impl Player {
    pub fn new(doc: &Document, start: NodeId) -> Self {
        let mut p = Player {
            doc: doc.clone(),
            current: start,
            back: Vec::new(),
            overlays: Vec::new(),
            anim: None,
            scroll: BTreeMap::new(),
            rev: BTreeMap::new(),
            effects: Vec::new(),
            now: 0.0,
            scroll_anim: None,
            timers: Vec::new(),
            hover: Vec::new(),
            actives: Vec::new(),
            press_target: None,
            snapshots: 0,
        };
        p.current = p.top_frame_of(start).unwrap_or(start);
        p.on_shown(p.current);
        p
    }

    /// Starts over from `start` on a fresh copy of `original`.
    pub fn restart(&mut self, original: &Document, start: NodeId) {
        let now = self.now;
        let rev = std::mem::take(&mut self.rev);
        *self = Player::new(original, start);
        self.now = now;
        // keep revisions growing so cached textures of the old copy are not reused
        self.rev = rev.into_iter().map(|(k, v)| (k, v + 1)).collect();
        self.on_shown(self.current);
    }

    // ------------------------------------------------------------ structure

    /// The page-level frame containing `id` (or `id` itself when top-level).
    pub fn top_frame_of(&self, mut id: NodeId) -> Option<NodeId> {
        loop {
            let n = self.doc.get(id)?;
            let p = n.parent?;
            if matches!(self.doc.get(p)?.kind, NodeKind::Page { .. }) {
                return matches!(n.kind, NodeKind::Page { .. }).then_some(id).or(Some(id));
            }
            id = p;
        }
    }

    pub fn size_of(&self, frame: NodeId) -> Vec2 {
        self.doc.get(frame).map(|n| n.size).unwrap_or(Vec2::ZERO)
    }

    pub fn frame_rev(&self, frame: NodeId) -> u64 {
        self.rev.get(&frame).copied().unwrap_or(0)
    }

    fn bump(&mut self, frame: NodeId) {
        *self.rev.entry(frame).or_insert(0) += 1;
    }

    /// Frames currently on screen from bottom to top.
    pub fn visible_frames(&self) -> Vec<NodeId> {
        let mut v = vec![self.current];
        v.extend(self.overlays.iter().map(|o| o.frame));
        v
    }

    fn frame_visible(&self, f: NodeId) -> bool {
        self.current == f || self.overlays.iter().any(|o| o.frame == f && !o.closing())
    }

    pub fn is_animating(&self) -> bool {
        self.anim.is_some() || self.scroll_anim.is_some() || self.overlays.iter().any(|o| o.anim.is_some()) || !self.timers.is_empty()
    }

    /// Whether anything at or above `target` reacts to the pointer (for the hand cursor).
    pub fn is_interactive(&self, target: NodeId) -> bool {
        [When::Click, When::Drag, When::Down, When::Up, When::HoverOn, When::PressOn, When::Enter, When::Leave]
            .iter()
            .any(|w| self.find_reactive(target, *w).is_some())
    }

    fn reactions_on(&self, node: NodeId, w: When) -> Vec<Reaction> {
        self.doc.get(node).map(|n| n.prototype.reactions.iter().filter(|r| trigger_matches(&r.trigger, w)).cloned().collect()).unwrap_or_default()
    }

    /// Deepest node from `target` up to its frame that has a reaction for `w`.
    fn find_reactive(&self, target: NodeId, w: When) -> Option<NodeId> {
        let mut cur = Some(target);
        while let Some(id) = cur {
            if !self.reactions_on(id, w).is_empty() {
                return Some(id);
            }
            cur = self.doc.get(id).and_then(|n| n.parent).filter(|p| !matches!(self.doc.get(*p).map(|n| &n.kind), Some(NodeKind::Page { .. })));
        }
        None
    }

    // ------------------------------------------------------------- clock

    /// Aligns the player's clock with the host's (timers keep their delay).
    pub fn rebase_clock(&mut self, now: f64) {
        let d = now - self.now;
        for t in &mut self.timers {
            t.at += d;
        }
        self.now = now;
    }

    /// Advances the clock: finishes transitions, runs due timers.
    pub fn tick(&mut self, now: f64) {
        self.now = now;
        if let Some(a) = &self.anim {
            if now >= a.start + a.dur {
                self.anim = None;
                self.on_shown(self.current);
            }
        }
        let mut shown = Vec::new();
        self.overlays.retain_mut(|o| {
            if let Some(a) = o.anim {
                if now >= a.start + a.dur {
                    o.anim = None;
                    if a.opening {
                        shown.push(o.frame);
                    } else {
                        return false;
                    }
                }
            }
            true
        });
        for f in shown {
            self.on_shown(f);
        }
        if let Some(s) = &self.scroll_anim {
            let raw = if s.dur <= 0.0 { 1.0 } else { (((now - s.start) / s.dur) as f32).clamp(0.0, 1.0) };
            let e = easing::ease(&s.easing, raw);
            let v = Vec2::new(s.from.x + (s.to.x - s.from.x) * e, s.from.y + (s.to.y - s.from.y) * e);
            let f = s.frame;
            self.scroll.insert(f, v);
            if raw >= 1.0 {
                self.scroll_anim = None;
            }
        }
        // timers
        let mut due = Vec::new();
        let mut i = 0;
        while i < self.timers.len() {
            if self.timers[i].at <= now {
                due.push(self.timers.remove(i));
            } else {
                i += 1;
            }
        }
        for t in due {
            if self.frame_visible(t.frame) && self.doc.get(t.node).is_some() {
                self.run(t.node, &t.reaction, None);
            }
        }
    }

    fn finish_anim(&mut self) {
        if self.anim.take().is_some() {
            self.on_shown(self.current);
        }
        let mut shown = Vec::new();
        self.overlays.retain_mut(|o| match o.anim.take() {
            Some(a) if a.opening => {
                shown.push(o.frame);
                true
            }
            Some(_) => false,
            None => true,
        });
        for f in shown {
            self.on_shown(f);
        }
        if let Some(s) = self.scroll_anim.take() {
            self.scroll.insert(s.frame, s.to);
        }
    }

    /// Schedules the AfterDelay reactions of everything in `frame`.
    fn on_shown(&mut self, frame: NodeId) {
        self.timers.retain(|t| t.frame != frame);
        let mut stack = vec![frame];
        let mut found = Vec::new();
        while let Some(id) = stack.pop() {
            let Some(n) = self.doc.get(id) else { continue };
            for r in &n.prototype.reactions {
                if let Trigger::AfterDelay { delay_ms } = r.trigger {
                    found.push((id, r.clone(), delay_ms));
                }
            }
            stack.extend(n.children.iter().copied());
        }
        for (node, reaction, ms) in found {
            self.timers.push(Timer { at: self.now + ms as f64 / 1000.0, node, reaction, frame });
        }
    }

    fn schedule(&mut self, node: NodeId, r: &Reaction) {
        let frame = self.top_frame_of(node).unwrap_or(self.current);
        self.timers.push(Timer { at: self.now + trigger_delay_ms(&r.trigger) as f64 / 1000.0, node, reaction: r.clone(), frame });
    }

    // ------------------------------------------------------------ pointer

    fn fire(&mut self, node: NodeId, w: When, undo: Option<&mut Vec<Undo>>) {
        let rs = self.reactions_on(node, w);
        let mut undo = undo;
        for r in rs {
            if trigger_delay_ms(&r.trigger) > 0 {
                self.schedule(node, &r);
            } else {
                self.run(node, &r, undo.as_deref_mut());
            }
        }
    }

    /// Pointer moved; `target` is the deepest layer under it (None over nothing).
    pub fn pointer_move(&mut self, target: Option<NodeId>) {
        let mut chain = Vec::new();
        let mut cur = target;
        while let Some(id) = cur {
            if self.doc.get(id).is_none() {
                break;
            }
            let reactive = [When::Enter, When::Leave, When::HoverOn].iter().any(|w| !self.reactions_on(id, *w).is_empty());
            if reactive {
                chain.push(id);
            }
            cur = self.doc.get(id).and_then(|n| n.parent).filter(|p| !matches!(self.doc.get(*p).map(|n| &n.kind), Some(NodeKind::Page { .. })));
        }
        if chain == self.hover || self.anim.is_some() {
            return;
        }
        let old = std::mem::replace(&mut self.hover, chain.clone());
        for n in old.iter().filter(|n| !chain.contains(n)) {
            // hover ended: undo while-hovering effects, then Leave
            if let Some(i) = self.actives.iter().position(|a| a.node == *n && a.when == When::HoverOn) {
                let a = self.actives.remove(i);
                self.undo(a.undo);
            }
            self.fire(*n, When::Leave, None);
        }
        for n in chain.iter().filter(|n| !old.contains(n)).rev() {
            self.fire(*n, When::Enter, None);
            if !self.reactions_on(*n, When::HoverOn).is_empty() {
                let mut u = Vec::new();
                self.fire(*n, When::HoverOn, Some(&mut u));
                self.actives.push(Active { node: *n, when: When::HoverOn, undo: u });
            }
        }
    }

    pub fn pointer_down(&mut self, target: Option<NodeId>) {
        self.finish_anim();
        self.press_target = target;
        let Some(t) = target else { return };
        if let Some(n) = self.find_reactive(t, When::Down) {
            self.fire(n, When::Down, None);
        }
        if let Some(n) = self.find_reactive(t, When::PressOn) {
            let mut u = Vec::new();
            self.fire(n, When::PressOn, Some(&mut u));
            self.actives.push(Active { node: n, when: When::PressOn, undo: u });
        }
    }

    /// Pointer released. `dragged` is true when the press turned into a drag/scroll.
    pub fn pointer_up(&mut self, target: Option<NodeId>, dragged: bool) {
        self.finish_anim();
        while let Some(i) = self.actives.iter().position(|a| a.when == When::PressOn) {
            let a = self.actives.remove(i);
            self.undo(a.undo);
        }
        self.press_target = None;
        if let Some(t) = target {
            if let Some(n) = self.find_reactive(t, When::Up) {
                self.fire(n, When::Up, None);
            }
        }
        if !dragged {
            self.click(target);
        }
    }

    /// A drag gesture started on `target`: fires On drag once.
    pub fn drag(&mut self, target: Option<NodeId>) -> bool {
        let Some(n) = target.and_then(|t| self.find_reactive(t, When::Drag)) else { return false };
        self.finish_anim();
        self.fire(n, When::Drag, None);
        true
    }

    fn click(&mut self, target: Option<NodeId>) {
        // Close-on-click-outside: a click that is not inside the topmost overlay closes it.
        if let Some(top) = self.overlays.iter().rposition(|o| !o.closing()) {
            let o = &self.overlays[top];
            let inside = target.and_then(|t| self.top_frame_of(t)) == Some(o.frame);
            if o.close_outside && !inside {
                self.close_overlay_at(top);
                return;
            }
        }
        if let Some(n) = target.and_then(|t| self.find_reactive(t, When::Click)) {
            self.fire(n, When::Click, None);
        }
    }

    /// Keyboard shortcut like "Space" or "Ctrl+K".
    pub fn key(&mut self, pressed: &str) {
        self.finish_anim();
        let mut hits = Vec::new();
        for f in self.visible_frames() {
            let mut stack = vec![f];
            while let Some(id) = stack.pop() {
                let Some(n) = self.doc.get(id) else { continue };
                for r in &n.prototype.reactions {
                    if matches!(&r.trigger, Trigger::KeyPress { keys } if keys_match(keys, pressed)) {
                        hits.push((id, r.clone()));
                    }
                }
                stack.extend(n.children.iter().rev().copied());
            }
        }
        for (id, r) in hits {
            self.run(id, &r, None);
        }
    }

    fn undo(&mut self, list: Vec<Undo>) {
        for u in list {
            match u {
                Undo::RestoreComponent { inst, comp } => self.change_to(inst, comp, None, None),
                Undo::CloseOverlay { frame } => {
                    if let Some(i) = self.overlays.iter().rposition(|o| o.frame == frame && !o.closing()) {
                        self.close_overlay_at(i);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------- actions

    fn run(&mut self, source: NodeId, r: &Reaction, mut undo: Option<&mut Vec<Undo>>) {
        for a in &r.actions {
            match a {
                Action::Navigate { dest, preserve_scroll } => self.navigate(*dest, r.transition, *preserve_scroll, true),
                Action::Back => self.go_back(),
                Action::OpenOverlay { dest, position, offset, close_on_click_outside, background } => {
                    if let Some(frame) = self.open_overlay(source, *dest, *position, *offset, *close_on_click_outside, *background, r.transition) {
                        if let Some(u) = undo.as_deref_mut() {
                            u.push(Undo::CloseOverlay { frame });
                        }
                    }
                }
                Action::SwapOverlay { dest } => self.swap_overlay(source, *dest, r.transition),
                Action::CloseOverlay => self.close_overlay(source),
                Action::ScrollTo { dest, offset } => self.scroll_to(*dest, *offset, r.transition),
                Action::OpenUrl { url, new_tab } => self.effects.push(Effect::OpenUrl { url: url.clone(), new_tab: *new_tab }),
                Action::ChangeTo { dest } => self.change_to(source, *dest, r.transition, undo.as_deref_mut()),
            }
        }
    }

    fn start_anim(&mut self, from: NodeId, to: NodeId, t: Transition, snapshot: Option<Snapshot>) {
        let dur = easing::effective_duration_ms(&t.easing, t.duration_ms) as f64 / 1000.0;
        self.anim = Some(Anim { from, to, kind: t.kind, easing: t.easing, start: self.now, dur, snapshot });
    }

    fn navigate(&mut self, dest: NodeId, transition: Option<Transition>, preserve_scroll: bool, push: bool) {
        let Some(frame) = self.top_frame_of(dest) else { return };
        self.finish_anim();
        self.clear_overlays();
        let from = self.current;
        if push {
            self.back.push(BackEntry { frame: from, via: transition.map(|t| Transition { kind: geom::reverse(t.kind), ..t }) });
        }
        self.timers.retain(|t| t.frame != from);
        self.current = frame;
        if !preserve_scroll {
            self.scroll.remove(&frame);
        }
        self.hover.clear();
        self.actives.clear();
        match transition.filter(|t| easing::effective_duration_ms(&t.easing, t.duration_ms) > 0) {
            Some(t) => self.start_anim(from, frame, t, None),
            None => self.on_shown(frame),
        }
    }

    fn go_back(&mut self) {
        let Some(e) = self.back.pop() else { return };
        self.navigate(e.frame, e.via, false, false);
    }

    fn clear_overlays(&mut self) {
        for o in std::mem::take(&mut self.overlays) {
            self.timers.retain(|t| t.frame != o.frame);
        }
    }

    /// The overlay whose frame contains `node`, else the topmost open overlay.
    fn overlay_of(&self, node: NodeId) -> Option<usize> {
        let owner = self.top_frame_of(node);
        self.overlays.iter().position(|o| Some(o.frame) == owner && !o.closing()).or_else(|| self.overlays.iter().rposition(|o| !o.closing()))
    }

    /// Rect of `node` in the coordinates of the frame it is shown over.
    fn rect_over_base(&self, node: NodeId) -> Rect {
        let Some(owner) = self.top_frame_of(node) else { return Rect::default() };
        let r = rect_in_frame(&self.doc, node, owner);
        match self.overlays.iter().find(|o| o.frame == owner) {
            Some(o) => Rect::new(r.x + o.origin.x, r.y + o.origin.y, r.w, r.h),
            None => r,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn open_overlay(
        &mut self,
        source: NodeId,
        dest: NodeId,
        position: OverlayPosition,
        offset: Vec2,
        close_outside: bool,
        background: Option<Color>,
        transition: Option<Transition>,
    ) -> Option<NodeId> {
        let frame = self.top_frame_of(dest)?;
        if frame == self.current || self.overlays.iter().any(|o| o.frame == frame && !o.closing()) {
            return None;
        }
        self.finish_anim();
        let trigger = self.rect_over_base(source);
        let base = self.size_of(self.current);
        let origin = geom::overlay_origin(position, offset, self.size_of(frame), base, trigger);
        let t = transition.filter(|t| easing::effective_duration_ms(&t.easing, t.duration_ms) > 0);
        let anim = t.map(|t| OverlayAnim { start: self.now, dur: easing::effective_duration_ms(&t.easing, t.duration_ms) as f64 / 1000.0, opening: true });
        self.overlays.push(Overlay { frame, origin, background, close_outside, position, offset, transition: t, anim });
        if anim.is_none() {
            self.on_shown(frame);
        }
        Some(frame)
    }

    fn swap_overlay(&mut self, source: NodeId, dest: NodeId, transition: Option<Transition>) {
        let Some(i) = self.overlay_of(source) else {
            self.open_overlay(source, dest, OverlayPosition::Center, Vec2::ZERO, false, None, transition);
            return;
        };
        let old = self.overlays.remove(i);
        self.timers.retain(|t| t.frame != old.frame);
        let Some(frame) = self.top_frame_of(dest) else { return };
        let origin = geom::overlay_origin(old.position, old.offset, self.size_of(frame), self.size_of(self.current), self.rect_over_base(source));
        let t = transition.filter(|t| easing::effective_duration_ms(&t.easing, t.duration_ms) > 0);
        let anim = t.map(|t| OverlayAnim { start: self.now, dur: easing::effective_duration_ms(&t.easing, t.duration_ms) as f64 / 1000.0, opening: true });
        self.overlays.insert(
            i,
            Overlay {
                frame,
                origin,
                background: old.background,
                close_outside: old.close_outside,
                position: old.position,
                offset: old.offset,
                transition: t,
                anim,
            },
        );
        if anim.is_none() {
            self.on_shown(frame);
        }
    }

    fn close_overlay(&mut self, source: NodeId) {
        if let Some(i) = self.overlay_of(source) {
            self.close_overlay_at(i);
        }
    }

    fn close_overlay_at(&mut self, i: usize) {
        let Some(o) = self.overlays.get_mut(i) else { return };
        self.timers.retain(|t| t.frame != o.frame);
        match o.transition {
            Some(t) => {
                o.anim = Some(OverlayAnim { start: self.now, dur: easing::effective_duration_ms(&t.easing, t.duration_ms) as f64 / 1000.0, opening: false });
            }
            None => {
                self.overlays.remove(i);
            }
        }
    }

    /// Valid scroll range per axis for a top-level frame.
    pub fn scroll_range(&self, frame: NodeId) -> Vec2 {
        let Some(n) = self.doc.get(frame) else { return Vec2::ZERO };
        let ext = content_extent(&self.doc, frame);
        let (ox, oy) = match n.prototype.overflow {
            Overflow::None => (false, false),
            Overflow::Horizontal => (true, false),
            Overflow::Vertical => (false, true),
            Overflow::Both => (true, true),
        };
        Vec2::new(if ox { (ext.x - n.size.x).max(0.0) } else { 0.0 }, if oy { (ext.y - n.size.y).max(0.0) } else { 0.0 })
    }

    pub fn scroll_of(&self, frame: NodeId) -> Vec2 {
        let r = self.scroll_range(frame);
        let s = self.scroll.get(&frame).copied().unwrap_or(Vec2::ZERO);
        Vec2::new(s.x.clamp(0.0, r.x), s.y.clamp(0.0, r.y))
    }

    /// Scrolls `frame` by a delta in frame units (positive = content moves up/left).
    pub fn scroll_by(&mut self, frame: NodeId, delta: Vec2) {
        let r = self.scroll_range(frame);
        let s = self.scroll_of(frame);
        self.scroll_anim = None;
        self.scroll.insert(frame, Vec2::new((s.x + delta.x).clamp(0.0, r.x), (s.y + delta.y).clamp(0.0, r.y)));
    }

    fn scroll_to(&mut self, dest: NodeId, offset: Vec2, transition: Option<Transition>) {
        let Some(frame) = self.top_frame_of(dest) else { return };
        if !self.frame_visible(frame) {
            return;
        }
        let r = self.scroll_range(frame);
        let pos = rect_in_frame(&self.doc, dest, frame);
        let to = Vec2::new((pos.x - offset.x).clamp(0.0, r.x), (pos.y - offset.y).clamp(0.0, r.y));
        let from = self.scroll_of(frame);
        match transition.map(|t| (t, easing::effective_duration_ms(&t.easing, t.duration_ms))).filter(|(_, d)| *d > 0) {
            Some((t, d)) => self.scroll_anim = Some(ScrollAnim { frame, from, to, easing: t.easing, start: self.now, dur: d as f64 / 1000.0 }),
            None => {
                self.scroll_anim = None;
                self.scroll.insert(frame, to);
            }
        }
    }

    fn change_to(&mut self, source: NodeId, dest: NodeId, transition: Option<Transition>, undo: Option<&mut Vec<Undo>>) {
        // nearest instance at or above the source layer
        let mut cur = Some(source);
        let mut inst = None;
        while let Some(id) = cur {
            let Some(n) = self.doc.get(id) else { break };
            if matches!(n.kind, NodeKind::Instance { .. }) {
                inst = Some(id);
                break;
            }
            cur = n.parent;
        }
        let Some(inst) = inst else { return };
        let Some(prev) = self.doc.get(inst).and_then(instance_component) else { return };
        let Some(frame) = self.top_frame_of(inst) else { return };
        let snapshot = transition
            .filter(|t| {
                matches!(t.kind, TransitionKind::SmartAnimate | TransitionKind::Dissolve) && easing::effective_duration_ms(&t.easing, t.duration_ms) > 0
            })
            .map(|t| {
                self.snapshots += 1;
                (t, Snapshot { id: self.snapshots, doc: super::smart::subtree_doc(&self.doc, frame) })
            });
        let cmd = swap_instance(&mut self.doc, inst, dest);
        if cmd.apply(&mut self.doc).is_err() {
            return;
        }
        if self.doc.get(inst).and_then(instance_component) == Some(prev) {
            return;
        }
        self.bump(frame);
        if let Some(u) = undo {
            u.push(Undo::RestoreComponent { inst, comp: prev });
        }
        if let Some((t, snap)) = snapshot {
            if self.anim.is_none() && frame == self.current {
                self.start_anim(frame, frame, t, Some(snap));
            }
        }
    }
}

// ------------------------------------------------------------- free helpers

/// Bounding box of `node` in the coordinate space of its ancestor `frame`.
pub fn rect_in_frame(doc: &Document, node: NodeId, frame: NodeId) -> Rect {
    let inv = doc.world_transform(frame).invert().unwrap_or(Affine::IDENTITY);
    let w = inv * doc.world_transform(node);
    let Some(n) = doc.get(node) else { return Rect::default() };
    let pts = [Vec2::new(0.0, 0.0), Vec2::new(n.size.x, 0.0), Vec2::new(n.size.x, n.size.y), Vec2::new(0.0, n.size.y)].map(|p| w.apply(p));
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in pts {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Right/bottom extent of the scrolling (non-fixed) children of a frame.
pub fn content_extent(doc: &Document, frame: NodeId) -> Vec2 {
    let Some(f) = doc.get(frame) else { return Vec2::ZERO };
    let mut ext = f.size;
    for c in &f.children {
        if f.prototype.fixed_children.contains(c) {
            continue;
        }
        let Some(n) = doc.get(*c) else { continue };
        if !n.visible {
            continue;
        }
        let r = rect_in_frame(doc, *c, frame);
        ext.x = ext.x.max(r.x + r.w);
        ext.y = ext.y.max(r.y + r.h);
    }
    ext
}

/// Deepest visible layer at `p` (frame coordinates, before scrolling).
/// Fixed children are tested at `p`, everything else at `p + scroll`.
pub fn hit_layer(doc: &Document, frame: NodeId, p: Vec2, scroll: Vec2) -> Option<NodeId> {
    let f = doc.get(frame)?;
    if !Rect::new(0.0, 0.0, f.size.x, f.size.y).contains(p) {
        return None;
    }
    for c in f.children.iter().rev() {
        let fixed = f.prototype.fixed_children.contains(c);
        let q = if fixed { p } else { Vec2::new(p.x + scroll.x, p.y + scroll.y) };
        if let Some(h) = hit_in(doc, *c, q) {
            return Some(h);
        }
    }
    Some(frame)
}

fn hit_in(doc: &Document, id: NodeId, p: Vec2) -> Option<NodeId> {
    let n = doc.get(id)?;
    if !n.visible {
        return None;
    }
    let local = n.transform.invert()?.apply(p);
    let inside = Rect::new(0.0, 0.0, n.size.x, n.size.y).contains(local);
    let clips = n.kind.frame_props().is_some_and(|f| f.clips_content);
    if n.kind.is_container() && (inside || !clips) {
        for c in n.children.iter().rev() {
            if let Some(h) = hit_in(doc, *c, local) {
                return Some(h);
            }
        }
    }
    inside.then_some(id)
}

/// Top-level frames of a page that can be prototype destinations.
pub fn top_level_frames(doc: &Document, page: NodeId) -> Vec<NodeId> {
    doc.get(page)
        .map(|p| {
            p.children
                .iter()
                .copied()
                .filter(|c| doc.get(*c).is_some_and(|n| n.visible && matches!(n.kind, NodeKind::Frame(_) | NodeKind::Component(_) | NodeKind::Section)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fx {
        doc: Document,
        a: NodeId,
        b: NodeId,
        c: NodeId,
        btn: NodeId,
        btn_b: NodeId,
    }

    fn tr(kind: TransitionKind, ms: u32) -> Option<Transition> {
        Some(Transition { kind, easing: Easing::Linear, duration_ms: ms })
    }

    fn react(trigger: Trigger, action: Action, transition: Option<Transition>) -> Reaction {
        Reaction { trigger, actions: vec![action], transition }
    }

    fn fixture() -> Fx {
        let mut doc = Document::new("t");
        let page = doc.add_page("p");
        let mut frames = Vec::new();
        for (i, name) in ["A", "B", "C"].iter().enumerate() {
            let id = doc.alloc_id();
            let mut f = Node::frame(id, Rect::new(i as f32 * 500.0, 0.0, 400.0, 800.0));
            f.name = (*name).into();
            frames.push(doc.insert(page, f).unwrap());
        }
        let (a, b, c) = (frames[0], frames[1], frames[2]);
        let id = doc.alloc_id();
        let btn = doc.insert(a, Node::rectangle(id, Rect::new(20.0, 700.0, 360.0, 48.0))).unwrap();
        let id = doc.alloc_id();
        let btn_b = doc.insert(b, Node::rectangle(id, Rect::new(20.0, 700.0, 360.0, 48.0))).unwrap();
        Fx { doc, a, b, c, btn, btn_b }
    }

    fn add(doc: &mut Document, node: NodeId, r: Reaction) {
        doc.get_mut(node).unwrap().prototype.reactions.push(r);
    }

    #[test]
    fn click_navigates_instantly_and_back_returns() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        add(&mut f.doc, f.btn_b, react(Trigger::OnClick, Action::Back, None));
        let mut p = Player::new(&f.doc, f.a);
        assert_eq!(p.current, f.a);
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.current, f.b);
        assert!(p.anim.is_none());
        assert_eq!(p.back.len(), 1);
        p.pointer_up(Some(f.btn_b), false);
        assert_eq!(p.current, f.a);
        assert!(p.back.is_empty(), "back pops, it does not push");
        p.pointer_up(Some(f.btn_b), false);
        assert_eq!(p.current, f.a, "back on an empty stack is a no-op");
    }

    #[test]
    fn dragged_release_is_not_a_click() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), true);
        assert_eq!(p.current, f.a);
    }

    #[test]
    fn click_bubbles_to_the_nearest_ancestor_with_a_reaction() {
        let mut f = fixture();
        add(&mut f.doc, f.a, react(Trigger::OnClick, Action::Navigate { dest: f.c, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.current, f.c);
    }

    #[test]
    fn back_stack_unwinds_in_order_with_reversed_transitions() {
        let mut f = fixture();
        let t = tr(TransitionKind::MoveIn(Direction::Left), 300);
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, t));
        add(&mut f.doc, f.btn_b, react(Trigger::OnClick, Action::Navigate { dest: f.c, preserve_scroll: false }, t));
        let back_btn = {
            let id = f.doc.alloc_id();
            f.doc.insert(f.c, Node::rectangle(id, Rect::new(0.0, 0.0, 50.0, 50.0))).unwrap()
        };
        add(&mut f.doc, back_btn, react(Trigger::OnClick, Action::Back, None));
        let mut p = Player::new(&f.doc, f.a);
        p.tick(1.0);
        p.pointer_up(Some(f.btn), false);
        p.tick(1.4);
        p.pointer_up(Some(f.btn_b), false);
        p.tick(2.0);
        assert_eq!(p.current, f.c);
        assert_eq!(p.back.iter().map(|e| e.frame).collect::<Vec<_>>(), vec![f.a, f.b]);
        p.pointer_up(Some(back_btn), false);
        assert_eq!(p.current, f.b);
        let a = p.anim.as_ref().expect("back plays the reversed transition");
        assert_eq!(a.kind, TransitionKind::MoveOut(Direction::Right));
        assert_eq!((a.from, a.to), (f.c, f.b));
    }

    #[test]
    fn transition_runs_for_its_duration_then_finishes() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, tr(TransitionKind::Dissolve, 400)));
        let mut p = Player::new(&f.doc, f.a);
        p.tick(10.0);
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.current, f.b);
        let a = p.anim.as_ref().unwrap();
        assert_eq!((a.from, a.to), (f.a, f.b));
        assert_eq!(a.raw(10.0), 0.0);
        p.tick(10.2);
        assert!((p.anim.as_ref().unwrap().raw(10.2) - 0.5).abs() < 1e-3);
        assert!(p.is_animating());
        p.tick(10.4);
        assert!(p.anim.is_none());
        p.tick(10.5);
        assert!(!p.is_animating());
    }

    #[test]
    fn a_new_event_during_a_transition_completes_it_first() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, tr(TransitionKind::Dissolve, 1000)));
        add(&mut f.doc, f.btn_b, react(Trigger::OnClick, Action::Navigate { dest: f.c, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        assert!(p.anim.is_some());
        p.pointer_up(Some(f.btn_b), false);
        assert_eq!(p.current, f.c);
        assert!(p.anim.is_none());
        assert_eq!(p.back.len(), 2);
    }

    #[test]
    fn spring_transitions_use_their_settle_time() {
        let mut f = fixture();
        let spring = Some(Transition {
            kind: TransitionKind::MoveIn(Direction::Left),
            easing: Easing::Spring { mass: 1.0, stiffness: 300.0, damping: 20.0 },
            duration_ms: 10,
        });
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, spring));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        assert!(p.anim.as_ref().unwrap().dur > 0.3);
    }

    #[test]
    fn after_delay_fires_once_the_frame_has_been_shown_long_enough() {
        let mut f = fixture();
        add(&mut f.doc, f.a, react(Trigger::AfterDelay { delay_ms: 1500 }, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.tick(0.0);
        p.restart(&f.doc, f.a);
        p.tick(1.0);
        assert_eq!(p.current, f.a);
        p.tick(1.6);
        assert_eq!(p.current, f.b);
        p.tick(10.0);
        assert_eq!(p.current, f.b, "timers fire once");
    }

    #[test]
    fn after_delay_is_cancelled_when_the_frame_is_left() {
        let mut f = fixture();
        add(&mut f.doc, f.a, react(Trigger::AfterDelay { delay_ms: 2000 }, Action::Navigate { dest: f.c, preserve_scroll: false }, None));
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.tick(0.0);
        p.pointer_up(Some(f.btn), false);
        p.tick(5.0);
        assert_eq!(p.current, f.b);
    }

    #[test]
    fn after_delay_restarts_when_returning_to_a_frame() {
        let mut f = fixture();
        add(&mut f.doc, f.b, react(Trigger::AfterDelay { delay_ms: 1000 }, Action::Back, None));
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.tick(0.0);
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.current, f.b);
        p.tick(1.1);
        assert_eq!(p.current, f.a);
        p.pointer_up(Some(f.btn), false);
        p.tick(1.5);
        assert_eq!(p.current, f.b, "fresh timer, not yet due");
        p.tick(2.3);
        assert_eq!(p.current, f.a);
    }

    fn overlay_fixture() -> (Fx, NodeId) {
        let mut f = fixture();
        let id = f.doc.alloc_id();
        let page = f.doc.pages[0];
        let mut o = Node::frame(id, Rect::new(2000.0, 0.0, 200.0, 100.0));
        o.name = "Modal".into();
        let modal = f.doc.insert(page, o).unwrap();
        let id = f.doc.alloc_id();
        let close = f.doc.insert(modal, Node::rectangle(id, Rect::new(0.0, 0.0, 30.0, 30.0))).unwrap();
        add(&mut f.doc, close, react(Trigger::OnClick, Action::CloseOverlay, None));
        add(
            &mut f.doc,
            f.btn,
            react(
                Trigger::OnClick,
                Action::OpenOverlay { dest: modal, position: OverlayPosition::Center, offset: Vec2::ZERO, close_on_click_outside: true, background: None },
                None,
            ),
        );
        (f, close)
    }

    #[test]
    fn overlay_opens_centered_and_closes_by_its_own_button() {
        let (f, close) = overlay_fixture();
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.overlays.len(), 1);
        assert_eq!(p.overlays[0].origin, Vec2::new(100.0, 350.0));
        assert_eq!(p.current, f.a, "the base stays");
        p.pointer_up(Some(close), false);
        assert!(p.overlays.is_empty());
    }

    #[test]
    fn click_outside_closes_overlay_and_swallows_the_click() {
        let (mut f, _) = overlay_fixture();
        add(&mut f.doc, f.a, react(Trigger::OnClick, Action::Navigate { dest: f.c, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.overlays.len(), 1);
        // click on the underlying frame: closes, does not navigate
        p.pointer_up(Some(f.a), false);
        assert!(p.overlays.is_empty());
        assert_eq!(p.current, f.a);
        p.pointer_up(Some(f.a), false);
        assert_eq!(p.current, f.c);
    }

    #[test]
    fn overlay_without_close_outside_stays_open() {
        let (mut f, _) = overlay_fixture();
        for a in &mut f.doc.get_mut(f.btn).unwrap().prototype.reactions[0].actions {
            if let Action::OpenOverlay { close_on_click_outside, .. } = a {
                *close_on_click_outside = false;
            }
        }
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        p.pointer_up(None, false);
        assert_eq!(p.overlays.len(), 1);
    }

    #[test]
    fn same_overlay_is_not_opened_twice_and_navigation_clears_overlays() {
        let (mut f, _) = overlay_fixture();
        let nav = f.doc.alloc_id();
        let nav = f.doc.insert(f.a, Node::rectangle(nav, Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        add(&mut f.doc, nav, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        p.overlays[0].close_outside = false;
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.overlays.len(), 1);
        p.pointer_up(Some(nav), false);
        assert!(p.overlays.is_empty());
        assert_eq!(p.current, f.b);
    }

    #[test]
    fn swap_overlay_replaces_in_place_keeping_position() {
        let (mut f, _) = overlay_fixture();
        let other = {
            let page = f.doc.pages[0];
            let id = f.doc.alloc_id();
            f.doc.insert(page, Node::frame(id, Rect::new(3000.0, 0.0, 100.0, 100.0))).unwrap()
        };
        let sw = f.doc.alloc_id();
        let modal = f.doc.get(f.btn).unwrap().prototype.reactions[0].actions[0].clone();
        let Action::OpenOverlay { dest: modal, .. } = modal else { panic!() };
        let sw = f.doc.insert(modal, Node::rectangle(sw, Rect::new(50.0, 50.0, 20.0, 20.0))).unwrap();
        add(&mut f.doc, sw, react(Trigger::OnClick, Action::SwapOverlay { dest: other }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        p.pointer_up(Some(sw), false);
        assert_eq!(p.overlays.len(), 1);
        assert_eq!(p.overlays[0].frame, other);
        assert_eq!(p.overlays[0].origin, Vec2::new(150.0, 350.0), "centered with the new size");
    }

    #[test]
    fn overlay_close_animation_keeps_it_until_done() {
        let (mut f, close) = overlay_fixture();
        f.doc.get_mut(close).unwrap().prototype.reactions[0].transition = None;
        let modal = f.doc.get(close).unwrap().parent.unwrap();
        // give the overlay an opening transition
        f.doc.get_mut(f.btn).unwrap().prototype.reactions[0].transition = tr(TransitionKind::Dissolve, 200);
        let mut p = Player::new(&f.doc, f.a);
        p.tick(1.0);
        p.pointer_up(Some(f.btn), false);
        assert!(p.overlays[0].anim.is_some());
        assert_eq!(p.overlays[0].shown(1.0), 0.0);
        p.tick(1.2);
        assert!(p.overlays[0].anim.is_none());
        assert_eq!(p.overlays[0].shown(1.2), 1.0);
        p.pointer_up(Some(close), false);
        assert_eq!(p.overlays.len(), 1);
        assert!(p.overlays[0].closing());
        assert_eq!(p.overlays[0].frame, modal);
        p.tick(1.3);
        assert!((p.overlays[0].shown(1.3) - 0.5).abs() < 1e-3);
        p.tick(1.5);
        assert!(p.overlays.is_empty());
    }

    #[test]
    fn hover_trigger_opens_overlay_and_closing_on_leave() {
        let (mut f, _) = overlay_fixture();
        let modal = match &f.doc.get(f.btn).unwrap().prototype.reactions[0].actions[0] {
            Action::OpenOverlay { dest, .. } => *dest,
            _ => unreachable!(),
        };
        f.doc.get_mut(f.btn).unwrap().prototype.reactions[0].trigger = Trigger::WhileHovering;
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_move(Some(f.btn));
        assert_eq!(p.overlays.len(), 1);
        assert_eq!(p.overlays[0].frame, modal);
        p.pointer_move(Some(f.btn));
        assert_eq!(p.overlays.len(), 1, "still hovering: nothing new");
        p.pointer_move(None);
        assert!(p.overlays.is_empty());
    }

    #[test]
    fn mouse_enter_with_delay_is_scheduled() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::MouseEnter { delay_ms: 500 }, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.tick(0.0);
        p.pointer_move(Some(f.btn));
        assert_eq!(p.current, f.a);
        p.tick(0.4);
        assert_eq!(p.current, f.a);
        p.tick(0.6);
        assert_eq!(p.current, f.b);
    }

    #[test]
    fn mouse_leave_fires_when_the_pointer_exits() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::MouseLeave { delay_ms: 0 }, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_move(Some(f.btn));
        assert_eq!(p.current, f.a);
        p.pointer_move(None);
        assert_eq!(p.current, f.b);
    }

    #[test]
    fn mouse_down_and_up_and_pressing() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::MouseDown { delay_ms: 0 }, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        add(&mut f.doc, f.btn_b, react(Trigger::MouseUp { delay_ms: 0 }, Action::Navigate { dest: f.c, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_down(Some(f.btn));
        assert_eq!(p.current, f.b);
        p.pointer_up(Some(f.btn_b), true);
        assert_eq!(p.current, f.c);
    }

    #[test]
    fn key_press_matches_any_order_and_case() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::KeyPress { keys: "Ctrl+K".into() }, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.key("Space");
        assert_eq!(p.current, f.a);
        p.key("k+ctrl");
        assert_eq!(p.current, f.b);
        assert!(keys_match("Shift+A", "a+shift"));
        assert!(!keys_match("", ""));
        assert!(!keys_match("A", "B"));
    }

    #[test]
    fn open_url_becomes_an_effect() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::OpenUrl { url: "https://example.com".into(), new_tab: true }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.effects, vec![Effect::OpenUrl { url: "https://example.com".into(), new_tab: true }]);
    }

    #[test]
    fn navigating_to_a_nested_layer_goes_to_its_frame() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.btn_b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.current, f.b);
        let mut g = fixture();
        add(&mut g.doc, g.btn, react(Trigger::OnClick, Action::Navigate { dest: NodeId(9999), preserve_scroll: false }, None));
        let mut p = Player::new(&g.doc, g.a);
        p.pointer_up(Some(g.btn), false);
        assert_eq!(p.current, g.a, "unknown destination is ignored");
    }

    fn scroll_fixture() -> (Fx, NodeId, NodeId) {
        let mut f = fixture();
        f.doc.get_mut(f.b).unwrap().prototype.overflow = Overflow::Vertical;
        let id = f.doc.alloc_id();
        let tall = f.doc.insert(f.b, Node::rectangle(id, Rect::new(0.0, 0.0, 400.0, 2000.0))).unwrap();
        let id = f.doc.alloc_id();
        let header = f.doc.insert(f.b, Node::rectangle(id, Rect::new(0.0, 0.0, 400.0, 60.0))).unwrap();
        f.doc.get_mut(f.b).unwrap().prototype.fixed_children.push(header);
        (f, tall, header)
    }

    #[test]
    fn scroll_range_ignores_fixed_children_and_non_scrollable_axes() {
        let (f, _tall, _) = scroll_fixture();
        let p = Player::new(&f.doc, f.b);
        assert_eq!(p.scroll_range(f.b), Vec2::new(0.0, 1200.0));
        assert_eq!(p.scroll_range(f.a), Vec2::ZERO, "overflow none never scrolls");
    }

    #[test]
    fn scroll_by_clamps_and_navigation_resets_it() {
        let (mut f, _tall, _) = scroll_fixture();
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.b);
        p.scroll_by(f.b, Vec2::new(50.0, 500.0));
        assert_eq!(p.scroll_of(f.b), Vec2::new(0.0, 500.0));
        p.scroll_by(f.b, Vec2::new(0.0, 5000.0));
        assert_eq!(p.scroll_of(f.b).y, 1200.0);
        p.scroll_by(f.b, Vec2::new(0.0, -9999.0));
        assert_eq!(p.scroll_of(f.b).y, 0.0);
        p.scroll_by(f.b, Vec2::new(0.0, 300.0));
        // navigate away and back without preserve: reset
        p.current = f.a;
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.scroll_of(f.b).y, 0.0);
    }

    #[test]
    fn preserve_scroll_keeps_the_offset() {
        let (mut f, _tall, _) = scroll_fixture();
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: true }, None));
        let mut p = Player::new(&f.doc, f.b);
        p.scroll_by(f.b, Vec2::new(0.0, 400.0));
        p.current = f.a;
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.scroll_of(f.b).y, 400.0);
    }

    #[test]
    fn scroll_to_moves_the_frame_so_the_target_is_at_the_offset() {
        let (mut f, tall, _) = scroll_fixture();
        let id = f.doc.alloc_id();
        f.doc.get_mut(tall).unwrap().kind = NodeKind::Frame(FrameProps::default());
        let target = f.doc.insert(tall, Node::rectangle(id, Rect::new(0.0, 900.0, 50.0, 50.0))).unwrap();
        add(&mut f.doc, f.btn_b, react(Trigger::OnClick, Action::ScrollTo { dest: target, offset: Vec2::new(0.0, 100.0) }, None));
        let mut p = Player::new(&f.doc, f.b);
        p.pointer_up(Some(f.btn_b), false);
        assert_eq!(p.scroll_of(f.b).y, 800.0);
    }

    #[test]
    fn animated_scroll_to_interpolates() {
        let (mut f, tall, _) = scroll_fixture();
        let id = f.doc.alloc_id();
        f.doc.get_mut(tall).unwrap().kind = NodeKind::Frame(FrameProps::default());
        let target = f.doc.insert(tall, Node::rectangle(id, Rect::new(0.0, 1000.0, 50.0, 50.0))).unwrap();
        add(&mut f.doc, f.btn_b, react(Trigger::OnClick, Action::ScrollTo { dest: target, offset: Vec2::ZERO }, tr(TransitionKind::Dissolve, 1000)));
        let mut p = Player::new(&f.doc, f.b);
        p.tick(0.0);
        p.pointer_up(Some(f.btn_b), false);
        p.tick(0.5);
        assert!((p.scroll_of(f.b).y - 500.0).abs() < 1.0);
        p.tick(1.0);
        assert_eq!(p.scroll_of(f.b).y, 1000.0);
        assert!(p.scroll_anim.is_none());
    }

    #[test]
    fn hit_layer_respects_scroll_and_fixed_children() {
        let (mut f, tall, header) = scroll_fixture();
        let id = f.doc.alloc_id();
        f.doc.get_mut(tall).unwrap().kind = NodeKind::Frame(FrameProps::default());
        let item = f.doc.insert(tall, Node::rectangle(id, Rect::new(10.0, 900.0, 50.0, 50.0))).unwrap();
        // not scrolled: header on top at y=30, the item is far below the frame
        assert_eq!(hit_layer(&f.doc, f.b, Vec2::new(20.0, 30.0), Vec2::ZERO), Some(header));
        // scrolled by 800: the item is at y=100 on screen, the header still at y=30
        assert_eq!(hit_layer(&f.doc, f.b, Vec2::new(20.0, 120.0), Vec2::new(0.0, 800.0)), Some(item));
        assert_eq!(hit_layer(&f.doc, f.b, Vec2::new(20.0, 30.0), Vec2::new(0.0, 800.0)), Some(header));
        assert_eq!(hit_layer(&f.doc, f.b, Vec2::new(-5.0, 30.0), Vec2::ZERO), None, "outside the frame");
    }

    #[test]
    fn hit_layer_ignores_hidden_layers() {
        let mut f = fixture();
        f.doc.get_mut(f.btn).unwrap().visible = false;
        assert_eq!(hit_layer(&f.doc, f.a, Vec2::new(30.0, 710.0), Vec2::ZERO), Some(f.a));
    }

    #[test]
    fn restart_resets_everything() {
        let mut f = fixture();
        add(&mut f.doc, f.btn, react(Trigger::OnClick, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.pointer_up(Some(f.btn), false);
        assert_eq!(p.current, f.b);
        p.restart(&f.doc, f.a);
        assert_eq!(p.current, f.a);
        assert!(p.back.is_empty() && p.overlays.is_empty() && p.anim.is_none());
    }

    #[test]
    fn rebasing_the_clock_keeps_timer_delays() {
        let mut f = fixture();
        add(&mut f.doc, f.a, react(Trigger::AfterDelay { delay_ms: 1000 }, Action::Navigate { dest: f.b, preserve_scroll: false }, None));
        let mut p = Player::new(&f.doc, f.a);
        p.rebase_clock(500.0);
        p.tick(500.5);
        assert_eq!(p.current, f.a, "not due 0.5s after the rebase");
        p.tick(501.1);
        assert_eq!(p.current, f.b);
    }

    fn variant_fixture() -> (Document, NodeId, NodeId, NodeId, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let id = d.alloc_id();
        let screen = d.insert(page, Node::frame(id, Rect::new(0.0, 0.0, 400.0, 800.0))).unwrap();
        let mut comps = Vec::new();
        for (i, c) in [Color::rgb(255, 0, 0), Color::rgb(0, 0, 255)].into_iter().enumerate() {
            let id = d.alloc_id();
            let mut f = Node::frame(id, Rect::new(1000.0 + i as f32 * 200.0, 0.0, 100.0, 40.0));
            f.name = format!("State={i}");
            f.fills = vec![Paint::solid(c)];
            let f = d.insert(page, f).unwrap();
            create_component(&mut d, f).apply(&mut d).unwrap();
            comps.push(f);
        }
        combine_as_variants(&mut d, &comps).apply(&mut d).unwrap();
        let cmd = create_instance(&mut d, comps[0], screen, Vec2::new(10.0, 10.0));
        let Command::Insert { nodes, .. } = &cmd else { panic!() };
        let inst = nodes[0].id;
        cmd.apply(&mut d).unwrap();
        (d, screen, inst, comps[0], comps[1])
    }

    #[test]
    fn change_to_swaps_the_variant_on_a_scratch_copy_only() {
        let (mut d, screen, inst, c1, c2) = variant_fixture();
        add(&mut d, inst, react(Trigger::OnClick, Action::ChangeTo { dest: c2 }, None));
        let mut p = Player::new(&d, screen);
        assert_eq!(p.doc.get(inst).and_then(instance_component), Some(c1));
        p.pointer_up(Some(inst), false);
        assert_eq!(p.doc.get(inst).and_then(instance_component), Some(c2));
        assert_eq!(d.get(inst).and_then(instance_component), Some(c1), "the user's document is untouched");
        assert_eq!(p.frame_rev(screen), 1, "textures of that frame are invalidated");
        // clicking a child of the instance finds the instance too
        let child = p.doc.get(inst).unwrap().children.first().copied();
        if let Some(child) = child {
            let mut q = Player::new(&d, screen);
            q.pointer_up(Some(child), false);
            assert_eq!(q.doc.get(inst).and_then(instance_component), Some(c2));
        }
    }

    #[test]
    fn change_to_while_hovering_reverts_on_leave() {
        let (mut d, screen, inst, c1, c2) = variant_fixture();
        add(&mut d, inst, react(Trigger::WhileHovering, Action::ChangeTo { dest: c2 }, None));
        let mut p = Player::new(&d, screen);
        p.pointer_move(Some(inst));
        assert_eq!(p.doc.get(inst).and_then(instance_component), Some(c2));
        p.pointer_move(None);
        assert_eq!(p.doc.get(inst).and_then(instance_component), Some(c1));
    }

    #[test]
    fn change_to_with_smart_animate_keeps_a_snapshot_of_the_old_look() {
        let (mut d, screen, inst, _c1, c2) = variant_fixture();
        add(&mut d, inst, react(Trigger::OnClick, Action::ChangeTo { dest: c2 }, tr(TransitionKind::SmartAnimate, 300)));
        let mut p = Player::new(&d, screen);
        p.pointer_up(Some(inst), false);
        let a = p.anim.as_ref().expect("animates in place");
        assert_eq!((a.from, a.to), (screen, screen));
        assert!(a.snapshot.is_some());
        p.tick(1.0);
        assert!(p.anim.is_none());
    }

    #[test]
    fn change_to_on_something_that_is_not_an_instance_does_nothing() {
        let (mut d, screen, _inst, _c1, c2) = variant_fixture();
        let id = d.alloc_id();
        let plain = d.insert(screen, Node::rectangle(id, Rect::new(0.0, 100.0, 10.0, 10.0))).unwrap();
        add(&mut d, plain, react(Trigger::OnClick, Action::ChangeTo { dest: c2 }, None));
        let mut p = Player::new(&d, screen);
        p.pointer_up(Some(plain), false);
        assert_eq!(p.frame_rev(screen), 0);
    }

    #[test]
    fn top_level_frames_lists_visible_frames() {
        let mut f = fixture();
        f.doc.get_mut(f.c).unwrap().visible = false;
        let page = f.doc.pages[0];
        assert_eq!(top_level_frames(&f.doc, page), vec![f.a, f.b]);
    }
}
