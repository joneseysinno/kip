//! Pratt parser (grammar §5.2) — M3 milestone.

pub mod ast;

use std::sync::Arc;

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::registry::Registry;
use crate::resolver::Resolver;

pub use ast::{Expr, NodeId};

/// Outcome of parse-with-lints API.
#[derive(Debug, Clone)]
pub struct ParseOutcome {
    /// Parsed expression on success.
    pub expr: Option<Arc<Expr>>,
    /// Hard errors.
    pub errors: Vec<Diag>,
    /// Non-fatal lints.
    pub lints: Vec<Diag>,
}

/// Parse source into an immutable shared AST (M3).
pub fn parse(_src: &str, _registry: &Registry) -> Result<Arc<Expr>, Vec<Diag>> {
    Err(vec![Diag::new(Diagnostic::error(
        ErrorCode::Parse,
        "parser not yet implemented (M3 milestone)",
        Span::empty(0),
    ))])
}

/// Parse with resolver-aware shadow lints (M3).
pub fn parse_checked(
    _src: &str,
    _registry: &Registry,
    _resolver: &dyn Resolver,
) -> ParseOutcome {
    ParseOutcome {
        expr: None,
        errors: vec![Diag::new(Diagnostic::error(
            ErrorCode::Parse,
            "parser not yet implemented (M3 milestone)",
            Span::empty(0),
        ))],
        lints: Vec::new(),
    }
}
