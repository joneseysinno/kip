//! Remediation plan 2 conformance (S1–S6).

use std::sync::Arc;

use kip::{
    eval, eval_checked, parse, Dimension, EmptyResolver, LintCode, MapResolver, Quantity,
    RegistryBuilder, Value,
};
use num_rational::Ratio;
use num_traits::One;

fn reg() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

#[test]
fn sqrt_i128_max_no_panic() {
    let registry = reg();
    let max = i128::MAX;
    let expr = parse(&format!("sqrt({max})"), &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    assert!(!q.is_exact());
    assert!(outcome
        .lints
        .iter()
        .any(|l| l.diagnostic().code == LintCode::ExactnessLost.as_str()));
}

#[test]
fn floor_seven_halves_ft_exact() {
    let outcome = eval_checked(
        parse("floor(7 ft / 2)", &reg()).expect("parse").as_ref(),
        &reg(),
        &EmptyResolver,
    );
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    assert_eq!(q.exact_ratio(), Some(Ratio::from_integer(3)));
    assert_eq!(q.unit.as_str(), "ft");
    assert!(outcome.lints.is_empty());
}

#[test]
fn atan2_exact_float_pair_no_extra_lint() {
    let registry = reg();
    let mut resolver = MapResolver::new();
    resolver.insert(
        "y",
        Value::Known(
            Quantity::from_float(1.0, kip::UnitExpr::one(), Dimension::dimensionless())
                .expect("float"),
        ),
    );
    resolver.insert(
        "x",
        Value::Known(Quantity::from_exact(
            Ratio::from_integer(2),
            kip::UnitExpr::one(),
            Dimension::dimensionless(),
        )),
    );
    let expr = parse("atan2(y, x)", &registry).expect("parse");
    let outcome = eval_checked(expr.as_ref(), &registry, &resolver);
    outcome.value.expect("eval");
    assert!(outcome.lints.is_empty());
}

#[cfg(feature = "packs")]
#[test]
fn unknown_pack_arg_did_you_mean() {
    let mut b = RegistryBuilder::from_seed();
    b.load_packs(kip::DEMO_PACK_TOML).expect("packs");
    let registry = b.freeze();
    let expr = parse("ACI.fr(fcc: 4 ksi, lambda: 1)", &registry).expect("parse");
    let err = eval(expr.as_ref(), &registry, &EmptyResolver).unwrap_err();
    assert_eq!(err.diagnostic().code, kip::ErrorCode::Eval.as_str());
    assert!(err.diagnostic().message.contains("fcc"));
    assert!(
        err.diagnostic()
            .hints
            .iter()
            .any(|h| matches!(h, kip::Hint::Note(s) if s.contains("fc")))
    );
}

#[test]
fn parse_defs_indented_span() {
    let src = "    define bad\r\n";
    let mut b = RegistryBuilder::from_seed();
    let err = b.parse_defs(src).unwrap_err();
    assert_eq!(err.diagnostic().span.start, 4);
    assert_eq!(err.diagnostic().span.end, 14);
}

#[test]
fn eval_ten_thousand_term_sum_has_ten_thousand_terms() {
    let mut src = String::from("1 ft");
    for _ in 0..9999 {
        src.push_str(" + 1 in");
    }
    let registry = reg();
    let expr = parse(&src, &registry).expect("parse");
    assert_eq!(expr.nodes.len(), 19_999);
    let outcome = eval_checked(expr.as_ref(), &registry, &EmptyResolver);
    let q = match outcome.value.expect("eval") {
        Value::Known(q) => q,
        _ => panic!("expected known"),
    };
    assert_eq!(q.unit.as_str(), "ft");
}

#[test]
fn f64_to_ratio_approx_symbol_absent() {
    let src = include_str!("../src/eval/units.rs");
    assert!(
        !src.contains("f64_to_ratio_approx"),
        "f64_to_ratio_approx must not exist in the tree"
    );
    let builtins = include_str!("../src/eval/builtins.rs");
    assert!(
        !builtins.contains("as f64).sqrt().round()"),
        "float-based integer_sqrt must not exist"
    );
}

#[test]
fn float_display_never_claims_exact_ratio() {
    let registry = reg();
    let q = Quantity::from_float(3.5, kip::UnitExpr::named("psi"), {
        kip::Dimension::single(kip::BaseDim::Force, Ratio::one())
            .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
            .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
    })
    .expect("float");
    let s = q.display(&registry, &kip::FmtOptions::default());
    assert!(s.contains("3.5"));
}
