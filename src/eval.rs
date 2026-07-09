//! Expression evaluator (pure function of expr, registry, resolver).

pub mod affine;
pub mod builtins;
pub mod constraint;
pub mod known;
pub mod partial;
pub mod units;
pub mod value;

use crate::diag::Diag;
use crate::parser::Expr;
use crate::registry::Registry;
use crate::resolver::Resolver;

pub use value::Value;
pub use known::PARALLEL_THRESHOLD;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Evaluate a parsed expression against a frozen registry and symbol resolver.
pub fn eval(expr: &Expr, registry: &Registry, resolver: &dyn Resolver) -> Result<Value, Diag> {
    known::eval_known(expr, registry, resolver)
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
