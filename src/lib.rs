//! # kip — engineering expression evaluator (US imperial)
//!
//! kip is a pure, thread-safe evaluator for engineering expressions with exact
//! rational arithmetic, partial (symbolic) evaluation, and a user-extensible unit
//! registry anchored to **inch, lbf, second, Rankine** (plus angle and custom
//! base dimensions).
//!
//! ## Force-based (gravitational) system
//!
//! kip uses a **force-based** dimensional system common in structural engineering:
//! **Force** is a base dimension (anchor: `lbf`), and **mass** is derived
//! (`slug = lbf·s²/ft`). There is no hidden *g*<sub>c</sub> constant in user-visible
//! math. SI-trained users: do not expect mass to be fundamental here.
//!
//! ## Concurrency (P1)
//!
//! ASTs (`Expr`), registries (`Registry`), and evaluation are immutable and
//! `Send + Sync`. Evaluation is a pure function — no globals, no locks.
//!
//! ## Version 0.1.0 scope
//!
//! This release delivers the M0 skeleton: core data model, diagnostics, registry
//! seed scaffolding, and API contracts. Lexer, parser, evaluator, and equation
//! packs follow in subsequent milestones.

#![warn(clippy::mod_module_files)]
#![warn(missing_docs)]

mod dim;
mod diag;
mod eval;
mod fmt;
mod lexer;
mod packs;
mod parser;
mod quantity;
mod registry;
mod resolver;

pub use diag::{Diag, Diagnostic, ErrorCode, Hint, LintCode, Severity, Span};
pub use dim::{BaseDim, CustomDimId, Dimension};
pub use eval::{eval, Value};
pub use eval::value::{ConstraintSet, Quantity, SymExpr, Symbol};
pub use fmt::FmtOptions;
pub use parser::{parse, parse_checked, Expr, NodeId, ParseOutcome};
pub use quantity::UnitExpr;
pub use registry::{Registry, RegistryBuilder};
pub use resolver::{EmptyResolver, MapResolver, Resolver};

#[cfg(feature = "parallel")]
pub use eval::{eval_batch, eval_scenarios};

/// Crate version string (matches `Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod send_sync {
    use super::*;
    use std::sync::Arc;

    const fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn public_types_are_send_sync() {
        assert_send_sync::<Dimension>();
        assert_send_sync::<Quantity>();
        assert_send_sync::<Value>();
        assert_send_sync::<Diag>();
        assert_send_sync::<Diagnostic>();
        assert_send_sync::<Registry>();
        assert_send_sync::<Expr>();
        assert_send_sync::<Arc<Registry>>();
        assert_send_sync::<Arc<Expr>>();
        assert_send_sync::<dyn Resolver>();
    }
}
