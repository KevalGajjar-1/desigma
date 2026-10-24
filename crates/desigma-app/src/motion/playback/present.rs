//! Motion in Present: a frame with keyframes or animation presets plays its page's timeline from
//! the moment it is shown (looping when the timeline loops, holding the last state otherwise),
//! while the prototype keeps working: transitions, overlays and clicks use the same frames.
//!
//! Each animated frame gets its own [`MotionPlayer`] over the prototype player's scratch
//! document (rebuilt when that frame's content revision changes, e.g. after a variable change),
//! its time is the real clock since the frame appeared, quantised to the timeline's fps so a
//! texture is re-rendered at most once per timeline frame.

use desigma_doc::motion::MotionPlayer;
use desigma_doc::{Document, NodeId};
use std::collections::{HashMap, HashSet};

/// Per-frame evaluation state of one Present session.
#[derive(Default)]
pub struct PresentMotion {
    /// Egui time at which each frame first appeared.
    started: HashMap<NodeId, f64>,
    /// Frames asked for during the current egui frame.
    seen: HashSet<NodeId>,
    /// Whether a frame (or anything inside it) is animated.
    has: HashMap<NodeId, bool>,
    players: HashMap<NodeId, (u64, MotionPlayer)>,
    /// Something shown is still moving: keep repainting.
    pub active: bool,
}

/// The evaluated time of a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameTime {
    pub t_ms: f32,
    /// Index of the timeline frame, usable in a cache key.
    pub step: u64,
}

impl PresentMotion {
    pub fn begin_frame(&mut self) {
        self.seen.clear();
        self.active = false;
    }

    /// Frames that left the screen start over when they come back.
    pub fn end_frame(&mut self) {
        let seen = &self.seen;
        self.started.retain(|f, _| seen.contains(f));
        self.players.retain(|f, _| seen.contains(f));
    }

    /// Forgets everything (the session restarted).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn animated(&mut self, doc: &Document, frame: NodeId) -> bool {
        *self.has.entry(frame).or_insert_with(|| {
            let mut any = false;
            doc.walk(frame, |n, _| any |= !n.motion.is_empty());
            any
        })
    }

    /// The time to show `frame` at, or `None` when it has no animation.
    pub fn time_of(&mut self, doc: &Document, frame: NodeId, now: f64) -> Option<FrameTime> {
        if !self.animated(doc, frame) {
            return None;
        }
        let page = doc.page_of(frame)?;
        let tl = doc.timeline(page);
        self.seen.insert(frame);
        let start = *self.started.entry(frame).or_insert(now);
        let elapsed = ((now - start).max(0.0) * 1000.0) as f32;
        if !tl.finished(elapsed) {
            self.active = true;
        }
        let t = tl.playback_time(elapsed);
        let step = (t / tl.frame_ms()).floor().max(0.0) as u64;
        // a finished timeline shows its end state exactly
        let t_ms = if tl.finished(elapsed) { tl.duration_ms as f32 } else { tl.frame_time_ms(step as u32) };
        Some(FrameTime { t_ms, step })
    }

    /// `doc` (the prototype's scratch document, at content revision `rev`) with the page's
    /// animation evaluated at `t_ms`.
    pub fn evaluated(&mut self, doc: &Document, frame: NodeId, rev: u64, t_ms: f32) -> Option<&Document> {
        let page = doc.page_of(frame)?;
        let stale = self.players.get(&frame).is_none_or(|(r, _)| *r != rev);
        if stale {
            self.players.insert(frame, (rev, MotionPlayer::new(doc, page)));
        }
        let (_, p) = self.players.get_mut(&frame)?;
        Some(p.seek(doc, t_ms))
    }

    /// The most recently evaluated document of `frame` (for hit testing what is on screen).
    pub fn shown(&self, frame: NodeId) -> Option<&Document> {
        self.players.get(&frame).map(|(_, p)| p.document())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::motion::{AnimProperty, AnimValue, Ease, TimelineSettings};
    use desigma_doc::{Node, Rect, Vec2};

    fn doc(looping: bool) -> (Document, NodeId, NodeId, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("P");
        let frame = d.alloc_id();
        d.insert(page, Node::frame(frame, Rect::new(0.0, 0.0, 100.0, 100.0))).unwrap();
        let plain = d.alloc_id();
        d.insert(page, Node::frame(plain, Rect::new(200.0, 0.0, 100.0, 100.0))).unwrap();
        let id = d.alloc_id();
        let mut n = Node::rectangle(id, Rect::new(0.0, 0.0, 10.0, 10.0));
        let pos = n.motion.ensure_track(AnimProperty::Position);
        pos.set_key(0, AnimValue::Vec2(Vec2::new(0.0, 0.0)), Some(Ease::Linear));
        pos.set_key(1000, AnimValue::Vec2(Vec2::new(80.0, 0.0)), None);
        d.insert(frame, n).unwrap();
        d.timelines.insert(page, TimelineSettings { duration_ms: 1000, fps: 10, looping });
        (d, frame, plain, id)
    }

    #[test]
    fn frames_play_from_when_they_appear() {
        let (d, frame, plain, id) = doc(false);
        let mut m = PresentMotion::default();
        m.begin_frame();
        assert!(m.time_of(&d, plain, 5.0).is_none(), "frames without animation are drawn as before");
        let t0 = m.time_of(&d, frame, 5.0).unwrap();
        assert_eq!((t0.t_ms, t0.step), (0.0, 0));
        assert!(m.active);
        m.end_frame();
        m.begin_frame();
        let t = m.time_of(&d, frame, 5.43).unwrap();
        assert_eq!(t.step, 4, "quantised to the 10 fps timeline");
        assert!((t.t_ms - 400.0).abs() < 1e-3);
        let doc_t = m.evaluated(&d, frame, 0, t.t_ms).unwrap();
        assert!((doc_t.get(id).unwrap().transform.tx - 32.0).abs() < 1e-3);
        assert!(m.shown(frame).is_some());
        m.end_frame();
        m.begin_frame();
        let end = m.time_of(&d, frame, 9.0).unwrap();
        assert_eq!(end.t_ms, 1000.0, "a one-shot timeline holds its last state");
        assert!(!m.active, "nothing left to repaint for");
    }

    #[test]
    fn looping_timelines_never_finish_and_restart_when_the_frame_returns() {
        let (d, frame, _p, _id) = doc(true);
        let mut m = PresentMotion::default();
        m.begin_frame();
        m.time_of(&d, frame, 1.0);
        m.end_frame();
        m.begin_frame();
        let t = m.time_of(&d, frame, 3.25).unwrap();
        assert!(m.active);
        assert!((t.t_ms - 200.0).abs() < 1e-3, "2.25 s into a 1 s loop is 250 ms, shown at the 200 ms frame: {}", t.t_ms);
        m.end_frame();
        // the frame was not drawn for a while: its clock restarts
        m.begin_frame();
        m.end_frame();
        m.begin_frame();
        let again = m.time_of(&d, frame, 10.0).unwrap();
        assert_eq!(again.step, 0);
    }

    #[test]
    fn evaluation_is_rebuilt_when_the_content_revision_changes() {
        let (mut d, frame, _p, id) = doc(false);
        let mut m = PresentMotion::default();
        m.begin_frame();
        m.time_of(&d, frame, 0.0);
        assert!(m.evaluated(&d, frame, 1, 500.0).is_some());
        d.get_mut(id).unwrap().name = "changed".into();
        assert_ne!(m.evaluated(&d, frame, 1, 500.0).unwrap().get(id).unwrap().name, "changed", "same revision: cached player");
        assert_eq!(m.evaluated(&d, frame, 2, 500.0).unwrap().get(id).unwrap().name, "changed");
    }
}
