//! Tokenizer state machine (grammar §4) — M1 milestone.

pub mod ftin;
pub mod token;

pub use token::Token;

/// Lexer span type (alias for [`crate::diag::Span`]).
#[allow(dead_code)]
pub type LexSpan = crate::diag::Span;

/// Lex source into tokens (M1).
#[allow(dead_code)]
pub fn lex(_src: &str) -> Result<Vec<Token>, crate::Diag> {
    Err(crate::diag::Diag::new(crate::diag::Diagnostic::error(
        crate::diag::ErrorCode::Parse,
        "lexer not yet implemented (M1 milestone)",
        crate::diag::Span::empty(0),
    )))
}
