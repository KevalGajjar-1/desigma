//! Fontsource API metadata (`GET https://api.fontsource.org/v1/fonts/{id}`).
//!
//! Shape (verified against the live API): `{ id, family, subsets[], weights[],
//! styles[], variable, license, variants: { "<weight>": { "<style>": {
//! "<subset>": { url: { ttf, woff, woff2 } } } } } }`.

use crate::FontError;
use serde_json::Value;
use std::collections::BTreeMap;

pub const API_BASE: &str = "https://api.fontsource.org/v1/fonts";

/// Fontsource id for a family: lowercase, spaces become '-'.
pub fn font_id(family: &str) -> String {
    family.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("-")
}

/// Metadata URL for a family.
pub fn metadata_url(family: &str) -> String {
    format!("{API_BASE}/{}", font_id(family))
}

/// Whether a Fontsource/SPDX license id is an open font license
/// (OFL, Apache, Ubuntu Font Licence, MIT, CC0).
pub fn is_open_license(license: &str) -> bool {
    let l = license.trim().to_uppercase();
    l.starts_with("OFL")
        || l.contains("OPEN FONT")
        || l.starts_with("APACHE")
        || l.starts_with("UFL")
        || l.contains("UBUNTU FONT")
        || l == "MIT"
        || l.starts_with("MIT-")
        || l.starts_with("CC0")
}

/// CSS font-matching rules for choosing among available weights.
pub fn nearest_weight(available: &[u16], wanted: u16) -> Option<u16> {
    if available.is_empty() {
        return None;
    }
    let mut sorted: Vec<u16> = available.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.contains(&wanted) {
        return Some(wanted);
    }
    let below_desc = || sorted.iter().rev().copied().filter(|w| *w < wanted);
    let above_asc = |lo: u16| sorted.iter().copied().filter(move |w| *w > wanted && *w >= lo);
    if wanted < 400 {
        below_desc().next().or_else(|| above_asc(0).next())
    } else if wanted > 500 {
        above_asc(0).next().or_else(|| below_desc().next())
    } else {
        // 400..=500: first try weights between wanted and 500, then below, then above 500.
        sorted.iter().copied().find(|w| *w > wanted && *w <= 500).or_else(|| below_desc().next()).or_else(|| above_asc(501).next())
    }
}

/// The variant chosen for a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub weight: u16,
    pub italic: bool,
    pub subset: String,
    pub url: String,
}

/// Parsed Fontsource metadata.
#[derive(Clone, Debug)]
pub struct FontMeta {
    pub id: String,
    pub family: String,
    pub license: String,
    pub variable: bool,
    pub subsets: Vec<String>,
    /// weight -> style ("normal"/"italic") -> subset -> TTF url (only variants that have a TTF).
    pub ttf: BTreeMap<u16, BTreeMap<String, BTreeMap<String, String>>>,
}

impl FontMeta {
    pub fn parse(json: &str) -> Result<Self, FontError> {
        let v: Value = serde_json::from_str(json).map_err(|e| FontError::Metadata(e.to_string()))?;
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let id = s("id").ok_or_else(|| FontError::Metadata("missing id".into()))?;
        let family = s("family").unwrap_or_else(|| id.clone());
        // `license` is a plain SPDX-ish string today; tolerate an object with `type`.
        let license = match v.get("license") {
            Some(Value::String(l)) => l.clone(),
            Some(Value::Object(o)) => o.get("type").and_then(Value::as_str).unwrap_or_default().to_string(),
            _ => String::new(),
        };
        let subsets = v.get("subsets").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
        let variants = v.get("variants").and_then(Value::as_object).ok_or_else(|| FontError::Metadata("missing variants".into()))?;
        let mut ttf: BTreeMap<u16, BTreeMap<String, BTreeMap<String, String>>> = BTreeMap::new();
        for (w, styles) in variants {
            let Ok(w) = w.parse::<u16>() else { continue };
            let Some(styles) = styles.as_object() else { continue };
            for (style, subs) in styles {
                let Some(subs) = subs.as_object() else { continue };
                for (subset, entry) in subs {
                    if let Some(url) = entry.pointer("/url/ttf").and_then(Value::as_str) {
                        ttf.entry(w).or_default().entry(style.clone()).or_default().insert(subset.clone(), url.to_string());
                    }
                }
            }
        }
        Ok(Self { id, family, license, variable: v.get("variable").and_then(Value::as_bool).unwrap_or(false), subsets, ttf })
    }

    pub fn is_open(&self) -> bool {
        is_open_license(&self.license)
    }

    /// Pick the TTF for `weight`/`italic`: italic falls back to normal if the
    /// family has no italics, the nearest weight is chosen by CSS rules, and the
    /// "latin" subset is preferred over the first one listed.
    pub fn pick(&self, weight: u16, italic: bool) -> Result<Pick, FontError> {
        let has_style = |style: &str| self.ttf.values().any(|m| m.contains_key(style));
        let style = if italic && has_style("italic") { "italic" } else { "normal" };
        let weights: Vec<u16> = self.ttf.iter().filter(|(_, m)| m.contains_key(style)).map(|(w, _)| *w).collect();
        let w = nearest_weight(&weights, weight).ok_or_else(|| FontError::NoTtf(self.family.clone()))?;
        let subs = &self.ttf[&w][style];
        let subset = if subs.contains_key("latin") {
            "latin".to_string()
        } else {
            self.subsets.iter().find(|s| subs.contains_key(*s)).or_else(|| subs.keys().next()).cloned().unwrap_or_default()
        };
        Ok(Pick { weight: w, italic: style == "italic", url: subs[&subset].clone(), subset })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARLITO: &str = include_str!("../tests/fixtures/fontsource_carlito.json");

    #[test]
    fn ids() {
        assert_eq!(font_id("JetBrains Mono"), "jetbrains-mono");
        assert_eq!(font_id("  DejaVu  Sans "), "dejavu-sans");
        assert_eq!(metadata_url("Roboto"), "https://api.fontsource.org/v1/fonts/roboto");
    }

    #[test]
    fn open_licenses() {
        for l in ["OFL-1.1", "Apache-2.0", "UFL-1.0", "MIT", "CC0-1.0", "SIL Open Font License 1.1"] {
            assert!(is_open_license(l), "{l}");
        }
        for l in ["", "Proprietary", "CC-BY-NC-4.0", "Commercial"] {
            assert!(!is_open_license(l), "{l}");
        }
    }

    #[test]
    fn css_weight_matching() {
        let a = [300, 400, 700];
        assert_eq!(nearest_weight(&a, 400), Some(400));
        assert_eq!(nearest_weight(&a, 500), Some(400)); // 500 -> 400 before going up
        assert_eq!(nearest_weight(&a, 600), Some(700)); // >500: ascending first
        assert_eq!(nearest_weight(&a, 900), Some(700));
        assert_eq!(nearest_weight(&a, 200), Some(300)); // <400: nothing below, go up
        assert_eq!(nearest_weight(&[100, 200, 300], 350), Some(300));
        assert_eq!(nearest_weight(&[400, 700], 300), Some(400));
        assert_eq!(nearest_weight(&[300, 600], 450), Some(300)); // 400..500: none in (450,500], below first
        assert_eq!(nearest_weight(&[300, 450, 600], 400), Some(450));
        assert_eq!(nearest_weight(&[600, 800], 500), Some(600)); // nothing below, above 500
        assert_eq!(nearest_weight(&[], 400), None);
    }

    #[test]
    fn parses_real_fixture() {
        let m = FontMeta::parse(CARLITO).unwrap();
        assert_eq!(m.id, "carlito");
        assert_eq!(m.family, "Carlito");
        assert_eq!(m.license, "OFL-1.1");
        assert!(m.is_open());
        assert!(!m.variable);
        assert_eq!(m.ttf.keys().copied().collect::<Vec<_>>(), vec![400, 700]);
        let p = m.pick(700, false).unwrap();
        assert_eq!(p.weight, 700);
        assert_eq!(p.subset, "latin");
        assert!(!p.italic);
        assert!(p.url.ends_with("latin-700-normal.ttf"), "{}", p.url);
        let p = m.pick(500, true).unwrap();
        assert!(p.italic);
        assert_eq!(p.weight, 400);
        assert!(p.url.ends_with("latin-400-italic.ttf"), "{}", p.url);
    }

    #[test]
    fn falls_back_when_no_latin_or_no_italic() {
        let json = r#"{"id":"x","family":"X","subsets":["cyrillic","greek"],"license":"Apache-2.0",
            "variants":{"400":{"normal":{"greek":{"url":{"ttf":"https://e/g.ttf"}},"cyrillic":{"url":{"ttf":"https://e/c.ttf"}}}}}}"#;
        let m = FontMeta::parse(json).unwrap();
        let p = m.pick(400, true).unwrap();
        assert!(!p.italic);
        assert_eq!(p.subset, "cyrillic"); // first of `subsets` that exists
        assert_eq!(p.url, "https://e/c.ttf");
    }

    #[test]
    fn rejects_garbage_and_missing_ttf() {
        assert!(matches!(FontMeta::parse("not json"), Err(FontError::Metadata(_))));
        assert!(matches!(FontMeta::parse("{}"), Err(FontError::Metadata(_))));
        let json = r#"{"id":"x","license":"OFL-1.1","variants":{"400":{"normal":{"latin":{"url":{"woff2":"https://e/a.woff2"}}}}}}"#;
        let m = FontMeta::parse(json).unwrap();
        assert!(matches!(m.pick(400, false), Err(FontError::NoTtf(_))));
    }

    #[test]
    fn license_as_object_is_tolerated() {
        let json = r#"{"id":"x","license":{"type":"OFL-1.1","url":"u"},"variants":{}}"#;
        assert_eq!(FontMeta::parse(json).unwrap().license, "OFL-1.1");
    }
}
