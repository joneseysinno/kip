//! Unit expressions as written by the user (never normalized away).

use core::fmt;

/// A unit expression preserving user syntax (e.g. `kip·ft`, `psi`, `ft`).
///
/// Full parsing attaches to the lexer/parser (M3). M0 carries a simple name or
/// compound representation sufficient for quantities and registry seed data.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum UnitExpr {
    /// A single registered unit name.
    Named(String),
    /// Compound unit written with middle dot or implicit juxtaposition.
    Compound(Vec<UnitExpr>),
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
            Self::Compound(parts) => parts
                .first()
                .map(|u| u.as_str())
                .unwrap_or("1"),
        }
    }
}

impl fmt::Debug for UnitExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(s) => write!(f, "{s}"),
            Self::Dimensionless => write!(f, "1"),
            Self::Compound(parts) => {
                for (i, p) in parts.iter().enumerate() {
                    if i > 0 {
                        write!(f, "·")?;
                    }
                    write!(f, "{p:?}")?;
                }
                Ok(())
            }
        }
    }
}

impl fmt::Display for UnitExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
