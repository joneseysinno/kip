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
//! ## Typical workflow
//!
//! 1. Build or load a [`Registry`] (`RegistryBuilder::from_seed`, [`RegistryBuilder::parse_defs`],
//!    [`RegistryBuilder::load_packs`]).
//! 2. [`parse`] an expression against that registry (optional [`Resolver`] for symbols).
//! 3. [`eval`] to a [`Value`] — fully [`Quantity`] or [`eval::value::SymExpr`] residual.
//! 4. Format with [`Quantity::display`] and [`FmtOptions`], or bind further with [`Value::bind`].
//!
//! ## Concurrency (P1)
//!
//! ASTs ([`Expr`]), registries ([`Registry`]), and evaluation are immutable and
//! `Send + Sync`. Evaluation is a pure function — no globals, no locks.
//! With the `parallel` feature: [`eval_batch`], [`eval_scenarios`], and intra-expression
//! `rayon` splits above [`PARALLEL_THRESHOLD`] nodes.
//!
//! ## Code equations (P2)
//!
//! Namespaced calls such as `ACI.fr(fc: 4000 psi, lambda: 1.0)` require named arguments.
//! Load pack TOML via [`RegistryBuilder::load_packs`] or [`load_packs`]; results carry
//! [`EquationProvenance`] on [`Quantity`].
//!
//! ## Version 0.1.0
//!
//! Full grammar conformance (lexer → packs → parallel/fmt). See `CHANGELOG.md` and
//! `VERSIONING.md` for release policy.

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
pub use eval::{eval, eval_checked, EvalOutcome, LintSink, Mag, Value};
pub use eval::units::convert_quantity;
pub use eval::value::{ConstraintSet, EquationProvenance, Quantity, SymBinaryOp, SymExpr, SymNode, SymUnaryOp, Symbol};
pub use fmt::{format_quantity, FmtOptions};
pub use lexer::{lex, lex_checked, LexOutcome, LexSpan, SpannedToken, Token};
pub use parser::{parse, parse_checked, BinaryOp, Callee, CallArg, CmpOp, Expr, ExprKind, ExprNode, NodeId, ParseOutcome, UnaryOp};
pub use quantity::{UnitExpr, UnitExponent};
pub use registry::{Registry, RegistryBuilder};
pub use resolver::{EmptyResolver, MapResolver, Resolver};
pub use packs::equation::{EquationRecord, EquationRegistry};
#[cfg(feature = "packs")]
pub use packs::{load_packs, load_packs_into, DEMO_PACK_TOML};

#[cfg(feature = "parallel")]
pub use eval::{eval_batch, eval_scenarios, PARALLEL_THRESHOLD};

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
        assert_send_sync::<SpannedToken>();
        assert_send_sync::<Token>();
        assert_send_sync::<dyn Resolver>();
    }
}
