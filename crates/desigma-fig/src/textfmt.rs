//! Text lists, paragraph settings and OpenType features in Figma's formats.
//!
//! REST text nodes carry `lineTypes` (`NONE` / `ORDERED` / `UNORDERED`) and
//! `lineIndentations` (one entry per line, i.e. per paragraph; list items start
//! at indentation 1) and `style.opentypeFlags` (`{"TNUM": 1}`); the `.fig`
//! equivalents (`textData.lines`, `toggledOnOTFeatures`, `fontVariant*`) are
//! converted to the same REST shape by [`crate::fig`] before they get here.

use desigma_doc::{LeadingTrim, ListItem, ListKind, TextDirection, TextProps};
use serde_json::{json, Map, Value};

/// OpenType feature settings from a REST `opentypeFlags` object, tags lowercased and sorted.
pub(crate) fn features_from_flags(v: &Value) -> Vec<(String, u32)> {
    let Some(o) = v.as_object() else { return Vec::new() };
    let mut out: Vec<(String, u32)> = o
        .iter()
        .filter_map(|(k, val)| {
            let n = val.as_u64().or_else(|| val.as_f64().map(|f| f.max(0.0) as u64)).or_else(|| val.as_bool().map(u64::from))?;
            (k.len() == 4).then(|| (k.to_lowercase(), n.min(u32::MAX as u64) as u32))
        })
        .collect();
    out.sort();
    out
}

/// Merges `flags` over `base` (a tag set again replaces its value).
pub(crate) fn merge_features(base: &mut Vec<(String, u32)>, flags: Vec<(String, u32)>) {
    for (tag, val) in flags {
        match base.iter_mut().find(|(t, _)| *t == tag) {
            Some(slot) => slot.1 = val,
            None => base.push((tag, val)),
        }
    }
    base.sort();
}

/// REST `opentypeFlags` for a style's features; `None` when it has none.
pub(crate) fn flags_value(features: &[(String, u32)]) -> Option<Value> {
    if features.is_empty() {
        return None;
    }
    let m: Map<String, Value> = features.iter().map(|(t, v)| (t.to_uppercase(), json!(v))).collect();
    Some(Value::Object(m))
}

/// Paragraph settings (`paragraphIndent`, `listSpacing`, `hangingPunctuation`,
/// `hangingList`, `leadingTrim`, `textDirection`) from a REST `style` object.
pub(crate) fn apply_paragraph(t: &mut TextProps, st: &Value) {
    let num = |k: &str| st.get(k).and_then(Value::as_f64).map(|f| f as f32);
    if let Some(v) = num("paragraphIndent") {
        t.paragraph.indent = v.max(0.0);
    }
    if let Some(v) = num("listSpacing") {
        t.paragraph.list_spacing = v.max(0.0);
    }
    if let Some(b) = st.get("hangingPunctuation").and_then(Value::as_bool) {
        t.paragraph.hanging_punctuation = b;
    }
    if let Some(b) = st.get("hangingList").and_then(Value::as_bool) {
        t.paragraph.hanging_list = b;
    }
    match st.get("leadingTrim").and_then(Value::as_str) {
        Some("CAP_HEIGHT") => t.paragraph.leading_trim = LeadingTrim::CapHeight,
        Some("NONE") => t.paragraph.leading_trim = LeadingTrim::None,
        _ => {}
    }
    match st.get("textDirection").and_then(Value::as_str) {
        Some("RTL") => t.direction = TextDirection::Rtl,
        Some("LTR") => t.direction = TextDirection::Ltr,
        Some("AUTO") => t.direction = TextDirection::Auto,
        _ => {}
    }
}

/// Lists from the node level `lineTypes` / `lineIndentations` arrays.
pub(crate) fn apply_lines(t: &mut TextProps, v: &Value) {
    let Some(types) = v.get("lineTypes").and_then(Value::as_array) else { return };
    let indents: Vec<u64> =
        v.get("lineIndentations").and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_u64().unwrap_or(0)).collect()).unwrap_or_default();
    let paras = desigma_doc::paragraph_ranges(&t.content).len();
    let items: Vec<Option<ListItem>> = (0..paras)
        .map(|i| {
            let kind = match types.get(i).and_then(Value::as_str) {
                Some("ORDERED") => ListKind::Numbered,
                Some("UNORDERED") => ListKind::Bullet,
                _ => return None,
            };
            let level = indents.get(i).copied().unwrap_or(1).saturating_sub(1).min(desigma_doc::MAX_LIST_LEVEL as u64) as u8;
            Some(ListItem { kind, level })
        })
        .collect();
    t.set_paragraph_lists(&items);
}

/// Writes `lineTypes` / `lineIndentations` for a text node (only when it has lists).
pub(crate) fn export_lines(t: &TextProps, m: &mut Map<String, Value>) {
    if !t.has_lists() {
        return;
    }
    let items = t.paragraph_lists();
    let types: Vec<&str> = items
        .iter()
        .map(|i| match i {
            None => "NONE",
            Some(ListItem { kind: ListKind::Numbered, .. }) => "ORDERED",
            Some(ListItem { kind: ListKind::Bullet, .. }) => "UNORDERED",
        })
        .collect();
    let indents: Vec<u32> = items.iter().map(|i| i.map_or(0, |i| i.level as u32 + 1)).collect();
    m.insert("lineTypes".into(), json!(types));
    m.insert("lineIndentations".into(), json!(indents));
}

/// Adds the paragraph settings of `t` to a REST `style` object (non-default values only).
pub(crate) fn export_paragraph(t: &TextProps, style: &mut Map<String, Value>) {
    let p = &t.paragraph;
    if p.indent != 0.0 {
        style.insert("paragraphIndent".into(), json!(p.indent));
    }
    if p.list_spacing != 0.0 {
        style.insert("listSpacing".into(), json!(p.list_spacing));
    }
    if p.hanging_punctuation {
        style.insert("hangingPunctuation".into(), json!(true));
    }
    if p.hanging_list {
        style.insert("hangingList".into(), json!(true));
    }
    if p.leading_trim == LeadingTrim::CapHeight {
        style.insert("leadingTrim".into(), json!("CAP_HEIGHT"));
    }
    match t.direction {
        TextDirection::Auto => {}
        TextDirection::Ltr => {
            style.insert("textDirection".into(), json!("LTR"));
        }
        TextDirection::Rtl => {
            style.insert("textDirection".into(), json!("RTL"));
        }
    }
}

/// REST `lineTypes` / `lineIndentations` from kiwi `textData.lines` entries
/// (`lineType` PLAIN / ORDERED_LIST / UNORDERED_LIST, `indentationLevel`), plus the
/// writing direction (`sourceDirectionality`).
pub(crate) fn lines_from_kiwi(lines: &[Value], out: &mut Map<String, Value>, style: &mut Map<String, Value>) {
    let mut types = Vec::new();
    let mut indents = Vec::new();
    let mut any_list = false;
    let (mut rtl, mut ltr) = (0, 0);
    for l in lines {
        let ty = match l.get("lineType").and_then(Value::as_str) {
            Some("ORDERED_LIST") => "ORDERED",
            Some("UNORDERED_LIST") => "UNORDERED",
            _ => "NONE",
        };
        any_list |= ty != "NONE";
        types.push(ty);
        indents.push(l.get("indentationLevel").and_then(Value::as_u64).unwrap_or(0));
        match l.get("sourceDirectionality").and_then(Value::as_str) {
            Some("RTL") => rtl += 1,
            Some("LTR") => ltr += 1,
            _ => {}
        }
    }
    if any_list {
        out.insert("lineTypes".into(), json!(types));
        out.insert("lineIndentations".into(), json!(indents));
    }
    if rtl > 0 && ltr == 0 {
        style.insert("textDirection".into(), json!("RTL"));
    } else if ltr > 0 && rtl == 0 {
        style.insert("textDirection".into(), json!("LTR"));
    }
}

/// OpenType toggles of a kiwi text message as REST `opentypeFlags`.
pub(crate) fn flags_from_kiwi(o: &Value) -> Option<Value> {
    let mut m = Map::new();
    let list = |k: &str| o.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    for (key, val) in [("toggledOnOTFeatures", 1), ("toggledOffOTFeatures", 0)] {
        for f in list(key) {
            if let Some(tag) = f.as_str().filter(|t| t.len() == 4) {
                m.insert(tag.to_uppercase(), json!(val));
            }
        }
    }
    let name = |k: &str| o.get(k).and_then(Value::as_str);
    match name("fontVariantNumericFigure") {
        Some("OLDSTYLE_NUMS" | "OLDSTYLE") => {
            m.insert("ONUM".into(), json!(1));
        }
        Some("LINING_NUMS" | "LINING") => {
            m.insert("LNUM".into(), json!(1));
        }
        _ => {}
    }
    match name("fontVariantNumericSpacing") {
        Some("TABULAR_NUMS" | "TABULAR") => {
            m.insert("TNUM".into(), json!(1));
        }
        Some("PROPORTIONAL_NUMS" | "PROPORTIONAL") => {
            m.insert("PNUM".into(), json!(1));
        }
        _ => {}
    }
    match name("fontVariantNumericFraction") {
        Some("DIAGONAL_FRACTIONS" | "DIAGONAL") => {
            m.insert("FRAC".into(), json!(1));
        }
        Some("STACKED_FRACTIONS" | "STACKED") => {
            m.insert("AFRC".into(), json!(1));
        }
        _ => {}
    }
    if name("fontVariantLigatures") == Some("NO_COMMON_LIGATURES") {
        m.insert("LIGA".into(), json!(0));
    }
    (!m.is_empty()).then_some(Value::Object(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::{TextAutoResize, TextStyle};

    fn text(s: &str) -> TextProps {
        TextProps { content: s.into(), style: TextStyle::default(), auto_resize: TextAutoResize::WidthAndHeight, ..Default::default() }
    }

    #[test]
    fn flags_round_trip() {
        let f = vec![("liga".to_string(), 0), ("ss01".to_string(), 1), ("tnum".to_string(), 1)];
        let v = flags_value(&f).unwrap();
        assert_eq!(v["TNUM"], 1);
        assert_eq!(features_from_flags(&v), f);
        assert!(flags_value(&[]).is_none());
        let mut base = vec![("tnum".to_string(), 1)];
        merge_features(&mut base, vec![("tnum".to_string(), 0), ("zero".to_string(), 1)]);
        assert_eq!(base, vec![("tnum".to_string(), 0), ("zero".to_string(), 1)]);
    }

    #[test]
    fn lines_round_trip() {
        let mut t = text("a\nb\nc\nd");
        t.set_paragraph_lists(&[
            Some(ListItem { kind: ListKind::Bullet, level: 0 }),
            Some(ListItem { kind: ListKind::Numbered, level: 1 }),
            None,
            Some(ListItem { kind: ListKind::Numbered, level: 2 }),
        ]);
        let mut m = Map::new();
        export_lines(&t, &mut m);
        assert_eq!(m["lineTypes"], json!(["UNORDERED", "ORDERED", "NONE", "ORDERED"]));
        assert_eq!(m["lineIndentations"], json!([1, 2, 0, 3]));
        let mut back = text("a\nb\nc\nd");
        apply_lines(&mut back, &Value::Object(m));
        assert_eq!(back.lists, t.lists);
        // no lists: nothing written
        let mut m = Map::new();
        export_lines(&text("x"), &mut m);
        assert!(m.is_empty());
    }

    #[test]
    fn paragraph_round_trip() {
        let mut t = text("x");
        t.paragraph.indent = 12.0;
        t.paragraph.list_spacing = 3.0;
        t.paragraph.hanging_punctuation = true;
        t.paragraph.hanging_list = true;
        t.paragraph.leading_trim = LeadingTrim::CapHeight;
        t.direction = TextDirection::Rtl;
        let mut st = Map::new();
        export_paragraph(&t, &mut st);
        let mut back = text("x");
        apply_paragraph(&mut back, &Value::Object(st));
        assert_eq!(back.paragraph, t.paragraph);
        assert_eq!(back.direction, TextDirection::Rtl);
        let mut st = Map::new();
        export_paragraph(&text("x"), &mut st);
        assert!(st.is_empty(), "defaults write nothing");
    }

    #[test]
    fn kiwi_conversions() {
        let lines = vec![
            json!({"lineType": "UNORDERED_LIST", "indentationLevel": 1, "sourceDirectionality": "RTL"}),
            json!({"lineType": "PLAIN", "indentationLevel": 0, "sourceDirectionality": "RTL"}),
        ];
        let (mut out, mut style) = (Map::new(), Map::new());
        lines_from_kiwi(&lines, &mut out, &mut style);
        assert_eq!(out["lineTypes"], json!(["UNORDERED", "NONE"]));
        assert_eq!(out["lineIndentations"], json!([1, 0]));
        assert_eq!(style["textDirection"], "RTL");
        let o = json!({"toggledOnOTFeatures": ["SS01"], "toggledOffOTFeatures": ["LIGA"], "fontVariantNumericSpacing": "TABULAR_NUMS"});
        let f = features_from_flags(&flags_from_kiwi(&o).unwrap());
        assert_eq!(f, vec![("liga".to_string(), 0), ("ss01".to_string(), 1), ("tnum".to_string(), 1)]);
    }
}
