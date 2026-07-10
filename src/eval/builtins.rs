#![deny(clippy::arithmetic_side_effects)]

use std::cmp::Ordering;

use num_rational::Ratio;
use num_traits::ToPrimitive;

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::dim::{BaseDim, Dimension};
use crate::eval::lint_sink::LintSink;
use crate::eval::mag::{Mag, TaintEvent};
use crate::eval::rational::rational_sqrt;
use crate::eval::units::{convert_quantity, halve_dimension, mag_cmp};
use crate::eval::value::{Quantity, SymUnaryOp, Value};
use crate::quantity::{UnitExpr, UnitExponent};
use crate::registry::Registry;

/// Evaluate a built-in function call.
pub fn eval_builtin(
    name: &str,
    args: &[Value],
    registry: &Registry,
    span: Span,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    match name {
        "sqrt" => eval_sqrt(args, span, lints),
        "abs" => eval_unary_quantity(args, |q| {
            Ok(Quantity::new(
                q.mag.abs(),
                q.unit.clone(),
                q.dim.clone(),
            ))
        }, span),
        "min" | "max" => eval_min_max(name, args, registry, span, lints),
        "floor" | "ceil" | "round" => eval_rounding(name, args, span),
        "sin" | "cos" | "tan" => eval_trig(name, args, registry, span, lints),
        "asin" | "acos" | "atan" => eval_inverse_trig(name, args, registry, span, lints),
        "atan2" => eval_atan2(args, registry, span, lints),
        "ln" | "log10" | "exp" => eval_transcendental(name, args, span, lints),
        _ => Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            format!("unknown function `{name}`"),
            span,
        ))),
    }
}

fn eval_sqrt(args: &[Value], span: Span, lints: &mut LintSink) -> Result<Value, Diag> {
    if args.len() != 1 {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            format!("`sqrt` requires 1 argument, found {}", args.len()),
            span,
        )));
    }
    match &args[0] {
        Value::Known(q) => {
            if q.mag.is_negative() {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    "square root of negative value",
                    span,
                )));
            }
            let dim = halve_dimension(&q.dim)?;
            let unit = UnitExpr::Pow {
                base: Box::new(q.unit.clone()),
                exp: UnitExponent::Ratio { num: 1, den: 2 },
            };
            let mag = match q.mag {
                Mag::Exact(r) => {
                    if let Some(root) = rational_sqrt(r) {
                        Mag::Exact(root)
                    } else {
                        let f = r.to_f64().unwrap_or(0.0).sqrt();
                        lints.record_mag_event(
                            TaintEvent::ExactnessLost,
                            span,
                            "square root produced an inexact result",
                        );
                        Mag::float(f).map_err(|_| non_finite(span))?
                    }
                }
                Mag::Float(f) => Mag::float(f.sqrt()).map_err(|_| non_finite(span))?,
            };
            Ok(Value::Known(Quantity::new(mag, unit, dim)))
        }
        Value::Symbolic(s) => Ok(crate::eval::partial::symbolic_unary(
            SymUnaryOp::Sqrt,
            s,
        )),
    }
}

fn eval_min_max(
    name: &str,
    args: &[Value],
    registry: &Registry,
    span: Span,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    if args.len() < 2 {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            format!("`{name}` requires at least two arguments"),
            span,
        )));
    }
    let mut acc = require_known_quantity(&args[0], span)?.clone();
    for arg in &args[1..] {
        let q = require_known_quantity(arg, span)?;
        let rhs = convert_quantity(q, &acc.unit, registry, lints, span)?;
        let pick_rhs = match mag_cmp(acc.mag, rhs.mag) {
            Some(Ordering::Greater) => name == "min",
            Some(Ordering::Less) => name == "max",
            Some(Ordering::Equal) => false,
            None => {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    "non-comparable magnitudes in min/max",
                    span,
                )));
            }
        };
        if pick_rhs {
            acc = rhs;
        }
    }
    Ok(Value::Known(acc))
}

/// Rounds half away from zero (matches `f64::round` on the float path).
fn eval_rounding(name: &str, args: &[Value], span: Span) -> Result<Value, Diag> {
    let q = require_quantity(args, 1, span)?;
    let mag = match q.mag {
        Mag::Exact(r) => {
            let rounded = match name {
                "floor" => r.floor(),
                "ceil" => r.ceil(),
                "round" => r.round(),
                _ => unreachable!(),
            };
            Mag::Exact(rounded)
        }
        Mag::Float(f) => {
            let rounded = match name {
                "floor" => f.floor(),
                "ceil" => f.ceil(),
                "round" => f.round(),
                _ => unreachable!(),
            };
            Mag::float(rounded).map_err(|_| non_finite(span))?
        }
    };
    Ok(Value::Known(Quantity::new(
        mag,
        q.unit.clone(),
        q.dim.clone(),
    )))
}

fn eval_trig(
    name: &str,
    args: &[Value],
    registry: &Registry,
    span: Span,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    let q = require_quantity(args, 1, span)?;
    require_angle(&q.dim, span)?;
    let input_exact = q.is_exact();
    let rad = to_radians(q, registry, lints, span)?;
    if input_exact {
        lints.record_mag_event(
            TaintEvent::ExactnessLost,
            span,
            format!("`{name}` produced an inexact result"),
        );
    }
    let f = rad.as_f64();
    let out = match name {
        "sin" => f.sin(),
        "cos" => f.cos(),
        "tan" => f.tan(),
        _ => unreachable!(),
    };
    Ok(Value::Known(dimensionless_float(out)?))
}

fn eval_inverse_trig(
    name: &str,
    args: &[Value],
    _registry: &Registry,
    span: Span,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    let q = require_quantity(args, 1, span)?;
    require_dimensionless(&q.dim, span)?;
    let input_exact = q.is_exact();
    let x = q.as_f64();
    let rad = match name {
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        _ => unreachable!(),
    };
    if input_exact {
        lints.record_mag_event(
            TaintEvent::ExactnessLost,
            span,
            format!("`{name}` produced an inexact result"),
        );
    }
    Ok(Value::Known(Quantity::from_float(
        rad.to_degrees(),
        UnitExpr::named("deg"),
        Dimension::single(BaseDim::Angle, Ratio::from_integer(1)),
    ).map_err(|_| non_finite(span))?))
}

fn eval_atan2(
    args: &[Value],
    _registry: &Registry,
    span: Span,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    if args.len() != 2 {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "`atan2` requires two arguments",
            span,
        )));
    }
    let y = require_known_quantity(&args[0], span)?;
    let x = require_known_quantity(&args[1], span)?;
    require_dimensionless(&y.dim, span)?;
    require_dimensionless(&x.dim, span)?;
    if y.is_exact() && x.is_exact() {
        lints.record_mag_event(
            TaintEvent::ExactnessLost,
            span,
            "`atan2` produced an inexact result",
        );
    }
    let rad = y.as_f64().atan2(x.as_f64());
    Ok(Value::Known(Quantity::from_float(
        rad.to_degrees(),
        UnitExpr::named("deg"),
        Dimension::single(BaseDim::Angle, Ratio::from_integer(1)),
    ).map_err(|_| non_finite(span))?))
}

fn eval_transcendental(
    name: &str,
    args: &[Value],
    span: Span,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    let q = require_quantity(args, 1, span)?;
    require_dimensionless(&q.dim, span)?;
    let x = q.as_f64();
    let out = match name {
        "ln" => x.ln(),
        "log10" => x.log10(),
        "exp" => x.exp(),
        _ => unreachable!(),
    };
    if q.is_exact() {
        lints.record_mag_event(
            TaintEvent::ExactnessLost,
            span,
            format!("`{name}` produced an inexact result"),
        );
    }
    Ok(Value::Known(dimensionless_float(out)?))
}

fn eval_unary_quantity(
    args: &[Value],
    f: impl FnOnce(&Quantity) -> Result<Quantity, Diag>,
    span: Span,
) -> Result<Value, Diag> {
    let q = require_quantity(args, 1, span)?;
    Ok(Value::Known(f(q)?))
}

fn require_quantity(args: &[Value], n: usize, span: Span) -> Result<&Quantity, Diag> {
    if args.len() != n {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            format!("expected {n} argument(s), found {}", args.len()),
            span,
        )));
    }
    require_known_quantity(&args[0], span)
}

fn require_known_quantity(v: &Value, span: Span) -> Result<&Quantity, Diag> {
    match v {
        Value::Known(q) => Ok(q),
        Value::Symbolic(_) => Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "expected a known quantity",
            span,
        ))),
    }
}

fn require_dimensionless(dim: &Dimension, span: Span) -> Result<(), Diag> {
    if !dim.is_dimensionless() {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::DimMismatch,
            "argument must be dimensionless",
            span,
        )));
    }
    Ok(())
}

fn require_angle(dim: &Dimension, span: Span) -> Result<(), Diag> {
    if dim != &Dimension::single(BaseDim::Angle, Ratio::from_integer(1)) {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::DimMismatch,
            "trigonometric argument must have angle dimension",
            span,
        )));
    }
    Ok(())
}

fn to_radians(
    q: &Quantity,
    registry: &Registry,
    lints: &mut LintSink,
    span: Span,
) -> Result<Quantity, Diag> {
    if q.unit.as_str() == "rad" {
        return Ok(q.clone());
    }
    convert_quantity(q, &UnitExpr::named("rad"), registry, lints, span)
}

fn dimensionless_float(f: f64) -> Result<Quantity, Diag> {
    Quantity::from_float(f, UnitExpr::one(), Dimension::dimensionless())
        .map_err(|_| non_finite(Span::empty(0)))
}

fn non_finite(span: Span) -> Diag {
    Diag::new(Diagnostic::error(
        ErrorCode::Eval,
        "non-finite numeric result",
        span,
    ))
}
