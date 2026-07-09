//! TOML equation-pack loader.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::eval::partial::{dimensionless_number, quantity_from_literal};
use crate::eval::units::dimension_of_unit;
use crate::eval::value::{EquationProvenance, Quantity};
use crate::packs::contract::{ArgContract, ArgRange, RangeSeverity};
use crate::packs::dimensionalize::dimensionalize_body;
use crate::packs::equation::{EquationRecord, EquationRegistry};
use crate::parser::ast::ExprKind;
use crate::quantity::UnitExpr;
use crate::registry::RegistryBuilder;
use crate::{parse, Registry};

/// Liberally licensed demo pack for tests and documentation.
pub const DEMO_PACK_TOML: &str = include_str!("../../testdata/demo_pack.toml");

#[derive(Debug, Deserialize)]
struct PackDocument {
    pack: PackInfo,
    #[serde(default)]
    equation: Vec<EquationEntry>,
    defs: Option<DefsSection>,
}

#[derive(Debug, Deserialize)]
struct PackInfo {
    id: String,
    title: String,
    edition: String,
    #[serde(default)]
    license: String,
}

#[derive(Debug, Deserialize)]
struct DefsSection {
    #[serde(default)]
    lines: Vec<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EquationEntry {
    id: String,
    namespace: String,
    cite: String,
    result: String,
    body: String,
    #[serde(default)]
    arg: Vec<ArgEntry>,
}

#[derive(Debug, Deserialize)]
struct ArgEntry {
    name: String,
    unit: String,
    range: Option<RangeToml>,
    default: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RangeToml {
    min: Option<String>,
    max: Option<String>,
    #[serde(default)]
    severity: String,
}

/// Parse TOML packs into a standalone equation registry (units must already exist).
pub fn load_packs(src: &str, registry: &Registry) -> Result<EquationRegistry, Diag> {
    let mut builder = RegistryBuilder::from_registry(registry);
    load_packs_into(&mut builder, src)?;
    Ok(EquationRegistry::from_map(std::mem::take(&mut builder.equations)))
}

/// Load equation packs into a registry builder (also ingests optional `[defs]`).
pub fn load_packs_into(builder: &mut RegistryBuilder, src: &str) -> Result<(), Diag> {
    let doc: PackDocument = toml::from_str(src).map_err(|e| {
        Diag::new(Diagnostic::error(
            ErrorCode::PackParse,
            format!("TOML pack parse error: {e}"),
            Span::empty(0),
        ))
    })?;

    if let Some(defs) = doc.defs {
        let text = if let Some(body) = defs.body {
            body
        } else {
            defs.lines.join("\n")
        };
        if !text.trim().is_empty() {
            builder.parse_defs(&text)?;
        }
    }

    let frozen = builder.clone().freeze();
    let reg = frozen.as_ref();

    for eq in doc.equation {
        let path_key = format!("{}.{}", eq.namespace, eq.id);
        let result_unit = parse_unit_expr(&eq.result, reg)?;
        let result_dim = dimension_of_unit(&result_unit, reg)?;

        let mut args = BTreeMap::new();
        for arg in eq.arg {
            let unit = parse_unit_expr(&arg.unit, reg)?;
            let dim = dimension_of_unit(&unit, reg)?;
            let range = arg
                .range
                .map(|r| parse_range(&r, &unit, dim.clone(), reg))
                .transpose()?;
            let default_expr = if let Some(def) = arg.default {
                if !dim.is_dimensionless() {
                    return Err(Diag::new(Diagnostic::error(
                        ErrorCode::PackParse,
                        format!(
                            "default value for dimensional argument `{}` is not allowed",
                            arg.name
                        ),
                        Span::empty(0),
                    )));
                }
                Some(parse(&def, reg).map_err(pack_body_err)?)
            } else {
                None
            };
            let contract = ArgContract {
                name: arg.name.clone(),
                unit,
                dim,
                range,
                default_expr,
            };
            if args.insert(arg.name, contract).is_some() {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::PackParse,
                    format!("duplicate argument in equation `{path_key}`"),
                    Span::empty(0),
                )));
            }
        }

        let raw_body = parse(&eq.body, reg).map_err(pack_body_err)?;
        let body = dimensionalize_body(&eq.body, raw_body.as_ref(), &args, &result_dim, reg)?;

        let record = Arc::new(EquationRecord {
            path_key: path_key.clone(),
            namespace: eq.namespace.clone(),
            id: eq.id.clone(),
            body,
            result_unit,
            result_dim,
            args,
            provenance: EquationProvenance {
                pack_id: doc.pack.id.clone(),
                title: doc.pack.title.clone(),
                edition: doc.pack.edition.clone(),
                license: doc.pack.license.clone(),
                namespace: eq.namespace,
                equation_id: eq.id,
                cite: eq.cite,
            },
        });
        EquationRegistry::insert(&mut builder.equations, record)?;
    }

    Ok(())
}

fn pack_body_err(diags: Vec<Diag>) -> Diag {
    diags.into_iter().next().unwrap_or_else(|| {
        Diag::new(Diagnostic::error(
            ErrorCode::PackBody,
            "pack equation body failed to parse",
            Span::empty(0),
        ))
    })
}

fn parse_unit_expr(unit_str: &str, registry: &Registry) -> Result<UnitExpr, Diag> {
    if unit_str == "1" {
        return Ok(UnitExpr::one());
    }
    let expr = parse(&format!("1 {unit_str}"), registry).map_err(pack_body_err)?;
    match &expr.root_node().kind {
        ExprKind::Quantity { unit, .. } => Ok(unit.clone()),
        _ => Err(Diag::new(Diagnostic::error(
            ErrorCode::PackParse,
            format!("invalid unit expression `{unit_str}`"),
            Span::empty(0),
        ))),
    }
}

fn parse_range(
    range: &RangeToml,
    contract_unit: &UnitExpr,
    contract_dim: crate::dim::Dimension,
    registry: &Registry,
) -> Result<ArgRange, Diag> {
    let severity = match range.severity.as_str() {
        "error" => RangeSeverity::Error,
        "lint" | "" => RangeSeverity::Lint,
        other => {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::PackParse,
                format!("unknown range severity `{other}` (use `lint` or `error`)"),
                Span::empty(0),
            )));
        }
    };
    let min = range
        .min
        .as_deref()
        .map(|s| parse_contract_quantity(s, contract_unit, contract_dim.clone(), registry))
        .transpose()?;
    let max = range
        .max
        .as_deref()
        .map(|s| parse_contract_quantity(s, contract_unit, contract_dim, registry))
        .transpose()?;
    Ok(ArgRange {
        min,
        max,
        severity,
    })
}

fn parse_contract_quantity(
    src: &str,
    contract_unit: &UnitExpr,
    contract_dim: crate::dim::Dimension,
    registry: &Registry,
) -> Result<Quantity, Diag> {
    let expr = parse(src, registry).map_err(pack_body_err)?;
    let span = expr.root_node().span;
    let q = match &expr.root_node().kind {
        ExprKind::Quantity {
            magnitude, unit, ..
        } => match quantity_from_literal(*magnitude, unit.clone(), registry, span)? {
            crate::eval::value::Value::Known(q) => q,
            _ => unreachable!(),
        },
        ExprKind::Number { value, .. } => match dimensionless_number(*value) {
            crate::eval::value::Value::Known(q) => q,
            _ => {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::PackParse,
                    "range bound must be a quantity literal",
                    span,
                )));
            }
        },
        _ => {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::PackParse,
                "range bound must be a quantity literal",
                span,
            )));
        }
    };
    if q.dim != contract_dim {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::PackParse,
            "range bound dimension does not match argument contract unit",
            span,
        )));
    }
    // Normalize to contract display unit for comparisons.
    q.convert_to(contract_unit, registry)
}
