//! Figma file support.
//! CONTRACT: `import_rest_json`, `import_fig`, `FigError` are used by the app.
//!
//! - [`import_rest_json`]: body of Figma REST `GET /v1/files/:key` (ideally
//!   requested with `geometry=paths`) -> [`Document`].
//! - [`import_fig`]: `.fig` binary (fig-kiwi container, optionally zip-wrapped).
//! - [`export_rest_json`]: [`Document`] -> REST-shaped JSON.
//! - [`image_refs`]: image hashes a document needs fetched separately
//!   (REST `GET /v1/files/:key/images`); `.fig` files embed them already.
//! - `rest` (cargo feature `net`): blocking REST client; the access token is
//!   always supplied by the caller at runtime and never stored or logged.

pub mod container;
mod export;
mod fig;
mod proto;
pub mod kiwi;
#[cfg(feature = "net")]
pub mod rest;
mod rest_import;
pub mod svgpath;
#[doc(hidden)]
pub mod testkit;

pub use desigma_doc::Document;
pub use export::export_rest_json;
pub use proto::{key_code, key_name};
pub use fig::{commands_blob_to_path, decode_fig_to_json, import_fig};
pub use rest_import::{image_refs, import_rest_json};

#[derive(Debug, thiserror::Error)]
pub enum FigError {
    #[error("invalid json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a fig file: {0}")]
    Format(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}
