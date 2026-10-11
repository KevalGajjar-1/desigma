//! Prototype interactions (Figma "Prototype" tab): triggers, actions and
//! transitions attached to nodes, plus flow starting points per page.

use crate::{NodeId, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Trigger {
    OnClick,
    OnDrag,
    WhileHovering,
    WhilePressing,
    MouseEnter {
        delay_ms: u32,
    },
    MouseLeave {
        delay_ms: u32,
    },
    MouseDown {
        delay_ms: u32,
    },
    MouseUp {
        delay_ms: u32,
    },
    AfterDelay {
        delay_ms: u32,
    },
    /// Key or gamepad press, e.g. "Space", "Ctrl+K".
    KeyPress {
        keys: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    EaseInBack,
    EaseOutBack,
    EaseInOutBack,
    /// CSS-style cubic-bezier control points.
    Bezier([f32; 4]),
    Spring {
        mass: f32,
        stiffness: f32,
        damping: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    Dissolve,
    SmartAnimate,
    MoveIn(Direction),
    MoveOut(Direction),
    Push(Direction),
    SlideIn(Direction),
    SlideOut(Direction),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransitionKind,
    pub easing: Easing,
    pub duration_ms: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayPosition {
    #[default]
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    /// Relative to the triggering layer, offset by `Overlay::offset`.
    Manual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Go to another top-level frame. `preserve_scroll` keeps scroll position.
    Navigate {
        dest: NodeId,
        preserve_scroll: bool,
    },
    OpenOverlay {
        dest: NodeId,
        position: OverlayPosition,
        offset: Vec2,
        close_on_click_outside: bool,
        background: Option<crate::Color>,
    },
    SwapOverlay {
        dest: NodeId,
    },
    CloseOverlay,
    Back,
    /// Scroll a scrollable ancestor so `dest` is visible.
    ScrollTo {
        dest: NodeId,
        offset: Vec2,
    },
    OpenUrl {
        url: String,
        new_tab: bool,
    },
    /// Change an instance to another variant of its component set.
    ChangeTo {
        dest: NodeId,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    pub trigger: Trigger,
    pub actions: Vec<Action>,
    /// `None` means instant.
    pub transition: Option<Transition>,
}

/// A prototype flow: a named starting frame on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub name: String,
    pub page: NodeId,
    pub start: NodeId,
    pub description: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    #[default]
    None,
    Horizontal,
    Vertical,
    Both,
}

/// Prototype settings for a frame.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PrototypeProps {
    pub reactions: Vec<Reaction>,
    /// Scroll behaviour when content overflows (frames only).
    pub overflow: Overflow,
    /// Children that stay fixed while the frame scrolls.
    pub fixed_children: Vec<NodeId>,
}

impl PrototypeProps {
    pub fn is_empty(&self) -> bool {
        self.reactions.is_empty() && self.overflow == Overflow::None && self.fixed_children.is_empty()
    }
}
