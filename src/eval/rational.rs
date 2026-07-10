//! Checked rational helpers shared by builtins and unit conversion.

use num_rational::Ratio;
use num_traits::One;

use crate::eval::mag::{Mag, MagOpResult, TaintEvent};

/// Integer square root of a perfect-square `i128`.
pub fn integer_sqrt(n: i128) -> Option<i128> {
    if n < 0 {
        return None;
    }
    let root = n.isqrt();
    (root.checked_mul(root)? == n).then_some(root)
}

/// Exact square root of a rational when both parts are perfect squares.
pub fn rational_sqrt(r: Ratio<i128>) -> Option<Ratio<i128>> {
    let num = integer_sqrt(*r.numer())?;
    let den = integer_sqrt(*r.denom())?;
    Some(Ratio::new(num, den))
}

/// Checked integer exponentiation (`None` on overflow).
pub fn checked_i128_pow(mut base: i128, mut exp: u32) -> Option<i128> {
    if exp == 0 {
        return Some(1);
    }
    let mut result = 1i128;
    while exp > 0 {
        if exp & 1 == 1 {
            result = result.checked_mul(base)?;
        }
        exp >>= 1;
        if exp > 0 {
            base = base.checked_mul(base)?;
        }
    }
    Some(result)
}

/// Raise a rational to an integer exponent with overflow → float fallback.
pub fn checked_ratio_pow(r: Ratio<i128>, exp: i32) -> MagOpResult {
    if exp == 0 {
        return MagOpResult {
            mag: Mag::Exact(Ratio::one()),
            event: None,
        };
    }
    let base_f = {
        let n: f64 = num_traits::ToPrimitive::to_f64(r.numer()).unwrap_or(0.0);
        let d: f64 = num_traits::ToPrimitive::to_f64(r.denom()).unwrap_or(1.0);
        n / d
    };
    let abs_exp = exp.unsigned_abs();
    let num_pow = checked_i128_pow(*r.numer(), abs_exp);
    let den_pow = checked_i128_pow(*r.denom(), abs_exp);
    match (num_pow, den_pow) {
        (Some(n), Some(d)) => {
            let mag = if exp >= 0 {
                Mag::Exact(Ratio::new(n, d))
            } else {
                Mag::Exact(Ratio::new(d, n))
            };
            MagOpResult {
                mag,
                event: None,
            }
        }
        _ => MagOpResult {
            mag: Mag::Float(base_f.powi(exp)),
            event: Some(TaintEvent::RationalOverflow),
        },
    }
}
