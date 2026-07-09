//! Formatting conformance (plan M7).

use kip::{parse, Dimension, FmtOptions, Quantity, RegistryBuilder};
use num_rational::Ratio;
use num_traits::One;

fn reg() -> std::sync::Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn length() -> Dimension {
    Dimension::single(kip::BaseDim::Length, Ratio::one())
}

#[test]
fn prefer_ft_in_from_feet() {
    let registry = reg();
    let v = kip::eval(
        parse("11 ft + 6 in", &registry).unwrap().as_ref(),
        &registry,
        &kip::EmptyResolver,
    )
    .unwrap();
    let q = v.quantity().unwrap();
    let opts = FmtOptions {
        prefer_ft_in: true,
        ft_in_denominator: 16,
        ..Default::default()
    };
    let s = q.display(&registry, &opts);
    assert!(s.contains("11'"), "got {s}");
    assert!(s.contains("6"), "got {s}");
}

#[test]
fn preferred_unit_display() {
    let registry = reg();
    let expr = parse("1 ft", &registry).unwrap();
    let v = kip::eval(expr.as_ref(), &registry, &kip::EmptyResolver).unwrap();
    let q = v.quantity().unwrap();
    let opts = FmtOptions {
        preferred_unit: Some("in".into()),
        ..Default::default()
    };
    let s = q.display(&registry, &opts);
    assert!(s.starts_with("12"), "got {s}");
    assert!(s.contains("in"), "got {s}");
}

#[test]
fn float_precision() {
    let registry = reg();
    let expr = parse("sqrt(4000 psi)", &registry).unwrap();
    let q = kip::eval(expr.as_ref(), &registry, &kip::EmptyResolver)
        .unwrap()
        .quantity()
        .unwrap()
        .clone();
    let opts = FmtOptions {
        precision: 2,
        ..Default::default()
    };
    let s = q.display(&registry, &opts);
    assert!(s.contains("63.25"), "got {s}");
}

#[test]
fn format_quantity_api() {
    let registry = reg();
    let q = Quantity::from_int(18, "in", length());
    let opts = FmtOptions::calc_sheet();
    let s = kip::format_quantity(&q, &registry, &opts);
    assert_eq!(s, "1'6\"");
}

#[test]
fn ft_in_denominator_snapping() {
    let registry = reg();
    let v = kip::eval(
        parse("11 ft + 6 in", &registry).unwrap().as_ref(),
        &registry,
        &kip::EmptyResolver,
    )
    .unwrap();
    let q = v.quantity().unwrap();
    let opts = FmtOptions {
        prefer_ft_in: true,
        ft_in_denominator: 16,
        ..Default::default()
    };
    let s = q.display(&registry, &opts);
    assert!(s.contains("11'"), "got {s}");
    assert!(s.contains("6"), "got {s}");
}
