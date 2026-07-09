//! Built-in functions with dimension semantics (M4).

use num_rational::Ratio;
use num_traits::Signed;

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::dim::{BaseDim, Dimension};
use crate::eval::units::{convert_quantity, halve_dimension};
use crate::eval::value::{Quantity, SymUnaryOp, Value};
use crate::quantity::{UnitExpr, UnitExponent};
use crate::registry::Registry;

/// Evaluate a built-in function call.
pub fn eval_builtin(
    name: &str,
    args: &[Value],
    registry: &Registry,
    span: Span,
) -> Result<Value, Diag> {
    match name {
        "sqrt" => eval_sqrt(args, span),
        "abs" => eval_unary_quantity(args, |q| {
            Ok(Quantity::new(
                q.magnitude.abs(),
                q.unit.clone(),
                q.dim.clone(),
            ))
        }, span),
        "min" | "max" => eval_min_max(name, args, registry, span),
        "floor" | "ceil" | "round" => eval_rounding(name, args, span),
        "sin" | "cos" | "tan" => eval_trig(name, args, registry, span),
        "asin" | "acos" | "atan" => eval_inverse_trig(name, args, registry, span),
        "atan2" => eval_atan2(args, registry, span),
        "ln" | "log10" | "exp" => eval_transcendental(name, args, span),
        _ => Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            format!("unknown function `{name}`"),
            span,
        ))),
    }
}

fn eval_sqrt(args: &[Value], span: Span) -> Result<Value, Diag> {
    if args.len() != 1 {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            format!("`sqrt` requires 1 argument, found {}", args.len()),
            span,
        )));
    }
    match &args[0] {
        Value::Known(q) => {
            if q.effective_magnitude().is_negative() {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    "square root of negative value",
                    span,
                )));
            }
            let dim = halve_dimension(&q.dim)?;
            let mag_f = q.as_f64().sqrt();
            let unit = UnitExpr::Pow {
                base: Box::new(q.unit.clone()),
                exp: UnitExponent::Ratio { num: 1, den: 2 },
            };
            Ok(Value::Known(Quantity {
                magnitude: Ratio::from_integer(1),
                float_mag: Some(mag_f),
                unit,
                dim,
                provenance: None,
            }))
        }
        Value::Symbolic(s) => Ok(crate::eval::partial::symbolic_unary(
            SymUnaryOp::Sqrt,
            s,
        )),
    }
}

fn eval_min_max(name: &str, args: &[Value], registry: &Registry, span: Span) -> Result<Value, Diag> {
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
        let rhs = convert_quantity(q, &acc.unit, registry)?;
        let pick_rhs = match name {
            "min" => rhs.as_f64() < acc.as_f64(),
            "max" => rhs.as_f64() > acc.as_f64(),
            _ => unreachable!(),
        };
        if pick_rhs {
            acc = rhs;
        }
    }
    Ok(Value::Known(acc))
}

fn eval_rounding(name: &str, args: &[Value], span: Span) -> Result<Value, Diag> {
    let q = require_quantity(args, 1, span)?;
    let f = q.as_f64();
    let rounded = match name {
        "floor" => f.floor(),
        "ceil" => f.ceil(),
        "round" => f.round(),
        _ => unreachable!(),
    };
    Ok(Value::Known(float_quantity(q, rounded)))
}

fn eval_trig(
    name: &str,
    args: &[Value],
    registry: &Registry,
    span: Span,
) -> Result<Value, Diag> {
    let q = require_quantity(args, 1, span)?;
    require_angle(&q.dim, span)?;
    let rad = to_radians(q, registry)?;
    let f = rad.as_f64();
    let out = match name {
        "sin" => f.sin(),
        "cos" => f.cos(),
        "tan" => f.tan(),
        _ => unreachable!(),
    };
    Ok(Value::Known(Quantity {
        magnitude: Ratio::from_integer(1),
        float_mag: Some(out),
        unit: UnitExpr::one(),
        dim: Dimension::dimensionless(),
        provenance: None,
    }))
}

fn eval_inverse_trig(
    name: &str,
    args: &[Value],
    _registry: &Registry,
    span: Span,
) -> Result<Value, Diag> {
    let q = require_quantity(args, 1, span)?;
    require_dimensionless(&q.dim, span)?;
    let x = q.as_f64();
    let rad = match name {
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        _ => unreachable!(),
    };
    let mut out = Quantity::from_int(
        0,
        "deg",
        Dimension::single(BaseDim::Angle, Ratio::from_integer(1)),
    );
    out.float_mag = Some(rad.to_degrees());
    Ok(Value::Known(out))
}

fn eval_atan2(args: &[Value], _registry: &Registry, span: Span) -> Result<Value, Diag> {
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
    let rad = y.as_f64().atan2(x.as_f64());
    let mut q = Quantity::from_int(
        0,
        "deg",
        Dimension::single(BaseDim::Angle, Ratio::from_integer(1)),
    );
    q.float_mag = Some(rad.to_degrees());
    Ok(Value::Known(q))
}

fn eval_transcendental(name: &str, args: &[Value], span: Span) -> Result<Value, Diag> {
    let q = require_quantity(args, 1, span)?;
    require_dimensionless(&q.dim, span)?;
    let x = q.as_f64();
    let out = match name {
        "ln" => x.ln(),
        "log10" => x.log10(),
        "exp" => x.exp(),
        _ => unreachable!(),
    };
    Ok(Value::Known(Quantity {
        magnitude: Ratio::from_integer(1),
        float_mag: Some(out),
        unit: UnitExpr::one(),
        dim: Dimension::dimensionless(),
        provenance: None,
    }))
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

fn to_radians(q: &Quantity, registry: &Registry) -> Result<Quantity, Diag> {
    if q.unit.as_str() == "rad" {
        return Ok(q.clone());
    }
    q.convert_to(&UnitExpr::named("rad"), registry)
}

fn float_quantity(base: &Quantity, f: f64) -> Quantity {
    Quantity {
        magnitude: base.magnitude,
        float_mag: Some(f),
        unit: base.unit.clone(),
        dim: base.dim.clone(),
        provenance: base.provenance.clone(),
    }
}
