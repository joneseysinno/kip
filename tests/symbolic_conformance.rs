//! Symbolic / partial evaluation conformance (grammar-spec §8).

use std::sync::Arc;

use kip::{
    eval, parse, BaseDim, Dimension, ErrorCode, MapResolver, Quantity, RegistryBuilder, Symbol,
    Value,
};
use num_rational::Ratio;
use num_traits::One;

fn reg() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn pressure() -> Dimension {
    Dimension::single(BaseDim::Force, Ratio::one())
        .div(&Dimension::single(BaseDim::Length, Ratio::one()))
        .div(&Dimension::single(BaseDim::Length, Ratio::one()))
}

#[test]
fn two_kip_times_l_is_symbolic() {
    let v = eval(
        parse("2 kip * L", &reg()).unwrap().as_ref(),
        &reg(),
        &kip::EmptyResolver,
    )
    .expect("eval");
    assert!(matches!(v, Value::Symbolic(_)));
    assert!(v.free_symbols().iter().any(|s| s.0 == "L"));
}

#[test]
fn partial_fold_two_f_r_plus_fc() {
    let registry = reg();
    let expr = parse("2*f_r + f'c", &registry).unwrap();
    let mut resolver = MapResolver::new();
    resolver.insert(
        "f_r",
        Value::Known(Quantity::from_int(450, "psi", pressure())),
    );
    let v = eval(expr.as_ref(), &registry, &resolver).expect("eval");
    let sym = match v {
        Value::Symbolic(s) => s,
        other => panic!("expected symbolic, got {other:?}"),
    };
    assert!(sym.text.contains("900"));
    assert!(sym.text.contains("psi"));
    assert!(sym.text.contains("f'c"));
    let fc = Symbol("f'c".into());
    assert_eq!(
        sym.constraints.dimension_of(&fc),
        Some(pressure())
    );
}

#[test]
fn sqrt_fc_plus_fc_is_dim_mismatch() {
    let registry = reg();
    let expr = parse("sqrt(f'c) + f'c", &registry).unwrap();
    let err = eval(expr.as_ref(), &registry, &kip::EmptyResolver).unwrap_err();
    assert_eq!(err.diagnostic().code, ErrorCode::DimMismatch.as_str());
}

#[test]
fn bind_resolves_remaining_symbol() {
    let registry = reg();
    let expr = parse("2*f_r + f'c", &registry).unwrap();
    let mut partial = MapResolver::new();
    partial.insert(
        "f_r",
        Value::Known(Quantity::from_int(450, "psi", pressure())),
    );
    let residual = eval(expr.as_ref(), &registry, &partial).expect("partial eval");
    let mut full = partial;
    full.insert(
        "f'c",
        Value::Known(Quantity::from_int(4000, "psi", pressure())),
    );
    let v = residual.bind(&full).expect("bind");
    let q = v.quantity().expect("fully known");
    assert_eq!(q.magnitude, Ratio::from_integer(4900));
    assert_eq!(q.unit.as_str(), "psi");
}

#[test]
fn symbolic_free_symbol_has_no_constraints_until_used() {
    let v = eval(
        parse("L", &reg()).unwrap().as_ref(),
        &reg(),
        &kip::EmptyResolver,
    )
    .expect("eval");
    assert!(v.constraints().symbol_dims.is_empty());
}
