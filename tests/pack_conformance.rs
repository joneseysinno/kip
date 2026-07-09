//! Equation-pack conformance (grammar-spec §8, plan M6).

use std::sync::Arc;

use kip::{
    eval, parse, ErrorCode, RegistryBuilder, Symbol, Value, DEMO_PACK_TOML,
};
use num_rational::Ratio;
use num_traits::One;

fn reg_with_demo_pack() -> Arc<kip::Registry> {
    let mut b = RegistryBuilder::from_seed();
    b.load_packs(DEMO_PACK_TOML).expect("load demo pack");
    b.freeze()
}

fn pressure() -> kip::Dimension {
    kip::Dimension::single(kip::BaseDim::Force, Ratio::one())
        .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
        .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
}

#[test]
fn aci_fr_known_value() {
    let registry = reg_with_demo_pack();
    let expr = parse("ACI.fr(fc: 4000 psi, lambda: 1.0)", &registry).unwrap();
    let v = eval(expr.as_ref(), &registry, &kip::EmptyResolver).expect("eval");
    let q = match v {
        Value::Known(q) => q,
        other => panic!("expected known, got {other:?}"),
    };
    assert_eq!(q.unit.as_str(), "psi");
    // 7.5 * 1.0 * sqrt(4000) ≈ 474.34 psi (float via sqrt)
    let actual = q.as_f64();
    assert!(
        (actual - 474.34).abs() < 0.1,
        "expected ~474.34 psi, got {actual}"
    );
    assert!(q.provenance.is_some());
    let prov = q.provenance.as_ref().unwrap();
    assert_eq!(prov.namespace, "ACI");
    assert_eq!(prov.equation_id, "fr");
}

#[test]
fn aci_fr_positional_is_error() {
    let registry = reg_with_demo_pack();
    let expr = parse("ACI.fr(4000 psi, 1.0)", &registry).unwrap();
    let err = eval(expr.as_ref(), &registry, &kip::EmptyResolver).unwrap_err();
    assert_eq!(err.diagnostic().code, ErrorCode::CodePositional.as_str());
}

#[test]
fn aci_fr_symbolic_pins_fc_dimension() {
    let registry = reg_with_demo_pack();
    let expr = parse("ACI.fr(fc: f'c, lambda: 1.0)", &registry).unwrap();
    let v = eval(expr.as_ref(), &registry, &kip::EmptyResolver).expect("eval");
    let sym = match v {
        Value::Symbolic(s) => s,
        other => panic!("expected symbolic, got {other:?}"),
    };
    let fc = Symbol("f'c".into());
    assert_eq!(sym.constraints.dimension_of(&fc), Some(pressure()));
    assert!(sym.text.contains("f'c"));
}

#[test]
fn aci_fr_unknown_equation() {
    let registry = reg_with_demo_pack();
    let expr = parse("ACI.unknown(fc: 4000 psi)", &registry).unwrap();
    let err = eval(expr.as_ref(), &registry, &kip::EmptyResolver).unwrap_err();
    assert_eq!(err.diagnostic().code, ErrorCode::UnknownEq.as_str());
}

#[test]
fn aci_fr_range_error() {
    let registry = reg_with_demo_pack();
    let expr = parse("ACI.fr(fc: 1000 psi, lambda: 1.0)", &registry).unwrap();
    let err = eval(expr.as_ref(), &registry, &kip::EmptyResolver).unwrap_err();
    assert_eq!(err.diagnostic().code, ErrorCode::Range.as_str());
}

#[test]
fn aci_fr_contract_converts_ksi() {
    let registry = reg_with_demo_pack();
    let expr = parse("ACI.fr(fc: 4 ksi, lambda: 1.0)", &registry).unwrap();
    let v = eval(expr.as_ref(), &registry, &kip::EmptyResolver).expect("eval");
    let q = match v {
        Value::Known(q) => q,
        other => panic!("expected known, got {other:?}"),
    };
    // Same as 4000 psi call
    assert_eq!(q.unit.as_str(), "psi");
    assert!((q.as_f64() - 474.34).abs() < 0.1);
}

#[test]
fn demo_pack_dimensionalizes_constants() {
    let registry = reg_with_demo_pack();
    let eq = registry
        .equations()
        .lookup(&["ACI".into(), "fr".into()])
        .expect("fr loaded");
    // Body should contain dimensionalized factor, not bare 7.5 alone at root.
    let root = &eq.body.root_node().kind;
    assert!(
        !matches!(root, kip::ExprKind::Number { text, .. } if text == "7.5"),
        "body should be dimensionalized at load"
    );
}


#[test]
fn load_packs_standalone_round_trip() {
    let seed = RegistryBuilder::from_seed().freeze();
    let eqs = kip::load_packs(DEMO_PACK_TOML, &seed).expect("load");
    assert!(eqs.lookup(&["ACI".into(), "fr".into()]).is_some());
}
