//! Concurrency conformance (M4+) — Miri and loom.

use std::sync::Arc;

use kip::{eval, parse, EmptyResolver, RegistryBuilder};
use num_rational::Ratio;

#[test]
fn shared_arc_registry_is_readable_from_threads() {
    let reg = RegistryBuilder::from_seed().freeze();
    let reg = Arc::clone(&reg);
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let reg = Arc::clone(&reg);
            std::thread::spawn(move || reg.unit("ft").map(|u| u.name.clone()))
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), Some("ft".into()));
    }
}

#[test]
fn eval_is_deterministic_across_threads() {
    let reg = RegistryBuilder::from_seed().freeze();
    let expr = Arc::new(parse("12 ft - 6 in", &reg).expect("parse"));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let expr = Arc::clone(&expr);
            let reg = Arc::clone(&reg);
            std::thread::spawn(move || {
                let resolver = EmptyResolver;
                let v = eval(expr.as_ref(), &reg, &resolver).expect("eval");
                let q = v.quantity().expect("known");
                (q.magnitude, q.unit.as_str().to_string())
            })
        })
        .collect();
    let mut results = handles.into_iter().map(|h| h.join().unwrap());
    let first = results.next().unwrap();
    assert_eq!(first.0, Ratio::new(23, 2));
    assert_eq!(first.1, "ft");
    for result in results {
        assert_eq!(result, first);
    }
}

#[test]
fn eval_batch_matches_serial() {
    let reg = RegistryBuilder::from_seed().freeze();
    let srcs = ["1 ft + 6 in", "12 ft - 6 in", "sqrt(4000 psi)"];
    let exprs: Vec<_> = srcs
        .iter()
        .map(|s| parse(s, &reg).expect("parse"))
        .collect();
    let serial: Vec<_> = exprs
        .iter()
        .map(|e| eval(e.as_ref(), &reg, &EmptyResolver).expect("eval"))
        .collect();
    let refs: Vec<_> = exprs.iter().map(|e| e.as_ref()).collect();
    let batch = kip::eval_batch(refs, &reg, &EmptyResolver);
    assert_eq!(serial.len(), batch.len());
    for (a, b) in serial.iter().zip(batch) {
        assert_eq!(a, b.as_ref().unwrap());
    }
}
