//! GPU canvas renderer for desigma documents.
//! See `README.md` for the backend decision. The CPU renderer in
//! `desigma-render` stays the deterministic reference; this crate must match it
//! within the tolerance checked by the fidelity tests.

mod context;

pub use context::{GpuContext, GpuError};
