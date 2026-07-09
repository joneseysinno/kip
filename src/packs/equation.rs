//! Loaded equation records and registry lookup.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::dim::Dimension;
use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::eval::value::EquationProvenance;
use crate::packs::contract::ArgContract;
use crate::parser::Expr;
use crate::quantity::UnitExpr;

/// One equation loaded from a pack (pre-parsed body, frozen contracts).
#[derive(Debug, Clone)]
pub struct EquationRecord {
    /// Lookup key (`ACI.fr`).
    pub path_key: String,
    /// Namespace segment (`ACI`).
    pub namespace: String,
    /// Equation id within namespace (`fr`).
    pub id: String,
    /// Pre-parsed, dimensionally consistent body.
    pub body: Arc<Expr>,
    /// Contract result unit as written.
    pub result_unit: UnitExpr,
    /// Cached result dimension.
    pub result_dim: Dimension,
    /// Named argument contracts in declaration order.
    pub args: BTreeMap<String, ArgContract>,
    /// Citation and pack metadata for calc sheets.
    pub provenance: EquationProvenance,
}

/// Immutable index of equations for one registry generation.
#[derive(Debug, Clone, Default)]
pub struct EquationRegistry {
    by_path: Arc<BTreeMap<String, Arc<EquationRecord>>>,
}

impl EquationRegistry {
    /// Empty registry (no packs loaded).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build from a map of path key → record.
    pub(crate) fn from_map(map: BTreeMap<String, Arc<EquationRecord>>) -> Self {
        Self {
            by_path: Arc::new(map),
        }
    }

    /// Lookup by path segments (`["ACI", "fr"]`).
    pub fn lookup(&self, path: &[String]) -> Option<&Arc<EquationRecord>> {
        let key = path.join(".");
        self.by_path.get(&key)
    }

    /// Iterate loaded equations.
    pub fn equations(&self) -> impl Iterator<Item = &Arc<EquationRecord>> {
        self.by_path.values()
    }

    /// Insert during pack loading (builder only).
    pub(crate) fn insert(
        map: &mut BTreeMap<String, Arc<EquationRecord>>,
        record: Arc<EquationRecord>,
    ) -> Result<(), Diag> {
        if map.contains_key(&record.path_key) {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::PackParse,
                format!("duplicate equation `{}`", record.path_key),
                Span::empty(0),
            )));
        }
        map.insert(record.path_key.clone(), record);
        Ok(())
    }
}
