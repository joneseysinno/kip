//! Symbolic residual construction, folding, and simplification (M5).

use num_rational::Ratio;
use num_traits::{CheckedAdd, CheckedSub, One};

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::dim::{BaseDim, Dimension};
use crate::eval::constraint::{unify_additive_terms, validate_node};
use crate::eval::lint_sink::LintSink;
use crate::eval::mag::Mag;
use crate::eval::units::{combine_mul, dimension_of_unit, unify_add, unify_sub};
use crate::eval::value::{
    ConstraintSet, Quantity, SymBinaryOp, SymExpr, SymNode, SymUnaryOp, Symbol, Value,
};
use crate::parser::ast::{BinaryOp, UnaryOp};
use crate::quantity::UnitExpr;
use crate::registry::Registry;

/// Build a symbolic value from a free symbol.
pub fn symbol_value(name: impl Into<String>) -> Value {
    let name = name.into();
    let sym = Symbol(name.clone());
    Value::Symbolic(SymExpr {
        root: SymNode::Symbol(sym.clone()),
        text: name,
        free_symbols: vec![sym],
        constraints: ConstraintSet::new(),
    })
}

/// Build a unary symbolic residual (`sqrt`, etc.).
pub fn symbolic_unary(op: SymUnaryOp, expr: &SymExpr) -> Value {
    let root = simplify(SymNode::Unary {
        op,
        operand: Box::new(expr.root.clone()),
    });
    finish_symbolic(root, expr.constraints.clone())
}

/// Partial addition/subtraction.
pub fn add_like(
    lhs: &Value,
    rhs: &Value,
    registry: &Registry,
    span: Span,
    add: bool,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    match (lhs, rhs) {
        (Value::Known(l), Value::Known(r)) => {
            let q = if add {
                unify_add(l, r, registry, span, lints)?
            } else {
                unify_sub(l, r, registry, span, lints)?
            };
            Ok(Value::Known(q))
        }
        (Value::Known(k), Value::Symbolic(s)) => {
            let mut constraints = s.constraints.clone();
            let left = SymNode::Known(k.clone());
            let right = s.root.clone();
            let op = if add { SymBinaryOp::Add } else { SymBinaryOp::Sub };
            unify_additive_terms(&left, &right, &mut constraints, span)?;
            let root = simplify(SymNode::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            });
            Ok(finish_symbolic(root, constraints))
        }
        (Value::Symbolic(s), Value::Known(k)) if add => {
            let mut constraints = s.constraints.clone();
            let left = s.root.clone();
            let right = SymNode::Known(k.clone());
            unify_additive_terms(&left, &right, &mut constraints, span)?;
            let root = simplify(SymNode::Binary {
                op: SymBinaryOp::Add,
                left: Box::new(left),
                right: Box::new(right),
            });
            Ok(finish_symbolic(root, constraints))
        }
        (Value::Symbolic(s), Value::Known(k)) => {
            let mut constraints = s.constraints.clone();
            let left = s.root.clone();
            let right = SymNode::Known(k.clone());
            unify_additive_terms(&left, &right, &mut constraints, span)?;
            let root = simplify(SymNode::Binary {
                op: SymBinaryOp::Sub,
                left: Box::new(left),
                right: Box::new(right),
            });
            Ok(finish_symbolic(root, constraints))
        }
        (Value::Symbolic(l), Value::Symbolic(r)) => {
            let constraints = merge_constraints(&l.constraints, &r.constraints, span)?;
            let op = if add { SymBinaryOp::Add } else { SymBinaryOp::Sub };
            let root = simplify(SymNode::Binary {
                op,
                left: Box::new(l.root.clone()),
                right: Box::new(r.root.clone()),
            });
            Ok(finish_symbolic(root, constraints))
        }
    }
}

/// Partial multiplication/division.
pub fn mul_div(
    lhs: &Value,
    rhs: &Value,
    _registry: &Registry,
    span: Span,
    mul: bool,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    match (lhs, rhs) {
        (Value::Known(l), Value::Known(r)) => {
            let q = if mul {
                combine_mul(l, r, span, lints)?
            } else {
                crate::eval::units::combine_div(l, r, span, lints)?
            };
            Ok(Value::Known(q))
        }
        (Value::Known(k), Value::Symbolic(s)) | (Value::Symbolic(s), Value::Known(k)) => {
            let (known, sym, flip) = if matches!(lhs, Value::Known(_)) {
                (k, s, false)
            } else {
                (k, s, true)
            };
            let folded = fold_known_coefficient(known, &sym.root, flip, mul, lints)?;
            Ok(finish_symbolic(folded, sym.constraints.clone()))
        }
        (Value::Symbolic(l), Value::Symbolic(r)) => {
            let constraints = merge_constraints(&l.constraints, &r.constraints, span)?;
            let op = if mul { SymBinaryOp::Mul } else { SymBinaryOp::Div };
            let root = simplify(SymNode::Binary {
                op,
                left: Box::new(l.root.clone()),
                right: Box::new(r.root.clone()),
            });
            Ok(finish_symbolic(root, constraints))
        }
    }
}

/// Partial exponentiation (known exponent only).
pub fn pow(lhs: &Value, rhs: &Value, span: Span, lints: &mut LintSink) -> Result<Value, Diag> {
    match (lhs, rhs) {
        (Value::Known(l), Value::Known(r)) => {
            Ok(Value::Known(crate::eval::units::combine_pow(l, r, span, lints)?))
        }
        (Value::Symbolic(s), Value::Known(r)) if r.dim.is_dimensionless() => {
            if let Some(ratio) = r.exact_ratio() {
                if ratio.denom() != &1 {
                    return Err(Diag::new(Diagnostic::error(
                        ErrorCode::Eval,
                        "symbolic exponent must be an integer",
                        span,
                    )));
                }
            } else {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    "symbolic exponent must be an integer",
                    span,
                )));
            }
            let root = simplify(SymNode::Binary {
                op: SymBinaryOp::Pow,
                left: Box::new(s.root.clone()),
                right: Box::new(SymNode::Known(r.clone())),
            });
            Ok(finish_symbolic(root, s.constraints.clone()))
        }
        _ => Err(Diag::new(Diagnostic::error(
            ErrorCode::Eval,
            "cannot raise a symbolic residual to a symbolic exponent",
            span,
        ))),
    }
}

/// Unary negation on values.
pub fn neg(value: &Value, _span: Span, _lints: &mut LintSink) -> Result<Value, Diag> {
    match value {
        Value::Known(q) => Ok(Value::Known(Quantity::new(
            q.mag.neg(),
            q.unit.clone(),
            q.dim.clone(),
        ))),
        Value::Symbolic(s) => {
            let root = simplify(SymNode::Unary {
                op: SymUnaryOp::Neg,
                operand: Box::new(s.root.clone()),
            });
            Ok(finish_symbolic(root, s.constraints.clone()))
        }
    }
}

/// Finalize evaluation: validate constraints and attach to symbolic result.
pub fn finalize(value: Value, span: Span) -> Result<Value, Diag> {
    match value {
        Value::Known(_) => Ok(value),
        Value::Symbolic(mut s) => {
            let mut constraints = s.constraints.clone();
            validate_node(&s.root, &mut constraints, span)?;
            s.constraints = constraints;
            s.text = format_node(&s.root);
            s.free_symbols = collect_symbols(&s.root);
            Ok(Value::Symbolic(s))
        }
    }
}

/// Substitute resolver bindings into a symbolic tree and simplify.
pub fn bind_symbolic(expr: &SymExpr, resolver: &dyn crate::Resolver) -> Result<Value, Diag> {
    let root = substitute(&expr.root, resolver)?;
    let simplified = simplify(root);
    if let SymNode::Known(q) = simplified {
        return Ok(Value::Known(q));
    }
    Ok(finish_symbolic(simplified, expr.constraints.clone()))
}

/// Map parser unary ops to symbolic unary ops.
#[allow(dead_code)]
pub fn map_unary(op: UnaryOp) -> SymUnaryOp {
    match op {
        UnaryOp::Neg => SymUnaryOp::Neg,
    }
}

/// Map parser binary ops to symbolic binary ops.
#[allow(dead_code)]
pub fn map_binary(op: BinaryOp) -> Option<SymBinaryOp> {
    match op {
        BinaryOp::Add => Some(SymBinaryOp::Add),
        BinaryOp::Sub => Some(SymBinaryOp::Sub),
        BinaryOp::Mul => Some(SymBinaryOp::Mul),
        BinaryOp::Div => Some(SymBinaryOp::Div),
        BinaryOp::Pow => Some(SymBinaryOp::Pow),
        BinaryOp::Cmp(_) => None,
    }
}

fn finish_symbolic(root: SymNode, constraints: ConstraintSet) -> Value {
    let free_symbols = collect_symbols(&root);
    let text = format_node(&root);
    Value::Symbolic(SymExpr {
        root,
        text,
        free_symbols,
        constraints,
    })
}

fn fold_known_coefficient(
    known: &Quantity,
    sym_root: &SymNode,
    flip: bool,
    mul: bool,
    lints: &mut LintSink,
) -> Result<SymNode, Diag> {
    if mul && !known.dim.is_dimensionless() {
        let left = SymNode::Known(known.clone());
        let right = sym_root.clone();
        return Ok(if flip {
            SymNode::Binary {
                op: SymBinaryOp::Mul,
                left: Box::new(right),
                right: Box::new(left),
            }
        } else {
            SymNode::Binary {
                op: SymBinaryOp::Mul,
                left: Box::new(left),
                right: Box::new(right),
            }
        });
    }

    if !mul {
        return Ok(SymNode::Binary {
            op: SymBinaryOp::Div,
            left: Box::new(if flip {
                sym_root.clone()
            } else {
                SymNode::Known(known.clone())
            }),
            right: Box::new(if flip {
                SymNode::Known(known.clone())
            } else {
                sym_root.clone()
            }),
        });
    }

    match sym_root {
        SymNode::Known(q) => Ok(SymNode::Known(combine_mul(known, q, Span::empty(0), lints)?)),
        _ => Ok(SymNode::Binary {
            op: SymBinaryOp::Mul,
            left: Box::new(if flip {
                sym_root.clone()
            } else {
                SymNode::Known(known.clone())
            }),
            right: Box::new(if flip {
                SymNode::Known(known.clone())
            } else {
                sym_root.clone()
            }),
        }),
    }
}

fn substitute(node: &SymNode, resolver: &dyn crate::Resolver) -> Result<SymNode, Diag> {
    match node {
        SymNode::Known(q) => Ok(SymNode::Known(q.clone())),
        SymNode::Symbol(sym) => {
            if let Some(Value::Known(q)) = resolver.resolve(&sym.0) {
                return Ok(SymNode::Known(q));
            }
            Ok(SymNode::Symbol(sym.clone()))
        }
        SymNode::Unary { op, operand } => Ok(SymNode::Unary {
            op: *op,
            operand: Box::new(substitute(operand, resolver)?),
        }),
        SymNode::Binary { op, left, right } => Ok(SymNode::Binary {
            op: *op,
            left: Box::new(substitute(left, resolver)?),
            right: Box::new(substitute(right, resolver)?),
        }),
    }
}

/// Conservative simplifier — correctness over prettiness.
pub fn simplify(node: SymNode) -> SymNode {
    match node {
        SymNode::Unary {
            op: SymUnaryOp::Neg,
            operand,
        } => {
            let inner = simplify(*operand);
            match inner {
                SymNode::Known(q) => SymNode::Known(Quantity::new(
                    q.mag.neg(),
                    q.unit.clone(),
                    q.dim.clone(),
                )),
                SymNode::Unary {
                    op: SymUnaryOp::Neg,
                    operand,
                } => *operand,
                other => SymNode::Unary {
                    op: SymUnaryOp::Neg,
                    operand: Box::new(other),
                },
            }
        }
        SymNode::Binary { op, left, right } => {
            let left = simplify(*left);
            let right = simplify(*right);
            match op {
                SymBinaryOp::Add => simplify_add(left, right),
                SymBinaryOp::Sub => simplify_sub(left, right),
                SymBinaryOp::Mul => simplify_mul(left, right),
                SymBinaryOp::Div => simplify_div(left, right),
                SymBinaryOp::Pow => SymNode::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            }
        }
        other => other,
    }
}

fn simplify_add(left: SymNode, right: SymNode) -> SymNode {
    if is_zero(&left) {
        return right;
    }
    if is_zero(&right) {
        return left;
    }
    if let (SymNode::Known(l), SymNode::Known(r)) = (&left, &right) {
        if l.dim == r.dim && l.unit == r.unit && l.is_exact() && r.is_exact() {
            if let (Mag::Exact(lm), Mag::Exact(rm)) = (l.mag, r.mag) {
                if let Some(sum) = lm.checked_add(&rm) {
                    return SymNode::Known(Quantity::new(
                        Mag::Exact(sum),
                        l.unit.clone(),
                        l.dim.clone(),
                    ));
                }
            }
        }
    }
    SymNode::Binary {
        op: SymBinaryOp::Add,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn simplify_sub(left: SymNode, right: SymNode) -> SymNode {
    if is_zero(&right) {
        return left;
    }
    if let (SymNode::Known(l), SymNode::Known(r)) = (&left, &right) {
        if l.dim == r.dim && l.unit == r.unit && l.is_exact() && r.is_exact() {
            if let (Mag::Exact(lm), Mag::Exact(rm)) = (l.mag, r.mag) {
                if let Some(diff) = lm.checked_sub(&rm) {
                    return SymNode::Known(Quantity::new(
                        Mag::Exact(diff),
                        l.unit.clone(),
                        l.dim.clone(),
                    ));
                }
            }
        }
    }
    SymNode::Binary {
        op: SymBinaryOp::Sub,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn simplify_mul(left: SymNode, right: SymNode) -> SymNode {
    if is_one(&left) {
        return right;
    }
    if is_one(&right) {
        return left;
    }
    if let (SymNode::Known(l), SymNode::Known(r)) = (&left, &right) {
        let mut sink = LintSink::new();
        if let Ok(q) = combine_mul(l, r, Span::empty(0), &mut sink) {
            return SymNode::Known(q);
        }
    }
    SymNode::Binary {
        op: SymBinaryOp::Mul,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn simplify_div(left: SymNode, right: SymNode) -> SymNode {
    SymNode::Binary {
        op: SymBinaryOp::Div,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn is_zero(node: &SymNode) -> bool {
    matches!(
        node,
        SymNode::Known(q) if q.dim.is_dimensionless() && q.mag.is_zero()
    )
}

fn is_one(node: &SymNode) -> bool {
    matches!(
        node,
        SymNode::Known(q) if q.dim.is_dimensionless() && q.exact_ratio() == Some(Ratio::from_integer(1))
    )
}

fn merge_constraints(
    a: &ConstraintSet,
    b: &ConstraintSet,
    span: Span,
) -> Result<ConstraintSet, Diag> {
    let mut out = a.clone();
    for (sym, entry) in &b.symbol_dims {
        out.pin_at(sym.clone(), entry.dim.clone(), span)?;
    }
    Ok(out)
}

fn collect_symbols(node: &SymNode) -> Vec<Symbol> {
    let mut out = Vec::new();
    collect_symbols_rec(node, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_symbols_rec(node: &SymNode, out: &mut Vec<Symbol>) {
    match node {
        SymNode::Symbol(s) => out.push(s.clone()),
        SymNode::Unary { operand, .. } => collect_symbols_rec(operand, out),
        SymNode::Binary { left, right, .. } => {
            collect_symbols_rec(left, out);
            collect_symbols_rec(right, out);
        }
        SymNode::Known(_) => {}
    }
}

fn format_node(node: &SymNode) -> String {
    match node {
        SymNode::Known(q) => format_quantity_text(q),
        SymNode::Symbol(s) => s.0.clone(),
        SymNode::Unary { op, operand } => match op {
            SymUnaryOp::Neg => format!("-({})", format_node(operand)),
            SymUnaryOp::Sqrt => format!("sqrt({})", format_node(operand)),
        },
        SymNode::Binary { op, left, right } => {
            let op_s = match op {
                SymBinaryOp::Add => "+",
                SymBinaryOp::Sub => "-",
                SymBinaryOp::Mul => "*",
                SymBinaryOp::Div => "/",
                SymBinaryOp::Pow => "^",
            };
            format!("{} {} {}", format_node(left), op_s, format_node(right))
        }
    }
}

fn format_quantity_text(q: &Quantity) -> String {
    match q.mag {
        Mag::Float(f) => format!("{f} {}", q.unit.as_str()),
        Mag::Exact(r) if r.denom() == &1 => format!("{} {}", r.numer(), q.unit.as_str()),
        Mag::Exact(r) => format!("{}/{} {}", r.numer(), r.denom(), q.unit.as_str()),
    }
}

/// Build a known quantity from a parsed literal.
pub fn quantity_from_literal(
    magnitude: Ratio<i128>,
    unit: UnitExpr,
    registry: &Registry,
    span: Span,
) -> Result<Value, Diag> {
    let dim = dimension_of_unit(&unit, registry).map_err(|mut d| {
        d.0.span = span;
        d
    })?;
    Ok(Value::Known(Quantity::from_exact(magnitude, unit, dim)))
}

pub fn length_literal(inches: Ratio<i128>) -> Value {
    Value::Known(Quantity::from_exact(
        inches,
        UnitExpr::named("in"),
        Dimension::single(BaseDim::Length, Ratio::one()),
    ))
}

pub fn dimensionless_number(value: Ratio<i128>) -> Value {
    Value::Known(Quantity::from_exact(
        value,
        UnitExpr::one(),
        Dimension::dimensionless(),
    ))
}
