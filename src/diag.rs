//! Diagnostics: error and lint codes with source spans.

use core::fmt;

/// Byte offset range in source text (half-open `[start, end)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    /// Start byte offset (inclusive).
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

impl Span {
    /// Empty span at a point.
    pub const fn empty(at: usize) -> Self {
        Self { start: at, end: at }
    }

    /// Span covering `[start, end)`.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Merge two spans into their convex hull.
    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Hard error — evaluation or parsing cannot proceed.
    Error,
    /// Warning lint — result may still be produced.
    Lint,
}

/// Structured error codes (grammar-spec §9 + plan §5.3 extensions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    /// Dimension mismatch at an operation site.
    DimMismatch,
    /// Unknown unit name.
    UnknownUnit,
    /// Unknown equation path.
    UnknownEq,
    /// Positional args on a code equation.
    CodePositional,
    /// Argument outside validity range (error severity).
    Range,
    /// Argument dimension incompatible with contract.
    ContractDim,
    /// TOML pack malformed.
    PackParse,
    /// Pack equation body failed to parse.
    PackBody,
    /// Unit definition cycle.
    DefCycle,
    /// Symbolic unit definition.
    DefSymbolic,
    /// Duplicate unit name.
    DupUnit,
    /// Affine unit in `define` (not yet supported).
    AffineDefine,
    /// Invalid anchor unit for dimension.
    AnchorInvalid,
    /// Affine unit cannot anchor a dimension.
    AnchorAffine,
    /// Duplicate anchor statement for one dimension.
    DupAnchor,
    /// Equation used inside an expression illegally.
    EqInExpr,
    /// Parse error (generic).
    Parse,
    /// Evaluation error (generic).
    Eval,
}

impl ErrorCode {
    /// Stable string code for hosts and tests.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DimMismatch => "E-DIM-MISMATCH",
            Self::UnknownUnit => "E-UNKNOWN-UNIT",
            Self::UnknownEq => "E-UNKNOWN-EQ",
            Self::CodePositional => "E-CODE-POSITIONAL",
            Self::Range => "E-RANGE",
            Self::ContractDim => "E-CONTRACT-DIM",
            Self::PackParse => "E-PACK-PARSE",
            Self::PackBody => "E-PACK-BODY",
            Self::DefCycle => "E-DEF-CYCLE",
            Self::DefSymbolic => "E-DEF-SYMBOLIC",
            Self::DupUnit => "E-DUP-UNIT",
            Self::AffineDefine => "E-AFFINE-DEFINE",
            Self::AnchorInvalid => "E-ANCHOR-INVALID",
            Self::AnchorAffine => "E-ANCHOR-AFFINE",
            Self::DupAnchor => "E-DUP-ANCHOR",
            Self::EqInExpr => "E-EQ-IN-EXPR",
            Self::Parse => "E-PARSE",
            Self::Eval => "E-EVAL",
        }
    }
}

/// Lint codes (non-fatal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LintCode {
    /// Argument outside declared validity range.
    Range,
    /// Exact rational arithmetic lost (first inexact op).
    ExactnessLost,
    /// `Ratio<i128>` overflow forced float fallback.
    RationalOverflow,
}

impl LintCode {
    /// Stable string code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Range => "L-RANGE",
            Self::ExactnessLost => "L-EXACTNESS-LOST",
            Self::RationalOverflow => "L-RATIONAL-OVERFLOW",
        }
    }
}

/// Optional structured hint for IDEs and calc sheets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hint {
    /// Expected dimension at this site.
    ExpectedDimension(String),
    /// Found dimension at this site.
    FoundDimension(String),
    /// Related span (e.g. constraining symbol definition).
    RelatedSpan(Span),
    /// Free-form note.
    Note(String),
}

/// A single diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Error or lint.
    pub severity: Severity,
    /// Stable code string.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Primary source span.
    pub span: Span,
    /// Optional structured hints.
    pub hints: Vec<Hint>,
}

impl Diagnostic {
    /// Build an error diagnostic.
    pub fn error(code: ErrorCode, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            code: code.as_str().into(),
            message: message.into(),
            span,
            hints: Vec::new(),
        }
    }

    /// Build a lint diagnostic.
    pub fn lint(code: LintCode, message: impl Into<String>, span: Span) -> Self {
        Self {
            severity: Severity::Lint,
            code: code.as_str().into(),
            message: message.into(),
            span,
            hints: Vec::new(),
        }
    }

    /// Attach hints.
    pub fn with_hints(mut self, hints: Vec<Hint>) -> Self {
        self.hints = hints;
        self
    }
}

/// Primary error type returned by parse/eval APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag(pub Diagnostic);

impl Diag {
    /// Wrap a diagnostic.
    pub fn new(diag: Diagnostic) -> Self {
        Self(diag)
    }

    /// Access inner diagnostic.
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.0
    }
}

impl fmt::Display for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}]: {}",
            self.0.code, self.0.span.start, self.0.message
        )
    }
}

impl std::error::Error for Diag {}
