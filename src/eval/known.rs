//! Iterative expression evaluation with partial folding (M4/M5).

use std::collections::HashMap;

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::eval::value::Value;
use crate::eval::builtins::eval_builtin;
use crate::eval::partial::{
    add_like, dimensionless_number, length_literal, mul_div, neg, pow, quantity_from_literal,
    symbol_value, finalize,
};
use crate::parser::ast::{
    BinaryOp, CallArg, Callee, Expr, ExprKind, NodeId, UnaryOp,
};
use crate::registry::Registry;
use crate::resolver::Resolver;

/// Evaluate `expr` against a frozen registry and symbol resolver.
pub(crate) fn eval_known(
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

    let root = values
        .remove(&expr.root)
        .ok_or_else(|| Diag::new(Diagnostic::error(ErrorCode::Eval, "empty expression", Span::empty(0))))?;
    finalize(root, expr.root_node().span)
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
        ExprKind::Number { value, .. } => Ok(dimensionless_number(*value)),
        ExprKind::Quantity {
            magnitude,
            unit,
            ..
        } => quantity_from_literal(*magnitude, unit.clone(), registry, span),
        ExprKind::Length { inches } => Ok(length_literal(*inches)),
        ExprKind::Ident { name } => resolve_ident(name, resolver),
        ExprKind::Unary { op, operand } => {
            let v = values.get(operand).ok_or(missing_child(span))?;
            match op {
                UnaryOp::Neg => neg(v, span),
            }
        }
        ExprKind::Binary { op, left, right } => {
            let lhs = values.get(left).ok_or(missing_child(span))?;
            let rhs = values.get(right).ok_or(missing_child(span))?;
            eval_binary(*op, lhs, rhs, registry, span)
        }
        ExprKind::Call { callee, args } => eval_call(callee, args, values, registry, resolver, span),
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
        BinaryOp::Add => add_like(lhs, rhs, registry, span, true),
        BinaryOp::Sub => add_like(lhs, rhs, registry, span, false),
        BinaryOp::Mul => mul_div(lhs, rhs, registry, span, true),
        BinaryOp::Div => mul_div(lhs, rhs, registry, span, false),
        BinaryOp::Pow => pow(lhs, rhs, span),
    }
}

fn eval_call(
    callee: &Callee,
    args: &[CallArg],
    values: &HashMap<NodeId, Value>,
    registry: &Registry,
    resolver: &dyn Resolver,
    span: Span,
) -> Result<Value, Diag> {
    match callee {
        Callee::Path(path) => {
            #[cfg(feature = "packs")]
            {
                crate::packs::call::eval_equation_call(
                    path, args, values, registry, resolver, span,
                )
            }
            #[cfg(not(feature = "packs"))]
            {
                let _ = (path, args, values, registry, resolver);
                Err(Diag::new(Diagnostic::error(
                    ErrorCode::UnknownEq,
                    "code equations require the `packs` feature",
                    span,
                )))
            }
        }
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

fn resolve_ident(name: &str, resolver: &dyn Resolver) -> Result<Value, Diag> {
    if let Some(v) = resolver.resolve(name) {
        return Ok(v);
    }
    Ok(symbol_value(name))
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
