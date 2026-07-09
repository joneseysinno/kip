//! Release hardening conformance (M8): anchor-invariant evaluation, fuzz corpus smoke.

use std::sync::Arc;

use kip::{eval, parse, EmptyResolver, RegistryBuilder, Value};

fn default_registry() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn shuffled_anchors_registry() -> Arc<kip::Registry> {
    let mut b = RegistryBuilder::from_seed();
    b.parse_defs(
        "anchor Length = ft
anchor Force = kip
anchor Time = hr
anchor Temperature = °R",
    )
    .expect("re-anchor");
    b.freeze()
}

fn eval_known(src: &str, reg: &kip::Registry) -> Value {
    let expr = parse(src, reg).expect("parse");
    eval(expr.as_ref(), reg, &EmptyResolver).expect("eval")
}

fn comparable_magnitude(v: &Value) -> f64 {
    v.quantity().expect("known").as_f64()
}

#[test]
fn anchor_invariance_eval_values_match() {
    let cases = [
        "12 ft - 6 in",
        "1 ft + 6 in",
        "2 kip + 500 lbf",
        "4000 psi",
        "sqrt(4000 psi)",
    ];
    let default = default_registry();
    let shuffled = shuffled_anchors_registry();
    for src in cases {
        let v0 = eval_known(src, &default);
        let v1 = eval_known(src, &shuffled);
        assert!(
            (comparable_magnitude(&v0) - comparable_magnitude(&v1)).abs() < 1e-9,
            "magnitude mismatch for `{src}`"
        );
        assert_eq!(
            v0.quantity().unwrap().dim,
            v1.quantity().unwrap().dim,
            "dimension mismatch for `{src}`"
        );
    }
}

#[test]
fn fuzz_corpus_lexer_seeds_never_panic() {
    let seeds = [
        "12'",
        "12'-",
        "12'-6",
        "12'-6 1/",
        "''",
        "2''",
        "f'c'",
        "1/0\"",
        "12 - 6",
        "4000 psi",
        "ACI.fr(fc: f'c, lambda: 1.0)",
    ];
    for seed in seeds {
        let _ = kip::lex_checked(seed);
    }
}

#[test]
fn fuzz_corpus_parser_seeds_never_panic() {
    let reg = default_registry();
    let seeds = [
        "1 ft + 6 in",
        "-2^2",
        "2^2^2",
        "sqrt(4000 psi)",
        "12 ft - 6 in",
        "2 kip * 12 ft",
    ];
    for seed in seeds {
        let _ = kip::parse_checked(seed, &reg, &EmptyResolver);
    }
}
