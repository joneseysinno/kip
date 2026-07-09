//! Pratt parser (grammar §5.2) — M3 milestone.

pub mod ast;
mod pratt;
mod unit;

use std::sync::Arc;

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::lexer::lex_checked;
use crate::registry::Registry;
use crate::resolver::Resolver;

pub use ast::{BinaryOp, CallArg, Callee, CmpOp, Expr, ExprKind, ExprNode, NodeId, UnaryOp};

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
pub fn parse(src: &str, registry: &Registry) -> Result<Arc<Expr>, Vec<Diag>> {
    match parse_checked(src, registry, &crate::resolver::EmptyResolver) {
        ParseOutcome {
            expr: Some(e),
            errors,
            lints,
        } if errors.is_empty() => {
            let _ = lints;
            Ok(e)
        }
        ParseOutcome { errors, .. } => Err(errors),
    }
}

/// Parse with resolver-aware shadow lints (M3).
pub fn parse_checked(
    src: &str,
    registry: &Registry,
    resolver: &dyn Resolver,
) -> ParseOutcome {
    let (expr, mut errors, mut lints) = pratt::PrattParser::parse(src, registry, Some(resolver));

    // Surface lexer hard errors if parser bailed before merging them.
    if expr.is_none() && errors.is_empty() {
        let lex = lex_checked(src);
        errors = lex.errors;
        lints.extend(lex.lints);
        if errors.is_empty() && !lex.tokens.is_empty() {
            errors.push(Diag::new(Diagnostic::error(
                ErrorCode::Parse,
                "parse failed",
                Span::empty(0),
            )));
        }
    }

    ParseOutcome {
        expr: expr.map(Arc::new),
        errors,
        lints,
    }
}
