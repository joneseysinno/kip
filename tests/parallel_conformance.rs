//! Parallel evaluation helpers (plan M7).

use std::sync::Arc;

use kip::{eval, eval_batch, eval_scenarios, parse, MapResolver, Quantity, RegistryBuilder, Value};
use num_rational::Ratio;
use num_traits::One;

fn reg() -> Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn pressure() -> kip::Dimension {
    kip::Dimension::single(kip::BaseDim::Force, Ratio::one())
        .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
        .div(&kip::Dimension::single(kip::BaseDim::Length, Ratio::one()))
}

#[test]
fn eval_batch_independent_expressions() {
    let registry = reg();
    let exprs: Vec<Arc<kip::Expr>> = ["1 ft + 6 in", "2 kip", "9 in^2"]
        .into_iter()
        .map(|s| parse(s, &registry).unwrap())
        .collect();
    let refs: Vec<_> = exprs.iter().map(|e| e.as_ref()).collect();
    let results = eval_batch(refs, &registry, &kip::EmptyResolver);
    assert_eq!(results.len(), 3);
    for r in results {
        assert!(r.is_ok());
    }
}

#[test]
fn eval_scenarios_same_expr() {
    let registry = reg();
    let expr = parse("f'c", &registry).unwrap();
    let scenarios: Vec<Box<dyn kip::Resolver>> = [3000_i32, 4000, 5000]
        .into_iter()
        .map(|fc| {
            let mut r = MapResolver::new();
            r.insert(
                "f'c",
                Value::Known(Quantity::from_int(i128::from(fc), "psi", pressure())),
            );
            Box::new(r) as Box<dyn kip::Resolver>
        })
        .collect();
    let results = eval_scenarios(expr.as_ref(), &registry, scenarios);
    assert_eq!(results.len(), 3);
    let mags: Vec<_> = results
        .into_iter()
        .map(|r| r.unwrap().quantity().unwrap().as_f64())
        .collect();
    assert_eq!(mags, vec![3000.0, 4000.0, 5000.0]);
}

#[test]
fn float_eval_bit_identical_across_threads() {
    let registry = reg();
    let expr = Arc::new(parse("sqrt(4000 psi)", &registry).unwrap());
    let mut first: Option<u64> = None;
    for _ in 0..1000 {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let expr = Arc::clone(&expr);
                let reg = Arc::clone(&registry);
                std::thread::spawn(move || {
                    let v = eval(expr.as_ref(), &reg, &kip::EmptyResolver).unwrap();
                    v.quantity().unwrap().as_f64().to_bits()
                })
            })
            .collect();
        for h in handles {
            let bits = h.join().unwrap();
            if let Some(f) = first {
                assert_eq!(bits, f, "non-deterministic float eval across threads");
            } else {
                first = Some(bits);
            }
        }
    }
}

#[test]
fn parallel_threshold_constant_is_reasonable() {
    assert!(kip::PARALLEL_THRESHOLD >= 8);
    assert!(kip::PARALLEL_THRESHOLD <= 256);
}
