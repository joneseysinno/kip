//! Builtin exactness conformance (remediation-plan-2 S0, plan-3 T4).

use std::sync::Arc;

use kip::{
    convert_quantity, eval_checked, parse, BaseDim, Dimension, EmptyResolver, LintCode, LintSink,
    Mag, Quantity, RegistryBuilder, UnitExpr, UnitExponent, Value,
};
use num_rational::Ratio;
use num_traits::One;
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

fn length_dim() -> Dimension {
    Dimension::single(BaseDim::Length, Ratio::one())
}

fn pressure_dim() -> Dimension {
    Dimension::single(BaseDim::Force, Ratio::one())
        .div(&Dimension::single(BaseDim::Length, Ratio::one()))
        .div(&Dimension::single(BaseDim::Length, Ratio::one()))
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

// --- Weighted magnitude strategy (T4) ---

fn weighted_magnitude() -> impl Strategy<Value = i128> {
    prop_oneof![
        4 => -10_000i128..10_000,
        2 => (1i128 << 53) - 2..(1i128 << 53) + 2,
        2 => 3_037_000_498i128..3_037_000_500,
        1 => Just(0i128),
    ]
}

fn unit_bearing_quantity() -> impl Strategy<Value = (i128, &'static str, Dimension)> {
    (
        weighted_magnitude(),
        prop_oneof![
            Just(("in", length_dim())),
            Just(("ft", length_dim())),
            Just(("psi", pressure_dim())),
            Just(("ksi", pressure_dim())),
        ],
    )
        .prop_map(|(n, (unit, dim))| (n, unit, dim))
}

proptest! {
    #[test]
    fn builtin_abs_exact_stays_exact(n in weighted_magnitude()) {
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
    fn sqrt_perfect_square_stays_exact(k in weighted_magnitude()) {
        prop_assume!(k >= 0);
        let sq = k.checked_mul(k);
        if let Some(sq) = sq {
            let src = format!("sqrt({sq})");
            let outcome = eval_builtin_expr(&src);
            if k <= 1_000_000 {
                let q = match outcome.value.as_ref().expect("eval") {
                    Value::Known(q) => q,
                    _ => panic!("expected known"),
                };
                prop_assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(k)));
                prop_assert!(outcome.lints.is_empty());
            } else {
                assert_exact_or_linted(&outcome);
            }
        }
    }

    #[test]
    fn floor_weighted_magnitudes_stay_exact_or_linted(n in weighted_magnitude()) {
        let src = format!("floor({n})");
        let outcome = eval_builtin_expr(&src);
        assert_exact_or_linted(&outcome);
    }

    #[test]
    fn min_max_unit_bearing_stays_exact_or_linted(
        (a, unit_a, dim_a) in unit_bearing_quantity(),
        (b, unit_b, dim_b) in unit_bearing_quantity(),
    ) {
        let registry = reg();
        let mut resolver = kip::MapResolver::new();
        if dim_a == dim_b {
            resolver.insert(
                "a",
                Value::Known(Quantity::from_int(a, unit_a, dim_a)),
            );
            resolver.insert(
                "b",
                Value::Known(Quantity::from_int(b, unit_b, dim_b)),
            );
            for op in ["min", "max"] {
                let expr = parse(&format!("{op}(a, b)"), &registry).expect("parse");
                let outcome = eval_checked(expr.as_ref(), &registry, &resolver);
                assert_exact_or_linted(&outcome);
            }
        }
    }

    #[test]
    fn ft_neg2_conversion_property(n in 1i32..1000) {
        let registry = reg();
        let unit = UnitExpr::Product(vec![
            UnitExpr::named("lbf"),
            UnitExpr::Pow {
                base: Box::new(UnitExpr::named("ft")),
                exp: UnitExponent::Int(-2),
            },
        ]);
        let q = Quantity::new(
            Mag::exact(Ratio::from_integer(i128::from(n))),
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
        assert_exact_or_linted(&kip::EvalOutcome {
            value: Ok(Value::Known(converted)),
            lints: lints.into_lints(),
        });
    }
}
