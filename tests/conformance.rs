//! Grammar conformance tests (grammar-spec §8) — populated per milestone.

use kip::{RegistryBuilder, VERSION};

#[test]
fn crate_version_is_0_1_0() {
    assert_eq!(VERSION, "0.1.0");
}

#[test]
fn m0_seed_registry_loads() {
    let reg = RegistryBuilder::from_seed().freeze();
    assert!(reg.unit("in").is_some());
    assert!(reg.unit("lbf").is_some());
    assert!(reg.unit("ft").is_some());
    assert_eq!(reg.generation(), 0);
}

#[test]
fn m0_dimension_mul_is_dimensionless_for_matching_pairs() {
    use kip::{BaseDim, Dimension};
    use num_rational::Ratio;
    use num_traits::One;

    let length = Dimension::single(BaseDim::Length, Ratio::one());
    let pressure = length.div(&length);
    assert!(pressure.is_dimensionless());
}
