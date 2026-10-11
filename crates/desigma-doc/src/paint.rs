use crate::Vec2;
use serde::{Deserialize, Serialize};

/// Straight (non-premultiplied) sRGB color, components 0..=1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: a as f32 / 255.0 }
    }
    /// Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA`.
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim_start_matches('#');
        let h = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            3 => {
                let v: Vec<u8> = s.chars().map(|c| c.to_digit(16).map(|d| d as u8 * 17)).collect::<Option<_>>()?;
                Some(Self::rgb(v[0], v[1], v[2]))
            }
            6 => Some(Self::rgb(h(0)?, h(2)?, h(4)?)),
            8 => Some(Self::rgba(h(0)?, h(2)?, h(4)?, h(6)?)),
            _ => None,
        }
    }
    pub fn to_hex(&self) -> String {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("{:02X}{:02X}{:02X}", c(self.r), c(self.g), c(self.b))
    }
    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    PassThrough,
    Darken,
    Multiply,
    ColorBurn,
    LinearBurn,
    Lighten,
    Screen,
    ColorDodge,
    LinearDodge,
    Overlay,
    SoftLight,
    HardLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub pos: f32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageScaleMode {
    #[default]
    Fill,
    Fit,
    Crop,
    Tile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PaintKind {
    Solid(Color),
    /// Handles in node-normalized space (0..1 of the node's size), Figma convention:
    /// [start, end, width-direction].
    LinearGradient {
        handles: [Vec2; 3],
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        handles: [Vec2; 3],
        stops: Vec<GradientStop>,
    },
    AngularGradient {
        handles: [Vec2; 3],
        stops: Vec<GradientStop>,
    },
    DiamondGradient {
        handles: [Vec2; 3],
        stops: Vec<GradientStop>,
    },
    /// `image_ref` keys into [`crate::Document::images`].
    Image {
        image_ref: String,
        scale_mode: ImageScaleMode,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Paint {
    pub kind: PaintKind,
    pub opacity: f32,
    pub visible: bool,
    pub blend: BlendMode,
}

impl Paint {
    pub fn solid(c: Color) -> Self {
        Self { kind: PaintKind::Solid(c), opacity: 1.0, visible: true, blend: BlendMode::Normal }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeAlign {
    #[default]
    Inside,
    Center,
    Outside,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeCap {
    #[default]
    None,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeJoin {
    #[default]
    Miter,
    Bevel,
    Round,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub paints: Vec<Paint>,
    pub weight: f32,
    /// Per-side weights (top, right, bottom, left) for rectangles/frames.
    pub side_weights: Option<[f32; 4]>,
    pub align: StrokeAlign,
    pub cap: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f32,
    pub dashes: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Effect {
    DropShadow { color: Color, offset: Vec2, radius: f32, spread: f32, visible: bool, show_behind: bool },
    InnerShadow { color: Color, offset: Vec2, radius: f32, spread: f32, visible: bool },
    LayerBlur { radius: f32, visible: bool },
    BackgroundBlur { radius: f32, visible: bool },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlignH {
    #[default]
    Left,
    Center,
    Right,
    Justified,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlignV {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAutoResize {
    #[default]
    None,
    WidthAndHeight,
    Height,
    Truncate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextCase {
    #[default]
    Original,
    Upper,
    Lower,
    Title,
    SmallCaps,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    Strikethrough,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum LineHeight {
    /// The font's natural line height (ascent + descent + line gap).
    Auto,
    Pixels(f32),
    /// Percent of the font size: `Percent(120.0)` = 1.2 × font_size.
    Percent(f32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font_family: String,
    /// CSS weight 100..=900.
    pub font_weight: u16,
    pub italic: bool,
    pub font_size: f32,
    pub line_height: LineHeight,
    /// Letter spacing in px.
    pub letter_spacing: f32,
    pub paragraph_spacing: f32,
    pub align_h: TextAlignH,
    pub align_v: TextAlignV,
    pub case: TextCase,
    pub decoration: TextDecoration,
    /// Text colour lives here; `Node::fills` is unused for text nodes.
    pub fills: Vec<Paint>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: "Inter".into(),
            font_weight: 400,
            italic: false,
            font_size: 12.0,
            line_height: LineHeight::Auto,
            letter_spacing: 0.0,
            paragraph_spacing: 0.0,
            align_h: TextAlignH::Left,
            align_v: TextAlignV::Top,
            case: TextCase::Original,
            decoration: TextDecoration::None,
            fills: vec![Paint::solid(Color::BLACK)],
        }
    }
}

/// A run of characters (byte range into the text) with style overrides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    pub start: usize,
    pub end: usize,
    pub style: TextStyle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum LayoutGrid {
    Grid { size: f32, color: Color },
    Columns { count: u32, gutter: f32, offset: f32, color: Color },
    Rows { count: u32, gutter: f32, offset: f32, color: Color },
}
