/// Proprietary (or system-only) family -> open replacement. Keys are lowercase.
/// Extend this table to add more; order does not matter.
pub static SUBSTITUTIONS: &[(&str, &str)] = &[
    // Inter: the closest open match for the UI system fonts.
    ("sf pro", "Inter"),
    ("sf pro text", "Inter"),
    ("sf pro display", "Inter"),
    ("sf pro rounded", "Inter"),
    ("-apple-system", "Inter"),
    ("blinkmacsystemfont", "Inter"),
    ("system-ui", "Inter"),
    ("ui-sans-serif", "Inter"),
    ("segoe ui", "Inter"),
    ("helvetica neue", "Inter"),
    ("whyte", "Inter"),
    ("graphik", "Inter"),
    ("circular", "Inter"),
    ("circular std", "Inter"),
    ("proxima nova", "Inter"),
    // Metric-compatible (Liberation / Chrome OS core fonts).
    ("helvetica", "Arimo"),
    ("arial", "Arimo"),
    ("times", "Tinos"),
    ("times new roman", "Tinos"),
    ("courier", "Cousine"),
    ("courier new", "Cousine"),
    ("calibri", "Carlito"),
    ("cambria", "Caladea"),
    // Look-alikes.
    ("georgia", "Gelasio"),
    ("verdana", "DejaVu Sans"),
    ("menlo", "JetBrains Mono"),
    ("monaco", "JetBrains Mono"),
    ("sf mono", "JetBrains Mono"),
    ("consolas", "JetBrains Mono"),
    ("ui-monospace", "JetBrains Mono"),
    ("gotham", "Montserrat"),
    ("futura", "Jost"),
];

/// Open-source stand-in for a proprietary `family`, if we know one
/// (case-insensitive; surrounding quotes/whitespace ignored).
pub fn substitute(family: &str) -> Option<&'static str> {
    let key = family.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_lowercase();
    SUBSTITUTIONS.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_families() {
        assert_eq!(substitute("SF Pro Text"), Some("Inter"));
        assert_eq!(substitute("  sf pro display "), Some("Inter"));
        assert_eq!(substitute("-apple-system"), Some("Inter"));
        assert_eq!(substitute("Helvetica Neue"), Some("Inter"));
        assert_eq!(substitute("Helvetica"), Some("Arimo"));
        assert_eq!(substitute("Arial"), Some("Arimo"));
        assert_eq!(substitute("Times New Roman"), Some("Tinos"));
        assert_eq!(substitute("Courier New"), Some("Cousine"));
        assert_eq!(substitute("Georgia"), Some("Gelasio"));
        assert_eq!(substitute("Verdana"), Some("DejaVu Sans"));
        assert_eq!(substitute("Calibri"), Some("Carlito"));
        assert_eq!(substitute("Cambria"), Some("Caladea"));
        assert_eq!(substitute("Menlo"), Some("JetBrains Mono"));
        assert_eq!(substitute("Consolas"), Some("JetBrains Mono"));
        assert_eq!(substitute("Gotham"), Some("Montserrat"));
        assert_eq!(substitute("Futura"), Some("Jost"));
        assert_eq!(substitute("\"Segoe UI\""), Some("Inter"));
    }

    #[test]
    fn unknown_and_open_families_are_untouched() {
        assert_eq!(substitute("Roboto"), None);
        assert_eq!(substitute("Inter"), None);
        assert_eq!(substitute(""), None);
    }

    #[test]
    fn table_keys_are_lowercase_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in SUBSTITUTIONS {
            assert_eq!(*k, k.to_lowercase());
            assert!(seen.insert(*k), "duplicate {k}");
        }
    }
}
