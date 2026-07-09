//! Expression evaluator (pure function of expr, registry, resolver).

pub mod builtins;
pub mod constraint;
pub mod partial;
pub mod value;

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::parser::Expr;
use crate::registry::Registry;
use crate::resolver::Resolver;

pub use value::Value;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Evaluate a parsed expression against a frozen registry and symbol resolver.
///
/// M0 returns `E-EVAL` for all inputs; full evaluation lands in M4.
pub fn eval(_expr: &Expr, _registry: &Registry, _resolver: &dyn Resolver) -> Result<Value, Diag> {
    Err(Diag::new(Diagnostic::error(
        ErrorCode::Eval,
        "evaluator not yet implemented (M4 milestone)",
        Span::empty(0),
    )))
}

#[cfg(feature = "parallel")]
/// Evaluate many independent expressions concurrently (M7).
pub fn eval_batch<'a>(
    exprs: impl rayon::iter::IntoParallelIterator<Item = &'a Expr>,
    registry: &Registry,
    resolver: &dyn Resolver,
) -> Vec<Result<Value, Diag>> {
    exprs
        .into_par_iter()
        .map(|expr| eval(expr, registry, resolver))
        .collect()
}

#[cfg(feature = "parallel")]
/// Evaluate one expression under many resolver scenarios (M7).
pub fn eval_scenarios(
    expr: &Expr,
    registry: &Registry,
    scenarios: impl rayon::iter::IntoParallelIterator<Item = Box<dyn Resolver>>,
) -> Vec<Result<Value, Diag>> {
    scenarios
        .into_par_iter()
        .map(|resolver| eval(expr, registry, resolver.as_ref()))
        .collect()
}
