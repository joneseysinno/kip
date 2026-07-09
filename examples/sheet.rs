//! Topo-sort host pattern: evaluate independent sheet levels with [`eval_batch`].
//!
//! kip does not own dependency graphs — the host binds names, topo-sorts, and
//! calls `eval_batch` once per level so every expression in a level can run in parallel.

use std::collections::BTreeMap;
use std::sync::Arc;

use kip::{eval_batch, parse, MapResolver, RegistryBuilder, Value};

/// One row in a calc sheet: name → source expression.
struct SheetRow {
    name: String,
    src: String,
}

fn main() {
    let registry = RegistryBuilder::from_seed().freeze();

    // Level 0: no cross-row dependencies.
    let level0 = vec![
        SheetRow {
            name: "L".into(),
            src: "40 ft".into(),
        },
        SheetRow {
            name: "w".into(),
            src: "12 in".into(),
        },
    ];

    // Level 1: depends on level 0 symbols.
    let level1 = vec![SheetRow {
        name: "M".into(),
        src: "2 kip * L".into(),
    }];

    let mut bindings: BTreeMap<String, Value> = BTreeMap::new();

    for level in [level0, level1] {
        let mut resolver = MapResolver::new();
        for (name, value) in &bindings {
            resolver.insert(name.clone(), value.clone());
        }

        let parsed: Vec<Arc<kip::Expr>> = level
            .iter()
            .map(|row| parse(&row.src, &registry).expect("parse"))
            .collect();

        let refs: Vec<_> = parsed.iter().map(|e| e.as_ref()).collect();
        let results = eval_batch(refs, &registry, &resolver);

        for (row, result) in level.iter().zip(results) {
            let value = result.expect("eval");
            println!("{} = {:?}", row.name, value);
            bindings.insert(row.name.clone(), value);
        }
    }

  let _ = bindings; // host persists bindings however it likes
}
