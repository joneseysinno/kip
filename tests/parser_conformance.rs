//! Parser conformance tests (grammar-spec §8 — pure syntax).

use kip::{
    parse, parse_checked, BinaryOp, Callee, Dimension, ErrorCode, ExprKind, LintCode, Mag, MapResolver,
    Quantity, RegistryBuilder, UnitExpr, Value,
};
use num_rational::Ratio;
use num_traits::One;

fn reg() -> std::sync::Arc<kip::Registry> {
    RegistryBuilder::from_seed().freeze()
}

fn parse_ok(src: &str) -> std::sync::Arc<kip::Expr> {
    let registry = reg();
    parse(src, &registry).unwrap_or_else(|e| panic!("parse `{src}` failed: {e:?}"))
}

fn parse_err(src: &str) -> Vec<kip::Diag> {
    parse(src, &reg()).unwrap_err()
}

#[test]
fn quantity_tight_unit_product() {
    let expr = parse_ok("2 kip*ft");
    match &expr.root_node().kind {
        ExprKind::Quantity { magnitude, unit, .. } => {
            assert_eq!(*magnitude, Ratio::from_integer(2));
            assert!(matches!(unit, UnitExpr::Product(parts) if parts.len() == 2));
        }
        other => panic!("expected quantity, got {other:?}"),
    }
}

#[test]
fn spaced_star_splits_unit_from_symbol() {
    let expr = parse_ok("2 kip * L");
    match &expr.root_node().kind {
        ExprKind::Binary {
            op: BinaryOp::Mul,
            left,
            right,
        } => {
            assert!(matches!(
                expr.node(*left).kind,
                ExprKind::Quantity { .. }
            ));
            assert!(matches!(
                &expr.node(*right).kind,
                ExprKind::Ident { name } if name == "L"
            ));
        }
        other => panic!("expected binary mul, got {other:?}"),
    }
}

#[test]
fn unknown_unit_in_attachment() {
    let errs = parse_err("2 kip*L");
    assert!(errs.iter().any(|d| d.diagnostic().code == ErrorCode::UnknownUnit.as_str()));
}

#[test]
fn one_quantity_with_div_and_pow() {
    let expr = parse_ok("120 lbf/ft^2");
    match &expr.root_node().kind {
        ExprKind::Quantity { unit, .. } => match unit {
            UnitExpr::Quotient(num, den) => {
                assert!(matches!(num.as_ref(), UnitExpr::Named(s) if s == "lbf"));
                assert!(matches!(
                    den.as_ref(),
                    UnitExpr::Pow { base, .. } if matches!(base.as_ref(), UnitExpr::Named(s) if s == "ft")
                ));
            }
            other => panic!("expected quotient unit, got {other:?}"),
        },
        other => panic!("expected quantity, got {other:?}"),
    }
}

#[test]
fn decimal_unit_exponent_literal() {
    let expr = parse_ok("4000 psi^0.5");
    match &expr.root_node().kind {
        ExprKind::Quantity { unit, .. } => {
            assert!(matches!(unit, UnitExpr::Pow { .. }));
        }
        other => panic!("expected quantity, got {other:?}"),
    }
}

#[test]
fn spaced_caret_is_expression_power() {
    let outcome = parse_checked("9 in ^2", &reg(), &kip::EmptyResolver);
    assert!(outcome
        .lints
        .iter()
        .any(|d| d.diagnostic().code == LintCode::SpacedCaret.as_str()));
    let expr = outcome.expr.expect("expr");
    match &expr.root_node().kind {
        ExprKind::Binary {
            op: BinaryOp::Pow,
            left,
            ..
        } => assert!(matches!(
            expr.node(*left).kind,
            ExprKind::Quantity { .. }
        )),
        other => panic!("expected pow, got {other:?}"),
    }
}

#[test]
fn tight_caret_in_unit() {
    let expr = parse_ok("9 in^2");
    match &expr.root_node().kind {
        ExprKind::Quantity { unit, .. } => {
            assert!(matches!(unit, UnitExpr::Pow { .. }));
        }
        other => panic!("expected quantity, got {other:?}"),
    }
}

#[test]
fn eq_in_expr_rejected() {
    let errs = parse_err("M = 12 kip*ft");
    assert!(errs.iter().any(|d| d.diagnostic().code == ErrorCode::EqInExpr.as_str()));
}

#[test]
fn code_equation_path_call() {
    let expr = parse_ok("ACI.fr(fc: f'c, lambda: 1.0)");
    match &expr.root_node().kind {
        ExprKind::Call { callee, args } => {
            assert!(matches!(callee, Callee::Path(p) if p == &["ACI", "fr"]));
            assert_eq!(args.len(), 2);
        }
        other => panic!("expected call, got {other:?}"),
    }
}

#[test]
fn function_call_with_quantity_arg() {
    let expr = parse_ok("sqrt(4000 psi)");
    match &expr.root_node().kind {
        ExprKind::Call { callee, args } => {
            assert!(matches!(callee, Callee::Ident(s) if s == "sqrt"));
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected call, got {other:?}"),
    }
}

#[test]
fn unary_minus_binds_looser_than_pow() {
    let expr = parse_ok("-2^2");
    match &expr.root_node().kind {
        ExprKind::Unary { operand, .. } => match &expr.node(*operand).kind {
            ExprKind::Binary {
                op: BinaryOp::Pow,
                left,
                right,
            } => {
                assert!(matches!(expr.node(*left).kind, ExprKind::Number { .. }));
                assert!(matches!(expr.node(*right).kind, ExprKind::Number { .. }));
            }
            other => panic!("expected pow inside neg, got {other:?}"),
        },
        other => panic!("expected unary neg, got {other:?}"),
    }
}

#[test]
fn right_assoc_pow_chain() {
    let expr = parse_ok("2^3^2");
    match &expr.root_node().kind {
        ExprKind::Binary {
            op: BinaryOp::Pow,
            left,
            right,
        } => {
            assert!(matches!(expr.node(*left).kind, ExprKind::Number { .. }));
            assert!(matches!(
                expr.node(*right).kind,
                ExprKind::Binary { op: BinaryOp::Pow, .. }
            ));
        }
        other => panic!("expected pow, got {other:?}"),
    }
}

#[test]
fn ten_thousand_term_sum_stack_safe() {
    let mut src = String::from("1");
    for i in 2..=10_000 {
        src.push_str(" + ");
        src.push_str(&i.to_string());
    }
    let expr = parse_ok(&src);
    match &expr.root_node().kind {
        ExprKind::Binary { op: BinaryOp::Add, .. } => {}
        other => panic!("expected add chain, got {other:?}"),
    }
}

#[test]
fn unit_shadow_lint_when_resolver_knows_symbol() {
    let registry = reg();
    let mut resolver = MapResolver::new();
    // `s` is seconds in seed; bind a dummy value to trigger shadow lint.
    resolver.insert(
        "s",
        Value::Known(Quantity::new(
            Mag::exact(Ratio::one()),
            UnitExpr::one(),
            Dimension::dimensionless(),
        )),
    );
    let outcome = parse_checked("5 s", &registry, &resolver);
    assert!(outcome.expr.is_some());
    assert!(outcome
        .lints
        .iter()
        .any(|d| d.diagnostic().code == LintCode::UnitShadow.as_str()));
}

#[test]
fn juxtaposed_idents_are_parse_error() {
    let errs = parse_err("f' c");
    assert!(!errs.is_empty());
}

#[test]
fn every_node_has_nonempty_span() {
    let expr = parse_ok("2 kip + 3*ft");
    for node in &expr.nodes {
        assert!(node.span.end >= node.span.start);
    }
}
