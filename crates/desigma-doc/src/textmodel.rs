//! Text model extensions: real lists, paragraph settings, writing direction and
//! leading trim, plus the algebra that keeps list ranges aligned with edits.
//!
//! Lists are stored on [`TextProps::lists`] as byte ranges that cover whole
//! paragraphs (from the first paragraph's start to the last paragraph's end,
//! newlines in between included). Everything works through a per-paragraph
//! view ([`TextProps::paragraph_lists`]) so edits only have to say which
//! paragraphs survive.

use crate::{TextProps, TextRun};
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Marker style of a list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ListKind {
    #[default]
    Bullet,
    Numbered,
}

/// A run of consecutive list paragraphs sharing one kind and nesting level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRange {
    /// Byte range in `content` covering whole paragraphs (end excludes the final newline).
    pub start: usize,
    pub end: usize,
    pub kind: ListKind,
    /// Nesting level, 0 = outermost.
    pub level: u8,
}

/// Highest nesting level.
pub const MAX_LIST_LEVEL: u8 = 8;

/// One paragraph's list membership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListItem {
    pub kind: ListKind,
    pub level: u8,
}

/// A resolved marker: the item plus its ordinal within the numbered sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListMarker {
    pub kind: ListKind,
    pub level: u8,
    /// 1-based ordinal for numbered items (0 for bullets).
    pub number: u32,
}

impl ListMarker {
    /// The text of a numbered marker ("1.", "a.", "i." by level); `None` for bullets.
    pub fn label(&self) -> Option<String> {
        (self.kind == ListKind::Numbered).then(|| number_label(self.level, self.number))
    }
}

/// Numbered marker text: digits on level 0, letters on level 1, roman numerals
/// on level 2, then the cycle repeats.
pub fn number_label(level: u8, n: u32) -> String {
    let n = n.max(1);
    match level % 3 {
        0 => format!("{n}."),
        1 => {
            let mut s = String::new();
            let mut k = n;
            while k > 0 {
                k -= 1;
                s.insert(0, (b'a' + (k % 26) as u8) as char);
                k /= 26;
            }
            format!("{s}.")
        }
        _ => format!("{}.", roman(n)),
    }
}

fn roman(mut n: u32) -> String {
    const T: [(u32, &str); 13] =
        [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
    let mut s = String::new();
    for (v, r) in T {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

/// Bullet glyph shape of a nesting level (filled disc, ring, filled square).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulletShape {
    Disc,
    Ring,
    Square,
}

pub fn bullet_shape(level: u8) -> BulletShape {
    match level % 3 {
        0 => BulletShape::Disc,
        1 => BulletShape::Ring,
        _ => BulletShape::Square,
    }
}

/// Writing direction of the paragraphs of a text layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextDirection {
    /// Each paragraph follows its first strong character.
    #[default]
    Auto,
    Ltr,
    Rtl,
}

/// Figma's "Vertical trim".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeadingTrim {
    #[default]
    None,
    /// First line's top at the cap height, last line's bottom at the baseline.
    CapHeight,
}

/// Paragraph level settings of a text layer ("Type settings" > Details).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParagraphProps {
    /// First-line indent of ordinary paragraphs, px.
    pub indent: f32,
    /// Extra space between consecutive list items, px (replaces paragraph spacing between them).
    pub list_spacing: f32,
    /// Opening punctuation at the start of a paragraph hangs outside the left margin
    /// (closing punctuation outside the right margin of right aligned text).
    pub hanging_punctuation: bool,
    /// List markers hang outside the text box instead of sitting inside it.
    pub hanging_list: bool,
    pub leading_trim: LeadingTrim,
}

impl Default for ParagraphProps {
    fn default() -> Self {
        Self { indent: 0.0, list_spacing: 0.0, hanging_punctuation: false, hanging_list: false, leading_trim: LeadingTrim::None }
    }
}

/// Byte ranges (excluding the newline) of every paragraph; always at least one.
pub fn paragraph_ranges(content: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for piece in content.split('\n') {
        out.push(start..start + piece.len());
        start += piece.len() + 1;
    }
    out
}

/// Index of the paragraph holding byte offset `at`.
pub fn paragraph_index(content: &str, at: usize) -> usize {
    content.as_bytes()[..at.min(content.len())].iter().filter(|b| **b == b'\n').count()
}

impl TextProps {
    /// List membership of every paragraph (index = paragraph number).
    pub fn paragraph_lists(&self) -> Vec<Option<ListItem>> {
        let paras = paragraph_ranges(&self.content);
        paras
            .iter()
            .map(|p| {
                self.lists
                    .iter()
                    .find(|l| l.start <= p.start && p.end <= l.end && (l.end > l.start || p.start == l.start))
                    .map(|l| ListItem { kind: l.kind, level: l.level.min(MAX_LIST_LEVEL) })
            })
            .collect()
    }

    /// Replaces `lists` from a per-paragraph description, merging neighbours of the same kind and level.
    pub fn set_paragraph_lists(&mut self, items: &[Option<ListItem>]) {
        let paras = paragraph_ranges(&self.content);
        let mut out: Vec<ListRange> = Vec::new();
        for (i, p) in paras.iter().enumerate() {
            let Some(Some(it)) = items.get(i) else { continue };
            let contiguous = i > 0 && matches!(items.get(i - 1), Some(Some(prev)) if prev == it);
            match out.last_mut() {
                Some(last) if contiguous => last.end = p.end,
                _ => out.push(ListRange { start: p.start, end: p.end, kind: it.kind, level: it.level }),
            }
        }
        self.lists = out;
    }

    /// Markers (with numbering) of every paragraph. Numbers continue across nested
    /// levels and restart after a non-list paragraph or a bullet at the same level.
    pub fn list_markers(&self) -> Vec<Option<ListMarker>> {
        let mut counters: Vec<u32> = Vec::new();
        self.paragraph_lists()
            .into_iter()
            .map(|it| match it {
                None => {
                    counters.clear();
                    None
                }
                Some(ListItem { kind: ListKind::Bullet, level }) => {
                    counters.truncate(level as usize);
                    Some(ListMarker { kind: ListKind::Bullet, level, number: 0 })
                }
                Some(ListItem { kind: ListKind::Numbered, level }) => {
                    counters.truncate(level as usize + 1);
                    counters.resize(level as usize + 1, 0);
                    counters[level as usize] += 1;
                    Some(ListMarker { kind: ListKind::Numbered, level, number: counters[level as usize] })
                }
            })
            .collect()
    }

    /// Whether any paragraph is a list item.
    pub fn has_lists(&self) -> bool {
        self.lists.iter().any(|l| l.end >= l.start)
    }
}

/// Per-paragraph list membership after replacing `range` of the content with
/// `new`: the paragraph holding the start of the range keeps its membership
/// (so splitting a list item with Enter yields two items and joining keeps the
/// first), later paragraphs keep theirs. Call before the content changes and
/// pass the result to [`TextProps::set_paragraph_lists`] afterwards. `None`
/// when the text has no lists.
pub fn lists_for_splice(t: &TextProps, range: Range<usize>, new: &str) -> Option<Vec<Option<ListItem>>> {
    if t.lists.is_empty() {
        return None;
    }
    let before = t.paragraph_lists();
    let p0 = paragraph_index(&t.content, range.start);
    let p1 = paragraph_index(&t.content, range.end);
    let added = new.matches('\n').count();
    let mut out: Vec<Option<ListItem>> = before[..p0.min(before.len())].to_vec();
    let head = before.get(p0).copied().flatten();
    out.extend(std::iter::repeat_n(head, 1 + added));
    if p1 + 1 < before.len() {
        out.extend_from_slice(&before[p1 + 1..]);
    }
    Some(out)
}

fn map_offset(p: usize, a: usize, b: usize, new_len: usize) -> usize {
    if p <= a {
        p
    } else if p >= b {
        p + new_len - (b - a)
    } else {
        a + new_len
    }
}

/// Replaces `range` of the content, keeping runs and list ranges aligned. The
/// new text takes no style of its own beyond what surrounding runs give it
/// (the first replaced char's run is not extended); see the app's rich text
/// algebra for style aware edits.
pub fn replace_text_range(t: &mut TextProps, range: Range<usize>, new: &str) {
    let (a, b) = (range.start, range.end);
    let lists = lists_for_splice(t, range.clone(), new);
    let mut runs = Vec::with_capacity(t.runs.len());
    for r in &t.runs {
        let (start, end) = (map_offset(r.start, a, b, new.len()), map_offset(r.end, a, b, new.len()));
        if start < end {
            runs.push(TextRun { start, end, style: r.style.clone() });
        }
    }
    t.runs = runs;
    t.content.replace_range(range, new);
    if let Some(l) = lists {
        t.set_paragraph_lists(&l);
    }
}

/// Converts the old literal list prefixes ("• " and "1. 2. ...") at paragraph
/// starts to real lists and strips the prefixes. Bullets convert whenever they
/// appear; numbers only as a run counting 1, 2, 3... of at least two
/// consecutive paragraphs (a lone "1. " could just be text). Does nothing when
/// the text already has lists. Returns whether anything changed.
pub fn migrate_literal_lists(t: &mut TextProps) -> bool {
    if !t.lists.is_empty() {
        return false;
    }
    let paras = paragraph_ranges(&t.content);
    let mut found: Vec<Option<(ListKind, usize)>> = vec![None; paras.len()];
    for (i, p) in paras.iter().enumerate() {
        let s = &t.content[p.clone()];
        if s.starts_with("\u{2022} ") {
            found[i] = Some((ListKind::Bullet, "\u{2022} ".len()));
        }
    }
    // numbered runs
    let num = |s: &str| -> Option<(u32, usize)> {
        let digits = s.bytes().take_while(u8::is_ascii_digit).count();
        (digits > 0 && digits <= 4 && s[digits..].starts_with(". ")).then(|| (s[..digits].parse().unwrap_or(0), digits + 2))
    };
    let mut i = 0;
    while i < paras.len() {
        if found[i].is_none() {
            if let Some((1, _)) = num(&t.content[paras[i].clone()]) {
                let mut j = i;
                let mut want = 1;
                while j < paras.len() && found[j].is_none() && matches!(num(&t.content[paras[j].clone()]), Some((n, _)) if n == want) {
                    want += 1;
                    j += 1;
                }
                if j - i >= 2 {
                    for (k, slot) in found.iter_mut().enumerate().take(j).skip(i) {
                        let (_, len) = num(&t.content[paras[k].clone()]).unwrap();
                        *slot = Some((ListKind::Numbered, len));
                    }
                    i = j;
                    continue;
                }
            }
        }
        i += 1;
    }
    if found.iter().all(Option::is_none) {
        return false;
    }
    let items: Vec<Option<ListItem>> = found.iter().map(|f| f.map(|(kind, _)| ListItem { kind, level: 0 })).collect();
    // strip prefixes back to front so offsets stay valid
    for (p, f) in paras.iter().zip(&found).rev() {
        if let Some((_, len)) = f {
            replace_text_range(t, p.start..p.start + len, "");
        }
    }
    t.set_paragraph_lists(&items);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TextAutoResize, TextStyle};

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

    fn item(kind: ListKind, level: u8) -> Option<ListItem> {
        Some(ListItem { kind, level })
    }

    #[test]
    fn paragraph_lists_round_trip() {
        let mut t = props("a\nb\nc\nd");
        let items = vec![item(ListKind::Bullet, 0), item(ListKind::Bullet, 0), None, item(ListKind::Numbered, 1)];
        t.set_paragraph_lists(&items);
        assert_eq!(t.lists.len(), 2);
        assert_eq!((t.lists[0].start, t.lists[0].end), (0, 3));
        assert_eq!((t.lists[1].start, t.lists[1].end), (6, 7));
        assert_eq!(t.paragraph_lists(), items);
    }

    #[test]
    fn empty_paragraph_can_be_a_list_item() {
        let mut t = props("a\n\nb");
        let items = vec![item(ListKind::Bullet, 0), item(ListKind::Bullet, 0), item(ListKind::Bullet, 0)];
        t.set_paragraph_lists(&items);
        assert_eq!(t.paragraph_lists(), items);
        let mut t = props("a\n\nb");
        let items = vec![None, item(ListKind::Bullet, 0), None];
        t.set_paragraph_lists(&items);
        assert_eq!(t.paragraph_lists(), items);
    }

    #[test]
    fn numbering_restarts_and_nests() {
        let mut t = props("a\nb\nc\nd\ne\nf\ng");
        t.set_paragraph_lists(&[
            item(ListKind::Numbered, 0),
            item(ListKind::Numbered, 1),
            item(ListKind::Numbered, 1),
            item(ListKind::Numbered, 0),
            None,
            item(ListKind::Numbered, 0),
            item(ListKind::Bullet, 0),
        ]);
        let n: Vec<u32> = t.list_markers().iter().map(|m| m.map(|m| m.number).unwrap_or(99)).collect();
        assert_eq!(n, vec![1, 1, 2, 2, 99, 1, 0]);
        assert_eq!(number_label(0, 3), "3.");
        assert_eq!(number_label(1, 2), "b.");
        assert_eq!(number_label(1, 27), "aa.");
        assert_eq!(number_label(2, 4), "iv.");
    }

    #[test]
    fn splice_keeps_lists_aligned() {
        let mut t = props("one\ntwo\nthree");
        t.set_paragraph_lists(&[item(ListKind::Bullet, 0), item(ListKind::Bullet, 0), None]);
        // Enter at the end of "one" adds a list item
        replace_text_range(&mut t, 3..3, "\n");
        assert_eq!(t.paragraph_lists(), vec![item(ListKind::Bullet, 0), item(ListKind::Bullet, 0), item(ListKind::Bullet, 0), None]);
        // joining two paragraphs keeps the first one's membership
        replace_text_range(&mut t, 3..4, "");
        assert_eq!(t.content, "one\ntwo\nthree");
        assert_eq!(t.paragraph_lists(), vec![item(ListKind::Bullet, 0), item(ListKind::Bullet, 0), None]);
        // editing inside shifts byte ranges
        replace_text_range(&mut t, 1..2, "XXXX");
        assert_eq!(t.paragraph_lists(), vec![item(ListKind::Bullet, 0), item(ListKind::Bullet, 0), None]);
        assert_eq!(t.lists[0].end, "oXXXXe\ntwo".len());
    }

    #[test]
    fn migrates_literal_prefixes() {
        let mut t = props("\u{2022} one\n\u{2022} two\nplain\n1. a\n2. b\n3. c\n7. lone");
        assert!(migrate_literal_lists(&mut t));
        assert_eq!(t.content, "one\ntwo\nplain\na\nb\nc\n7. lone");
        let k: Vec<Option<ListKind>> = t.paragraph_lists().iter().map(|i| i.map(|i| i.kind)).collect();
        assert_eq!(
            k,
            vec![Some(ListKind::Bullet), Some(ListKind::Bullet), None, Some(ListKind::Numbered), Some(ListKind::Numbered), Some(ListKind::Numbered), None]
        );
        assert!(!migrate_literal_lists(&mut t), "already real lists");
        // a lone "1. " is ambiguous and stays text
        let mut t = props("1. only one");
        assert!(!migrate_literal_lists(&mut t));
        assert_eq!(t.content, "1. only one");
        // not counting up from 1
        let mut t = props("3. a\n4. b");
        assert!(!migrate_literal_lists(&mut t));
    }
}
