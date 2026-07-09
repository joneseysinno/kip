//! Concurrency conformance (M4+) — Miri and loom.

use std::sync::Arc;

use kip::{eval, EmptyResolver, RegistryBuilder};

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
fn eval_stub_is_deterministic_across_threads() {
    let reg = RegistryBuilder::from_seed().freeze();
    let expr = Arc::new(kip::Expr::hole());
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let expr = Arc::clone(&expr);
            let reg = Arc::clone(&reg);
            std::thread::spawn(move || {
                let resolver = EmptyResolver;
                eval(expr.as_ref(), &reg, &resolver)
                    .unwrap_err()
                    .to_string()
            })
        })
        .collect();
    let mut results = handles.into_iter().map(|h| h.join().unwrap());
    let first = results.next().unwrap();
    for result in results {
        assert_eq!(result, first);
    }
}
