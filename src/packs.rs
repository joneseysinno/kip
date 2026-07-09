//! Equation-pack TOML loader (M6 milestone).

pub mod call;
pub mod contract;
pub mod dimensionalize;
pub mod equation;
#[cfg(feature = "packs")]
pub mod loader;

pub use equation::EquationRegistry;

#[cfg(feature = "packs")]
pub use loader::{load_packs, load_packs_into, DEMO_PACK_TOML};

#[cfg(not(feature = "packs"))]
#[allow(missing_docs)]
pub const DEMO_PACK_TOML: &str = "";

#[cfg(feature = "packs")]
#[allow(dead_code)]
pub fn load_packs_standalone(
    src: &str,
    registry: &crate::Registry,
) -> Result<EquationRegistry, crate::Diag> {
    loader::load_packs(src, registry)
}

#[cfg(not(feature = "packs"))]
/// Stub when `packs` feature is disabled.
pub fn load_packs_standalone(_src: &str, _registry: &crate::Registry) -> Result<EquationRegistry, crate::Diag> {
    Err(crate::diag::Diag::new(crate::diag::Diagnostic::error(
        crate::diag::ErrorCode::PackParse,
        "equation packs require the `packs` feature",
        crate::diag::Span::empty(0),
    )))
}

#[cfg(not(feature = "packs"))]
#[allow(missing_docs)]
pub fn load_packs(_src: &str, _registry: &crate::Registry) -> Result<EquationRegistry, crate::Diag> {
    load_packs_standalone(_src, _registry)
}

#[cfg(not(feature = "packs"))]
#[allow(missing_docs)]
pub fn load_packs_into(_builder: &mut crate::RegistryBuilder, _src: &str) -> Result<(), crate::Diag> {
    Err(crate::diag::Diag::new(crate::diag::Diagnostic::error(
        crate::diag::ErrorCode::PackParse,
        "equation packs require the `packs` feature",
        crate::diag::Span::empty(0),
    )))
}
