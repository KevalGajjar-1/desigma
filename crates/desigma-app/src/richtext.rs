//! Rich text algebra over `TextProps`: per-range styling by splitting and
//! merging `TextRun`s, and content edits (splice) that keep the runs aligned.
//!
//! Model recap: `TextProps::style` is the base style (and the only source of
//! paragraph level fields: alignment and paragraph spacing). `runs` hold FULL
//! styles over UTF-8 byte ranges of `content` that differ from the base; where
//! runs overlap the later one wins (as in the renderer). All functions here
//! keep runs normalised: sorted, non-overlapping, non-empty, never equal to the
//! base style, with equal neighbours merged, and ranges on char boundaries.
//!
//! Lists: the document model has no list or indent attribute, so bulleted and
//! numbered lists are emulated with literal prefixes ("• ", "1. ") at the start
//! of each paragraph; there is no hanging indent.

use desigma_doc::{LineHeight, Paint, TextCase, TextDecoration, TextProps, TextRun, TextStyle};
use std::ops::Range;

/// A maximal byte range of content sharing one effective style.
#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub start: usize,
    pub end: usize,
    pub style: TextStyle,
}

/// Effective style of every byte of the content, as ordered gap-free segments
/// (empty for empty content). Adjacent equal styles are merged.
pub fn segments(t: &TextProps) -> Vec<Seg> {
    let len = t.content.len();
    if len == 0 {
        return vec![];
    }
    let mut cuts = vec![0, len];
    for r in &t.runs {
        cuts.push(r.start.min(len));
        cuts.push(r.end.min(len));
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<Seg> = Vec::new();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a == b {
            continue;
        }
        let mut style = &t.style;
        for r in &t.runs {
            if a >= r.start && a < r.end {
                style = &r.style;
            }
        }
        match out.last_mut() {
            Some(last) if last.style == *style => last.end = b,
            _ => out.push(Seg { start: a, end: b, style: style.clone() }),
        }
    }
    out
}

/// Rebuilds `runs` (and possibly the base style) from gap-free segments.
/// When every segment shares one style it becomes the base style (keeping the
/// base paragraph fields) and `runs` is cleared.
pub fn set_segments(t: &mut TextProps, mut segs: Vec<Seg>) {
    segs.retain(|s| s.end > s.start);
    // merge equal neighbours
    let mut merged: Vec<Seg> = Vec::new();
    for s in segs {
        match merged.last_mut() {
            Some(last) if last.style == s.style && last.end == s.start => last.end = s.end,
            _ => merged.push(s),
        }
    }
    if merged.len() == 1 && merged[0].start == 0 && merged[0].end == t.content.len() {
        let old = &t.style;
        let mut base = merged[0].style.clone();
        base.align_h = old.align_h;
        base.align_v = old.align_v;
        base.paragraph_spacing = old.paragraph_spacing;
        t.style = base;
        t.runs.clear();
        return;
    }
    t.runs = merged.into_iter().filter(|s| s.style != t.style).map(|s| TextRun { start: s.start, end: s.end, style: s.style }).collect();
}

/// Splits segments so that `at` is a boundary.
fn split_at(segs: &mut Vec<Seg>, at: usize) {
    if let Some(i) = segs.iter().position(|s| s.start < at && at < s.end) {
        let right = Seg { start: at, end: segs[i].end, style: segs[i].style.clone() };
        segs[i].end = at;
        segs.insert(i + 1, right);
    }
}

fn clamp_range(t: &TextProps, r: Range<usize>) -> Range<usize> {
    let len = t.content.len();
    let mut a = r.start.min(len);
    let mut b = r.end.min(len);
    while a > 0 && !t.content.is_char_boundary(a) {
        a -= 1;
    }
    while b < len && !t.content.is_char_boundary(b) {
        b += 1;
    }
    a..b.max(a)
}

/// Applies `f` to the style of every byte in `range` (clamped to char
/// boundaries). Returns false when the range is empty (nothing to style).
pub fn apply_style(t: &mut TextProps, range: Range<usize>, f: impl Fn(&mut TextStyle)) -> bool {
    let range = clamp_range(t, range);
    if range.is_empty() {
        return false;
    }
    let mut segs = segments(t);
    split_at(&mut segs, range.start);
    split_at(&mut segs, range.end);
    for s in &mut segs {
        if s.start >= range.start && s.end <= range.end {
            f(&mut s.style);
        }
    }
    set_segments(t, segs);
    true
}

/// Applies `f` to the whole text: the base style and every run.
pub fn apply_all(t: &mut TextProps, f: impl Fn(&mut TextStyle)) {
    let len = t.content.len();
    if len == 0 {
        f(&mut t.style);
        t.runs.clear();
        return;
    }
    // Paragraph level fields live on the base, so it is edited directly too.
    let mut segs = segments(t);
    for s in &mut segs {
        f(&mut s.style);
    }
    let mut base = t.style.clone();
    f(&mut base);
    t.style = base;
    set_segments(t, segs);
}

/// The effective style at byte `at` (the char starting there; the base style for empty text).
pub fn style_at(t: &TextProps, at: usize) -> TextStyle {
    let mut style = &t.style;
    for r in &t.runs {
        if at >= r.start && at < r.end {
            style = &r.style;
        }
    }
    style.clone()
}

/// Style used for typing at `caret`: the char before it (or after, at the start).
pub fn style_for_typing(t: &TextProps, caret: usize) -> TextStyle {
    if t.content.is_empty() {
        return t.style.clone();
    }
    if caret > 0 {
        let prev = t.content[..caret.min(t.content.len())].char_indices().next_back().map(|(i, _)| i).unwrap_or(0);
        style_at(t, prev)
    } else {
        style_at(t, 0)
    }
}

/// Replaces `range` of the content with `new` and keeps runs aligned. The new
/// text gets `style` when given, otherwise the style of the first replaced
/// char (or of the char before an insertion point). Returns the byte offset
/// just after the inserted text.
pub fn splice(t: &mut TextProps, range: Range<usize>, new: &str, style: Option<TextStyle>) -> usize {
    let range = clamp_range(t, range);
    let inherit = if range.is_empty() { style_for_typing(t, range.start) } else { style_at(t, range.start) };
    let new_style = style.unwrap_or(inherit);
    let mut segs = segments(t);
    split_at(&mut segs, range.start);
    split_at(&mut segs, range.end);
    let delta = new.len() as isize - range.len() as isize;
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len() + 1);
    let mut inserted = false;
    for s in segs {
        if s.end <= range.start {
            out.push(s);
        } else if s.start >= range.end {
            if !inserted {
                out.push(Seg { start: range.start, end: range.start + new.len(), style: new_style.clone() });
                inserted = true;
            }
            out.push(Seg { start: (s.start as isize + delta) as usize, end: (s.end as isize + delta) as usize, style: s.style });
        }
        // else: inside the replaced range, dropped
    }
    if !inserted {
        out.push(Seg { start: range.start, end: range.start + new.len(), style: new_style });
    }
    t.content.replace_range(range.clone(), new);
    set_segments(t, out);
    range.start + new.len()
}

/// A property that is either the same over a range or mixed.
pub type Mixed<T> = Option<T>;

/// Per property values over a range (`None` = mixed), for the inspector.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    pub family: Mixed<String>,
    pub weight: Mixed<u16>,
    pub italic: Mixed<bool>,
    pub size: Mixed<f32>,
    pub line_height: Mixed<LineHeight>,
    pub letter_spacing: Mixed<f32>,
    pub fills: Mixed<Vec<Paint>>,
    pub decoration: Mixed<TextDecoration>,
    pub case: Mixed<TextCase>,
}

fn same<T: PartialEq + Clone>(items: &[&TextStyle], f: impl Fn(&TextStyle) -> T) -> Mixed<T> {
    let first = f(items.first()?);
    items.iter().all(|s| f(s) == first).then_some(first)
}

/// Summary of the styles over `range`; a collapsed range reads the style at the caret.
pub fn summary(t: &TextProps, range: Range<usize>) -> Summary {
    let range = clamp_range(t, range);
    let styles: Vec<TextStyle> = if range.is_empty() || t.content.is_empty() {
        vec![if t.content.is_empty() { t.style.clone() } else { style_for_typing(t, range.start) }]
    } else {
        segments(t).into_iter().filter(|s| s.start < range.end && s.end > range.start).map(|s| s.style).collect()
    };
    let refs: Vec<&TextStyle> = styles.iter().collect();
    Summary {
        family: same(&refs, |s| s.font_family.clone()),
        weight: same(&refs, |s| s.font_weight),
        italic: same(&refs, |s| s.italic),
        size: same(&refs, |s| s.font_size),
        line_height: same(&refs, |s| s.line_height),
        letter_spacing: same(&refs, |s| s.letter_spacing),
        fills: same(&refs, |s| s.fills.clone()),
        decoration: same(&refs, |s| s.decoration),
        case: same(&refs, |s| s.case),
    }
}

/// Font size in px a line height resolves to for a given size (for display).
pub fn line_height_px(lh: LineHeight, size: f32) -> Option<f32> {
    match lh {
        LineHeight::Auto => None,
        LineHeight::Pixels(p) => Some(p),
        LineHeight::Percent(p) => Some(size * p / 100.0),
    }
}

// ------------------------------------------------------------------ lists
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    Bullet,
    Numbered,
}

pub const BULLET: &str = "\u{2022} ";

/// Length in bytes of a list prefix at the start of `para`, with its kind.
pub fn list_prefix(para: &str) -> Option<(ListKind, usize)> {
    if para.starts_with(BULLET) {
        return Some((ListKind::Bullet, BULLET.len()));
    }
    let digits = para.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits > 0 && digits <= 4 && para[digits..].starts_with(". ") {
        return Some((ListKind::Numbered, digits + 2));
    }
    None
}

/// Byte ranges (excluding the newline) of the paragraphs touched by `range`.
pub fn paragraphs_in(content: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for piece in content.split('\n') {
        let end = start + piece.len();
        let touches = if range.is_empty() { range.start >= start && range.start <= end } else { range.start <= end && range.end > start };
        if touches {
            out.push(start..end);
        }
        start = end + 1;
    }
    out
}

/// Toggles a list of `kind` on the paragraphs touched by `range`: removes it
/// if all of them already have that kind, otherwise (re)applies it. Returns
/// the new range covering the same paragraphs.
pub fn toggle_list(t: &mut TextProps, range: Range<usize>, kind: ListKind) -> Range<usize> {
    let paras = paragraphs_in(&t.content, range.clone());
    if paras.is_empty() {
        return range;
    }
    let all_have = paras.iter().all(|p| list_prefix(&t.content[p.clone()]).map(|(k, _)| k) == Some(kind));
    // Work back to front so earlier offsets stay valid, then renumber.
    let first = paras.first().unwrap().start;
    let mut last_end = paras.last().unwrap().end;
    for p in paras.iter().rev() {
        let existing = list_prefix(&t.content[p.clone()]);
        if let Some((_, n)) = existing {
            splice(t, p.start..p.start + n, "", None);
            last_end -= n;
        }
        if !all_have {
            let style = if t.content.is_empty() || p.start >= t.content.len() { None } else { Some(style_at(t, p.start)) };
            let prefix = match kind {
                ListKind::Bullet => BULLET.to_string(),
                ListKind::Numbered => "1. ".to_string(),
            };
            splice(t, p.start..p.start, &prefix, style);
            last_end += prefix.len();
        }
    }
    if !all_have && kind == ListKind::Numbered {
        last_end = renumber(t, first, last_end);
    }
    first..last_end
}

/// Renumbers consecutive numbered paragraphs inside `first..last_end` from 1; returns the new end.
fn renumber(t: &mut TextProps, first: usize, mut last_end: usize) -> usize {
    let mut n = 1;
    loop {
        // offsets shift as numbers change width, so recompute the paragraphs each time
        let cur = paragraphs_in(&t.content, first..last_end.max(first + 1));
        let Some(p) = cur.get(n - 1).cloned() else { break };
        if let Some((ListKind::Numbered, len)) = list_prefix(&t.content[p.clone()]) {
            let label = format!("{n}. ");
            if t.content[p.start..p.start + len] != label {
                let style = Some(style_at(t, p.start));
                splice(t, p.start..p.start + len, &label, style);
                last_end = (last_end as isize + label.len() as isize - len as isize) as usize;
            }
            n += 1;
        } else {
            break;
        }
    }
    last_end
}

/// Text to insert after Enter at `caret` to continue a list: the next bullet or
/// number. `None` when the paragraph is not a list item. When the item is
/// empty (only its prefix) returns `Some` with `exit = true`: the prefix
/// should be removed instead of continuing.
pub fn list_continuation(content: &str, caret: usize) -> Option<(String, bool, Range<usize>)> {
    let ps = paragraphs_in(content, caret..caret);
    let p = ps.first()?.clone();
    let para = &content[p.clone()];
    let (kind, n) = list_prefix(para)?;
    if caret < p.start + n {
        return None;
    }
    if para.len() == n {
        return Some((String::new(), true, p.start..p.start + n));
    }
    let next = match kind {
        ListKind::Bullet => BULLET.to_string(),
        ListKind::Numbered => {
            let num: u32 = para[..n - 2].parse().unwrap_or(0);
            format!("{}. ", num + 1)
        }
    };
    Some((next, false, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::{Color, TextAutoResize};

    fn props(s: &str) -> TextProps {
        TextProps {
            content: s.into(),
            style: TextStyle::default(),
            runs: vec![],
            auto_resize: TextAutoResize::WidthAndHeight,
            max_lines: None,
            ..Default::default()
        }
    }

    fn bold(s: &mut TextStyle) {
        s.font_weight = 700;
    }

    /// Runs invariants that every operation must keep.
    fn check(t: &TextProps) {
        let mut prev_end = 0;
        for r in &t.runs {
            assert!(r.start < r.end, "empty run {r:?}");
            assert!(r.start >= prev_end, "unsorted/overlapping {:?}", t.runs);
            assert!(r.end <= t.content.len());
            assert!(t.content.is_char_boundary(r.start) && t.content.is_char_boundary(r.end), "{r:?} in {:?}", t.content);
            assert_ne!(r.style, t.style, "run equal to base");
            prev_end = r.end;
        }
        for w in t.runs.windows(2) {
            assert!(!(w[0].end == w[1].start && w[0].style == w[1].style), "unmerged neighbours");
        }
    }

    fn rd(t: &TextProps) -> Vec<(usize, usize, u16)> {
        t.runs.iter().map(|r| (r.start, r.end, r.style.font_weight)).collect()
    }

    #[test]
    fn style_middle_splits_and_whole_merges_into_base() {
        let mut t = props("hello world");
        assert!(apply_style(&mut t, 2..5, bold));
        check(&t);
        assert_eq!(rd(&t), vec![(2, 5, 700)]);
        assert_eq!(segments(&t).len(), 3);
        // extend over neighbours: merges into one run
        apply_style(&mut t, 5..8, bold);
        assert_eq!(rd(&t), vec![(2, 8, 700)]);
        // un-bold part of it
        apply_style(&mut t, 4..6, |s| s.font_weight = 400);
        assert_eq!(rd(&t), vec![(2, 4, 700), (6, 8, 700)]);
        // bold everything: runs vanish and the base style carries it
        apply_style(&mut t, 0..11, bold);
        assert!(t.runs.is_empty());
        assert_eq!(t.style.font_weight, 700);
        check(&t);
    }

    #[test]
    fn empty_range_and_clamping() {
        let mut t = props("abc");
        assert!(!apply_style(&mut t, 2..2, bold));
        assert!(apply_style(&mut t, 1..99, bold));
        assert_eq!(rd(&t), vec![(1, 3, 700)]);
        assert!(!apply_style(&mut props(""), 0..0, bold));
    }

    #[test]
    fn multibyte_and_emoji_ranges_stay_on_char_boundaries() {
        let mut t = props("aé😀中b");
        // é is 2 bytes (1..3), 😀 is 4 (3..7), 中 is 3 (7..10)
        assert!(apply_style(&mut t, 2..4, bold), "mid-char offsets are widened");
        check(&t);
        assert_eq!(rd(&t), vec![(1, 7, 700)]);
        let mut t = props("aé😀中b");
        apply_style(&mut t, 4..8, bold);
        check(&t);
        assert_eq!(rd(&t), vec![(3, 10, 700)]);
        // splice inside a run with emoji
        let mut t = props("a😀b");
        apply_style(&mut t, 1..5, bold);
        let after = splice(&mut t, 5..5, "é", None);
        assert_eq!(after, 7);
        assert_eq!(t.content, "a😀éb");
        assert_eq!(rd(&t), vec![(1, 7, 700)], "typing after bold text stays bold");
        check(&t);
    }

    #[test]
    fn splice_shifts_and_inherits() {
        let mut t = props("abcdef");
        apply_style(&mut t, 2..4, bold); // cd
                                         // insert before the bold run: plain
        splice(&mut t, 0..0, "XX", None);
        assert_eq!(t.content, "XXabcdef");
        assert_eq!(rd(&t), vec![(4, 6, 700)]);
        // insert in the middle of the run: bold, run grows
        splice(&mut t, 5..5, "__", None);
        assert_eq!(t.content, "XXabc__def");
        assert_eq!(rd(&t), vec![(4, 8, 700)]);
        // delete across the run boundary
        splice(&mut t, 3..6, "", None);
        assert_eq!(t.content, "XXa_def");
        check(&t);
        assert_eq!(rd(&t), vec![(3, 5, 700)]);
        // replace everything
        splice(&mut t, 0..8, "new", None);
        assert_eq!(t.content, "new");
        assert!(t.runs.is_empty());
        // explicit style for typed text
        let mut b = t.style.clone();
        b.font_weight = 700;
        splice(&mut t, 3..3, "!", Some(b));
        assert_eq!(rd(&t), vec![(3, 4, 700)]);
        check(&t);
    }

    #[test]
    fn replace_selection_takes_first_char_style() {
        let mut t = props("abcd");
        apply_style(&mut t, 0..2, bold);
        splice(&mut t, 1..3, "ZZZ", None);
        assert_eq!(t.content, "aZZZd");
        assert_eq!(rd(&t), vec![(0, 4, 700)]);
        check(&t);
    }

    #[test]
    fn delete_to_empty_clears_runs() {
        let mut t = props("ab");
        apply_style(&mut t, 0..1, bold);
        splice(&mut t, 0..2, "", None);
        assert!(t.runs.is_empty() && t.content.is_empty());
    }

    #[test]
    fn overlapping_legacy_runs_normalise_with_later_winning() {
        let mut t = props("abcdef");
        let mut a = t.style.clone();
        a.font_size = 20.0;
        let mut b = t.style.clone();
        b.font_size = 30.0;
        t.runs = vec![TextRun { start: 0, end: 4, style: a }, TextRun { start: 2, end: 6, style: b }];
        let segs = segments(&t);
        assert_eq!(segs.iter().map(|s| (s.start, s.end, s.style.font_size)).collect::<Vec<_>>(), vec![(0, 2, 20.0), (2, 6, 30.0)]);
        apply_style(&mut t, 0..1, |s| s.italic = true);
        check(&t);
    }

    #[test]
    fn summary_reports_mixed() {
        let mut t = props("hello world");
        apply_style(&mut t, 0..5, |s| {
            s.font_size = 24.0;
            s.font_weight = 700;
        });
        let all = summary(&t, 0..11);
        assert_eq!(all.size, None);
        assert_eq!(all.weight, None);
        assert_eq!(all.family, Some("Inter".to_string()));
        let first = summary(&t, 0..5);
        assert_eq!((first.size, first.weight), (Some(24.0), Some(700)));
        let caret_end = summary(&t, 11..11);
        assert_eq!(caret_end.size, Some(12.0));
        let caret_mid = summary(&t, 3..3);
        assert_eq!(caret_mid.size, Some(24.0), "caret reads the char before it");
        assert_eq!(summary(&props(""), 0..0).size, Some(12.0));
    }

    #[test]
    fn colour_decoration_and_case_apply_per_range() {
        let mut t = props("one two");
        apply_style(&mut t, 4..7, |s| {
            s.fills = vec![Paint::solid(Color::rgba(255, 0, 0, 255))];
            s.decoration = TextDecoration::Underline;
            s.case = TextCase::Upper;
        });
        check(&t);
        let s = summary(&t, 0..7);
        assert_eq!((s.fills, s.decoration, s.case), (None, None, None));
        let s = summary(&t, 4..7);
        assert_eq!((s.decoration, s.case), (Some(TextDecoration::Underline), Some(TextCase::Upper)));
    }

    #[test]
    fn apply_all_edits_base_and_runs() {
        let mut t = props("ab cd");
        apply_style(&mut t, 0..2, |s| s.font_size = 30.0);
        apply_all(&mut t, |s| s.font_weight = 700);
        check(&t);
        assert_eq!(t.style.font_weight, 700);
        assert_eq!(t.runs.len(), 1);
        assert_eq!((t.runs[0].style.font_size, t.runs[0].style.font_weight), (30.0, 700));
        let mut e = props("");
        apply_all(&mut e, |s| s.font_size = 40.0);
        assert_eq!(e.style.font_size, 40.0);
    }

    #[test]
    fn base_alignment_survives_merging_into_base() {
        let mut t = props("abc");
        t.style.align_h = desigma_doc::TextAlignH::Center;
        apply_style(&mut t, 0..3, bold);
        assert_eq!(t.style.align_h, desigma_doc::TextAlignH::Center);
        assert_eq!(t.style.font_weight, 700);
    }

    #[test]
    fn bullets_toggle_and_keep_styles() {
        let mut t = props("one\ntwo\nthree");
        let n = t.content.len();
        let r = toggle_list(&mut t, 0..n, ListKind::Bullet);
        assert_eq!(t.content, "• one\n• two\n• three");
        assert_eq!(r, 0..t.content.len());
        // toggling again removes
        let r2 = toggle_list(&mut t, r, ListKind::Bullet);
        assert_eq!(t.content, "one\ntwo\nthree");
        assert_eq!(r2, 0..13);
        check(&t);
    }

    #[test]
    fn numbered_lists_renumber_and_switch_kind() {
        let mut t = props("a\nb\nc");
        let r = toggle_list(&mut t, 0..5, ListKind::Numbered);
        assert_eq!(t.content, "1. a\n2. b\n3. c");
        let r = toggle_list(&mut t, r, ListKind::Bullet);
        assert_eq!(t.content, "• a\n• b\n• c");
        let _ = toggle_list(&mut t, r, ListKind::Numbered);
        assert_eq!(t.content, "1. a\n2. b\n3. c");
        // only the touched paragraph
        let mut t = props("x\ny");
        toggle_list(&mut t, 3..3, ListKind::Bullet);
        assert_eq!(t.content, "x\n• y");
    }

    #[test]
    fn list_in_multibyte_and_styled_text() {
        let mut t = props("é\n😀");
        apply_style(&mut t, 0..2, bold);
        let n = t.content.len();
        toggle_list(&mut t, 0..n, ListKind::Bullet);
        assert_eq!(t.content, "• é\n• 😀");
        check(&t);
        assert_eq!(style_at(&t, 0).font_weight, 700, "prefix takes the paragraph's style");
    }

    #[test]
    fn enter_continues_or_ends_a_list() {
        assert_eq!(list_continuation("• a", 5).map(|(s, e, _)| (s, e)), Some(("• ".to_string(), false)));
        assert_eq!(list_continuation("1. a\n2. b", 9).map(|(s, e, _)| (s, e)), Some(("3. ".to_string(), false)));
        assert_eq!(list_continuation("• ", 4).map(|(s, e, _)| (s, e)), Some((String::new(), true)));
        assert_eq!(list_continuation("plain", 3), None);
        assert_eq!(list_continuation("• a", 1), None, "caret inside the prefix");
    }

    #[test]
    fn paragraph_ranges() {
        let c = "ab\n\ncd";
        assert_eq!(paragraphs_in(c, 0..0), vec![0..2]);
        assert_eq!(paragraphs_in(c, 3..3), vec![3..3]);
        assert_eq!(paragraphs_in(c, 1..5), vec![0..2, 3..3, 4..6]);
        assert_eq!(paragraphs_in(c, 2..2), vec![0..2]);
    }
}
