//! Expression evaluator (pure function of expr, registry, resolver).

pub mod affine;
pub mod builtins;
pub mod constraint;
pub mod known;
pub mod lint_sink;
pub mod mag;
pub mod partial;
pub mod rational;
pub mod units;
pub mod value;

use crate::diag::Diag;
use crate::parser::Expr;
use crate::registry::Registry;
use crate::resolver::Resolver;

pub use lint_sink::LintSink;
pub use mag::Mag;
pub use value::Value;
pub use known::PARALLEL_THRESHOLD;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Result of checked evaluation (value + non-fatal lints).
#[derive(Debug, Clone)]
pub struct EvalOutcome {
    /// Evaluated value or error.
    pub value: Result<Value, Diag>,
    /// Lints collected during evaluation.
    pub lints: Vec<Diag>,
}

/// Evaluate a parsed expression against a frozen registry and symbol resolver.
pub fn eval(expr: &Expr, registry: &Registry, resolver: &dyn Resolver) -> Result<Value, Diag> {
    eval_checked(expr, registry, resolver).value
}

/// Evaluate and collect lints (mirror of `lex_checked` / `parse_checked`).
pub fn eval_checked(
    expr: &Expr,
    registry: &Registry,
    resolver: &dyn Resolver,
) -> EvalOutcome {
    let mut lints = LintSink::new();
    let value = known::eval_known_checked(expr, registry, resolver, &mut lints);
    EvalOutcome {
        value,
        lints: lints.into_lints(),
    }
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
