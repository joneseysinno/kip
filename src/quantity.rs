//! Unit expressions as written by the user (never normalized away).

use core::fmt;

/// Exponent on a unit factor (`ft^2`, `psi^(1/2)`, `psi^0.5`).
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum UnitExponent {
    /// Integer exponent (`ft^2`, `psi^-1`).
    Int(i32),
    /// Rational exponent in parentheses (`psi^(1/2)`).
    Ratio {
        /// Numerator.
        num: i32,
        /// Denominator.
        den: i32,
    },
    /// Decimal exponent (`psi^0.5`).
    Decimal(String),
}

impl fmt::Debug for UnitExponent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(n) => write!(f, "{n}"),
            Self::Ratio { num, den } => write!(f, "{num}/{den}"),
            Self::Decimal(s) => write!(f, "{s}"),
        }
    }
}

/// A unit expression preserving user syntax (e.g. `kip·ft`, `lbf/ft^2`).
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum UnitExpr {
    /// A single registered unit name.
    Named(String),
    /// Product of factors (`kip*ft`, `kip·ft`).
    Product(Vec<UnitExpr>),
    /// Quotient via tight `/` (`lbf/ft`).
    Quotient(Box<UnitExpr>, Box<UnitExpr>),
    /// Unit raised to a power (`ft^2`, `psi^0.5`).
    Pow {
        /// Base unit expression.
        base: Box<UnitExpr>,
        /// Exponent.
        exp: UnitExponent,
    },
    /// Dimensionless (numeric literals, `1`).
    Dimensionless,
}

impl UnitExpr {
    /// Named unit from a string slice.
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// Dimensionless unit marker.
    pub fn one() -> Self {
        Self::Dimensionless
    }

    /// Primary display name for this unit expression.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Named(s) => s.as_str(),
            Self::Dimensionless => "1",
            Self::Product(parts) => parts
                .first()
                .map(|u| u.as_str())
                .unwrap_or("1"),
            Self::Quotient(num, _) => num.as_str(),
            Self::Pow { base, .. } => base.as_str(),
        }
    }
}

impl fmt::Debug for UnitExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(s) => write!(f, "{s}"),
            Self::Dimensionless => write!(f, "1"),
            Self::Product(parts) => {
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        write!(f, "·")?;
                    }
                    write!(f, "{p:?}")?;
                }
                Ok(())
            }
            Self::Quotient(num, den) => write!(f, "{num:?}/{den:?}"),
            Self::Pow { base, exp } => write!(f, "{base:?}^{exp:?}"),
        }
    }
}

impl fmt::Display for UnitExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
