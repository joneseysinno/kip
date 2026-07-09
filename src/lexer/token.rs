//! Lexer token types with exact rational payloads for length literals.

use num_rational::Ratio;

use crate::diag::Span;

/// A token with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    /// Token kind and payload.
    pub token: Token,
    /// Source span.
    pub span: Span,
}

/// Lexer token (grammar-spec §3).
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// End of input.
    Eof,
    /// Exact decimal literal (`INT`, `DECIMAL`, or tight `SCI`).
    Number {
        /// Normalized text (no `_` separators).
        text: String,
        /// Exact rational when representable.
        value: Ratio<i128>,
    },
    /// Identifier (variable, unit name, function name).
    Ident(String),
    /// Feet tick literal (`NUMBER '`).
    Feet {
        /// Exact total length in inches.
        inches: Ratio<i128>,
    },
    /// Inches tick literal (`inch_val "`).
    Inches {
        /// Exact length in inches.
        inches: Ratio<i128>,
    },
    /// Feet-inch compound (`NUMBER ' … inch_val "`).
    FtIn {
        /// Exact total length in inches.
        inches: Ratio<i128>,
    },
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*` (expression context)
    Star,
    /// `/`
    Slash,
    /// `^`
    Caret,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `,`
    Comma,
    /// `=` (registry / sheet convention)
    Eq,
    /// `.` (decimal point or path separator context)
    Dot,
    /// `·` or `×` (unit-expression alias for `*`)
    UnitMul,
    /// `::` (sheet-layer annotation convention)
    ColonColon,
    /// `:` (registry primary-unit form)
    Colon,
    /// `>=` `<=` `>` `<` `==` (reserved v1.1)
    Gte,
    /// `<=` (reserved v1.1).
    Lte,
    /// `>` (reserved v1.1).
    Gt,
    /// `<` (reserved v1.1).
    Lt,
    /// `==` (reserved v1.1).
    EqEq,
}

impl Token {
    /// Whether this is a length literal token.
    pub fn is_length_literal(&self) -> bool {
        matches!(self, Self::Feet { .. } | Self::Inches { .. } | Self::FtIn { .. })
    }
}
