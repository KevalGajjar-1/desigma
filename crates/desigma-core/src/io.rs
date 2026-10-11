//! `.desigma` file format: versioned, human-readable JSON.

use std::path::Path;

use crate::document::{Document, FORMAT_VERSION};

pub const FILE_EXTENSION: &str = "desigma";

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("could not access file: {0}")]
    Io(#[from] std::io::Error),
    #[error("file is not a valid Desigma document: {0}")]
    Parse(#[from] serde_json::Error),
    #[error(
        "file was saved by a newer version of Desigma (format {0}, supported {FORMAT_VERSION})"
    )]
    TooNew(u32),
}

pub fn to_string(doc: &Document) -> Result<String, FileError> {
    Ok(serde_json::to_string_pretty(doc)?)
}

pub fn from_str(s: &str) -> Result<Document, FileError> {
    let mut doc: Document = serde_json::from_str(s)?;
    if doc.format_version > FORMAT_VERSION {
        return Err(FileError::TooNew(doc.format_version));
    }
    doc.format_version = FORMAT_VERSION;
    doc.repair_ids();
    Ok(doc)
}

/// Writes atomically (temp file + rename) so a crash never leaves a half-written file.
pub fn save(doc: &Document, path: &Path) -> Result<(), FileError> {
    let data = to_string(doc)?;
    let tmp = path.with_extension(format!("{FILE_EXTENSION}.tmp"));
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Document, FileError> {
    from_str(&std::fs::read_to_string(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;
    use crate::{NodeKind, Rect, TextData};

    #[test]
    fn roundtrip_preserves_document() {
        let mut ed = Editor::default();
        ed.create_node(
            NodeKind::Frame { clip_content: true },
            Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        ed.create_node(
            NodeKind::Text(TextData::new("Hello")),
            Rect::new(10.0, 10.0, 40.0, 14.0),
        );
        let s = to_string(&ed.doc).unwrap();
        assert_eq!(from_str(&s).unwrap(), ed.doc);
    }

    #[test]
    fn rejects_newer_format() {
        let doc = Document {
            format_version: FORMAT_VERSION + 1,
            ..Document::default()
        };
        let s = serde_json::to_string(&doc).unwrap();
        assert!(matches!(from_str(&s), Err(FileError::TooNew(_))));
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("desigma-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.desigma");
        let doc = Document::default();
        save(&doc, &path).unwrap();
        assert_eq!(load(&path).unwrap(), doc);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
