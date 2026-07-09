//! Registry definition conformance (grammar-spec §8).

use kip::{ErrorCode, RegistryBuilder};
use num_rational::Ratio;
use num_traits::One;

#[test]
fn define_kip_kips_aliases() {
    let mut b = RegistryBuilder::from_seed();
    b.parse_defs("define my_kip, my_kips = 1000 lbf").unwrap();
    let reg = b.freeze();
    let u = reg.unit("my_kip").expect("my_kip");
    assert_eq!(u.anchor_ratio, Ratio::from_integer(1000));
    assert!(reg.unit("my_kips").is_some());
}

#[test]
fn dimension_and_primary_unit() {
    let mut b = RegistryBuilder::from_seed();
    b.parse_defs(
        "dimension Currency\ndefine USD, $ : Currency",
    )
    .unwrap();
    let reg = b.freeze();
    assert!(reg.custom_dimension("Currency").is_some());
    assert!(reg.unit("USD").is_some());
    assert!(reg.unit("$").is_some());
}

#[test]
fn define_cycle_both_reported() {
    let mut b = RegistryBuilder::from_seed();
    let err = b
        .parse_defs("define a = 2 b\ndefine b = 3 a")
        .unwrap_err();
    assert_eq!(err.diagnostic().code, ErrorCode::DefCycle.as_str());
}

#[test]
fn define_symbolic_free_symbol() {
    let mut b = RegistryBuilder::from_seed();
    let err = b.parse_defs("define x = 2 * L").unwrap_err();
    assert_eq!(err.diagnostic().code, ErrorCode::DefSymbolic.as_str());
}

#[test]
fn define_duplicate_unit() {
    let mut b = RegistryBuilder::from_seed();
    let err = b
        .parse_defs("define tonf = 2000 lbf\ndefine tonf = 3000 lbf")
        .unwrap_err();
    assert_eq!(err.diagnostic().code, ErrorCode::DupUnit.as_str());
}

#[test]
fn define_affine_rejected() {
    let mut b = RegistryBuilder::from_seed();
    let err = b.parse_defs("define degC = 1 K").unwrap_err();
    assert_eq!(err.diagnostic().code, ErrorCode::AffineDefine.as_str());
}

#[test]
fn anchor_length_to_ft() {
    let mut b = RegistryBuilder::from_seed();
    b.parse_defs("anchor Length = ft").unwrap();
    let reg = b.freeze();
    let ft = reg.unit("ft").unwrap();
    let inch = reg.unit("in").unwrap();
    assert_eq!(ft.anchor_ratio, Ratio::one());
    assert_eq!(inch.anchor_ratio, Ratio::new(1, 12));
}

#[test]
fn anchor_affine_rejected() {
    let mut b = RegistryBuilder::from_seed();
    let err = b.parse_defs("anchor Temperature = °F").unwrap_err();
    assert_eq!(err.diagnostic().code, ErrorCode::AnchorAffine.as_str());
}

#[test]
fn anchor_invariance_physical_ratios() {
    let mut default = RegistryBuilder::from_seed();
    default.parse_defs("define test_plf = 120 lbf/ft").unwrap();
    let reg_default = default.freeze();

    let mut reanchored = RegistryBuilder::from_seed();
    reanchored
        .parse_defs("define test_plf = 120 lbf/ft\nanchor Length = ft")
        .unwrap();
    let reg_ft = reanchored.freeze();

    let u1 = reg_default.unit("test_plf").unwrap();
    let u2 = reg_ft.unit("test_plf").unwrap();
    assert_eq!(u1.dimension, u2.dimension);
    assert!(u1.anchor_ratio > Ratio::from_integer(0));
    assert!(u2.anchor_ratio > Ratio::from_integer(0));
}

#[test]
fn dump_defs_round_trip_user_units() {
    let src = r#"
dimension Currency
define USD, $ : Currency
define tonf, tons = 2000 lbf
anchor Force = kip
"#;
    let mut b = RegistryBuilder::from_seed();
    b.parse_defs(src).unwrap();
    let reg = b.freeze();
    let dumped = reg.dump_defs();
    let mut b2 = RegistryBuilder::from_seed();
    b2.parse_defs(&dumped).unwrap();
    let reg2 = b2.freeze();
    for name in ["tonf", "USD", "$"] {
        let a = reg.unit(name).unwrap();
        let b = reg2.unit(name).unwrap();
        assert_eq!(a.anchor_ratio, b.anchor_ratio, "{name}");
        assert_eq!(a.dimension, b.dimension, "{name}");
    }
}
