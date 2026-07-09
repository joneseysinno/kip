//! Automatic constant dimensionalization of empirical pack bodies.

use std::sync::Arc;

use num_rational::Ratio;

use crate::dim::Dimension;
use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::eval::units::{dimension_of_unit, halve_dimension};
use crate::packs::contract::ArgContract;
use crate::parser::ast::{BinaryOp, Callee, Expr, ExprKind, NodeId, UnaryOp};
use crate::registry::Registry;

/// Rewrite a pack body so dimensionless empirical constants carry the units
/// implied by argument contracts and the declared result dimension.
pub fn dimensionalize_body(
    body_src: &str,
    body: &Expr,
    args: &std::collections::BTreeMap<String, ArgContract>,
    result_dim: &Dimension,
    registry: &Registry,
) -> Result<Arc<Expr>, Diag> {
    let body_dim = infer_dim(body, body.root, args)?;
    if body_dim == *result_dim {
        return Ok(Arc::new(body.clone()));
    }

    let needed = result_dim.div(&body_dim);
    if needed.is_dimensionless() {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::PackBody,
            "pack body dimension does not match declared result and cannot be dimensionalized",
            Span::empty(0),
        )));
    }

    let factor = unit_factor_expr(&needed, args, registry)?;
    let new_src = format!("{factor} * ({body_src})");
    crate::parse(&new_src, registry).map_err(|diags| {
        diags.into_iter().next().unwrap_or_else(|| {
            Diag::new(Diagnostic::error(
                ErrorCode::PackBody,
                "dimensionalized pack body failed to parse",
                Span::empty(0),
            ))
        })
    })
}

fn unit_factor_expr(
    needed: &Dimension,
    args: &std::collections::BTreeMap<String, ArgContract>,
    registry: &Registry,
) -> Result<String, Diag> {
    for contract in args.values() {
        if contract.dim == *needed {
            return Ok(format!("1 {}", contract.unit.as_str()));
        }
        if let Ok(half) = halve_dimension(&contract.dim) {
            if half == *needed {
                return Ok(format!("sqrt(1 {})", contract.unit.as_str()));
            }
        }
        let double = contract.dim.mul(&contract.dim);
        if double == *needed {
            return Ok(format!("(1 {})^2", contract.unit.as_str()));
        }
    }

    // Fall back: build from result unit names in contracts.
    for contract in args.values() {
        let udim = dimension_of_unit(&contract.unit, registry)?;
        if udim == *needed {
            return Ok(format!("1 {}", contract.unit.as_str()));
        }
    }

    Err(Diag::new(Diagnostic::error(
        ErrorCode::PackBody,
        format!("cannot derive unit factor for dimension {needed:?}"),
        Span::empty(0),
    )))
}

fn infer_dim(
    expr: &Expr,
    id: NodeId,
    args: &std::collections::BTreeMap<String, ArgContract>,
) -> Result<Dimension, Diag> {
    let span = expr.node(id).span;
    match &expr.node(id).kind {
        ExprKind::Number { .. } => Ok(Dimension::dimensionless()),
        ExprKind::Quantity { .. } => {
            // Should not appear in pre-dimensionalize bodies.
            Err(Diag::new(Diagnostic::error(
                ErrorCode::PackBody,
                "quantity literals in pack bodies must be dimensionalized at load time",
                span,
            )))
        }
        ExprKind::Length { .. } => Err(Diag::new(Diagnostic::error(
            ErrorCode::PackBody,
            "length tick literals are not allowed in pack bodies",
            span,
        ))),
        ExprKind::Ident { name } => args.get(name).map(|c| c.dim.clone()).ok_or_else(|| {
            Diag::new(Diagnostic::error(
                ErrorCode::PackBody,
                format!("unknown argument `{name}` in pack body"),
                span,
            ))
        }),
        ExprKind::Unary { op, operand } => match op {
            UnaryOp::Neg => infer_dim(expr, *operand, args),
        },
        ExprKind::Binary { op, left, right } => match op {
            BinaryOp::Add | BinaryOp::Sub => {
                let l = infer_dim(expr, *left, args)?;
                let r = infer_dim(expr, *right, args)?;
                if l != r {
                    return Err(Diag::new(Diagnostic::error(
                        ErrorCode::PackBody,
                        "additive pack body terms have mismatched dimensions",
                        span,
                    )));
                }
                Ok(l)
            }
            BinaryOp::Mul => Ok(infer_dim(expr, *left, args)?
                .mul(&infer_dim(expr, *right, args)?)),
            BinaryOp::Div => Ok(infer_dim(expr, *left, args)?
                .div(&infer_dim(expr, *right, args)?)),
            BinaryOp::Pow => {
                let base = infer_dim(expr, *left, args)?;
                match &expr.node(*right).kind {
                    ExprKind::Number { value, .. } if value.denom() == &1 => {
                        let exp = Ratio::new(
                            (*value.numer()).try_into().unwrap_or(1),
                            1,
                        );
                        Ok(base.pow(exp))
                    }
                    _ => Err(Diag::new(Diagnostic::error(
                        ErrorCode::PackBody,
                        "pack body exponents must be integer literals",
                        span,
                    ))),
                }
            }
            BinaryOp::Cmp(_) => Err(Diag::new(Diagnostic::error(
                ErrorCode::PackBody,
                "comparisons are not allowed in pack bodies",
                span,
            ))),
        },
        ExprKind::Call { callee, args: call_args } => {
            let name = match callee {
                Callee::Ident(s) => s.as_str(),
                Callee::Path(_) => {
                    return Err(Diag::new(Diagnostic::error(
                        ErrorCode::PackBody,
                        "code-equation calls are not allowed inside pack bodies",
                        span,
                    )));
                }
            };
            if name != "sqrt" || call_args.len() != 1 {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::PackBody,
                    format!("unsupported function `{name}` in pack body"),
                    span,
                )));
            }
            let crate::parser::CallArg::Positional(arg_id) = call_args[0] else {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::PackBody,
                    "pack body calls require positional arguments",
                    span,
                )));
            };
            halve_dimension(&infer_dim(expr, arg_id, args)?)
        }
    }
}
