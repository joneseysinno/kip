//! Unit resolution, conversion, and leftmost-wins unification.

use num_rational::Ratio;
use num_traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, FromPrimitive, One, Zero};

use crate::diag::{Diag, Diagnostic, ErrorCode, Hint, Span};
use crate::dim::Dimension;
use crate::eval::value::Quantity;
use crate::quantity::{UnitExpr, UnitExponent};
use crate::registry::Registry;

/// Resolve the dimension of a written unit expression.
pub fn dimension_of_unit(unit: &UnitExpr, registry: &Registry) -> Result<Dimension, Diag> {
    match unit {
        UnitExpr::Dimensionless => Ok(Dimension::dimensionless()),
        UnitExpr::Named(name) => registry
            .unit(name)
            .map(|u| u.dimension.clone())
            .ok_or_else(|| unknown_unit(name, Span::empty(0))),
        UnitExpr::Product(parts) => parts
            .iter()
            .try_fold(Dimension::dimensionless(), |acc, part| {
                Ok(acc.mul(&dimension_of_unit(part, registry)?))
            }),
        UnitExpr::Quotient(num, den) => Ok(
            dimension_of_unit(num, registry)?.div(&dimension_of_unit(den, registry)?),
        ),
        UnitExpr::Pow { base, exp } => {
            let dim = dimension_of_unit(base, registry)?;
            let e = unit_exponent_to_ratio(exp)?;
            Ok(dim.pow(e))
        }
    }
}

/// Exact anchor magnitude for a quantity written in user units.
pub fn magnitude_in_anchor_units(q: &Quantity, registry: &Registry) -> Result<Ratio<i128>, Diag> {
    let factor = unit_to_anchor_factor(&q.unit, registry)?;
    Ok(q.effective_magnitude() * factor)
}

/// Convert a quantity to another unit expression (same dimension).
pub fn convert_quantity(
    q: &Quantity,
    target: &UnitExpr,
    registry: &Registry,
) -> Result<Quantity, Diag> {
    let target_dim = dimension_of_unit(target, registry)?;
    if q.dim != target_dim {
        return Err(dim_mismatch(
            &q.dim,
            &target_dim,
            Span::empty(0),
            "cannot convert between different dimensions",
        ));
    }

    if is_affine_unit_expr(&q.unit, registry) || is_affine_unit_expr(target, registry) {
        return convert_affine(q, target, registry);
    }

    let anchor = magnitude_in_anchor_units(q, registry)?;
    let target_factor = unit_to_anchor_factor(target, registry)?;
    if target_factor.is_zero() {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "zero conversion factor",
            Span::empty(0),
        )));
    }
    let mag = anchor / target_factor;
    Ok(Quantity::new(mag, target.clone(), target_dim))
}

/// Leftmost-wins addition/subtraction unification.
pub fn unify_add(
    left: &Quantity,
    right: &Quantity,
    registry: &Registry,
    span: Span,
) -> Result<Quantity, Diag> {
    if left.dim != right.dim {
        return Err(dim_mismatch(
            &left.dim,
            &right.dim,
            span,
            "cannot add or subtract unlike dimensions",
        ));
    }

    if is_affine_unit_expr(&left.unit, registry) || is_affine_unit_expr(&right.unit, registry) {
        if affine_same_display_unit(&left.unit, &right.unit) {
            let rhs = convert_quantity(right, &left.unit, registry)?;
            let mag = rational_add(left.effective_magnitude(), rhs.effective_magnitude())?;
            return Ok(Quantity {
                magnitude: mag,
                float_mag: None,
                unit: left.unit.clone(),
                dim: left.dim.clone(),
            });
        }
        use crate::eval::affine::{absolute_from_rankine, to_rankine};
        let l = to_rankine(left, registry)?;
        let r = to_rankine(right, registry)?;
        let sum = Quantity::new(
            l.effective_magnitude() + r.effective_magnitude(),
            l.unit.clone(),
            l.dim.clone(),
        );
        return absolute_from_rankine(&sum, &left.unit, registry);
    }

    let rhs = convert_quantity(right, &left.unit, registry)?;
    let mag = rational_add(left.effective_magnitude(), rhs.effective_magnitude())?;
    Ok(Quantity {
        magnitude: mag,
        float_mag: None,
        unit: left.unit.clone(),
        dim: left.dim.clone(),
    })
}

/// Leftmost-wins subtraction.
pub fn unify_sub(
    left: &Quantity,
    right: &Quantity,
    registry: &Registry,
    span: Span,
) -> Result<Quantity, Diag> {
    if left.dim != right.dim {
        return Err(dim_mismatch(
            &left.dim,
            &right.dim,
            span,
            "cannot add or subtract unlike dimensions",
        ));
    }

    if is_affine_unit_expr(&left.unit, registry) || is_affine_unit_expr(&right.unit, registry) {
        if affine_same_display_unit(&left.unit, &right.unit) {
            let rhs = convert_quantity(right, &left.unit, registry)?;
            let mag = rational_sub(left.effective_magnitude(), rhs.effective_magnitude())?;
            return Ok(Quantity {
                magnitude: mag,
                float_mag: None,
                unit: left.unit.clone(),
                dim: left.dim.clone(),
            });
        }
        use crate::eval::affine::{absolute_from_rankine, to_rankine};
        let l = to_rankine(left, registry)?;
        let r = to_rankine(right, registry)?;
        let diff = Quantity::new(
            l.effective_magnitude() - r.effective_magnitude(),
            l.unit.clone(),
            l.dim.clone(),
        );
        return absolute_from_rankine(&diff, &left.unit, registry);
    }

    let rhs = convert_quantity(right, &left.unit, registry)?;
    let mag = rational_sub(left.effective_magnitude(), rhs.effective_magnitude())?;
    Ok(Quantity {
        magnitude: mag,
        float_mag: None,
        unit: left.unit.clone(),
        dim: left.dim.clone(),
    })
}

/// Multiply two quantities (dimension composition).
pub fn combine_mul(left: &Quantity, right: &Quantity, _span: Span) -> Result<Quantity, Diag> {
    let unit = compose_unit_expr(&left.unit, &right.unit, true);
    let dim = left.dim.mul(&right.dim);
    let mag = rational_mul(left.effective_magnitude(), right.effective_magnitude())?;
    Ok(Quantity::new(mag, unit, dim))
}

/// Divide two quantities.
pub fn combine_div(left: &Quantity, right: &Quantity, span: Span) -> Result<Quantity, Diag> {
    if right.effective_magnitude().is_zero() {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "division by zero",
            span,
        )));
    }
    let unit = compose_unit_expr(&left.unit, &right.unit, false);
    let dim = left.dim.div(&right.dim);
    let mag = rational_div(left.effective_magnitude(), right.effective_magnitude())?;
    Ok(Quantity::new(mag, unit, dim))
}

/// Raise quantity to a dimensionless exponent.
pub fn combine_pow(
    left: &Quantity,
    exp: &Quantity,
    span: Span,
) -> Result<Quantity, Diag> {
    if !exp.dim.is_dimensionless() {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::DimMismatch,
            "exponent must be dimensionless",
            span,
        )));
    }
    let e = ratio_to_i32(exp.effective_magnitude(), span)?;
    let dim = left.dim.pow(Ratio::from_integer(e));
    let mag = left.effective_magnitude().pow(e);
    let unit = if e == 1 {
        left.unit.clone()
    } else if e == 0 {
        UnitExpr::one()
    } else {
        UnitExpr::Pow {
            base: Box::new(left.unit.clone()),
            exp: UnitExponent::Int(e),
        }
    };
    Ok(Quantity::new(mag, unit, dim))
}

/// Halve dimension exponents (for `sqrt`).
pub fn halve_dimension(dim: &Dimension) -> Result<Dimension, Diag> {
    let half = Ratio::new(1, 2);
    Ok(dim.pow(half))
}

fn unit_to_anchor_factor(unit: &UnitExpr, registry: &Registry) -> Result<Ratio<i128>, Diag> {
    match unit {
        UnitExpr::Dimensionless => Ok(Ratio::one()),
        UnitExpr::Named(name) => registry
            .unit(name)
            .map(|u| u.anchor_ratio)
            .ok_or_else(|| unknown_unit(name, Span::empty(0))),
        UnitExpr::Product(parts) => parts
            .iter()
            .try_fold(Ratio::one(), |acc, part| Ok(acc * unit_to_anchor_factor(part, registry)?)),
        UnitExpr::Quotient(num, den) => Ok(
            unit_to_anchor_factor(num, registry)? / unit_to_anchor_factor(den, registry)?,
        ),
        UnitExpr::Pow { base, exp } => {
            let base_f = unit_to_anchor_factor(base, registry)?;
            let factor = unit_exponent_factor_f64(exp)?;
            Ok(f64_to_ratio_approx(ratio_to_f64(base_f).powf(factor)))
        }
    }
}

fn unit_exponent_to_ratio(exp: &UnitExponent) -> Result<Ratio<i32>, Diag> {
    match exp {
        UnitExponent::Int(n) => Ok(Ratio::from_integer(*n)),
        UnitExponent::Ratio { num, den } => {
            if *den == 0 {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    "zero denominator in unit exponent",
                    Span::empty(0),
                )));
            }
            Ok(Ratio::new(*num, *den))
        }
        UnitExponent::Decimal(s) => {
            if s == "0.5" {
                Ok(Ratio::new(1, 2))
            } else if let Some(f) = s.parse::<f64>().ok().and_then(Ratio::<i32>::from_f64) {
                Ok(f)
            } else {
                Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    "invalid decimal unit exponent",
                    Span::empty(0),
                )))
            }
        }
    }
}

fn unit_exponent_factor_f64(exp: &UnitExponent) -> Result<f64, Diag> {
    match exp {
        UnitExponent::Int(n) => Ok(*n as f64),
        UnitExponent::Ratio { num, den } => {
            if *den == 0 {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    "zero denominator in unit exponent",
                    Span::empty(0),
                )));
            }
            Ok(*num as f64 / *den as f64)
        }
        UnitExponent::Decimal(s) => s.parse().map_err(|_| {
            Diag::new(Diagnostic::error(
                ErrorCode::Eval,
                "invalid decimal unit exponent",
                Span::empty(0),
            ))
        }),
    }
}

fn compose_unit_expr(lhs: &UnitExpr, rhs: &UnitExpr, mul: bool) -> UnitExpr {
    if mul {
        match (lhs, rhs) {
            (UnitExpr::Dimensionless, u) | (u, UnitExpr::Dimensionless) => u.clone(),
            (UnitExpr::Product(parts), rhs) => {
                let mut parts = parts.clone();
                parts.push(rhs.clone());
                UnitExpr::Product(parts)
            }
            (lhs, UnitExpr::Product(parts)) => {
                let mut out = vec![lhs.clone()];
                out.extend(parts.iter().cloned());
                UnitExpr::Product(out)
            }
            _ => UnitExpr::Product(vec![lhs.clone(), rhs.clone()]),
        }
    } else {
        UnitExpr::Quotient(Box::new(lhs.clone()), Box::new(rhs.clone()))
    }
}

fn is_affine_unit_expr(unit: &UnitExpr, registry: &Registry) -> bool {
    match unit {
        UnitExpr::Named(name) => registry.unit(name).is_some_and(|u| u.affine),
        _ => false,
    }
}

fn affine_same_display_unit(left: &UnitExpr, right: &UnitExpr) -> bool {
    match (left, right) {
        (UnitExpr::Named(a), UnitExpr::Named(b)) => a == b,
        _ => false,
    }
}

fn convert_affine(
    q: &Quantity,
    target: &UnitExpr,
    registry: &Registry,
) -> Result<Quantity, Diag> {
    use crate::eval::affine::{absolute_from_rankine, to_rankine};

    let rankine = to_rankine(q, registry)?;
    absolute_from_rankine(&rankine, target, registry)
}

fn rational_add(a: Ratio<i128>, b: Ratio<i128>) -> Result<Ratio<i128>, Diag> {
    a.checked_add(&b).ok_or_else(overflow_diag)
}

fn rational_sub(a: Ratio<i128>, b: Ratio<i128>) -> Result<Ratio<i128>, Diag> {
    a.checked_sub(&b).ok_or_else(overflow_diag)
}

fn rational_mul(a: Ratio<i128>, b: Ratio<i128>) -> Result<Ratio<i128>, Diag> {
    a.checked_mul(&b).ok_or_else(overflow_diag)
}

fn rational_div(a: Ratio<i128>, b: Ratio<i128>) -> Result<Ratio<i128>, Diag> {
    a.checked_div(&b).ok_or_else(overflow_diag)
}

fn f64_to_ratio_approx(f: f64) -> Ratio<i128> {
    const SCALE: i128 = 1_000_000_000_000;
    Ratio::new((f * SCALE as f64).round() as i128, SCALE)
}

fn ratio_to_f64(r: Ratio<i128>) -> f64 {
    let n: f64 = num_traits::ToPrimitive::to_f64(r.numer()).unwrap_or(0.0);
    let d: f64 = num_traits::ToPrimitive::to_f64(r.denom()).unwrap_or(1.0);
    n / d
}

fn ratio_to_i32(r: Ratio<i128>, span: Span) -> Result<i32, Diag> {
    if r.denom() != &1i128 {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "non-integer exponent",
            span,
        )));
    }
    (*r.numer()).try_into().map_err(|_| {
        Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "exponent out of range",
            span,
        ))
    })
}

fn overflow_diag() -> Diag {
    Diag::new(Diagnostic::error(
        ErrorCode::Eval,
        "rational overflow",
        Span::empty(0),
    ))
}

fn unknown_unit(name: &str, span: Span) -> Diag {
    Diag::new(Diagnostic::error(
        ErrorCode::UnknownUnit,
        format!("unknown unit `{name}`"),
        span,
    ))
}

fn dim_mismatch(expected: &Dimension, found: &Dimension, span: Span, msg: &str) -> Diag {
    Diag::new(
        Diagnostic::error(ErrorCode::DimMismatch, msg, span).with_hints(vec![
            Hint::ExpectedDimension(format!("{expected:?}")),
            Hint::FoundDimension(format!("{found:?}")),
        ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::BaseDim;
    use crate::RegistryBuilder;
    use num_traits::FromPrimitive;

    #[test]
    fn convert_ft_to_in() {
        let reg = RegistryBuilder::from_seed().freeze();
        let q = Quantity::from_int(1, "ft", Dimension::single(BaseDim::Length, Ratio::one()));
        let converted = convert_quantity(&q, &UnitExpr::named("in"), &reg).unwrap();
        assert_eq!(converted.magnitude, Ratio::from_i32(12).unwrap());
        assert_eq!(converted.unit.as_str(), "in");
    }
}
