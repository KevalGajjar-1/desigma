//! Real text measurement for the editor (caret placement, auto-resize).
//!
//! `desigma_render` keeps its text layout private, so the app shapes text with
//! cosmic-text directly, using the same font set as the renderer: the bundled
//! Inter plus system fonts and anything registered at runtime.

use cosmic_text::{fontdb, Align, Attrs, Buffer, Family, FontSystem, Hinting, Metrics, Shaping, Style, Weight, Wrap};
use desigma_doc as d;
use desigma_doc::{TextProps, Vec2};
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

struct Fonts {
    fs: FontSystem,
    families: HashSet<String>,
}

thread_local! {
    static FONTS: RefCell<Option<Fonts>> = const { RefCell::new(None) };
}

fn family_names(fs: &FontSystem) -> HashSet<String> {
    fs.db().faces().flat_map(|f| f.families.iter().map(|(n, _)| n.to_lowercase())).collect()
}

fn init() -> Fonts {
    let mut db = fontdb::Database::new();
    let mut bundled = HashSet::new();
    let mut owned = HashSet::new();
    for asset in desigma_fonts::bundled() {
        owned.insert(asset.family.to_lowercase());
        bundled.extend(db.load_font_source(fontdb::Source::Binary(Arc::new(asset.data))));
    }
    db.load_system_fonts();
    let shadowed: Vec<fontdb::ID> = db
        .faces()
        .filter(|f| !bundled.contains(&f.id) && f.families.iter().any(|(n, _)| owned.contains(&n.to_lowercase())))
        .map(|f| f.id)
        .collect();
    for id in shadowed {
        db.remove_face(id);
    }
    db.set_sans_serif_family("Inter");
    let fs = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
    let families = family_names(&fs);
    Fonts { fs, families }
}

fn with_fonts<R>(f: impl FnOnce(&mut Fonts) -> R) -> R {
    FONTS.with(|cell| {
        let mut slot = cell.borrow_mut();
        let fonts = slot.get_or_insert_with(init);
        f(fonts)
    })
}

/// Makes a font fetched at runtime known to the measurer (mirrors `desigma_render::register_font`).
pub fn register_font(data: Vec<u8>) {
    with_fonts(|f| {
        f.fs.db_mut().load_font_data(data);
        f.families = family_names(&f.fs);
    });
}

fn family_for(fonts: &Fonts, requested: &str) -> Option<String> {
    if fonts.families.contains(&requested.to_lowercase()) {
        return Some(requested.to_string());
    }
    let sub = desigma_fonts::substitute(requested)?;
    fonts.families.contains(&sub.to_lowercase()).then(|| sub.to_string())
}

fn auto_ratio(fonts: &mut Fonts, st: &d::TextStyle, family: Option<&str>) -> f32 {
    let fam = match family {
        Some(n) => fontdb::Family::Name(n),
        None => fontdb::Family::SansSerif,
    };
    let q = fontdb::Query {
        families: &[fam],
        weight: fontdb::Weight(st.font_weight),
        stretch: fontdb::Stretch::Normal,
        style: if st.italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
    };
    let Some(id) = fonts.fs.db().query(&q) else { return 1.2 };
    let Some(font) = fonts.fs.get_font(id, fontdb::Weight(st.font_weight)) else { return 1.2 };
    let m = font.as_swash().metrics(&[]);
    let upem = m.units_per_em as f32;
    let r = if upem > 0.0 { (m.ascent + m.descent + m.leading) / upem } else { 1.2 };
    if r.is_finite() && r > 0.5 {
        r
    } else {
        1.2
    }
}

/// Line height of the base style in px.
pub fn line_height(props: &TextProps) -> f32 {
    with_fonts(|f| {
        let st = &props.style;
        match st.line_height {
            d::LineHeight::Pixels(p) => p,
            d::LineHeight::Percent(p) => st.font_size * p / 100.0,
            d::LineHeight::Auto => {
                let fam = family_for(f, &st.font_family);
                auto_ratio(f, st, fam.as_deref()) * st.font_size
            }
        }
    })
}

fn shaped_text(props: &TextProps) -> String {
    match props.style.case {
        d::TextCase::Upper | d::TextCase::SmallCaps => props.content.to_uppercase(),
        d::TextCase::Lower => props.content.to_lowercase(),
        _ => props.content.clone(),
    }
}

/// Runs `f` on the shaped buffer of `props`. `width` wraps the text (None = no wrap).
fn with_buffer<R>(props: &TextProps, width: Option<f32>, f: impl FnOnce(&mut Buffer, f32) -> R) -> R {
    let lh = line_height(props);
    with_fonts(|fonts| {
        let st = &props.style;
        let family = family_for(fonts, &st.font_family);
        let fsize = st.font_size.max(0.1);
        let fam = match &family {
            Some(n) => Family::Name(n),
            None => Family::SansSerif,
        };
        let mut attrs = Attrs::new().family(fam).weight(Weight(st.font_weight)).style(if st.italic { Style::Italic } else { Style::Normal }).metrics(Metrics::new(fsize, lh));
        if st.letter_spacing != 0.0 {
            attrs = attrs.letter_spacing(st.letter_spacing / fsize);
        }
        let mut buffer = Buffer::new(&mut fonts.fs, Metrics::new(fsize, lh));
        buffer.set_hinting(Hinting::Disabled);
        buffer.set_size(width, None);
        buffer.set_wrap(if width.is_some() { Wrap::WordOrGlyph } else { Wrap::None });
        let align = match st.align_h {
            d::TextAlignH::Left => Align::Left,
            d::TextAlignH::Center => Align::Center,
            d::TextAlignH::Right => Align::Right,
            d::TextAlignH::Justified => Align::Justified,
        };
        buffer.set_text(&shaped_text(props), &attrs, Shaping::Advanced, Some(align));
        buffer.shape_until_scroll(&mut fonts.fs, false);
        f(&mut buffer, lh)
    })
}

/// Natural size of the text: widest line by total height. `width` wraps lines.
pub fn measure(props: &TextProps, width: Option<f32>) -> Vec2 {
    with_buffer(props, width, |b, lh| {
        let (mut w, mut bottom, mut lines) = (0.0f32, 0.0f32, 0usize);
        for run in b.layout_runs() {
            w = w.max(run.line_w);
            bottom = bottom.max(run.line_top + run.line_height);
            lines += 1;
        }
        if lines == 0 {
            bottom = lh;
        }
        // Paragraph spacing is added between source lines.
        let paragraphs = props.content.matches('\n').count() as f32;
        Vec2::new(w.ceil().max(1.0), (bottom + paragraphs * props.style.paragraph_spacing).ceil().max(1.0))
    })
}

/// Caret rectangle (x, y, height) in node-local space for a char-index caret.
pub fn caret_rect(props: &TextProps, width: Option<f32>, caret: usize) -> (f32, f32, f32) {
    let content = shaped_text(props);
    let (line, byte) = line_and_byte(&content, caret);
    with_buffer(props, width, |b, lh| {
        let mut best: Option<(f32, f32, f32)> = None;
        for run in b.layout_runs() {
            if run.line_i != line {
                continue;
            }
            let (x, found) = match (run.glyphs.first(), run.glyphs.last()) {
                (Some(first), Some(last)) => {
                    if byte < first.start {
                        (first.x, true)
                    } else if let Some(g) = run.glyphs.iter().find(|g| byte >= g.start && byte < g.end) {
                        (g.x, true)
                    } else if byte >= last.end {
                        (last.x + last.w, true)
                    } else {
                        (last.x + last.w, false)
                    }
                }
                _ => (0.0, true),
            };
            let cand = (x, run.line_top, run.line_height);
            if found {
                best = Some(cand);
                // A caret at a wrap boundary belongs to the next visual line.
                if run.glyphs.last().map(|l| byte < l.end).unwrap_or(true) {
                    break;
                }
            } else if best.is_none() {
                best = Some(cand);
            }
        }
        best.unwrap_or((0.0, 0.0, lh))
    })
}

/// Char index nearest to the node-local point (x, y).
pub fn hit_caret(props: &TextProps, width: Option<f32>, x: f32, y: f32) -> usize {
    let content = shaped_text(props);
    let total = content.chars().count();
    with_buffer(props, width, |b, _| {
        let Some(c) = b.hit(x, y) else { return total };
        let mut idx = 0;
        for (li, l) in content.split('\n').enumerate() {
            if li == c.line {
                let in_line = l.char_indices().take_while(|(bi, _)| *bi < c.index).count();
                return idx + in_line;
            }
            idx += l.chars().count() + 1;
        }
        total
    })
}

/// (paragraph index, byte offset inside that paragraph) of a char-index caret.
fn line_and_byte(content: &str, caret: usize) -> (usize, usize) {
    let mut remaining = caret;
    for (li, l) in content.split('\n').enumerate() {
        let n = l.chars().count();
        if remaining <= n {
            let byte = l.char_indices().nth(remaining).map(|(b, _)| b).unwrap_or(l.len());
            return (li, byte);
        }
        remaining -= n + 1;
    }
    (0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(s: &str, size: f32) -> TextProps {
        TextProps {
            content: s.into(),
            style: d::TextStyle { font_size: size, ..Default::default() },
            runs: vec![],
            auto_resize: d::TextAutoResize::WidthAndHeight,
            max_lines: None,
        }
    }

    #[test]
    fn width_grows_with_content_and_size() {
        let a = measure(&props("Hi", 16.0), None);
        let b = measure(&props("Hello world", 16.0), None);
        let c = measure(&props("Hello world", 32.0), None);
        assert!(b.x > a.x && c.x > b.x * 1.5);
        assert!(a.y >= 16.0 && a.y < 30.0);
        // Proportional font: 'i' is narrower than 'W'.
        assert!(measure(&props("iiii", 16.0), None).x < measure(&props("WWWW", 16.0), None).x);
    }

    #[test]
    fn lines_and_wrapping() {
        let one = measure(&props("abc", 16.0), None);
        let two = measure(&props("abc\ndef", 16.0), None);
        assert!(two.y > one.y * 1.8);
        let wrapped = measure(&props("one two three four five six", 16.0), Some(60.0));
        assert!(wrapped.y > one.y * 2.0 && wrapped.x <= 60.5);
        assert!(measure(&props("", 16.0), None).y >= 16.0);
    }

    #[test]
    fn caret_follows_text() {
        let p = props("Hello\nWorld", 16.0);
        let (x0, y0, h) = caret_rect(&p, None, 0);
        let (x3, _, _) = caret_rect(&p, None, 3);
        let (xe, _, _) = caret_rect(&p, None, 5);
        let (x6, y6, _) = caret_rect(&p, None, 6);
        assert_eq!(x0, 0.0);
        assert!(x3 > x0 && xe > x3);
        assert_eq!(x6, 0.0);
        assert!(y6 >= y0 + h - 0.5);
    }

    #[test]
    fn hit_roundtrips_caret() {
        let p = props("Hello\nWorld", 16.0);
        for caret in [0usize, 2, 5, 6, 9, 11] {
            let (x, y, h) = caret_rect(&p, None, caret);
            assert_eq!(hit_caret(&p, None, x + 0.5, y + h / 2.0), caret, "caret {caret}");
        }
    }
}
