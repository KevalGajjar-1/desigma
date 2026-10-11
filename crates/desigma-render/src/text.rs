//! Bundled fonts and a simple left-to-right text layout (no shaping yet).

use std::sync::Arc;

use desigma_core::{TextAlign, TextData};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};
use vello::peniko::{Blob, FontData};

static INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
static INTER_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
static INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");

/// Raw bytes of the bundled Inter faces, for UI toolkits that want the same font.
pub fn inter_regular_bytes() -> &'static [u8] {
    INTER_REGULAR
}

pub fn inter_medium_bytes() -> &'static [u8] {
    INTER_MEDIUM
}

pub fn inter_semibold_bytes() -> &'static [u8] {
    INTER_SEMIBOLD
}

pub struct Face {
    pub data: FontData,
    bytes: &'static [u8],
}

impl Face {
    fn new(bytes: &'static [u8]) -> Self {
        Self {
            data: FontData::new(Blob::new(Arc::new(bytes)), 0),
            bytes,
        }
    }

    fn font_ref(&self) -> FontRef<'static> {
        FontRef::new(self.bytes).expect("bundled font is valid")
    }
}

pub struct Fonts {
    regular: Face,
    medium: Face,
    semibold: Face,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            regular: Face::new(INTER_REGULAR),
            medium: Face::new(INTER_MEDIUM),
            semibold: Face::new(INTER_SEMIBOLD),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Default)]
pub struct TextLayout {
    pub glyphs: Vec<PlacedGlyph>,
    pub width: f64,
    pub height: f64,
}

impl Fonts {
    pub fn face(&self, weight: u16) -> &Face {
        match weight {
            0..=449 => &self.regular,
            450..=549 => &self.medium,
            _ => &self.semibold,
        }
    }

    /// Lays out `text`. When `wrap_width` is set, words wrap greedily at that width.
    pub fn layout(&self, text: &TextData, wrap_width: Option<f64>) -> TextLayout {
        let font = self.face(text.font_weight).font_ref();
        let size = text.font_size.max(1.0) as f32;
        let charmap = font.charmap();
        let gm = font.glyph_metrics(Size::new(size), LocationRef::default());
        let metrics = font.metrics(Size::new(size), LocationRef::default());
        let line_height = text.resolved_line_height() as f32;
        // Center the font's ascent+descent box inside each line, as Figma does.
        let content = metrics.ascent - metrics.descent;
        let baseline_offset = (line_height - content) / 2.0 + metrics.ascent;

        let advance = |ch: char| -> (u32, f32) {
            let gid = charmap.map(ch).unwrap_or_default();
            (gid.to_u32(), gm.advance_width(gid).unwrap_or(size * 0.5))
        };

        // Break into visual lines of (char, advance) runs.
        let mut lines: Vec<Vec<(u32, f32)>> = vec![];
        for para in text.content.split('\n') {
            let mut line: Vec<(u32, f32)> = vec![];
            let mut line_w = 0.0f32;
            for word in split_keep_spaces(para) {
                let glyphs: Vec<(u32, f32)> = word.chars().map(advance).collect();
                let word_w: f32 = glyphs.iter().map(|g| g.1).sum();
                if let Some(max) = wrap_width {
                    let is_space = word.chars().all(char::is_whitespace);
                    if !line.is_empty() && !is_space && (line_w + word_w) as f64 > max {
                        // Drop trailing spaces before wrapping.
                        lines.push(std::mem::take(&mut line));
                        line_w = 0.0;
                    }
                }
                line_w += word_w;
                line.extend(glyphs);
            }
            lines.push(line);
        }

        let mut out = TextLayout::default();
        let line_widths: Vec<f32> = lines.iter().map(|l| l.iter().map(|g| g.1).sum()).collect();
        let max_w = line_widths.iter().copied().fold(0.0f32, f32::max);
        let box_w = wrap_width.map(|w| w as f32).unwrap_or(max_w);
        for (i, line) in lines.iter().enumerate() {
            let lw = line_widths[i];
            let mut x = match text.align {
                TextAlign::Left => 0.0,
                TextAlign::Center => (box_w - lw) / 2.0,
                TextAlign::Right => box_w - lw,
            };
            let y = i as f32 * line_height + baseline_offset;
            for (id, adv) in line {
                out.glyphs.push(PlacedGlyph { id: *id, x, y });
                x += adv;
            }
        }
        out.width = max_w.ceil() as f64;
        out.height = (lines.len() as f32 * line_height) as f64;
        out
    }

    /// Size of a text box laid out without wrapping (Figma's "Auto width").
    pub fn measure(&self, text: &TextData) -> (f64, f64) {
        let l = self.layout(text, None);
        (l.width.max(1.0), l.height)
    }
}

/// Splits into alternating runs of non-space and space characters.
fn split_keep_spaces(s: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    let mut prev_space: Option<bool> = None;
    for (i, ch) in s.char_indices() {
        let sp = ch.is_whitespace();
        if prev_space.is_some_and(|p| p != sp) {
            out.push(&s[start..i]);
            start = i;
        }
        prev_space = Some(sp);
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_grows_with_content() {
        let fonts = Fonts::default();
        let (w1, h1) = fonts.measure(&TextData::new("Hi"));
        let (w2, _) = fonts.measure(&TextData::new("Hello world"));
        assert!(w2 > w1);
        assert!((h1 - 14.4).abs() < 0.01);
        let (_, h3) = fonts.measure(&TextData::new("a\nb"));
        assert!((h3 - 28.8).abs() < 0.01);
    }

    #[test]
    fn wrapping_adds_lines() {
        let fonts = Fonts::default();
        let t = TextData::new("one two three four five");
        let narrow = fonts.layout(&t, Some(40.0));
        assert!(narrow.height > fonts.layout(&t, None).height);
    }

    #[test]
    fn split_spaces() {
        assert_eq!(split_keep_spaces("ab  cd"), vec!["ab", "  ", "cd"]);
    }
}
