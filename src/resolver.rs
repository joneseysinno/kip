//! Symbol resolver trait for host-supplied bindings.

use std::collections::BTreeMap;

use crate::eval::Value;

/// Resolve free symbols to known or symbolic values during parse/eval.
///
/// Implementations must be `Send + Sync` so evaluation can run concurrently (P1).
pub trait Resolver: Send + Sync {
    /// Look up a symbol by name.
    fn resolve(&self, name: &str) -> Option<Value>;
}

/// Resolver that never binds symbols.
#[derive(Debug, Clone, Default)]
pub struct EmptyResolver;

impl Resolver for EmptyResolver {
    fn resolve(&self, _name: &str) -> Option<Value> {
        None
    }
}

/// Simple map-backed resolver for tests and examples.
#[derive(Debug, Clone, Default)]
pub struct MapResolver {
    bindings: BTreeMap<String, Value>,
}

impl MapResolver {
    /// Empty map resolver.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a binding.
    pub fn insert(&mut self, name: impl Into<String>, value: Value) {
        self.bindings.insert(name.into(), value);
    }
}

impl Resolver for MapResolver {
    fn resolve(&self, name: &str) -> Option<Value> {
        self.bindings.get(name).cloned()
    }
}
