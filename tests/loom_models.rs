//! Loom concurrency models (plan M7). Run with `RUSTFLAGS='--cfg loom' cargo test --test loom_models`.
#![cfg(loom)]

use std::sync::Arc;

use kip::{eval_batch, parse, RegistryBuilder, EmptyResolver};

#[test]
fn loom_eval_batch_shared_registry() {
    loom::model(|| {
        let reg = Arc::new(RegistryBuilder::from_seed().freeze());
        let e1 = parse("1 ft", &reg).unwrap();
        let e2 = parse("6 in", &reg).unwrap();
        let reg2 = Arc::clone(&reg);

        let handle = loom::thread::spawn(move || {
            let exprs = [e1.as_ref(), e2.as_ref()];
            eval_batch(exprs, &reg2, &EmptyResolver)
        });

        let results = handle.join().unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.is_ok()));
    });
}

#[test]
fn loom_registry_generation_readers() {
    loom::model(|| {
        let reg = Arc::new(RegistryBuilder::from_seed().freeze());
        let reg2 = Arc::clone(&reg);

        let h1 = loom::thread::spawn(move || reg.unit("ft").map(|u| u.name.clone()));
        let h2 = loom::thread::spawn(move || reg2.unit("in").map(|u| u.name.clone()));

        assert_eq!(h1.join().unwrap(), Some("ft".into()));
        assert_eq!(h2.join().unwrap(), Some("in".into()));
    });
}
