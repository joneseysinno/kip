//! Evaluate a code-equation call (`ACI.fr(fc: …, lambda: …)`).

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::diag::{Diag, Diagnostic, ErrorCode, Hint, LintCode, Span};
use crate::eval::known::eval_known_checked;
use crate::eval::lint_sink::LintSink;
use crate::eval::partial::finalize;
use crate::eval::units::{convert_quantity, mag_cmp};
use crate::eval::value::{ConstraintSet, Quantity, Value};
use crate::packs::contract::RangeSeverity;
use crate::packs::equation::EquationRecord;
use crate::parser::ast::CallArg;
use crate::parser::NodeId;
use crate::registry::Registry;
use crate::resolver::Resolver;

/// Evaluate a code-equation call (`ACI.fr(fc: …, lambda: …)`).
pub fn eval_equation_call(
    path: &[String],
    args: &[CallArg],
    values: &HashMap<NodeId, Value>,
    registry: &Registry,
    resolver: &dyn Resolver,
    span: Span,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    let eq = registry
        .equations()
        .lookup(path)
        .ok_or_else(|| {
            Diag::new(Diagnostic::error(
                ErrorCode::UnknownEq,
                format!("unknown equation `{}`", path.join(".")),
                span,
            ))
        })?;

    if args.iter().any(|a| matches!(a, CallArg::Positional(_))) {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::CodePositional,
            "code equations require named arguments",
            span,
        )));
    }

    for arg in args {
        if let CallArg::Named { name, .. } = arg {
            if !eq.args.contains_key(name) {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Eval,
                    format!("unknown argument `{name}` for equation `{}`", eq.path_key),
                    span,
                )));
            }
        }
    }

    let mut bindings = BTreeMap::new();
    let mut constraints = ConstraintSet::new();

    for (name, contract) in &eq.args {
        let provided = args.iter().find_map(|a| match a {
            CallArg::Named { name: n, value } if n == name => Some(*value),
            _ => None,
        });

        let arg_value = if let Some(id) = provided {
            values.get(&id).cloned().ok_or(missing_child(span))?
        } else if let Some(default) = &contract.default_expr {
            eval_known_checked(default.as_ref(), registry, resolver, lints)?
        } else {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::Eval,
                format!("missing required argument `{name}`"),
                span,
            )));
        };

        let prepared = prepare_arg(
            &arg_value,
            contract,
            eq,
            registry,
            span,
            &mut constraints,
            lints,
        )?;
        bindings.insert(name.clone(), prepared);
    }

    let result = eval_with_bindings(eq, &bindings, registry, resolver, span, lints)?;

    match result {
        Value::Known(mut q) => {
            if q.dim != eq.result_dim {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::ContractDim,
                    format!(
                        "equation `{}` body produced {:?}, expected {:?}",
                        eq.path_key, q.dim, eq.result_dim
                    ),
                    span,
                )));
            }
            if q.unit != eq.result_unit {
                q = convert_quantity(&q, &eq.result_unit, registry)?;
            }
            q.provenance = Some(Arc::new(eq.provenance.clone()));
            Ok(Value::Known(q))
        }
        Value::Symbolic(mut s) => {
            s.constraints = merge_constraints(s.constraints, constraints);
            finalize(Value::Symbolic(s), span)
        }
    }
}

fn prepare_arg(
    value: &Value,
    contract: &crate::packs::contract::ArgContract,
    eq: &EquationRecord,
    registry: &Registry,
    span: Span,
    constraints: &mut ConstraintSet,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    match value {
        Value::Known(q) => {
            if q.dim != contract.dim {
                return Err(Diag::new(
                    Diagnostic::error(
                        ErrorCode::ContractDim,
                        format!(
                            "argument `{}` dimension {:?} does not match contract {:?}",
                            contract.name, q.dim, contract.dim
                        ),
                        span,
                    )
                    .with_hints(vec![
                        Hint::ExpectedDimension(format!("{:?}", contract.dim)),
                        Hint::FoundDimension(format!("{:?}", q.dim)),
                    ]),
                ));
            }
            check_range(q, contract, eq, registry, span, lints)?;
            let converted = convert_quantity(q, &contract.unit, registry)?;
            Ok(Value::Known(converted))
        }
        Value::Symbolic(s) => {
            for sym in &s.free_symbols {
                constraints.pin_at(sym.clone(), contract.dim.clone(), span)?;
            }
            Ok(value.clone())
        }
    }
}

fn check_range(
    q: &Quantity,
    contract: &crate::packs::contract::ArgContract,
    eq: &EquationRecord,
    registry: &Registry,
    span: Span,
    lints: &mut LintSink,
) -> Result<(), Diag> {
    let Some(range) = &contract.range else {
        return Ok(());
    };
    let in_contract = convert_quantity(q, &contract.unit, registry)?;
    let mag = in_contract.mag;
    let below = range
        .min
        .as_ref()
        .is_some_and(|m| mag_cmp(mag, m.mag) == Some(Ordering::Less));
    let above = range
        .max
        .as_ref()
        .is_some_and(|m| mag_cmp(mag, m.mag) == Some(Ordering::Greater));
    if below || above {
        let msg = format!(
            "argument `{}` is outside the valid range for `{}` ({})",
            contract.name, eq.path_key, eq.provenance.cite
        );
        if range.severity == RangeSeverity::Error {
            return Err(Diag::new(Diagnostic::error(ErrorCode::Range, msg, span)));
        }
        lints.push(Diag::new(Diagnostic::lint(LintCode::Range, msg, span)));
    }
    Ok(())
}

fn eval_with_bindings(
    eq: &EquationRecord,
    bindings: &BTreeMap<String, Value>,
    registry: &Registry,
    outer: &dyn Resolver,
    span: Span,
    lints: &mut LintSink,
) -> Result<Value, Diag> {
    let resolver = BindingResolver {
        bindings,
        outer,
    };
    eval_known_checked(eq.body.as_ref(), registry, &resolver, lints).map_err(|e| {
        if e.diagnostic().code == ErrorCode::Eval.as_str() {
            Diag::new(Diagnostic::error(
                ErrorCode::PackBody,
                format!("equation `{}` body failed during evaluation", eq.path_key),
                span,
            ))
        } else {
            e
        }
    })
}

struct BindingResolver<'a> {
    bindings: &'a BTreeMap<String, Value>,
    outer: &'a dyn Resolver,
}

impl Resolver for BindingResolver<'_> {
    fn resolve(&self, name: &str) -> Option<Value> {
        self.bindings
            .get(name)
            .cloned()
            .or_else(|| self.outer.resolve(name))
    }
}

fn merge_constraints(mut base: ConstraintSet, extra: ConstraintSet) -> ConstraintSet {
    for (sym, c) in extra.symbol_dims {
        if let Some(existing) = base.symbol_dims.get(&sym) {
            if existing.dim == c.dim {
                let mut sites = existing.sites.clone();
                sites.extend(c.sites);
                base.symbol_dims.insert(
                    sym,
                    crate::eval::value::SymbolConstraint { dim: c.dim, sites },
                );
            } else {
                base.symbol_dims.insert(sym, c);
            }
        } else {
            base.symbol_dims.insert(sym, c);
        }
    }
    base
}

fn missing_child(span: Span) -> Diag {
    Diag::new(Diagnostic::error(
        ErrorCode::Eval,
        "internal evaluation error: missing argument value",
        span,
    ))
}
