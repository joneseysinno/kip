//! Evaluator conformance tests (grammar-spec §8 — known values).

use std::sync::Arc;

use kip::{
    eval, parse, Dimension, MapResolver, Quantity, RegistryBuilder, UnitExpr, Value,
};
use num_rational::Ratio;
use num_traits::{FromPrimitive, One};

fn reg() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn eval_expr(src: &str) -> Value {
    let registry = reg();
    let expr = parse(src, &registry).expect("parse");
    eval(expr.as_ref(), &registry, &kip::EmptyResolver).expect("eval")
}

fn known_qty(v: Value) -> Quantity {
    match v {
        Value::Known(q) => q,
        other => panic!("expected known quantity, got {other:?}"),
    }
}

#[test]
fn one_ft_plus_six_in_is_one_and_half_ft() {
    let q = known_qty(eval_expr("1 ft + 6 in"));
    assert_eq!(q.unit.as_str(), "ft");
    assert_eq!(q.exact_ratio(), Some(Ratio::new(3, 2)));
}

#[test]
fn six_in_plus_one_ft_is_eighteen_in() {
    let q = known_qty(eval_expr("6 in + 1 ft"));
    assert_eq!(q.unit.as_str(), "in");
    assert_eq!(q.exact_ratio(), Some(Ratio::from_i32(18).unwrap()));
}

#[test]
fn twelve_ft_minus_six_in_is_eleven_and_half_ft() {
    let q = known_qty(eval_expr("12 ft - 6 in"));
    assert_eq!(q.unit.as_str(), "ft");
    assert_eq!(q.exact_ratio(), Some(Ratio::new(23, 2)));
}

#[test]
fn nine_in_squared_literal() {
    let q = known_qty(eval_expr("9 in^2"));
    assert_eq!(q.exact_ratio(), Some(Ratio::from_i32(9).unwrap()));
    assert!(matches!(q.unit, UnitExpr::Pow { .. }));
}

#[test]
fn nine_in_spaced_caret_squared_is_eighty_one_in_squared() {
    let q = known_qty(eval_expr("9 in ^2"));
    assert_eq!(q.exact_ratio(), Some(Ratio::from_i32(81).unwrap()));
    assert!(matches!(q.unit, UnitExpr::Pow { .. }));
}

#[test]
fn sqrt_four_thousand_psi() {
    let q = known_qty(eval_expr("sqrt(4000 psi)"));
    assert!((q.as_f64() - 63.245_553_203_367_59).abs() < 1e-9);
    assert!(matches!(q.unit, UnitExpr::Pow { .. }));
}

#[test]
fn convert_to_round_trip_rational() {
    let registry = reg();
    let q = known_qty(eval_expr("12 ft"));
    let inches = q.convert_to(&UnitExpr::named("in"), &registry).unwrap();
    let back = inches.convert_to(&UnitExpr::named("ft"), &registry).unwrap();
    assert_eq!(back.exact_ratio(), q.exact_ratio());
    assert_eq!(back.unit.as_str(), "ft");
}

#[test]
fn affine_fahrenheit_addition() {
    let q = known_qty(eval_expr("32 °F + 10 °F"));
    assert_eq!(q.unit.as_str(), "°F");
    assert_eq!(q.exact_ratio(), Some(Ratio::from_i32(42).unwrap()));
}

#[test]
fn dimension_mismatch_on_add() {
    let registry = reg();
    let expr = parse("1 ft + 1 lbf", &registry).unwrap();
    let err = eval(expr.as_ref(), &registry, &kip::EmptyResolver).unwrap_err();
    assert_eq!(err.diagnostic().code, "E-DIM-MISMATCH");
}

#[test]
fn resolver_supplies_known_symbol() {
    let registry = reg();
    let expr = parse("2 * f_r", &registry).unwrap();
    let mut resolver = MapResolver::new();
    // Use proper pressure dimension
    let pressure = {
        use kip::BaseDim;
        Dimension::single(BaseDim::Force, Ratio::one())
            .div(&Dimension::single(BaseDim::Length, Ratio::one()))
            .div(&Dimension::single(BaseDim::Length, Ratio::one()))
    };
    resolver.insert(
        "f_r",
        Value::Known(Quantity::from_int(450, "psi", pressure)),
    );
    let v = eval(expr.as_ref(), &registry, &resolver).expect("eval");
    let q = known_qty(v);
    assert_eq!(q.exact_ratio(), Some(Ratio::from_i32(900).unwrap()));
}

#[test]
fn eval_ten_thousand_term_sum() {
    let mut src = String::from("1 ft");
    for _ in 0..9999 {
        src.push_str(" + 1 in");
    }
    let q = known_qty(eval_expr(&src));
    assert_eq!(q.unit.as_str(), "ft");
}
