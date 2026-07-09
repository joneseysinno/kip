//! Affine temperature conversions over absolute Rankine.

use num_rational::Ratio;
use num_traits::{One, Zero};

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::dim::{BaseDim, Dimension};
use crate::eval::units::dimension_of_unit;
use crate::eval::value::Quantity;
use crate::quantity::UnitExpr;
use crate::registry::Registry;

/// Convert a quantity with an affine temperature unit to absolute Rankine.
pub fn to_rankine(q: &Quantity, registry: &Registry) -> Result<Quantity, Diag> {
    let UnitExpr::Named(name) = &q.unit else {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "affine conversion requires a named temperature unit",
            Span::empty(0),
        )));
    };
    let mag = q.effective_magnitude();
    let rankine = match name.as_str() {
        "°F" | "F" => mag + Ratio::new(45967, 100),
        "°C" | "C" => (mag + Ratio::new(27315, 100)) * Ratio::new(9, 5),
        "K" => mag * Ratio::new(9, 5),
        "°R" | "R" => mag,
        other => {
            if registry.unit(other).is_some_and(|u| u.affine) {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    format!("affine conversion for `{other}` is not implemented"),
                    Span::empty(0),
                )));
            }
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::Eval,
                format!("`{other}` is not an affine temperature unit"),
                Span::empty(0),
            )));
        }
    };
    Ok(Quantity::new(
        rankine,
        UnitExpr::named("°R"),
        Dimension::single(BaseDim::Temperature, Ratio::one()),
    ))
}

/// Convert an absolute Rankine quantity into a target affine or absolute unit.
pub fn absolute_from_rankine(
    rankine: &Quantity,
    target: &UnitExpr,
    registry: &Registry,
) -> Result<Quantity, Diag> {
    let UnitExpr::Named(name) = target else {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "affine conversion target must be a named unit",
            Span::empty(0),
        )));
    };
    let r = rankine.effective_magnitude();
    let (mag, unit_name) = match name.as_str() {
        "°R" | "R" => (r, "°R"),
        "°F" | "F" => (r - Ratio::new(45967, 100), "°F"),
        "°C" | "C" => (r * Ratio::new(5, 9) - Ratio::new(27315, 100), "°C"),
        "K" => (r * Ratio::new(5, 9), "K"),
        other => {
            if registry.unit(other).map(|u| u.affine).unwrap_or(false) {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    format!("affine conversion for `{other}` is not implemented"),
                    Span::empty(0),
                )));
            }
            // Linear non-affine unit in temperature dimension.
            let dim = dimension_of_unit(target, registry)?;
            let factor = registry
                .unit(other)
                .map(|u| u.anchor_ratio)
                .ok_or_else(|| {
                    Diag::new(Diagnostic::error(
                        ErrorCode::UnknownUnit,
                        format!("unknown unit `{other}`"),
                        Span::empty(0),
                    ))
                })?;
            if factor.is_zero() {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    "zero conversion factor",
                    Span::empty(0),
                )));
            }
            return Ok(Quantity::new(r / factor, target.clone(), dim));
        }
    };
    Ok(Quantity::new(
        mag,
        UnitExpr::named(unit_name),
        Dimension::single(BaseDim::Temperature, Ratio::one()),
    ))
}
