//! Dimension constraint unification over free symbols (M5).

use crate::diag::{Diag, Diagnostic, ErrorCode, Hint, Span};
use crate::dim::Dimension;
use crate::eval::value::{ConstraintSet, SymBinaryOp, SymNode, SymUnaryOp, Symbol};

/// Pin or unify a symbol's dimension, recording provenance.
pub fn pin_symbol(
    constraints: &mut ConstraintSet,
    sym: Symbol,
    dim: Dimension,
    span: Span,
) -> Result<(), Diag> {
    constraints.pin_at(sym, dim, span)
}

/// Validate dimension consistency of a symbolic residual tree.
#[allow(dead_code)]
pub fn validate_expression(root: &SymNode, span: Span) -> Result<ConstraintSet, Diag> {
    let mut constraints = ConstraintSet::new();
    validate_node(root, &mut constraints, span)?;
    Ok(constraints)
}

/// Walk a symbolic tree and validate dimension constraints.
pub fn validate_node(
    node: &SymNode,
    constraints: &mut ConstraintSet,
    span: Span,
) -> Result<(), Diag> {
    match node {
        SymNode::Known(_) | SymNode::Symbol(_) => Ok(()),
        SymNode::Unary { operand, .. } => validate_node(operand, constraints, span),
        SymNode::Binary { op, left, right } => {
            match op {
                SymBinaryOp::Add | SymBinaryOp::Sub => {
                    check_sqrt_symbol_add(left, right, span)?;
                    unify_additive_terms(left, right, constraints, span)
                }
                SymBinaryOp::Mul | SymBinaryOp::Div | SymBinaryOp::Pow => {
                    validate_node(left, constraints, span)?;
                    validate_node(right, constraints, span)?;
                    Ok(())
                }
            }
        }
    }
}

fn check_sqrt_symbol_add(left: &SymNode, right: &SymNode, span: Span) -> Result<(), Diag> {
    if let (
        SymNode::Unary {
            op: SymUnaryOp::Sqrt,
            operand,
        },
        SymNode::Symbol(sym),
    ) = (left, right)
    {
        if matches!(operand.as_ref(), SymNode::Symbol(s) if s == sym) {
            return Err(Diag::new(
                Diagnostic::error(
                    ErrorCode::DimMismatch,
                    format!(
                        "cannot add `sqrt({})` and `{}` — no dimension satisfies d^(1/2) = d",
                        sym.0, sym.0
                    ),
                    span,
                )
                .with_hints(vec![Hint::Note(
                    "sqrt halves exponents; adding the result to the original symbol is dimensionally inconsistent unless dimensionless".into(),
                )]),
            ));
        }
    }
    Ok(())
}

/// Unify two concrete dimensions at a binary site.
pub fn unify_dimensions(
    expected: &Dimension,
    found: &Dimension,
    span: Span,
    message: &str,
) -> Result<(), Diag> {
    if expected == found {
        return Ok(());
    }
    Err(Diag::new(
        Diagnostic::error(ErrorCode::DimMismatch, message, span).with_hints(vec![
            Hint::ExpectedDimension(format!("{expected:?}")),
            Hint::FoundDimension(format!("{found:?}")),
        ]),
    ))
}

/// Infer the dimension of a symbolic subtree, accumulating symbol pins.
pub fn infer_dimension(
    node: &SymNode,
    constraints: &mut ConstraintSet,
    span: Span,
) -> Result<Dimension, Diag> {
    match node {
        SymNode::Known(q) => Ok(q.dim.clone()),
        SymNode::Symbol(sym) => constraints
            .dimension_of(sym)
            .ok_or_else(|| unbound_symbol(sym, span)),
        SymNode::Unary { op, operand } => match op {
            SymUnaryOp::Neg => infer_dimension(operand, constraints, span),
            SymUnaryOp::Sqrt => {
                let inner = infer_dimension(operand, constraints, span)?;
                Ok(halve_dimension(&inner))
            }
        },
        SymNode::Binary { op, left, right } => {
            let left_dim = infer_dimension(left, constraints, span)?;
            let right_dim = infer_dimension(right, constraints, span)?;
            match op {
                SymBinaryOp::Add | SymBinaryOp::Sub => {
                    unify_dimensions(
                        &left_dim,
                        &right_dim,
                        span,
                        "dimension mismatch in addition/subtraction",
                    )?;
                    Ok(left_dim)
                }
                SymBinaryOp::Mul => Ok(left_dim.mul(&right_dim)),
                SymBinaryOp::Div => Ok(left_dim.div(&right_dim)),
                SymBinaryOp::Pow => Ok(left_dim),
            }
        }
    }
}

/// Check add/sub sites involving partially known symbolic sums.
pub fn unify_additive_terms(
    left: &SymNode,
    right: &SymNode,
    constraints: &mut ConstraintSet,
    span: Span,
) -> Result<(), Diag> {
    if let Some((sym, known_dim)) = symbol_with_known_additive_partner(left, right) {
        pin_symbol(constraints, sym, known_dim, span)?;
    } else if let Some((sym, known_dim)) = symbol_with_known_additive_partner(right, left) {
        pin_symbol(constraints, sym, known_dim, span)?;
    }

    let left_dim = infer_dimension(left, constraints, span)?;
    let right_dim = infer_dimension(right, constraints, span)?;
    unify_dimensions(
        &left_dim,
        &right_dim,
        span,
        "dimension mismatch in addition/subtraction",
    )
}

fn symbol_with_known_additive_partner(
    left: &SymNode,
    right: &SymNode,
) -> Option<(Symbol, Dimension)> {
    match (left, right) {
        (SymNode::Known(q), SymNode::Symbol(sym)) => Some((sym.clone(), q.dim.clone())),
        _ => None,
    }
}

fn halve_dimension(dim: &Dimension) -> Dimension {
    use num_rational::Ratio;
    dim.pow(Ratio::new(1, 2))
}

fn unbound_symbol(sym: &Symbol, span: Span) -> Diag {
    Diag::new(Diagnostic::error(
        ErrorCode::DimMismatch,
        format!("symbol `{}` has no dimension constraint", sym.0),
        span,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::BaseDim;
    use crate::eval::value::{SymBinaryOp, SymNode, SymUnaryOp};
    use num_rational::Ratio;
    use num_traits::One;

    fn pressure() -> Dimension {
        Dimension::single(BaseDim::Force, Ratio::one())
            .div(&Dimension::single(BaseDim::Length, Ratio::one()))
            .div(&Dimension::single(BaseDim::Length, Ratio::one()))
    }

    #[test]
    fn sqrt_symbol_plus_symbol_is_unsatisfiable_for_pressure() {
        let sym = Symbol("f'c".into());
        let root = SymNode::Binary {
            op: SymBinaryOp::Add,
            left: Box::new(SymNode::Unary {
                op: SymUnaryOp::Sqrt,
                operand: Box::new(SymNode::Symbol(sym.clone())),
            }),
            right: Box::new(SymNode::Symbol(sym.clone())),
        };
        let mut cs = ConstraintSet::new();
        cs.pin_at(sym, pressure(), Span::empty(0))
            .unwrap();
        let err = infer_dimension(&root, &mut cs, Span::empty(0)).unwrap_err();
        assert_eq!(err.diagnostic().code, "E-DIM-MISMATCH");
    }
}
