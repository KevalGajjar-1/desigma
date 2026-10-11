//! sRGB colors and paints.

use serde::{Deserialize, Serialize};

/// Non-premultiplied sRGB color with components in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    /// Figma's default shape fill (#D9D9D9).
    pub const DEFAULT_SHAPE: Self = Self::from_rgb8(0xD9, 0xD9, 0xD9);
    /// Figma's default canvas background (#F5F5F5) in light mode.
    pub const CANVAS_LIGHT: Self = Self::from_rgb8(0xF5, 0xF5, 0xF5);
    /// Figma's default canvas background (#1E1E1E) in dark mode.
    pub const CANVAS_DARK: Self = Self::from_rgb8(0x1E, 0x1E, 0x1E);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn from_rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    pub fn to_rgba8(self) -> [u8; 4] {
        let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [c(self.r), c(self.g), c(self.b), c(self.a)]
    }

    /// Uppercase hex without alpha, as shown in Figma's color fields (`D9D9D9`).
    pub fn to_hex(self) -> String {
        let [r, g, b, _] = self.to_rgba8();
        format!("{r:02X}{g:02X}{b:02X}")
    }

    /// Parses `RGB`, `RRGGBB` or `RRGGBBAA`, with or without a leading `#`.
    /// Keeps the current alpha when the input has none.
    pub fn parse_hex(s: &str, alpha: f32) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            3 => {
                let mut out = [0u8; 3];
                for (i, ch) in s.chars().enumerate() {
                    let v = ch.to_digit(16)? as u8;
                    out[i] = v * 17;
                }
                Some(Self::from_rgb8(out[0], out[1], out[2]).with_alpha(alpha))
            }
            6 => Some(Self::from_rgb8(byte(0)?, byte(2)?, byte(4)?).with_alpha(alpha)),
            8 => Some(
                Self::from_rgb8(byte(0)?, byte(2)?, byte(4)?).with_alpha(byte(6)? as f32 / 255.0),
            ),
            _ => None,
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }
}

/// A fill or stroke paint. Only solid colors for now; gradients and images
/// slot in here later the same way Figma models them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Paint {
    Solid {
        color: Color,
        #[serde(default = "default_true")]
        visible: bool,
    },
}

fn default_true() -> bool {
    true
}

impl Paint {
    pub fn solid(color: Color) -> Self {
        Paint::Solid {
            color,
            visible: true,
        }
    }

    pub fn is_visible(&self) -> bool {
        match self {
            Paint::Solid { visible, .. } => *visible,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = Color::parse_hex("#0d99ff", 1.0).unwrap();
        assert_eq!(c.to_hex(), "0D99FF");
        assert_eq!(Color::parse_hex("fff", 1.0).unwrap().to_hex(), "FFFFFF");
        assert_eq!(
            Color::parse_hex("00000080", 1.0).unwrap().to_rgba8()[3],
            128
        );
        assert!(Color::parse_hex("zz", 1.0).is_none());
    }
}
