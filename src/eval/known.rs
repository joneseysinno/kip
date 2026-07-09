//! Iterative expression evaluation with partial folding (M4/M5/M7).

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

/// Minimum subtree node count before `rayon::join` splits siblings (M7).
#[cfg(feature = "parallel")]
pub const PARALLEL_THRESHOLD: usize = 32;

/// Minimum subtree node count before `rayon::join` splits siblings (M7).
#[cfg(not(feature = "parallel"))]
pub const PARALLEL_THRESHOLD: usize = usize::MAX;

/// Evaluate `expr` against a frozen registry and symbol resolver.
pub(crate) fn eval_known(
    expr: &Expr,
    registry: &Registry,
    resolver: &dyn Resolver,
) -> Result<Value, Diag> {
    let sizes = compute_subtree_sizes(expr);
    let mut values = eval_tree(expr, expr.root, registry, resolver, &sizes)?;
    let root = values
        .remove(&expr.root)
        .ok_or_else(|| Diag::new(Diagnostic::error(ErrorCode::Eval, "empty expression", Span::empty(0))))?;
    finalize(root, expr.root_node().span)
}

fn eval_tree(
    expr: &Expr,
    id: NodeId,
    registry: &Registry,
    resolver: &dyn Resolver,
    sizes: &[usize],
) -> Result<HashMap<NodeId, Value>, Diag> {
    #[cfg(feature = "parallel")]
    {
        if let ExprKind::Binary { left, right, op } = &expr.node(id).kind {
            if sizes[left.0 as usize] >= PARALLEL_THRESHOLD
                && sizes[right.0 as usize] >= PARALLEL_THRESHOLD
            {
                let span = expr.node(id).span;
                let (left_map, right_map) = rayon::join(
                    || eval_tree(expr, *left, registry, resolver, sizes),
                    || eval_tree(expr, *right, registry, resolver, sizes),
                );
                let mut values = left_map?;
                values.extend(right_map?);
                let lhs = values
                    .get(left)
                    .cloned()
                    .ok_or_else(|| missing_child(span))?;
                let rhs = values
                    .get(right)
                    .cloned()
                    .ok_or_else(|| missing_child(span))?;
                values.insert(id, eval_binary(*op, &lhs, &rhs, registry, span)?);
                return Ok(values);
            }
        }
    }

    eval_sequential(expr, id, registry, resolver)
}

fn eval_sequential(
    expr: &Expr,
    root: NodeId,
    registry: &Registry,
    resolver: &dyn Resolver,
) -> Result<HashMap<NodeId, Value>, Diag> {
    let order = postorder(expr, root);
    let mut values: HashMap<NodeId, Value> = HashMap::with_capacity(order.len());

    for node_id in order {
        let value = eval_node(expr, node_id, &values, registry, resolver)?;
        values.insert(node_id, value);
    }

    Ok(values)
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

fn compute_subtree_sizes(expr: &Expr) -> Vec<usize> {
    let mut sizes = vec![1usize; expr.nodes.len()];
    for id in postorder(expr, expr.root) {
        let children = match &expr.node(id).kind {
            ExprKind::Unary { operand, .. } => sizes[operand.0 as usize],
            ExprKind::Binary { left, right, .. } => {
                sizes[left.0 as usize] + sizes[right.0 as usize]
            }
            ExprKind::Call { args, .. } => args
                .iter()
                .map(|arg| match arg {
                    CallArg::Positional(node) | CallArg::Named { value: node, .. } => {
                        sizes[node.0 as usize]
                    }
                })
                .sum(),
            _ => 0,
        };
        sizes[id.0 as usize] = 1 + children;
    }
    sizes
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
