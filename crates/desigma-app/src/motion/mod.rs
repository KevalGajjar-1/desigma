//! Motion (Beta) mode: the editor-side state shared by the timeline panel, the inspector and
//! playback. The animation model itself lives in `desigma_doc::motion`.

pub mod playback;

/// Motion mode UI state. Created identically by the timeline panel, the inspector and playback.
#[derive(Default)]
pub struct MotionUi {
    /// Motion mode is on: the canvas shows the page evaluated at `playhead_ms`.
    pub enabled: bool,
    /// Playhead position in ms.
    pub playhead_ms: u32,
    /// The timeline is playing (the play button sets this; playback clears it at the end).
    pub playing: bool,
    pub selected_keys: Vec<desigma_doc::motion::edit::KeyRef>,
    pub selected_clip: Option<(desigma_doc::NodeId, u32)>,
    pub easing_target: bool,
}
