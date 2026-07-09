//! Iterative evaluation of fully-known expressions (M4).

use std::collections::HashMap;

use num_rational::Ratio;
use num_traits::One;

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::dim::{BaseDim, Dimension};
use crate::eval::builtins::eval_builtin;
use crate::eval::units::{
    combine_div, combine_mul, combine_pow, dimension_of_unit, unify_add, unify_sub,
};
use crate::eval::value::{Quantity, Symbol, SymExpr, Value};
use crate::parser::ast::{
    BinaryOp, CallArg, Callee, Expr, ExprKind, NodeId, UnaryOp,
};
use crate::quantity::UnitExpr;
use crate::registry::Registry;
use crate::resolver::Resolver;

/// Evaluate `expr` against a frozen registry and symbol resolver.
pub fn eval_known(
    expr: &Expr,
    registry: &Registry,
    resolver: &dyn Resolver,
) -> Result<Value, Diag> {
    let order = postorder(expr, expr.root);
    let mut values: HashMap<NodeId, Value> = HashMap::with_capacity(order.len());

    for id in order {
        let value = eval_node(expr, id, &values, registry, resolver)?;
        values.insert(id, value);
    }

    values
        .remove(&expr.root)
        .ok_or_else(|| Diag::new(Diagnostic::error(ErrorCode::Eval, "empty expression", Span::empty(0))))
}

fn eval_node(
    expr: &Expr,
    id: NodeId,
    values: &HashMap<NodeId, Value>,
    registry: &Registry,
    resolver: &dyn Resolver,
) -> Result<Value, Diag> {
    let span = expr.node(id).span;
    match &expr.node(id).kind {
        ExprKind::Number { value, .. } => Ok(Value::Known(Quantity::new(
            *value,
            UnitExpr::one(),
            Dimension::dimensionless(),
        ))),
        ExprKind::Quantity {
            magnitude,
            unit,
            ..
        } => {
            let dim = dimension_of_unit(unit, registry).map_err(|mut d| {
                d.0.span = span;
                d
            })?;
            Ok(Value::Known(Quantity::new(*magnitude, unit.clone(), dim)))
        }
        ExprKind::Length { inches } => Ok(Value::Known(Quantity::new(
            *inches,
            UnitExpr::named("in"),
            Dimension::single(BaseDim::Length, Ratio::one()),
        ))),
        ExprKind::Ident { name } => resolve_ident(name, resolver, span),
        ExprKind::Unary { op, operand } => {
            let v = values.get(operand).ok_or(missing_child(span))?;
            eval_unary(*op, v, span)
        }
        ExprKind::Binary { op, left, right } => {
            let lhs = values.get(left).ok_or(missing_child(span))?;
            let rhs = values.get(right).ok_or(missing_child(span))?;
            eval_binary(*op, lhs, rhs, registry, span)
        }
        ExprKind::Call { callee, args } => eval_call(callee, args, values, registry, resolver, span),
    }
}

fn eval_unary(op: UnaryOp, v: &Value, _span: Span) -> Result<Value, Diag> {
    match op {
        UnaryOp::Neg => match v {
            Value::Known(q) => Ok(Value::Known(Quantity::new(
                -q.effective_magnitude(),
                q.unit.clone(),
                q.dim.clone(),
            ))),
            Value::Symbolic(s) => Ok(Value::Symbolic(SymExpr {
                text: format!("-({})", s.text),
                free_symbols: s.free_symbols.clone(),
                constraints: s.constraints.clone(),
            })),
        },
    }
}

fn eval_binary(
    op: BinaryOp,
    lhs: &Value,
    rhs: &Value,
    registry: &Registry,
    span: Span,
) -> Result<Value, Diag> {
    match op {
        BinaryOp::Cmp(_) => Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "comparison operators are reserved for v1.1",
            span,
        ))),
        BinaryOp::Add => eval_add_like(lhs, rhs, registry, span, true),
        BinaryOp::Sub => eval_add_like(lhs, rhs, registry, span, false),
        BinaryOp::Mul => eval_mul_div(lhs, rhs, registry, span, true),
        BinaryOp::Div => eval_mul_div(lhs, rhs, registry, span, false),
        BinaryOp::Pow => {
            let lq = require_known(lhs, span)?;
            let rq = require_known(rhs, span)?;
            Ok(Value::Known(combine_pow(lq, rq, span)?))
        }
    }
}

fn eval_add_like(
    lhs: &Value,
    rhs: &Value,
    registry: &Registry,
    span: Span,
    add: bool,
) -> Result<Value, Diag> {
    match (lhs, rhs) {
        (Value::Known(l), Value::Known(r)) => {
            let q = if add {
                unify_add(l, r, registry, span)?
            } else {
                unify_sub(l, r, registry, span)?
            };
            Ok(Value::Known(q))
        }
        (Value::Symbolic(_), _) | (_, Value::Symbolic(_)) => symbolic_binary(lhs, rhs, if add { "+" } else { "-" }),
    }
}

fn eval_mul_div(
    lhs: &Value,
    rhs: &Value,
    _registry: &Registry,
    span: Span,
    mul: bool,
) -> Result<Value, Diag> {
    match (lhs, rhs) {
        (Value::Known(l), Value::Known(r)) => {
            let q = if mul {
                combine_mul(l, r, span)?
            } else {
                combine_div(l, r, span)?
            };
            Ok(Value::Known(q))
        }
        (Value::Symbolic(_), _) | (_, Value::Symbolic(_)) => {
            symbolic_binary(lhs, rhs, if mul { "*" } else { "/" })
        }
    }
}

fn eval_call(
    callee: &Callee,
    args: &[CallArg],
    values: &HashMap<NodeId, Value>,
    registry: &Registry,
    _resolver: &dyn Resolver,
    span: Span,
) -> Result<Value, Diag> {
    match callee {
        Callee::Path(_) => Err(Diag::new(Diagnostic::error(
            ErrorCode::UnknownEq,
            "code equations are not loaded (M6 milestone)",
            span,
        ))),
        Callee::Ident(name) => {
            let mut positional = Vec::new();
            for arg in args {
                match arg {
                    CallArg::Positional(id) => {
                        positional.push(values.get(id).cloned().ok_or(missing_child(span))?)
                    }
                    CallArg::Named { .. } => {
                        return Err(Diag::new(Diagnostic::error(
                            ErrorCode::Eval,
                            "named arguments are only supported for code equations (M6)",
                            span,
                        )));
                    }
                }
            }
            eval_builtin(name, &positional, registry, span)
        }
    }
}

fn resolve_ident(name: &str, resolver: &dyn Resolver, _span: Span) -> Result<Value, Diag> {
    if let Some(v) = resolver.resolve(name) {
        return Ok(v);
    }
    Ok(Value::Symbolic(SymExpr {
        text: name.to_string(),
        free_symbols: vec![Symbol(name.to_string())],
        constraints: Default::default(),
    }))
}

fn require_known(v: &Value, span: Span) -> Result<&Quantity, Diag> {
    match v {
        Value::Known(q) => Ok(q),
        Value::Symbolic(_) => Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "expected a known quantity",
            span,
        ))),
    }
}

fn symbolic_binary(lhs: &Value, rhs: &Value, op: &str) -> Result<Value, Diag> {
    let text = format!("({}) {op} ({})", value_text(lhs), value_text(rhs));
    let mut free = lhs.free_symbols().to_vec();
    for s in rhs.free_symbols() {
        if !free.contains(s) {
            free.push(s.clone());
        }
    }
    Ok(Value::Symbolic(SymExpr {
        text,
        free_symbols: free,
        constraints: Default::default(),
    }))
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Known(q) => format!("{} {}", q.magnitude, q.unit.as_str()),
        Value::Symbolic(s) => s.text.clone(),
    }
}

fn missing_child(span: Span) -> Diag {
    Diag::new(Diagnostic::error(
        ErrorCode::Eval,
        "internal evaluation error: missing child value",
        span,
    ))
}

fn postorder(expr: &Expr, root: NodeId) -> Vec<NodeId> {
    let mut order = Vec::new();
    let mut stack = vec![(root, false)];
    while let Some((id, expanded)) = stack.pop() {
        if expanded {
            order.push(id);
            continue;
        }
        stack.push((id, true));
        match &expr.node(id).kind {
            ExprKind::Unary { operand, .. } => stack.push((*operand, false)),
            ExprKind::Binary { left, right, .. } => {
                stack.push((*right, false));
                stack.push((*left, false));
            }
            ExprKind::Call { args, .. } => {
                for arg in args.iter().rev() {
                    match arg {
                        CallArg::Positional(id) | CallArg::Named { value: id, .. } => {
                            stack.push((*id, false));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    order
}
