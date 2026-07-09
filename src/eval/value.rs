//! Evaluation results: known quantities and symbolic residuals.

use std::collections::BTreeMap;
use std::sync::Arc;

use num_rational::Ratio;

use crate::diag::Span;
use crate::dim::Dimension;
use crate::quantity::UnitExpr;
use crate::{Diag, Resolver};

/// A free symbol name in a partial evaluation residual.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(pub String);

/// Provenance for a quantity produced by a code equation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquationProvenance {
    /// Pack identifier from TOML.
    pub pack_id: String,
    /// Human-readable pack title.
    pub title: String,
    /// Code edition string.
    pub edition: String,
    /// License field from pack metadata.
    pub license: String,
    /// Equation namespace (`ACI`).
    pub namespace: String,
    /// Equation id within namespace (`fr`).
    pub equation_id: String,
    /// Section citation string.
    pub cite: String,
}

/// Recorded dimension constraint with provenance sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolConstraint {
    /// Required dimension.
    pub dim: Dimension,
    /// Spans that contributed this constraint.
    pub sites: Vec<Span>,
}

/// Dimension constraints accumulated over free symbols.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConstraintSet {
    /// Required dimension per symbol.
    pub symbol_dims: BTreeMap<Symbol, SymbolConstraint>,
}

impl ConstraintSet {
    /// Empty constraint set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pin or unify a symbol's dimension, recording provenance.
    pub fn pin_at(&mut self, sym: Symbol, dim: Dimension, span: Span) -> Result<(), Diag> {
        use crate::diag::{Diagnostic, ErrorCode, Hint};

        if let Some(existing) = self.symbol_dims.get(&sym) {
            if existing.dim != dim {
                let primary = existing.sites.first().copied().unwrap_or(span);
                return Err(Diag::new(
                    Diagnostic::error(
                        ErrorCode::DimMismatch,
                        format!("symbol `{}` has conflicting dimension constraints", sym.0),
                        primary,
                    )
                    .with_hints(vec![
                        Hint::ExpectedDimension(format!("{:?}", existing.dim)),
                        Hint::FoundDimension(format!("{dim:?}")),
                        Hint::RelatedSpan(span),
                    ]),
                ));
            }
            return Ok(());
        }
        self.symbol_dims.insert(
            sym,
            SymbolConstraint {
                dim,
                sites: vec![span],
            },
        );
        Ok(())
    }

    /// Legacy pin without provenance (tests).
    pub fn pin(&mut self, sym: Symbol, dim: Dimension) -> Result<(), Diag> {
        self.pin_at(sym, dim, Span::empty(0))
    }

    /// Lookup a symbol's pinned dimension.
    pub fn dimension_of(&self, sym: &Symbol) -> Option<Dimension> {
        self.symbol_dims.get(sym).map(|c| c.dim.clone())
    }
}

/// Unary operators in symbolic residuals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymUnaryOp {
    /// Negation.
    Neg,
    /// Square root.
    Sqrt,
}

/// Binary operators in symbolic residuals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymBinaryOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division.
    Div,
    /// Exponentiation.
    Pow,
}

/// Node in a symbolic expression tree.
#[allow(clippy::large_enum_variant)] // `Quantity` preserves full unit syntax + provenance.
#[derive(Debug, Clone, PartialEq)]
pub enum SymNode {
    /// Fully known quantity leaf.
    Known(Quantity),
    /// Free symbol leaf.
    Symbol(Symbol),
    /// Unary operation.
    Unary {
        /// Operator.
        op: SymUnaryOp,
        /// Operand.
        operand: Box<SymNode>,
    },
    /// Binary operation.
    Binary {
        /// Operator.
        op: SymBinaryOp,
        /// Left operand.
        left: Box<SymNode>,
        /// Right operand.
        right: Box<SymNode>,
    },
}

/// Symbolic residual expression (M5).
#[derive(Debug, Clone, PartialEq)]
pub struct SymExpr {
    /// Structured residual tree.
    pub root: SymNode,
    /// Display-oriented representation.
    pub text: String,
    /// Free symbols referenced.
    pub free_symbols: Vec<Symbol>,
    /// Dimension constraints.
    pub constraints: ConstraintSet,
}

/// Result of evaluation: fully known or partially symbolic.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Fully evaluated to a quantity.
    Known(Quantity),
    /// Residual expression with constraints.
    Symbolic(SymExpr),
}

/// A unit-preserving engineering quantity.
///
/// Invariant: **`unit` is exactly as written** — never normalized to a canonical
/// base form. Conversion happens only at unification points.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantity {
    /// Exact rational magnitude when available.
    pub magnitude: Ratio<i128>,
    /// Set when exactness was lost; stays float thereafter.
    pub float_mag: Option<f64>,
    /// Unit as written by the user.
    pub unit: UnitExpr,
    /// Cached dimension vector.
    pub dim: Dimension,
    /// Source equation when produced by a pack call.
    pub provenance: Option<Arc<EquationProvenance>>,
}

impl Quantity {
    /// Construct a known exact quantity.
    pub fn new(magnitude: Ratio<i128>, unit: UnitExpr, dim: Dimension) -> Self {
        Self {
            magnitude,
            float_mag: None,
            unit,
            dim,
            provenance: None,
        }
    }

    /// Construct from integer numerator with named unit and dimension.
    pub fn from_int(n: i128, unit: impl Into<String>, dim: Dimension) -> Self {
        Self::new(Ratio::from_integer(n), UnitExpr::named(unit), dim)
    }

    /// Whether this value still has exact rational magnitude.
    pub fn is_exact(&self) -> bool {
        self.float_mag.is_none()
    }

    /// Effective magnitude as `f64` (exact values converted lossily for display).
    pub fn as_f64(&self) -> f64 {
        if let Some(f) = self.float_mag {
            f
        } else {
            let n: f64 = num_traits::ToPrimitive::to_f64(self.magnitude.numer()).unwrap_or(0.0);
            let d: f64 = num_traits::ToPrimitive::to_f64(self.magnitude.denom()).unwrap_or(1.0);
            n / d
        }
    }

    /// Convert to another unit via registry anchor ratios (M4).
    pub fn convert_to(
        &self,
        unit: &UnitExpr,
        registry: &crate::Registry,
    ) -> Result<Quantity, Diag> {
        crate::eval::units::convert_quantity(self, unit, registry)
    }

    /// Format for display (M7).
    pub fn display(&self, registry: &crate::Registry, opts: &crate::FmtOptions) -> String {
        crate::fmt::format_quantity(self, registry, opts)
    }

    /// Effective rational magnitude for exact arithmetic.
    pub(crate) fn effective_magnitude(&self) -> Ratio<i128> {
        self.magnitude
    }
}

impl Value {
    /// Evaluate a symbolic residual further with additional bindings (M5).
    pub fn bind(&self, resolver: &dyn Resolver) -> Result<Value, Diag> {
        match self {
            Self::Known(_) => Ok(self.clone()),
            Self::Symbolic(s) => {
                let bound = crate::eval::partial::bind_symbolic(s, resolver)?;
                crate::eval::partial::finalize(bound, Span::empty(0))
            }
        }
    }

    /// Free symbols in a symbolic value (empty for known).
    pub fn free_symbols(&self) -> &[Symbol] {
        match self {
            Self::Known(_) => &[],
            Self::Symbolic(s) => &s.free_symbols,
        }
    }

    /// Constraint set (empty for known).
    pub fn constraints(&self) -> &ConstraintSet {
        match self {
            Self::Known(_) => empty_constraints(),
            Self::Symbolic(s) => &s.constraints,
        }
    }

    /// Borrow inner quantity if fully known.
    pub fn quantity(&self) -> Option<&Quantity> {
        match self {
            Self::Known(q) => Some(q),
            Self::Symbolic(_) => None,
        }
    }
}

/// Macro helper for tests and registry builder (exact integer magnitudes).
#[macro_export]
macro_rules! qty {
    ($n:expr, $unit:expr) => {
        $crate::eval::value::Quantity::from_int(
            $n,
            $unit,
            $crate::dim::Dimension::dimensionless(),
        )
    };
    ($n:expr, $unit:expr, $dim:expr) => {
        $crate::eval::value::Quantity::from_int($n, $unit, $dim)
    };
}

/// Rational arithmetic policy: checked ops with float fallback on overflow.
#[allow(dead_code)]
pub mod rational {
    use super::*;
    use crate::diag::{Diag, Diagnostic, LintCode, Span};
    use num_traits::CheckedAdd;
    use num_traits::CheckedMul;
    use num_traits::CheckedSub;

    /// Add two rationals, falling back to float on overflow.
    pub fn add(a: Ratio<i128>, b: Ratio<i128>) -> Result<Ratio<i128>, (f64, Diag)> {
        match a.checked_add(&b) {
            Some(r) => Ok(r),
            None => {
                let fa = ratio_to_f64(a);
                let fb = ratio_to_f64(b);
                let lint = Diag::new(Diagnostic::lint(
                    LintCode::RationalOverflow,
                    "rational addition overflowed i128; using float",
                    Span::empty(0),
                ));
                Err((fa + fb, lint))
            }
        }
    }

    /// Multiply two rationals, falling back to float on overflow.
    pub fn mul(a: Ratio<i128>, b: Ratio<i128>) -> Result<Ratio<i128>, (f64, Diag)> {
        match a.checked_mul(&b) {
            Some(r) => Ok(r),
            None => {
                let lint = Diag::new(Diagnostic::lint(
                    LintCode::RationalOverflow,
                    "rational multiplication overflowed i128; using float",
                    Span::empty(0),
                ));
                Err((ratio_to_f64(a) * ratio_to_f64(b), lint))
            }
        }
    }

    /// Subtract two rationals, falling back to float on overflow.
    pub fn sub(a: Ratio<i128>, b: Ratio<i128>) -> Result<Ratio<i128>, (f64, Diag)> {
        match a.checked_sub(&b) {
            Some(r) => Ok(r),
            None => {
                let lint = Diag::new(Diagnostic::lint(
                    LintCode::RationalOverflow,
                    "rational subtraction overflowed i128; using float",
                    Span::empty(0),
                ));
                Err((ratio_to_f64(a) - ratio_to_f64(b), lint))
            }
        }
    }

    fn ratio_to_f64(r: Ratio<i128>) -> f64 {
        let n: f64 = num_traits::ToPrimitive::to_f64(r.numer()).unwrap_or(0.0);
        let d: f64 = num_traits::ToPrimitive::to_f64(r.denom()).unwrap_or(1.0);
        n / d
    }
}

fn empty_constraints() -> &'static ConstraintSet {
    use std::sync::OnceLock;
    static EMPTY: OnceLock<ConstraintSet> = OnceLock::new();
    EMPTY.get_or_init(ConstraintSet::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::BaseDim;
    use num_traits::One;

    #[test]
    fn quantity_exactness() {
        let q = Quantity::from_int(12, "ft", Dimension::single(BaseDim::Length, Ratio::one()));
        assert!(q.is_exact());
        assert_eq!(q.as_f64(), 12.0);
    }

    #[test]
    fn constraint_pin_conflict() {
        let mut cs = ConstraintSet::new();
        let sym = Symbol("x".into());
        cs.pin(
            sym.clone(),
            Dimension::single(BaseDim::Force, Ratio::one()),
        )
        .unwrap();
        assert!(cs
            .pin(sym, Dimension::single(BaseDim::Length, Ratio::one()))
            .is_err());
    }
}
