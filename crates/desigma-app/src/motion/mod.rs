//! Motion (Beta) mode: the timeline panel, the keyframe inspector and playback.

pub mod timeline;

#[derive(Default)]
pub struct MotionUi {
    pub enabled: bool, // Motion mode on (toolbar mode switch)
    pub playhead_ms: u32,
    pub playing: bool,
    pub selected_keys: Vec<desigma_doc::motion::edit::KeyRef>,
    pub selected_clip: Option<(desigma_doc::NodeId, u32)>, // (node, clip id)
    pub easing_target: bool,                               // segment selected -> inspector shows Easing panel
}
