//! Opening and saving documents.

use desigma_doc::Document;
use std::path::{Path, PathBuf};

pub const NATIVE_EXT: &str = "desigma";

/// Loads a document from disk, dispatching on the file extension.
pub fn load_path(path: &Path) -> Result<Document, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut doc = match ext.as_str() {
        "fig" => {
            let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
            desigma_fig::import_fig(&bytes).map_err(|e| format!("Cannot import .fig: {e}"))?
        }
        "json" => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
            match desigma_fig::import_rest_json(&text) {
                Ok(d) => d,
                Err(fig_err) => Document::from_json(&text)
                    .map_err(|e| format!("Not a Figma REST file ({fig_err}) nor a desigma file ({e})"))?,
            }
        }
        _ => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
            Document::from_json(&text).map_err(|e| format!("Invalid desigma file: {e}"))?
        }
    };
    if doc.name.is_empty() {
        doc.name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string();
    }
    ensure_page(&mut doc);
    Ok(doc)
}

pub fn ensure_page(doc: &mut Document) {
    doc.pages.retain(|p| doc.nodes.contains_key(p));
    if doc.pages.is_empty() {
        doc.add_page("Page 1");
    }
}

/// Writes the native format; returns the final path (extension enforced).
pub fn save_path(doc: &Document, path: &Path) -> Result<PathBuf, String> {
    let mut p = path.to_path_buf();
    if p.extension().is_none() {
        p.set_extension(NATIVE_EXT);
    }
    let json = doc.to_json().map_err(|e| format!("Cannot serialize: {e}"))?;
    std::fs::write(&p, json).map_err(|e| format!("Cannot write {}: {e}", p.display()))?;
    Ok(p)
}

pub fn pick_open() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("Design files", &["fig", "json", "desigma"])
        .add_filter("Figma file", &["fig"])
        .add_filter("JSON", &["json"])
        .add_filter("desigma", &[NATIVE_EXT])
        .pick_file()
}

pub fn pick_save(default_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().add_filter("desigma", &[NATIVE_EXT]).set_file_name(format!("{default_name}.{NATIVE_EXT}")).save_file()
}

pub fn pick_export(default_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name(format!("{default_name}.png")).save_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::{Node, Rect};

    fn tmp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("desigma_test_{}_{name}", std::process::id()));
        p
    }

    #[test]
    fn native_roundtrip() {
        let mut d = Document::new("Saved");
        let page = d.add_page("P");
        let id = d.alloc_id();
        d.insert(page, Node::rectangle(id, Rect::new(1.0, 2.0, 3.0, 4.0))).unwrap();
        let p = save_path(&d, &tmp("a")).unwrap();
        assert_eq!(p.extension().unwrap(), NATIVE_EXT);
        let back = load_path(&p).unwrap();
        assert_eq!(back.name, "Saved");
        assert_eq!(back.nodes.len(), 2);
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn json_falls_back_to_native() {
        let mut d = Document::new("J");
        d.add_page("P");
        let p = tmp("b.json");
        std::fs::write(&p, d.to_json().unwrap()).unwrap();
        let back = load_path(&p).unwrap();
        assert_eq!(back.name, "J");
        std::fs::remove_file(p).ok();
    }

    #[test]
    fn errors_are_readable() {
        let p = tmp("c.desigma");
        std::fs::write(&p, "not json").unwrap();
        assert!(load_path(&p).unwrap_err().contains("Invalid desigma file"));
        std::fs::remove_file(p).ok();
        assert!(load_path(Path::new("/definitely/missing.fig")).is_err());
    }

    #[test]
    fn empty_doc_gets_a_page() {
        let mut d = Document::new("E");
        ensure_page(&mut d);
        assert_eq!(d.pages.len(), 1);
    }
}
