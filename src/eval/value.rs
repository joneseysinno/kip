//! Evaluation results: known quantities and symbolic residuals.

use std::collections::BTreeMap;

use num_rational::Ratio;

use crate::dim::Dimension;
use crate::quantity::UnitExpr;
use crate::{Diag, Resolver};

/// A free symbol name in a partial evaluation residual.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(pub String);

/// Dimension constraints accumulated over free symbols.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConstraintSet {
    /// Required dimension per symbol.
    pub symbol_dims: BTreeMap<Symbol, Dimension>,
}

impl ConstraintSet {
    /// Empty constraint set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Pin or unify a symbol's dimension.
    pub fn pin(&mut self, sym: Symbol, dim: Dimension) -> Result<(), crate::Diag> {
        use crate::diag::{Diag, Diagnostic, ErrorCode, Hint, Span};

        if let Some(existing) = self.symbol_dims.get(&sym) {
            if existing != &dim {
                return Err(Diag::new(
                    Diagnostic::error(
                        ErrorCode::DimMismatch,
                        format!("symbol `{}` has conflicting dimension constraints", sym.0),
                        Span::empty(0),
                    )
                    .with_hints(vec![
                        Hint::ExpectedDimension(format!("{existing:?}")),
                        Hint::FoundDimension(format!("{dim:?}")),
                    ]),
                ));
            }
        } else {
            self.symbol_dims.insert(sym, dim);
        }
        Ok(())
    }
}

/// Simplified symbolic residual (M5 expands this).
#[derive(Debug, Clone, PartialEq)]
pub struct SymExpr {
    /// Display-oriented representation until M5 AST is wired.
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
}

impl Quantity {
    /// Construct a known exact quantity.
    pub fn new(magnitude: Ratio<i128>, unit: UnitExpr, dim: Dimension) -> Self {
        Self {
            magnitude,
            float_mag: None,
            unit,
            dim,
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
    pub fn display(&self, opts: &crate::FmtOptions) -> String {
        crate::fmt::format_quantity(self, opts)
    }

    /// Effective rational magnitude for exact arithmetic.
    pub(crate) fn effective_magnitude(&self) -> Ratio<i128> {
        self.magnitude
    }
}

impl Value {
    /// Evaluate a symbolic residual further with additional bindings (M5).
    pub fn bind(&self, _resolver: &dyn Resolver) -> Result<Value, Diag> {
        Err(crate::diag::Diag::new(crate::diag::Diagnostic::error(
            crate::diag::ErrorCode::Eval,
            "bind not yet implemented (M5 milestone)",
            crate::diag::Span::empty(0),
        )))
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
