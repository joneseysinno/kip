//! Remediation conformance tests (remediation-plan.md R0–R6).

use std::sync::Arc;

use kip::{
    eval, eval_checked, parse, Dimension, EmptyResolver, LintCode, MapResolver, Quantity,
    RegistryBuilder, Value, PARALLEL_THRESHOLD,
};
use num_rational::Ratio;
use num_traits::One;

fn pressure() -> kip::Dimension {
    kip::Dimension::single(kip::BaseDim::Force, Ratio::one())
        .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
        .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
}

fn reg() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn known(v: Value) -> Quantity {
    match v {
        Value::Known(q) => q,
        other => panic!("expected known, got {other:?}"),
    }
}

#[test]
fn ln2_times_ft_plus_ft_is_float() {
    let registry = reg();
    let expr = parse("ln(2) * 1 ft + 1 ft", &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    let q = known(outcome.value.expect("eval"));
    assert!(!q.is_exact());
    assert!((q.as_f64() - 1.693147).abs() < 1e-5);
    assert_eq!(q.unit.as_str(), "ft");
    assert!(outcome
        .lints
        .iter()
        .any(|l| l.diagnostic().code == LintCode::ExactnessLost.as_str()));
}

#[test]
fn sqrt_four_is_exact_two() {
    let registry = reg();
    let expr = parse("sqrt(4)", &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    let q = known(outcome.value.expect("eval"));
    assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(2)));
    assert!(outcome.lints.is_empty());
}

#[test]
fn sqrt_negative_float_errors() {
    let registry = reg();
    let mut resolver = MapResolver::new();
    resolver.insert(
        "x",
        Value::Known(
            Quantity::from_float(-4.0, kip::UnitExpr::one(), Dimension::dimensionless())
                .expect("float"),
        ),
    );
    let expr = parse("sqrt(x)", &registry).expect("parse");
    let err = eval(expr.as_ref(), &registry, &resolver).unwrap_err();
    assert_eq!(err.diagnostic().code, kip::ErrorCode::Eval.as_str());
}

#[test]
fn symbolic_minus_known_fc_minus_psi() {
    let registry = reg();
    let mut resolver = MapResolver::new();
    resolver.insert(
        "f'c",
        Value::Known(Quantity::from_int(
            4000,
            "psi",
            pressure(),
        )),
    );
    let expr = parse("f'c - 100 psi", &registry).expect("parse");
    let v = eval(expr.as_ref(), &registry, &EmptyResolver).expect("eval");
    match &v {
        Value::Known(q) => {
            assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(3900)));
            assert_eq!(q.unit.as_str(), "psi");
        }
        Value::Symbolic(s) => {
            assert!(s.text.contains("f'c"));
            assert!(s.text.contains("100 psi"));
        }
    }
    let bound = v.bind(&resolver).expect("bind");
    let q = known(bound);
    assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(3900)));
}

#[test]
fn affine_same_unit_add_lints() {
    let registry = reg();
    let expr = parse("32 °F + 10 °F", &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    let q = known(outcome.value.expect("eval"));
    assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(42)));
    assert!(outcome
        .lints
        .iter()
        .any(|l| l.diagnostic().code == LintCode::AffineDelta.as_str()));
}

#[test]
fn affine_cross_unit_add_errors() {
    let registry = reg();
    let expr = parse("32 °F + 5 °C", &registry).expect("parse");
    let err = eval(expr.as_ref(), &registry, &EmptyResolver).unwrap_err();
    assert_eq!(err.diagnostic().code, kip::ErrorCode::AffineMixed.as_str());
}

#[test]
fn affine_cross_unit_sub_in_left_unit() {
    let registry = reg();
    let expr = parse("70 °F - 20 °C", &registry).expect("parse");
    let q = known(eval(expr.as_ref(), &registry, &EmptyResolver).expect("eval"));
    assert_eq!(q.unit.as_str(), "°F");
    assert!(q.is_exact());
}

#[test]
fn lint_merge_deterministic_parallel_vs_serial() {
    let registry = reg();
    let src = "ln(2) + ln(3) + ln(5) + ln(7)";
    let expr = parse(src, &registry).expect("parse");
    let serial = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    let _ = PARALLEL_THRESHOLD;
    let parallel = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    let serial_codes: Vec<_> = serial
        .lints
        .iter()
        .map(|l| l.diagnostic().code.clone())
        .collect();
    let parallel_codes: Vec<_> = parallel
        .lints
        .iter()
        .map(|l| l.diagnostic().code.clone())
        .collect();
    assert_eq!(serial_codes, parallel_codes);
}
