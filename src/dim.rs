//! Base dimensions and dimension vectors.

use core::fmt;
use core::ops::{Add, Mul, Sub};

use num_rational::Ratio;
use num_traits::Zero;
use smallvec::SmallVec;

/// Identifier for a user-declared base dimension (e.g. `dimension Currency`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomDimId(pub u32);

/// Fundamental dimension in the force-based imperial system.
///
/// Anchors (default units per dimension) are **registry data**, not compile-time
/// constants. These variants name dimensions only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BaseDim {
    /// Length — default anchor: inch (`in`).
    Length,
    /// Force — default anchor: pound-force (`lbf`).
    Force,
    /// Time — default anchor: second (`s`).
    Time,
    /// Absolute temperature — default anchor: Rankine (`°R`).
    Temperature,
    /// Angle pseudo-dimension — default anchor: radian (`rad`).
    Angle,
    /// User-declared base dimension.
    Custom(CustomDimId),
}

/// A dimension as a sorted map of base dimension → rational exponent.
///
/// Stored inline for up to eight exponents (common case is allocation-free).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Dimension {
    exponents: SmallVec<[(BaseDim, Ratio<i32>); 8]>,
}

impl Dimension {
    /// Dimensionless quantity.
    pub fn dimensionless() -> Self {
        Self {
            exponents: SmallVec::new(),
        }
    }

    /// Single base dimension raised to `exp`.
    pub fn single(dim: BaseDim, exp: Ratio<i32>) -> Self {
        if exp.is_zero() {
            return Self::dimensionless();
        }
        Self {
            exponents: smallvec::smallvec![(dim, exp)],
        }
    }

    /// Exponents as a sorted slice.
    pub fn exponents(&self) -> &[(BaseDim, Ratio<i32>)] {
        &self.exponents
    }

    /// Whether this is dimensionless.
    pub fn is_dimensionless(&self) -> bool {
        self.exponents.is_empty()
    }

    /// Raise every exponent by `factor`.
    pub fn pow(&self, factor: Ratio<i32>) -> Self {
        let exponents = self
            .exponents
            .iter()
            .map(|&(d, e)| (d, e * factor))
            .filter(|&(_, e)| !e.is_zero())
            .collect();
        Self { exponents }
    }

    /// Multiply dimensions (add exponents).
    pub fn mul(&self, rhs: &Self) -> Self {
        Self::combine(self, rhs, |a, b| a + b)
    }

    /// Divide dimensions (subtract exponents).
    pub fn div(&self, rhs: &Self) -> Self {
        Self::combine(self, rhs, |a, b| a - b)
    }

    fn combine(
        lhs: &Self,
        rhs: &Self,
        op: impl Fn(Ratio<i32>, Ratio<i32>) -> Ratio<i32>,
    ) -> Self {
        let mut exponents: SmallVec<[(BaseDim, Ratio<i32>); 8]> = SmallVec::new();
        let mut all_dims: SmallVec<[BaseDim; 16]> = SmallVec::new();

        for (d, _) in &lhs.exponents {
            if !all_dims.contains(d) {
                all_dims.push(*d);
            }
        }
        for (d, _) in &rhs.exponents {
            if !all_dims.contains(d) {
                all_dims.push(*d);
            }
        }
        all_dims.sort_unstable();
        all_dims.dedup();

        for dim in all_dims {
            let ea = lhs
                .exponents
                .iter()
                .find(|(d, _)| *d == dim)
                .map(|(_, e)| *e)
                .unwrap_or_else(Ratio::zero);
            let eb = rhs
                .exponents
                .iter()
                .find(|(d, _)| *d == dim)
                .map(|(_, e)| *e)
                .unwrap_or_else(Ratio::zero);
            let exp = op(ea, eb);
            if !exp.is_zero() {
                exponents.push((dim, exp));
            }
        }

        Self { exponents }
    }
}

impl Default for Dimension {
    fn default() -> Self {
        Self::dimensionless()
    }
}

impl Add for &Dimension {
    type Output = Dimension;

    /// Dimensions must match for addition (checked by the evaluator).
    fn add(self, _rhs: Self) -> Dimension {
        self.clone()
    }
}

impl Sub for &Dimension {
    type Output = Dimension;

    fn sub(self, _rhs: Self) -> Dimension {
        self.clone()
    }
}

impl Mul for &Dimension {
    type Output = Dimension;

    fn mul(self, rhs: Self) -> Dimension {
        self.mul(rhs)
    }
}

impl fmt::Debug for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_dimensionless() {
            return write!(f, "1");
        }
        for (i, (dim, exp)) in self.exponents.iter().enumerate() {
            if i > 0 {
                write!(f, "·")?;
            }
            write!(f, "{dim:?}^{exp}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::{FromPrimitive, One};

    #[test]
    fn dimension_mul_div() {
        let length = Dimension::single(BaseDim::Length, Ratio::one());
        let force = Dimension::single(BaseDim::Force, Ratio::one());
        let pressure = length.div(&force).mul(&force).div(&length);
        assert!(pressure.is_dimensionless());
    }

    #[test]
    fn sqrt_halves_exponent() {
        let pressure = Dimension::single(BaseDim::Force, Ratio::one())
            .div(&Dimension::single(BaseDim::Length, Ratio::one()));
        let half = Ratio::from_i32(1).unwrap() / Ratio::from_i32(2).unwrap();
        let root = pressure.pow(half);
        let force_exp = root
            .exponents()
            .iter()
            .find(|(d, _)| *d == BaseDim::Force)
            .map(|(_, e)| *e)
            .unwrap();
        assert_eq!(force_exp, half);
    }
}
