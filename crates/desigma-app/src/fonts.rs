//! Background loading of the open-source fonts a document uses.
//!
//! After a document is opened, every family/weight/style it references is
//! resolved through `desigma-fonts` (bundled, disk cache, then Fontsource) on a
//! worker thread. Results are registered with the renderer on the UI thread.

use desigma_doc::Document;
use desigma_fonts::{ensure_fonts_with, FontCache, FontReport};
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver};

#[derive(Default)]
pub struct FontLoader {
    pending: Option<Receiver<FontReport>>,
}

/// Outcome of a finished load, for the toast.
pub struct FontsLoaded {
    pub loaded: usize,
    pub failed: Vec<String>,
}

impl FontLoader {
    /// Starts resolving the document's fonts; a previous load is abandoned.
    pub fn load_for(&mut self, doc: &Document) {
        let requests = desigma_fonts::requests_for(doc);
        if requests.is_empty() {
            self.pending = None;
            return;
        }
        // The renderer's font database is thread-local, so check installed
        // families here on the UI thread, not in the worker.
        let installed: HashSet<String> = requests
            .iter()
            .map(|r| r.family.clone())
            .filter(|family| {
                desigma_render::resolve_font(family, 400, false).is_some_and(|f| f.family.eq_ignore_ascii_case(family))
            })
            .collect();
        let doc = doc.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let report = ensure_fonts_with(&doc, &FontCache::new(), |family| installed.contains(family));
            let _ = tx.send(report);
        });
        self.pending = Some(rx);
    }

    /// Registers finished fonts with the renderer. Returns `Some` once per load.
    pub fn poll(&mut self) -> Option<FontsLoaded> {
        let report = self.pending.as_ref()?.try_recv().ok()?;
        self.pending = None;
        let loaded = report.loaded.len();
        for f in report.loaded {
            crate::textmeasure::register_font(f.data.clone());
            desigma_render::register_font(f.data);
        }
        let failed = report.failed.iter().map(|(req, err)| format!("{} {}: {err}", req.family, req.weight)).collect();
        Some(FontsLoaded { loaded, failed })
    }

    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }
}
