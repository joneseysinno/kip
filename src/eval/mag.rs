//! Magnitude representation: exact rational or float after taint.

#![allow(clippy::should_implement_trait)]

use std::cmp::Ordering;

use num_rational::Ratio;
use num_traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, One, Signed, Zero};

/// Event emitted when exact arithmetic is lost or overflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaintEvent {
    /// First transition from exact rational to float.
    ExactnessLost,
    /// `Ratio<i128>` overflow forced float fallback.
    RationalOverflow,
}

/// Result of a magnitude operation (may carry a taint event for lint emission).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MagOpResult {
    /// Resulting magnitude.
    pub mag: Mag,
    /// Taint event, if any.
    pub event: Option<TaintEvent>,
}

/// Magnitude of a quantity: exact rational, or float after exactness loss.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mag {
    /// Exact rational magnitude.
    Exact(Ratio<i128>),
    /// Inexact float magnitude (tainted).
    Float(f64),
}

impl Mag {
    /// Construct an exact magnitude.
    pub fn exact(r: Ratio<i128>) -> Self {
        Self::Exact(r)
    }

    #[allow(clippy::result_unit_err)]
    /// Construct a float magnitude; rejects non-finite values.
    pub fn float(f: f64) -> Result<Self, ()> {
        if f.is_finite() {
            Ok(Self::Float(f))
        } else {
            Err(())
        }
    }

    /// Whether this magnitude is still exact.
    pub fn is_exact(self) -> bool {
        matches!(self, Self::Exact(_))
    }

    /// Lossy `f64` view.
    pub fn as_f64(self) -> f64 {
        match self {
            Self::Exact(r) => {
                let n: f64 = num_traits::ToPrimitive::to_f64(r.numer()).unwrap_or(0.0);
                let d: f64 = num_traits::ToPrimitive::to_f64(r.denom()).unwrap_or(1.0);
                n / d
            }
            Self::Float(f) => f,
        }
    }

    /// Exact rational when still exact.
    pub fn exact_ratio(self) -> Option<Ratio<i128>> {
        match self {
            Self::Exact(r) => Some(r),
            Self::Float(_) => None,
        }
    }

    /// Compare two magnitudes (float path uses `f64` ordering).
    pub fn partial_cmp(self, other: Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Exact(a), Self::Exact(b)) => Some(a.cmp(&b)),
            (Self::Float(a), Self::Float(b)) => a.partial_cmp(&b),
            (Self::Exact(a), Self::Float(b)) => Self::as_f64(Self::Exact(a)).partial_cmp(&b),
            (Self::Float(a), Self::Exact(b)) => a.partial_cmp(&Self::as_f64(Self::Exact(b))),
        }
    }

    /// Negation.
    pub fn neg(self) -> Self {
        match self {
            Self::Exact(r) => Self::Exact(-r),
            Self::Float(f) => Self::Float(-f),
        }
    }

    /// Absolute value.
    pub fn abs(self) -> Self {
        match self {
            Self::Exact(r) => Self::Exact(r.abs()),
            Self::Float(f) => Self::Float(f.abs()),
        }
    }

    /// Whether the magnitude is negative.
    pub fn is_negative(self) -> bool {
        match self {
            Self::Exact(r) => r.is_negative(),
            Self::Float(f) => f.is_sign_negative() && f != 0.0,
        }
    }

    /// Whether the magnitude is zero.
    pub fn is_zero(self) -> bool {
        match self {
            Self::Exact(r) => r.is_zero(),
            Self::Float(f) => f == 0.0,
        }
    }

    /// Addition with taint propagation.
    pub fn add(self, rhs: Self) -> MagOpResult {
        match (self, rhs) {
            (Self::Exact(a), Self::Exact(b)) => match a.checked_add(&b) {
                Some(r) => MagOpResult {
                    mag: Self::Exact(r),
                    event: None,
                },
                None => MagOpResult {
                    mag: Self::Float(self.as_f64() + rhs.as_f64()),
                    event: Some(TaintEvent::RationalOverflow),
                },
            },
            _ => MagOpResult {
                mag: Self::Float(self.as_f64() + rhs.as_f64()),
                event: if self.is_exact() && rhs.is_exact() {
                    Some(TaintEvent::ExactnessLost)
                } else {
                    None
                },
            },
        }
    }

    /// Subtraction with taint propagation.
    pub fn sub(self, rhs: Self) -> MagOpResult {
        match (self, rhs) {
            (Self::Exact(a), Self::Exact(b)) => match a.checked_sub(&b) {
                Some(r) => MagOpResult {
                    mag: Self::Exact(r),
                    event: None,
                },
                None => MagOpResult {
                    mag: Self::Float(self.as_f64() - rhs.as_f64()),
                    event: Some(TaintEvent::RationalOverflow),
                },
            },
            _ => MagOpResult {
                mag: Self::Float(self.as_f64() - rhs.as_f64()),
                event: if self.is_exact() && rhs.is_exact() {
                    Some(TaintEvent::ExactnessLost)
                } else {
                    None
                },
            },
        }
    }

    /// Multiplication with taint propagation.
    pub fn mul(self, rhs: Self) -> MagOpResult {
        match (self, rhs) {
            (Self::Exact(a), Self::Exact(b)) => match a.checked_mul(&b) {
                Some(r) => MagOpResult {
                    mag: Self::Exact(r),
                    event: None,
                },
                None => MagOpResult {
                    mag: Self::Float(self.as_f64() * rhs.as_f64()),
                    event: Some(TaintEvent::RationalOverflow),
                },
            },
            _ => MagOpResult {
                mag: Self::Float(self.as_f64() * rhs.as_f64()),
                event: if self.is_exact() && rhs.is_exact() {
                    Some(TaintEvent::ExactnessLost)
                } else {
                    None
                },
            },
        }
    }

    #[allow(clippy::result_unit_err)]
    /// Division with taint propagation.
    pub fn div(self, rhs: Self) -> Result<MagOpResult, ()> {
        if rhs.is_zero() {
            return Err(());
        }
        Ok(match (self, rhs) {
            (Self::Exact(a), Self::Exact(b)) => match a.checked_div(&b) {
                Some(r) => MagOpResult {
                    mag: Self::Exact(r),
                    event: None,
                },
                None => MagOpResult {
                    mag: Self::Float(self.as_f64() / rhs.as_f64()),
                    event: Some(TaintEvent::RationalOverflow),
                },
            },
            _ => MagOpResult {
                mag: Self::Float(self.as_f64() / rhs.as_f64()),
                event: if self.is_exact() && rhs.is_exact() {
                    Some(TaintEvent::ExactnessLost)
                } else {
                    None
                },
            },
        })
    }

    /// Integer exponentiation.
    pub fn pow_int(self, exp: i32) -> MagOpResult {
        if exp == 0 {
            return MagOpResult {
                mag: Self::Exact(Ratio::one()),
                event: None,
            };
        }
        match self {
            Self::Exact(r) => {
                let mag = if exp > 0 {
                    r.pow(exp)
                } else {
                    Ratio::one() / r.pow(-exp)
                };
                MagOpResult {
                    mag: Self::Exact(mag),
                    event: None,
                }
            }
            Self::Float(f) => MagOpResult {
                mag: Self::Float(f.powi(exp)),
                event: None,
            },
        }
    }
}
