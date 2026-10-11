//! Text layout with cosmic-text. Glyph outlines are converted to tiny-skia
//! paths (no hinting, greyscale AA via the path rasteriser), grouped by the
//! style they belong to so fills (solid / gradient / image) apply uniformly.

use cosmic_text::{
    fontdb, Align, Attrs, Buffer, CacheKeyFlags, Command, Family, FontSystem, Hinting, Metrics, Shaping, Style, SwashCache,
    TextDecoration as CtDecoration, UnderlineStyle, Weight, Wrap,
};
use desigma_doc as d;
use desigma_doc::{TextAutoResize, TextProps, Vec2};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use tiny_skia as sk;

struct Fonts {
    fs: FontSystem,
    swash: SwashCache,
    /// Lowercased family names present in the font database.
    families: HashSet<String>,
    /// Faces embedded via `desigma-fonts` (preferred over any system copy).
    bundled: HashSet<fontdb::ID>,
}

thread_local! {
    static FONTS: RefCell<Option<Fonts>> = const { RefCell::new(None) };
    static LAYOUTS: RefCell<HashMap<u64, Rc<TextLayout>>> = RefCell::new(HashMap::new());
}

/// Outlines of laid out text, one path per style (index 0 = base style,
/// i + 1 = `runs[i].style`).
pub struct TextLayout {
    pub styles: Vec<d::TextStyle>,
    pub paths: Vec<Option<sk::Path>>,
}

fn init_fonts() -> Fonts {
    let mut db = fontdb::Database::new();
    // Bundled fonts go in first and any system face claiming the same family
    // is dropped, so "Inter" always resolves to the embedded copy.
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
    Fonts { fs, swash: SwashCache::new(), families, bundled }
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

fn with_fonts<R>(f: impl FnOnce(&mut Fonts) -> R) -> R {
    FONTS.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(init_fonts());
        }
        f(slot.as_mut().unwrap())
    })
}

/// Registers extra font data (e.g. a font fetched by `desigma-fonts`) with the
/// shared font system. Cached layouts are discarded.
pub fn register_font(data: Vec<u8>) {
    with_fonts(|f| {
        f.fs.db_mut().load_font_data(data);
        f.families = family_names(&f.fs);
    });
    LAYOUTS.with(|l| l.borrow_mut().clear());
}

/// Compatible alias of [`register_font`].
pub fn load_font_data(data: Vec<u8>) {
    register_font(data)
}

/// Which font a text style will be shaped with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedFont {
    /// Family name in the font database (after substitution).
    pub family: String,
    /// The matched face is one embedded in the binary.
    pub bundled: bool,
}

/// Resolves `family` the way the text layout does: the family itself if it is
/// in the font database, else its open-source substitute, else the default
/// sans-serif (Inter). Returns `None` only if the database has no match at all.
pub fn resolve_font(family: &str, weight: u16, italic: bool) -> Option<ResolvedFont> {
    with_fonts(|f| {
        let name = resolve_family(f, family);
        let fam = match &name {
            Some(n) => fontdb::Family::Name(n),
            None => fontdb::Family::SansSerif,
        };
        let q = fontdb::Query {
            families: &[fam],
            weight: fontdb::Weight(weight),
            stretch: fontdb::Stretch::Normal,
            style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
        };
        let id = f.fs.db().query(&q)?;
        let face = f.fs.db().face(id)?;
        Some(ResolvedFont { family: face.families.first()?.0.clone(), bundled: f.bundled.contains(&id) })
    })
}

/// Family name to ask the font database for, or `None` to use default sans-serif.
fn resolve_family(fonts: &Fonts, requested: &str) -> Option<String> {
    if fonts.families.contains(&requested.to_lowercase()) {
        return Some(requested.to_string());
    }
    let sub = desigma_fonts::substitute(requested)?;
    fonts.families.contains(&sub.to_lowercase()).then(|| sub.to_string())
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

struct Segment {
    text: String,
    style: usize,
    small: bool,
}

fn build_segments(props: &TextProps) -> Vec<Segment> {
    let content = &props.content;
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
    for (b, ch) in content.char_indices() {
        let idx = style_at(b);
        let case = case_of(idx);
        let mut small = false;
        let piece: String = match case {
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

fn style_hash(props: &TextProps, size: Vec2) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}|{}|{}", props, size.x, size.y).hash(&mut h);
    h.finish()
}

/// Lays out `props` inside a box of `size`; cached per content/style/size.
pub fn layout(props: &TextProps, size: Vec2) -> Rc<TextLayout> {
    let key = style_hash(props, size);
    if let Some(hit) = LAYOUTS.with(|l| l.borrow().get(&key).cloned()) {
        return hit;
    }
    let lay = Rc::new(with_fonts(|f| compute_layout(f, props, size)));
    LAYOUTS.with(|l| {
        let mut l = l.borrow_mut();
        if l.len() > 1024 {
            l.clear();
        }
        l.insert(key, lay.clone());
    });
    lay
}

fn compute_layout(fonts: &mut Fonts, props: &TextProps, size: Vec2) -> TextLayout {
    let mut styles = vec![props.style.clone()];
    styles.extend(props.runs.iter().map(|r| r.style.clone()));
    let mut builders: Vec<sk::PathBuilder> = styles.iter().map(|_| sk::PathBuilder::new()).collect();

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
    let mut buffer = Buffer::new(&mut fonts.fs, Metrics::new(base_size, base_lh));
    buffer.set_hinting(Hinting::Disabled);
    let wrap_width = if size.x > 0.0 { Some(size.x) } else { None };
    buffer.set_size(wrap_width, None);
    buffer.set_wrap(match props.auto_resize {
        TextAutoResize::WidthAndHeight => Wrap::None,
        _ => Wrap::WordOrGlyph,
    });
    let align = match props.style.align_h {
        d::TextAlignH::Left => Align::Left,
        d::TextAlignH::Center => Align::Center,
        d::TextAlignH::Right => Align::Right,
        d::TextAlignH::Justified => Align::Justified,
    };
    let default_attrs = attrs_for(&Segment { text: String::new(), style: 0, small: false });
    let spans: Vec<(&str, Attrs)> = segs.iter().map(|s| (s.text.as_str(), attrs_for(s))).collect();
    buffer.set_rich_text(spans, &default_attrs, Shaping::Advanced, Some(align));
    buffer.shape_until_scroll(&mut fonts.fs, false);

    // measure
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

    for run in buffer.layout_runs() {
        let baseline = run.line_y + run.line_i as f32 * para + oy;
        for g in run.glyphs {
            let style = g.metadata.min(builders.len() - 1);
            let phys = g.physical((0.0, 0.0), 1.0);
            let gx = g.x + g.font_size * g.x_offset;
            let gy = baseline + g.y - g.font_size * g.y_offset;
            let Some(cmds) = fonts.swash.get_outline_commands(&mut fonts.fs, phys.cache_key) else { continue };
            let pb = &mut builders[style];
            for c in cmds {
                match c {
                    Command::MoveTo(p) => pb.move_to(gx + p.x, gy - p.y),
                    Command::LineTo(p) => pb.line_to(gx + p.x, gy - p.y),
                    Command::QuadTo(c1, p) => pb.quad_to(gx + c1.x, gy - c1.y, gx + p.x, gy - p.y),
                    Command::CurveTo(c1, c2, p) => {
                        pb.cubic_to(gx + c1.x, gy - c1.y, gx + c2.x, gy - c2.y, gx + p.x, gy - p.y)
                    }
                    Command::Close => pb.close(),
                }
            }
        }
        for span in run.decorations {
            let glyphs = &run.glyphs[span.glyph_range.clone()];
            let Some(first) = glyphs.first() else { continue };
            let style = first.metadata.min(builders.len() - 1);
            let (mut x0, mut x1) = (f32::MAX, f32::MIN);
            for g in glyphs {
                x0 = x0.min(g.x);
                x1 = x1.max(g.x + g.w);
            }
            if x1 <= x0 {
                continue;
            }
            let fs = span.font_size;
            let td = &span.data.text_decoration;
            let mut rect = |offset: f32, thickness: f32| {
                let t = (thickness * fs).max(1.0);
                let y = baseline - offset * fs;
                if let Some(r) = sk::Rect::from_xywh(x0, y, x1 - x0, t) {
                    builders[style].push_rect(r);
                }
            };
            if !matches!(td.underline, UnderlineStyle::None) {
                rect(span.data.underline_metrics.offset, span.data.underline_metrics.thickness);
            }
            if td.strikethrough {
                rect(span.data.strikethrough_metrics.offset, span.data.strikethrough_metrics.thickness);
            }
        }
    }
    TextLayout { styles, paths: builders.into_iter().map(|b| b.finish()).collect() }
}
