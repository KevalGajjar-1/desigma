//! Text measurement and caret geometry for the editor.
//!
//! Everything is derived from `desigma_render::text_geometry`, which shapes
//! with exactly the same code as the renderer (same fonts, runs, case, wrap),
//! so carets and selections line up with the drawn glyphs. Positions are
//! UTF-8 byte offsets into `TextProps::content` and node-local pixels.

use desigma_doc as d;
use desigma_doc::{TextProps, Vec2};
use desigma_render::{text_geometry, LineGeom, TextGeometry};
use std::ops::Range;
use std::rc::Rc;

/// Natural size of the text: widest line by total height. `width` wraps lines.
pub fn measure(props: &TextProps, width: Option<f32>) -> Vec2 {
    let mut p = props.clone();
    let size = match width {
        Some(w) => {
            if p.auto_resize == d::TextAutoResize::WidthAndHeight {
                p.auto_resize = d::TextAutoResize::Height;
            }
            Vec2::new(w, 0.0)
        }
        None => {
            p.auto_resize = d::TextAutoResize::WidthAndHeight;
            Vec2::ZERO
        }
    };
    let g = text_geometry(&p, size);
    Vec2::new(g.width.ceil().max(1.0), g.height.ceil().max(1.0))
}

/// Geometry for editing: laid out in the node's real box.
pub fn geometry(props: &TextProps, size: Vec2) -> Rc<TextGeometry> {
    text_geometry(props, size)
}

/// Index of the visual line a caret at byte `at` is drawn on. A caret at a soft
/// wrap boundary belongs to the next line (as in Figma).
pub fn line_of(g: &TextGeometry, at: usize) -> usize {
    // Lines drop the whitespace they wrap at, so an offset in that gap stays at the end of its line.
    g.lines.iter().rposition(|l| l.start <= at).unwrap_or(0)
}

fn x_in_line(content: &str, l: &LineGeom, at: usize) -> f32 {
    let Some(first) = l.glyphs.first() else { return l.empty_x };
    let last = l.glyphs.last().unwrap();
    if at <= first.start {
        return first.x;
    }
    if at >= last.end {
        return last.x + last.w;
    }
    for g in &l.glyphs {
        if at == g.start {
            return g.x;
        }
        if at > g.start && at < g.end {
            // inside a cluster (ligature, combining marks): interpolate by chars
            let total = content[g.start..g.end].chars().count().max(1);
            let before = content[g.start..at.min(content.len())].chars().count();
            return g.x + g.w * before as f32 / total as f32;
        }
    }
    last.x + last.w
}

/// Caret rectangle (x, y, height) in node-local space for a byte offset.
pub fn caret_rect(props: &TextProps, size: Vec2, at: usize) -> (f32, f32, f32) {
    let g = geometry(props, size);
    let l = &g.lines[line_of(&g, at)];
    (x_in_line(&props.content, l, at), l.top, l.height)
}

/// Selection rectangles (x, y, w, h) for the byte range, one per visual line.
pub fn selection_rects(props: &TextProps, size: Vec2, range: Range<usize>) -> Vec<(f32, f32, f32, f32)> {
    let g = geometry(props, size);
    let mut out = Vec::new();
    if range.is_empty() {
        return out;
    }
    let n = g.lines.len();
    for (i, l) in g.lines.iter().enumerate() {
        let next_same_para = g.lines.get(i + 1).is_some_and(|nl| nl.para == l.para);
        // The newline ending a paragraph is selected too when the range runs past it.
        let nl_selected = !next_same_para && i + 1 < n && range.end > l.end && range.start <= l.end;
        let lo = range.start.max(l.start);
        let hi = range.end.min(l.end);
        if lo >= hi && !nl_selected {
            // empty line fully inside the range: show a small newline marker
            if l.glyphs.is_empty() && range.start <= l.start && range.end > l.start {
                out.push((l.empty_x, l.top, 4.0, l.height));
            }
            continue;
        }
        let x0 = if lo < hi { x_in_line(&props.content, l, lo) } else { l.glyphs.last().map(|g| g.x + g.w).unwrap_or(l.empty_x) };
        let mut x1 = if lo < hi { x_in_line(&props.content, l, hi) } else { x0 };
        if nl_selected {
            x1 += 4.0;
        }
        out.push((x0, l.top, (x1 - x0).max(0.0), l.height));
    }
    out
}

/// Byte offset nearest to the node-local point (x, y).
pub fn hit_caret(props: &TextProps, size: Vec2, x: f32, y: f32) -> usize {
    let g = geometry(props, size);
    let li = g.lines.iter().rposition(|l| l.top <= y).unwrap_or(0);
    let l = &g.lines[li];
    let wrapped = g.lines.get(li + 1).is_some_and(|n| n.para == l.para);
    let Some(first) = l.glyphs.first() else { return l.start };
    if x <= first.x {
        return first.start;
    }
    for gl in &l.glyphs {
        if x < gl.x + gl.w {
            // nearer edge; inside a cluster snap to its start/end
            return if x < gl.x + gl.w / 2.0 { gl.start } else { gl.end };
        }
    }
    let last = l.glyphs.last().unwrap();
    // a caret after a wrap-trailing space would render on the next line
    if wrapped && props.content[last.start..last.end].chars().all(char::is_whitespace) {
        last.start
    } else {
        last.end
    }
}

/// Start and end byte offsets of the visual line holding `at` (Home / End).
pub fn line_bounds(props: &TextProps, size: Vec2, at: usize) -> (usize, usize) {
    let g = geometry(props, size);
    let l = &g.lines[line_of(&g, at)];
    let wrapped = g.lines.get(line_of(&g, at) + 1).is_some_and(|n| n.para == l.para);
    let mut end = l.end;
    if wrapped {
        // keep the caret on this line: before a trailing space
        if let Some(last) = l.glyphs.last() {
            if props.content[last.start..last.end].chars().all(char::is_whitespace) {
                end = last.start;
            }
        }
    }
    (l.start, end)
}

/// Caret one visual line up (-1) or down (+1) keeping the column `goal_x`.
/// At the first/last line it goes to the start/end of the text.
pub fn move_vertical(props: &TextProps, size: Vec2, at: usize, dir: i32, goal_x: f32) -> usize {
    let g = geometry(props, size);
    let li = line_of(&g, at) as i32 + dir;
    if li < 0 {
        return 0;
    }
    if li as usize >= g.lines.len() {
        return props.content.len();
    }
    let l = &g.lines[li as usize];
    hit_caret(props, size, goal_x, l.top + l.height / 2.0)
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
            ..Default::default()
        }
    }

    const Z: Vec2 = Vec2 { x: 0.0, y: 0.0 };

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
    fn measure_follows_run_styles() {
        let mut p = props("ab cd", 12.0);
        let small = measure(&p, None);
        let mut big = p.style.clone();
        big.font_size = 48.0;
        p.runs = vec![d::TextRun { start: 3, end: 5, style: big }];
        let m = measure(&p, None);
        assert!(m.x > small.x * 1.8 && m.y > small.y * 2.0, "{small:?} -> {m:?}");
    }

    #[test]
    fn caret_follows_text() {
        let p = props("Hello\nWorld", 16.0);
        let (x0, y0, h) = caret_rect(&p, Z, 0);
        let (x3, _, _) = caret_rect(&p, Z, 3);
        let (xe, _, _) = caret_rect(&p, Z, 5);
        let (x6, y6, _) = caret_rect(&p, Z, 6);
        assert_eq!(x0, 0.0);
        assert!(x3 > x0 && xe > x3);
        assert_eq!(x6, 0.0);
        assert!(y6 >= y0 + h - 0.5);
    }

    #[test]
    fn caret_x_matches_glyph_positions() {
        // Caret x for every offset equals the shaped glyph edge: the sum of the
        // advances of the preceding glyphs, i.e. where the renderer draws them.
        let p = props("Hamburgefonstiv ijl", 20.0);
        let g = geometry(&p, Z);
        let glyphs = &g.lines[0].glyphs;
        let mut last = -1.0;
        for (i, gl) in glyphs.iter().enumerate() {
            let (x, _, _) = caret_rect(&p, Z, gl.start);
            assert_eq!(x, gl.x, "glyph {i}");
            assert!(x > last);
            last = x;
        }
        let end = p.content.len();
        let (xe, _, _) = caret_rect(&p, Z, end);
        let lastg = glyphs.last().unwrap();
        assert_eq!(xe, lastg.x + lastg.w);
        // and a measured width agrees with the caret at the end
        assert!((measure(&p, None).x - xe).abs() <= 1.0);
    }

    #[test]
    fn caret_x_matches_rendered_ink() {
        // The renderer's ink for the first n letters never extends past the caret after n.
        let p = props("Hello", 24.0);
        for n in 1..=5usize {
            let mut q = p.clone();
            q.content = p.content[..n].to_string();
            let ink = desigma_render::text_geometry(&q, Z);
            let (cx, _, _) = caret_rect(&p, Z, n);
            // (kerning with the following letter may shift the edge by a fraction of a px)
            assert!((ink.width - cx).abs() < 1.0, "n={n}: width {} caret {cx}", ink.width);
        }
    }

    #[test]
    fn hit_roundtrips_caret() {
        let p = props("Hello\nWorld", 16.0);
        for caret in [0usize, 2, 5, 6, 9, 11] {
            let (x, y, h) = caret_rect(&p, Z, caret);
            assert_eq!(hit_caret(&p, Z, x + 0.5, y + h / 2.0), caret, "caret {caret}");
        }
    }

    #[test]
    fn multibyte_offsets_roundtrip() {
        let p = props("aé😀中b", 18.0);
        for (caret, _) in p.content.char_indices().chain(std::iter::once((p.content.len(), ' '))) {
            let (x, y, h) = caret_rect(&p, Z, caret);
            assert_eq!(hit_caret(&p, Z, x + 0.01, y + h / 2.0), caret, "caret {caret}");
        }
    }

    #[test]
    fn wrapped_caret_moves_to_next_line() {
        let mut p = props("one two three four", 16.0);
        p.auto_resize = d::TextAutoResize::Height;
        let size = Vec2::new(70.0, 100.0);
        let g = geometry(&p, size);
        assert!(g.lines.len() >= 3);
        let l0 = &g.lines[0];
        // wrapping drops the space between the lines: the offset before it ends line 0,
        // the offset after it starts line 1
        let (_, y_end, _) = caret_rect(&p, size, l0.end);
        assert_eq!(y_end, g.lines[0].top);
        let (_, y_next, _) = caret_rect(&p, size, g.lines[1].start);
        assert_eq!(y_next, g.lines[1].top);
        let (hs, he) = line_bounds(&p, size, 2);
        assert_eq!((hs, he), (0, l0.end));
        // clicking far right of line 0 stays on line 0
        let hit = hit_caret(&p, size, 500.0, g.lines[0].top + 2.0);
        assert!(hit <= l0.end && line_of(&g, hit) == 0, "hit {hit}");
    }

    #[test]
    fn vertical_moves_keep_column() {
        let p = props("abcdef\nab\nabcdef", 16.0);
        let (gx, _, _) = caret_rect(&p, Z, 5);
        let down = move_vertical(&p, Z, 5, 1, gx);
        assert_eq!(down, 9, "short line clamps to its end");
        let down2 = move_vertical(&p, Z, down, 1, gx);
        assert_eq!(down2, 10 + 5);
        assert_eq!(move_vertical(&p, Z, 2, -1, 0.0), 0);
        assert_eq!(move_vertical(&p, Z, 12, 1, 0.0), p.content.len());
    }

    #[test]
    fn selection_rects_cover_lines() {
        let p = props("abc\ndef", 16.0);
        let r = selection_rects(&p, Z, 1..6);
        assert_eq!(r.len(), 2);
        assert!(r[0].0 > 0.0 && r[0].2 > 0.0);
        assert_eq!(r[1].0, 0.0);
        assert!(selection_rects(&p, Z, 3..3).is_empty());
    }

    #[test]
    fn empty_text_has_a_caret_line() {
        let p = props("", 16.0);
        let (x, y, h) = caret_rect(&p, Z, 0);
        assert_eq!((x, y), (0.0, 0.0));
        assert!(h >= 16.0);
        assert_eq!(hit_caret(&p, Z, 5.0, 5.0), 0);
    }

    #[test]
    fn centered_empty_line_sits_in_the_middle() {
        let mut p = props("a\n\nb", 16.0);
        p.style.align_h = d::TextAlignH::Center;
        p.auto_resize = d::TextAutoResize::Height;
        let size = Vec2::new(100.0, 80.0);
        let (x, _, _) = caret_rect(&p, size, 2);
        assert!((x - 50.0).abs() < 0.01, "{x}");
    }
}
