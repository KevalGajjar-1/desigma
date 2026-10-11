//! Text layout for SVG export (cosmic-text). Mirrors the renderer's layout
//! (same segments, metrics and wrapping) but keeps glyph runs per line so the
//! writer can emit either `<text>` lines or outlined glyph paths.

use cosmic_text::{
    fontdb, Align, Attrs, Buffer, CacheKeyFlags, Command, Family, FontSystem, Hinting, Metrics, Shaping, Style, SwashCache,
    TextDecoration as CtDecoration, UnderlineStyle, Weight, Wrap,
};
use desigma_doc as d;
use desigma_doc::{PathCmd, TextAutoResize, TextProps, Vec2};
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

struct Fonts {
    fs: FontSystem,
    swash: SwashCache,
    families: HashSet<String>,
}

thread_local! {
    static FONTS: RefCell<Option<Fonts>> = const { RefCell::new(None) };
}

fn family_names(fs: &FontSystem) -> HashSet<String> {
    let mut out = HashSet::new();
    for face in fs.db().faces() {
        for (name, _) in &face.families {
            out.insert(name.to_lowercase());
        }
    }
    out
}

fn init_fonts() -> Fonts {
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
    Fonts { fs, swash: SwashCache::new(), families }
}

fn with_fonts<R>(f: impl FnOnce(&mut Fonts) -> R) -> R {
    FONTS.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(init_fonts());
        }
        f(slot.as_mut().unwrap())
    })
}

/// Makes an extra font (e.g. one downloaded by `desigma-fonts`) available to
/// outlined text export. Mirrors `desigma_render::register_font`.
pub fn register_font(data: Vec<u8>) {
    with_fonts(|f| {
        f.fs.db_mut().load_font_data(data);
        f.families = family_names(&f.fs);
    });
}

fn resolve_family(fonts: &Fonts, requested: &str) -> Option<String> {
    if fonts.families.contains(&requested.to_lowercase()) {
        return Some(requested.to_string());
    }
    let sub = desigma_fonts::substitute(requested)?;
    fonts.families.contains(&sub.to_lowercase()).then(|| sub.to_string())
}

/// A run of glyphs of one style on one line.
#[derive(Clone, Debug)]
pub struct Span {
    /// Index into [`TextOut::styles`].
    pub style: usize,
    pub x: f32,
    pub text: String,
    /// Glyph outlines (and underline/strike bars) in node-local space.
    pub outline: Vec<PathCmd>,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub baseline: f32,
    pub spans: Vec<Span>,
}

#[derive(Clone, Debug)]
pub struct TextOut {
    /// 0 = base style, i + 1 = `runs[i].style`.
    pub styles: Vec<d::TextStyle>,
    pub lines: Vec<Line>,
}

struct Segment {
    text: String,
    style: usize,
    small: bool,
}

fn build_segments(props: &TextProps) -> Vec<Segment> {
    let case_of = |idx: usize| if idx == 0 { props.style.case } else { props.runs[idx - 1].style.case };
    let style_at = |b: usize| -> usize {
        let mut s = 0;
        for (i, r) in props.runs.iter().enumerate() {
            if b >= r.start && b < r.end {
                s = i + 1;
            }
        }
        s
    };
    let mut segs: Vec<Segment> = Vec::new();
    let mut word_start = true;
    for (b, ch) in props.content.char_indices() {
        let idx = style_at(b);
        let mut small = false;
        let piece: String = match case_of(idx) {
            d::TextCase::Original => ch.to_string(),
            d::TextCase::Upper => ch.to_uppercase().collect(),
            d::TextCase::Lower => ch.to_lowercase().collect(),
            d::TextCase::Title => {
                if word_start && ch.is_alphabetic() {
                    ch.to_uppercase().collect()
                } else {
                    ch.to_string()
                }
            }
            d::TextCase::SmallCaps => {
                if ch.is_lowercase() {
                    small = true;
                    ch.to_uppercase().collect()
                } else {
                    ch.to_string()
                }
            }
        };
        word_start = !(ch.is_alphanumeric() || ch == '\'' || ch == '\u{2019}');
        match segs.last_mut() {
            Some(last) if last.style == idx && last.small == small => last.text.push_str(&piece),
            _ => segs.push(Segment { text: piece, style: idx, small }),
        }
    }
    segs
}

fn auto_line_ratio(fonts: &mut Fonts, st: &d::TextStyle, family: Option<&str>) -> f32 {
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
    if upem <= 0.0 {
        return 1.2;
    }
    let r = (m.ascent + m.descent + m.leading) / upem;
    if r.is_finite() && r > 0.5 {
        r
    } else {
        1.2
    }
}

/// Lays out `props` in a box of `size`.
pub fn layout(props: &TextProps, size: Vec2) -> TextOut {
    with_fonts(|f| compute(f, props, size))
}

fn compute(fonts: &mut Fonts, props: &TextProps, size: Vec2) -> TextOut {
    let mut styles = vec![props.style.clone()];
    styles.extend(props.runs.iter().map(|r| r.style.clone()));
    let family_names: Vec<Option<String>> = styles.iter().map(|s| resolve_family(fonts, &s.font_family)).collect();
    let ratios: Vec<f32> = styles
        .iter()
        .enumerate()
        .map(|(i, s)| match s.line_height {
            d::LineHeight::Auto => auto_line_ratio(fonts, s, family_names[i].as_deref()),
            _ => 1.0,
        })
        .collect();
    let line_height_for = |idx: usize, font_size: f32| -> f32 {
        match styles[idx].line_height {
            d::LineHeight::Auto => ratios[idx] * font_size,
            d::LineHeight::Pixels(p) => p,
            d::LineHeight::Percent(p) => font_size * p / 100.0,
        }
    };
    let segs = build_segments(props);
    let attrs_for = |seg: &Segment| -> Attrs<'_> {
        let st = &styles[seg.style];
        let fsize = (st.font_size * if seg.small { 0.8 } else { 1.0 }).max(0.1);
        let family = match &family_names[seg.style] {
            Some(n) => Family::Name(n),
            None => Family::SansSerif,
        };
        let mut a = Attrs::new()
            .family(family)
            .weight(Weight(st.font_weight))
            .style(if st.italic { Style::Italic } else { Style::Normal })
            .metrics(Metrics::new(fsize, line_height_for(seg.style, fsize)))
            .metadata(seg.style)
            .cache_key_flags(CacheKeyFlags::DISABLE_HINTING);
        if st.letter_spacing != 0.0 {
            a = a.letter_spacing(st.letter_spacing / fsize);
        }
        let mut deco = CtDecoration::new();
        match st.decoration {
            d::TextDecoration::None => {}
            d::TextDecoration::Underline => deco.underline = UnderlineStyle::Single,
            d::TextDecoration::Strikethrough => deco.strikethrough = true,
        }
        if deco.has_decoration() {
            a.text_decoration = deco;
        }
        a
    };
    let base_size = props.style.font_size.max(0.1);
    let base_lh = line_height_for(0, base_size);
    let wrap_width = if size.x > 0.0 { Some(size.x) } else { None };
    let align = match props.style.align_h {
        d::TextAlignH::Left => Align::Left,
        d::TextAlignH::Center => Align::Center,
        d::TextAlignH::Right => Align::Right,
        d::TextAlignH::Justified => Align::Justified,
    };
    let default_attrs = attrs_for(&Segment { text: String::new(), style: 0, small: false });
    let mut buffer = Buffer::new(&mut fonts.fs, Metrics::new(base_size, base_lh));
    buffer.set_hinting(Hinting::Disabled);
    buffer.set_size(wrap_width, None);
    buffer.set_wrap(match props.auto_resize {
        TextAutoResize::WidthAndHeight => Wrap::None,
        _ => Wrap::WordOrGlyph,
    });
    let spans: Vec<(&str, Attrs)> = segs.iter().map(|s| (s.text.as_str(), attrs_for(s))).collect();
    buffer.set_rich_text(spans, &default_attrs, Shaping::Advanced, Some(align));
    buffer.shape_until_scroll(&mut fonts.fs, false);

    let para = props.style.paragraph_spacing;
    let mut total_h = 0.0f32;
    for run in buffer.layout_runs() {
        total_h = total_h.max(run.line_top + run.line_height + run.line_i as f32 * para);
    }
    let oy = if size.y > 0.0 {
        match props.style.align_v {
            d::TextAlignV::Top => 0.0,
            d::TextAlignV::Center => (size.y - total_h) / 2.0,
            d::TextAlignV::Bottom => size.y - total_h,
        }
    } else {
        0.0
    };

    let last_style = styles.len() - 1;
    let mut lines = Vec::new();
    for run in buffer.layout_runs() {
        let baseline = run.line_y + run.line_i as f32 * para + oy;
        let mut spans: Vec<Span> = Vec::new();
        for g in run.glyphs {
            let style = g.metadata.min(last_style);
            let piece = run.text.get(g.start..g.end).unwrap_or("");
            if spans.last().map(|s| s.style) != Some(style) {
                spans.push(Span { style, x: g.x, text: String::new(), outline: Vec::new() });
            }
            let span = spans.last_mut().unwrap();
            span.text.push_str(piece);
            let phys = g.physical((0.0, 0.0), 1.0);
            let gx = g.x + g.font_size * g.x_offset;
            let gy = baseline + g.y - g.font_size * g.y_offset;
            let Some(cmds) = fonts.swash.get_outline_commands(&mut fonts.fs, phys.cache_key) else { continue };
            let v = |x: f32, y: f32| Vec2::new(gx + x, gy - y);
            for c in cmds {
                span.outline.push(match c {
                    Command::MoveTo(p) => PathCmd::MoveTo(v(p.x, p.y)),
                    Command::LineTo(p) => PathCmd::LineTo(v(p.x, p.y)),
                    Command::QuadTo(c1, p) => PathCmd::QuadTo(v(c1.x, c1.y), v(p.x, p.y)),
                    Command::CurveTo(c1, c2, p) => PathCmd::CubicTo(v(c1.x, c1.y), v(c2.x, c2.y), v(p.x, p.y)),
                    Command::Close => PathCmd::Close,
                });
            }
        }
        for deco in run.decorations {
            let glyphs = &run.glyphs[deco.glyph_range.clone()];
            let Some(first) = glyphs.first() else { continue };
            let style = first.metadata.min(last_style);
            let (mut x0, mut x1) = (f32::MAX, f32::MIN);
            for g in glyphs {
                x0 = x0.min(g.x);
                x1 = x1.max(g.x + g.w);
            }
            if x1 <= x0 {
                continue;
            }
            let fs = deco.font_size;
            let td = &deco.data.text_decoration;
            let mut bar = |offset: f32, thickness: f32| {
                let t = (thickness * fs).max(1.0);
                let y = baseline - offset * fs;
                if let Some(span) = spans.iter_mut().find(|s| s.style == style) {
                    span.outline.extend([
                        PathCmd::MoveTo(Vec2::new(x0, y)),
                        PathCmd::LineTo(Vec2::new(x1, y)),
                        PathCmd::LineTo(Vec2::new(x1, y + t)),
                        PathCmd::LineTo(Vec2::new(x0, y + t)),
                        PathCmd::Close,
                    ]);
                }
            };
            if !matches!(td.underline, UnderlineStyle::None) {
                bar(deco.data.underline_metrics.offset, deco.data.underline_metrics.thickness);
            }
            if td.strikethrough {
                bar(deco.data.strikethrough_metrics.offset, deco.data.strikethrough_metrics.thickness);
            }
        }
        if !spans.is_empty() {
            lines.push(Line { baseline, spans });
        }
    }
    TextOut { styles, lines }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lays_out_lines_with_outlines() {
        let mut p = TextProps {
            content: "Hello world and more".into(),
            style: d::TextStyle { font_size: 16.0, line_height: d::LineHeight::Pixels(20.0), ..Default::default() },
            runs: vec![],
            auto_resize: TextAutoResize::Height,
            max_lines: None,
        };
        let out = layout(&p, Vec2::new(60.0, 100.0));
        assert!(out.lines.len() >= 2, "wraps in a 60px box");
        assert!(out.lines[0].spans[0].outline.len() > 4);
        assert!((out.lines[1].baseline - out.lines[0].baseline - 20.0).abs() < 0.01);
        p.auto_resize = TextAutoResize::WidthAndHeight;
        assert_eq!(layout(&p, Vec2::new(60.0, 100.0)).lines.len(), 1);
    }
}
