//! Equation-pack TOML loader (M6 milestone).

#[cfg(feature = "packs")]
pub mod contract;
#[cfg(feature = "packs")]
pub mod dimensionalize;

/// Load equation packs from TOML (M6).
#[cfg(feature = "packs")]
#[allow(dead_code)]
pub fn load_packs(_src: &str) -> Result<(), crate::Diag> {
    Err(crate::diag::Diag::new(crate::diag::Diagnostic::error(
        crate::diag::ErrorCode::PackParse,
        "equation packs not yet implemented (M6 milestone)",
        crate::diag::Span::empty(0),
    )))
}

#[cfg(not(feature = "packs"))]
/// Stub when `packs` feature is disabled.
pub fn load_packs(_src: &str) -> Result<(), crate::Diag> {
    Err(crate::diag::Diag::new(crate::diag::Diagnostic::error(
        crate::diag::ErrorCode::PackParse,
        "equation packs require the `packs` feature",
        crate::diag::Span::empty(0),
    )))
}
