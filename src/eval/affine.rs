//! Affine temperature conversions over absolute Rankine.

use num_rational::Ratio;
use num_traits::{One, Zero};

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::dim::{BaseDim, Dimension};
use crate::eval::mag::Mag;
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
    let rankine = match q.mag {
        Mag::Exact(mag) => match name.as_str() {
            "°F" | "F" => Mag::Exact(mag + Ratio::new(45967, 100)),
            "°C" | "C" => Mag::Exact((mag + Ratio::new(27315, 100)) * Ratio::new(9, 5)),
            "K" => Mag::Exact(mag * Ratio::new(9, 5)),
            "°R" | "R" => Mag::Exact(mag),
            other => return Err(affine_named_error(other, registry)),
        },
        Mag::Float(f) => {
            let r = match name.as_str() {
                "°F" | "F" => f + 459.67,
                "°C" | "C" => (f + 273.15) * 9.0 / 5.0,
                "K" => f * 9.0 / 5.0,
                "°R" | "R" => f,
                other => return Err(affine_named_error(other, registry)),
            };
            Mag::float(r).map_err(|_| non_finite(Span::empty(0)))?
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
    match rankine.mag {
        Mag::Exact(r) => match name.as_str() {
            "°R" | "R" => Ok(quantity_temp(Mag::Exact(r), "°R")),
            "°F" | "F" => Ok(quantity_temp(
                Mag::Exact(r - Ratio::new(45967, 100)),
                "°F",
            )),
            "°C" | "C" => Ok(quantity_temp(
                Mag::Exact(r * Ratio::new(5, 9) - Ratio::new(27315, 100)),
                "°C",
            )),
            "K" => Ok(quantity_temp(Mag::Exact(r * Ratio::new(5, 9)), "K")),
            other => affine_target_linear(other, r, target, registry),
        },
        Mag::Float(f) => match name.as_str() {
            "°R" | "R" => Ok(quantity_temp(Mag::Float(f), "°R")),
            "°F" | "F" => Ok(quantity_temp(
                Mag::float(f - 459.67).map_err(|_| non_finite(Span::empty(0)))?,
                "°F",
            )),
            "°C" | "C" => Ok(quantity_temp(
                Mag::float(f * 5.0 / 9.0 - 273.15).map_err(|_| non_finite(Span::empty(0)))?,
                "°C",
            )),
            "K" => Ok(quantity_temp(
                Mag::float(f * 5.0 / 9.0).map_err(|_| non_finite(Span::empty(0)))?,
                "K",
            )),
            other => {
                let dim = dimension_of_unit(target, registry)?;
                let factor = registry
                    .unit(other)
                    .map(|u| u.anchor_ratio)
                    .ok_or_else(|| unknown_unit(other))?;
                if factor.is_zero() {
                    return Err(Diag::new(Diagnostic::error(
                        ErrorCode::Eval,
                        "zero conversion factor",
                        Span::empty(0),
                    )));
                }
                let mag_f = f / ratio_to_f64(factor);
                Ok(Quantity::new(
                    Mag::float(mag_f).map_err(|_| non_finite(Span::empty(0)))?,
                    target.clone(),
                    dim,
                ))
            }
        },
    }
}

fn quantity_temp(mag: Mag, unit_name: &str) -> Quantity {
    Quantity::new(
        mag,
        UnitExpr::named(unit_name),
        Dimension::single(BaseDim::Temperature, Ratio::one()),
    )
}

fn affine_target_linear(
    other: &str,
    r: Ratio<i128>,
    target: &UnitExpr,
    registry: &Registry,
) -> Result<Quantity, Diag> {
    if registry.unit(other).is_some_and(|u| u.affine) {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            format!("affine conversion for `{other}` is not implemented"),
            Span::empty(0),
        )));
    }
    let dim = dimension_of_unit(target, registry)?;
    let factor = registry
        .unit(other)
        .map(|u| u.anchor_ratio)
        .ok_or_else(|| unknown_unit(other))?;
    if factor.is_zero() {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "zero conversion factor",
            Span::empty(0),
        )));
    }
    Ok(Quantity::new(
        Mag::Exact(r / factor),
        target.clone(),
        dim,
    ))
}

fn affine_named_error(other: &str, registry: &Registry) -> Diag {
    if registry.unit(other).is_some_and(|u| u.affine) {
        return Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            format!("affine conversion for `{other}` is not implemented"),
            Span::empty(0),
        ));
    }
    Diag::new(Diagnostic::error(
        ErrorCode::Eval,
        format!("`{other}` is not an affine temperature unit"),
        Span::empty(0),
    ))
}

fn unknown_unit(name: &str) -> Diag {
    Diag::new(Diagnostic::error(
        ErrorCode::UnknownUnit,
        format!("unknown unit `{name}`"),
        Span::empty(0),
    ))
}

fn non_finite(span: Span) -> Diag {
    Diag::new(Diagnostic::error(
        ErrorCode::Eval,
        "non-finite numeric result",
        span,
    ))
}

fn ratio_to_f64(r: Ratio<i128>) -> f64 {
    let n: f64 = num_traits::ToPrimitive::to_f64(r.numer()).unwrap_or(0.0);
    let d: f64 = num_traits::ToPrimitive::to_f64(r.denom()).unwrap_or(1.0);
    n / d
}
