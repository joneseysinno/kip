//! Remediation-plan-3 (T-series) conformance rows.

use std::sync::Arc;

use kip::{
    convert_quantity, eval_checked, parse, BaseDim, Dimension, EmptyResolver, LintCode, LintSink,
    Mag, Quantity, RegistryBuilder, UnitExpr, UnitExponent, Value,
};
use num_rational::Ratio;
use num_traits::One;

fn reg() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn pressure_dim() -> Dimension {
    Dimension::single(BaseDim::Force, Ratio::one())
        .div(&Dimension::single(BaseDim::Length, Ratio::one()))
        .div(&Dimension::single(BaseDim::Length, Ratio::one()))
}

fn length_dim() -> Dimension {
    Dimension::single(BaseDim::Length, Ratio::one())
}

fn area_dim() -> Dimension {
    length_dim().mul(&length_dim())
}

#[test]
fn lbf_ft_neg2_to_psi_is_exact_two() {
    let registry = reg();
    let unit = UnitExpr::Product(vec![
        UnitExpr::named("lbf"),
        UnitExpr::Pow {
            base: Box::new(UnitExpr::named("ft")),
            exp: UnitExponent::Int(-2),
        },
    ]);
    let q = Quantity::new(
        Mag::exact(Ratio::from_integer(288)),
        unit,
        pressure_dim(),
    );
    let mut lints = LintSink::new();
    let converted = convert_quantity(
        &q,
        &UnitExpr::named("psi"),
        &registry,
        &mut lints,
        kip::Span::empty(0),
    )
    .expect("convert");
    assert_eq!(converted.exact_ratio(), Some(Ratio::from_integer(2)));
    assert_eq!(converted.unit.as_str(), "psi");
    assert!(lints.lints().is_empty());
}

#[test]
fn sqrt_four_ft_squared_to_in_is_exact_twenty_four() {
    let registry = reg();
    let expr = parse("sqrt(4 ft^2)", &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    assert!(outcome.lints.is_empty());
    let mut lints = LintSink::new();
    let converted = convert_quantity(
        &q,
        &UnitExpr::named("in"),
        &registry,
        &mut lints,
        kip::Span::empty(0),
    )
    .expect("convert");
    assert_eq!(converted.exact_ratio(), Some(Ratio::from_integer(24)));
    assert_eq!(converted.unit.as_str(), "in");
    assert!(lints.lints().is_empty());
}

#[test]
fn sqrt_one_ft_times_one_in_to_in_is_float_with_exactness_lint() {
    let registry = reg();
    let expr = parse("sqrt(1 ft * 1 in)", &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    let mut lints = LintSink::new();
    let converted = convert_quantity(
        &q,
        &UnitExpr::named("in"),
        &registry,
        &mut lints,
        kip::Span::empty(0),
    )
    .expect("convert");
    assert!(!converted.is_exact());
    assert_eq!(
        lints
            .lints()
            .iter()
            .filter(|l| l.diagnostic().code == LintCode::ExactnessLost.as_str())
            .count(),
        1
    );
}

#[test]
fn huge_anchor_factor_overflow_lints_without_panic() {
    let src = "define H = 20000000000000000000 in";
    let mut builder = RegistryBuilder::from_seed();
    builder.parse_defs(src).expect("defs");
    let registry = builder.freeze();
    let unit = UnitExpr::Product(vec![
        UnitExpr::named("H"),
        UnitExpr::named("H"),
    ]);
    let q = Quantity::new(Mag::exact(Ratio::one()), unit, area_dim());
    let target = UnitExpr::Pow {
        base: Box::new(UnitExpr::named("in")),
        exp: UnitExponent::Int(2),
    };
    let mut lints = LintSink::new();
    let converted = convert_quantity(
        &q,
        &target,
        &registry,
        &mut lints,
        kip::Span::empty(0),
    )
    .expect("convert");
    assert!(!converted.is_exact());
    assert!(
        lints
            .lints()
            .iter()
            .any(|l| l.diagnostic().code == LintCode::RationalOverflow.as_str()),
        "expected rational overflow lint"
    );
}

#[test]
fn scale_and_round_as_i128_pattern_absent() {
    for path in ["src/eval/units.rs", "src/eval/builtins.rs"] {
        let src = std::fs::read_to_string(path).unwrap_or_else(|_| panic!("read {path}"));
        assert!(!src.contains("SCALE"), "{path} must not contain SCALE laundering");
        assert!(
            !src.contains(".round() as i128"),
            "{path} must not contain .round() as i128 laundering"
        );
    }
}

#[test]
fn sin_30_deg_still_exactly_one_exactness_lint() {
    let registry = reg();
    let expr = parse("sin(30 deg)", &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    outcome.value.expect("eval");
    assert_eq!(
        outcome
            .lints
            .iter()
            .filter(|l| l.diagnostic().code == LintCode::ExactnessLost.as_str())
            .count(),
        1
    );
}
