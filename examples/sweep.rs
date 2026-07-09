//! Scenario sweep: partial-eval once, then [`Value::bind`] per scenario in parallel.
//!
//! When most symbols are fixed and one varies, evaluate to a symbolic residual once,
//! then bind the swept variable per scenario — cheaper than re-walking the full AST.

use kip::{eval, parse, MapResolver, Quantity, RegistryBuilder, Value};
use num_rational::Ratio;
use num_traits::One;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

fn pressure() -> kip::Dimension {
    kip::Dimension::single(kip::BaseDim::Force, Ratio::one())
        .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
        .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
}

fn main() {
    let registry = RegistryBuilder::from_seed().freeze();
    let expr = parse("2*f_r + f'c", &registry).expect("parse");

    let mut partial = MapResolver::new();
    partial.insert(
        "f_r",
        Value::Known(Quantity::from_int(450, "psi", pressure())),
    );

    let residual = eval(expr.as_ref(), &registry, &partial).expect("partial eval");
    let Value::Symbolic(_) = residual else {
        panic!("expected symbolic residual");
    };

    let sweep_psi: Vec<i32> = (3000..=8000).step_by(1000).collect();
    let scenarios: Vec<MapResolver> = sweep_psi
        .iter()
        .map(|&fc| {
            let mut r = partial.clone();
            r.insert(
                "f'c",
                Value::Known(Quantity::from_int(i128::from(fc), "psi", pressure())),
            );
            r
        })
        .collect();

    #[cfg(feature = "parallel")]
    let bound: Vec<_> = scenarios
        .par_iter()
        .map(|resolver| residual.bind(resolver).expect("bind"))
        .collect();

    #[cfg(not(feature = "parallel"))]
    let bound: Vec<_> = scenarios
        .iter()
        .map(|resolver| residual.bind(resolver).expect("bind"))
        .collect();

    for (fc, v) in sweep_psi.iter().zip(bound) {
        if let Value::Known(q) = v {
            println!("f'c = {fc} psi  =>  {} psi", q.as_f64());
        }
    }
}
