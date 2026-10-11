/// Everything that can go wrong while resolving a font. Never panics.
#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("network access unavailable: {0}")]
    Offline(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("font not found: {0}")]
    NotFound(String),
    #[error("font '{family}' is licensed '{license}', which is not an open license")]
    NotOpenSource { family: String, license: String },
    #[error("no TTF file available for '{0}'")]
    NoTtf(String),
    #[error("unexpected font metadata: {0}")]
    Metadata(String),
    #[error("cache I/O error: {0}")]
    Io(#[from] std::io::Error),
}
