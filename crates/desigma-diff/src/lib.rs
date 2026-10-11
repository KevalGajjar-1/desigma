//! Image comparison used to enforce pixel fidelity.

pub struct DiffReport {
    pub width: u32,
    pub height: u32,
    /// Pixels whose max channel delta exceeds the tolerance.
    pub mismatched: u64,
    pub ratio: f64,
}
