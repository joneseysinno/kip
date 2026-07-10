//! Builtin exactness conformance (remediation-plan-2 S0).

use std::sync::Arc;

use kip::{
    eval_checked, parse, Dimension, EmptyResolver, LintCode, Quantity, RegistryBuilder, Value,
};
use num_rational::Ratio;
use proptest::prelude::*;

fn reg() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn eval_builtin_expr(src: &str) -> kip::EvalOutcome {
    let registry = reg();
    let expr = parse(src, &registry).expect("parse");
    eval_checked(expr.as_ref(), &registry, &EmptyResolver)
}

fn assert_exact_or_linted(outcome: &kip::EvalOutcome) {
    let v = outcome.value.as_ref().expect("eval should succeed");
    match v {
        Value::Known(q) => {
            assert!(q.as_f64().is_finite(), "non-finite builtin result");
            if q.is_exact() {
                return;
            }
        }
        Value::Symbolic(_) => return,
    }
    assert!(
        outcome
            .lints
            .iter()
            .any(|l| {
                let c = l.diagnostic().code.as_str();
                c == LintCode::ExactnessLost.as_str()
                    || c == LintCode::RationalOverflow.as_str()
            }),
        "float builtin result without exactness lint"
    );
}

// --- Fixed regime rows (S0 §1.2) ---

#[test]
fn sqrt_large_perfect_square_is_exact() {
    let k = 10_000_000_000_000_000_000i128;
    let sq = k * k;
    let outcome = eval_builtin_expr(&format!("sqrt({sq})"));
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(k)));
    assert!(outcome.lints.is_empty());
}

#[test]
fn floor_preserves_exact_beyond_f64_integer_precision() {
    let n = (1i128 << 60) + 1;
    let registry = reg();
    let mut resolver = kip::MapResolver::new();
    resolver.insert(
        "x",
        Value::Known(Quantity::from_exact(
            Ratio::new(n, 2),
            kip::UnitExpr::one(),
            Dimension::dimensionless(),
        )),
    );
    let expr = parse("floor(x)", &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &resolver);
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(n >> 1)));
    assert!(outcome.lints.is_empty());
}

#[test]
fn round_half_away_from_zero_positive() {
    let outcome = eval_builtin_expr("round(5/2)");
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(3)));
}

#[test]
fn round_half_away_from_zero_negative() {
    let outcome = eval_builtin_expr("round(-5/2)");
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(-3)));
}

#[test]
fn min_exact_distinguishes_sub_f64_resolution() {
    let registry = reg();
    let mut resolver = kip::MapResolver::new();
    let big = Ratio::new(10_000_000_000_000_000_000, 1);
    let small = big - Ratio::new(1, 1_000_000_000_000_000_000);
    resolver.insert(
        "a",
        Value::Known(Quantity::from_exact(
            big,
            kip::UnitExpr::one(),
            Dimension::dimensionless(),
        )),
    );
    resolver.insert(
        "b",
        Value::Known(Quantity::from_exact(
            small,
            kip::UnitExpr::one(),
            Dimension::dimensionless(),
        )),
    );
    let expr = parse("min(a, b)", &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &resolver);
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    assert_eq!(q.exact_ratio(), Some(small));
}

#[test]
fn sin_30_deg_exactness_lint_at_call_site() {
    let outcome = eval_builtin_expr("sin(30 deg)");
    outcome.value.expect("eval");
    assert!(!outcome.lints.is_empty());
    let lint = outcome
        .lints
        .iter()
        .find(|l| l.diagnostic().code == LintCode::ExactnessLost.as_str())
        .expect("exactness lint");
    assert!(lint.diagnostic().message.contains("sin"));
}

// --- Property (S0 §1.1) ---

proptest! {
    #[test]
    fn builtin_abs_exact_stays_exact(n in -10_000i128..10_000) {
        let src = format!("abs({n})");
        let outcome = eval_builtin_expr(&src);
        assert_exact_or_linted(&outcome);
        if let Value::Known(q) = outcome.value.expect("eval") {
            if n >= 0 {
                prop_assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(n)));
            }
        }
    }

    #[test]
    fn sqrt_perfect_square_stays_exact(k in 0i32..1_000_000) {
        let k = i128::from(k);
        let sq = k.checked_mul(k).expect("square");
        let src = format!("sqrt({sq})");
        let outcome = eval_builtin_expr(&src);
        let q = match outcome.value.expect("eval") {
            Value::Known(q) => q,
            _ => panic!("expected known"),
        };
        prop_assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(k)));
        prop_assert!(outcome.lints.is_empty());
    }

    #[test]
    fn floor_small_exact_integers_stay_exact(n in -10_000i128..10_000) {
        let src = format!("floor({n})");
        let outcome = eval_builtin_expr(&src);
        assert_exact_or_linted(&outcome);
        if let Value::Known(q) = outcome.value.expect("eval") {
            prop_assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(n)));
        }
    }
}
