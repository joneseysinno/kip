//! UnitExpr simplification conformance (collect like terms, cancel).

use std::sync::Arc;

use kip::{eval, parse, EmptyResolver, RegistryBuilder};

fn reg() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn eval_unit(expr: &str) -> String {
    let reg = reg();
    let ast = parse(expr, &reg).unwrap();
    let val = eval(ast.as_ref(), &reg, &EmptyResolver).unwrap();
    let qty = val.quantity().unwrap();
    format!("{}", qty.unit)
}

// b * h^3  →  in^4  (not in·in^3)
#[test]
fn mul_pow_collects_like_unit() {
    assert_eq!(eval_unit("(1 in) * (1 in)^3"), "in^4");
}

// Stress formula: M*c/I  →  kip/in^2
#[test]
fn stress_formula_unit() {
    // M = 100 kip·in,  c = 6 in,  I = 200 in^4
    let reg = reg();
    let ast = parse("(100 kip*in * 6 in) / (200 in^4)", &reg).unwrap();
    let val = eval(ast.as_ref(), &reg, &EmptyResolver).unwrap();
    let qty = val.quantity().unwrap();
    assert_eq!(format!("{}", qty.unit), "kip/in^2");
}

// Complete cancellation  →  Dimensionless
#[test]
fn complete_cancel_is_dimensionless() {
    assert_eq!(eval_unit("(3 kip) / (3 kip)"), "1");
}

// Partial cancellation in denominator
#[test]
fn partial_cancel_in_denominator() {
    assert_eq!(eval_unit("(1 in^4) / (1 in^3)"), "in");
}

// Negative exponent in product  →  quotient form
#[test]
fn product_with_inverse() {
    // lbf / in^2 written as lbf * in^-2 equivalent
    assert_eq!(eval_unit("(1 lbf) / (1 in^2)"), "lbf/in^2");
}

// sqrt produces half-integer exponent
#[test]
fn sqrt_half_exponent() {
    // sqrt(in^2) = in
    let reg = reg();
    let ast = parse("sqrt(1 in^2)", &reg).unwrap();
    let val = eval(ast.as_ref(), &reg, &EmptyResolver).unwrap();
    let qty = val.quantity().unwrap();
    assert_eq!(format!("{}", qty.unit), "in");
}

// Dimensionless stays dimensionless
#[test]
fn dimensionless_stays_dimensionless() {
    assert_eq!(eval_unit("2 * 3"), "1");
}
