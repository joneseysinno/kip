//! Token types and exact-value payloads for dimensional literals.

/// Lexer token (M1 expands this to the full grammar §4 set).
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Token {
    /// End of input.
    Eof,
    /// Placeholder until M1.
    Error {
        /// Diagnostic span.
        span: crate::diag::Span,
        /// Message.
        message: String,
    },
}
