//! Per-argument unit contracts for equation packs.

use std::sync::Arc;

use crate::dim::Dimension;
use crate::eval::value::Quantity;
use crate::parser::Expr;
use crate::quantity::UnitExpr;

/// Whether an out-of-range argument is a lint or hard error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeSeverity {
    /// `L-RANGE` — evaluation may continue (not surfaced by `eval` v1).
    Lint,
    /// `E-RANGE` — evaluation fails.
    Error,
}

/// Declared validity range for an argument (in contract units).
#[derive(Debug, Clone)]
pub struct ArgRange {
    /// Inclusive minimum, if any.
    pub min: Option<Quantity>,
    /// Inclusive maximum, if any.
    pub max: Option<Quantity>,
    /// How to treat violations.
    pub severity: RangeSeverity,
}

/// Contract for one named pack-equation argument.
#[derive(Debug, Clone)]
pub struct ArgContract {
    /// Parameter name (`fc`, `lambda`, …).
    pub name: String,
    /// Required unit expression (`psi`, `1`, …).
    pub unit: UnitExpr,
    /// Cached dimension of `unit`.
    pub dim: Dimension,
    /// Optional validity range.
    pub range: Option<ArgRange>,
    /// Pre-parsed default expression (dimensionless modifiers only).
    pub default_expr: Option<Arc<Expr>>,
}
