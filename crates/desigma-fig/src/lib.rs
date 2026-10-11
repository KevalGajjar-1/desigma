//! Figma file support.
//! CONTRACT: `import_rest_json`, `import_fig`, `FigError` are used by the app.

use desigma_doc::Document;

#[derive(Debug, thiserror::Error)]
pub enum FigError {
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a fig file: {0}")]
    Format(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

/// Imports the body of Figma REST `GET /v1/files/:key`.
pub fn import_rest_json(json: &str) -> Result<Document, FigError> {
    let _ = json;
    Err(FigError::Unsupported("REST import not implemented yet".into()))
}

/// Imports a `.fig` file (raw fig-kiwi or zip-wrapped) from bytes.
pub fn import_fig(bytes: &[u8]) -> Result<Document, FigError> {
    let _ = bytes;
    Err(FigError::Unsupported(".fig import not implemented yet".into()))
}
