//! `define` / `dimension` / `anchor` text parser (grammar §6).

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::dim::{BaseDim, Dimension};
use num_traits::One;
use crate::registry::anchor::DimAnchor;
use crate::registry::eval_expr::eval_def_expr;
use crate::registry::resolve::{topo_sort, PendingDefine};
use crate::registry::{RegistryBuilder, UnitLookup};

/// Parsed definition statement.
#[derive(Debug, Clone)]
pub enum DefStmt {
    /// `dimension Currency`
    Dimension {
        /// Dimension name.
        name: String,
        /// Span.
        span: Span,
    },
    /// `define a, b = expr`
    DefineLinear {
        /// Names (primary + aliases).
        names: Vec<String>,
        /// RHS expression text.
        expr: String,
        /// Span.
        span: Span,
    },
    /// `define USD, $ : Currency`
    DefinePrimary {
        /// Names.
        names: Vec<String>,
        /// Dimension name.
        dim_name: String,
        /// Span.
        span: Span,
    },
    /// `anchor Length = ft`
    Anchor {
        /// Dimension name.
        dim_name: String,
        /// Anchor unit name.
        unit_name: String,
        /// Span.
        span: Span,
    },
}

/// Parse definition lines and apply to the builder.
pub fn parse_defs(builder: &mut RegistryBuilder, src: &str) -> Result<(), Diag> {
    let mut stmts = Vec::new();
    let mut byte_offset = 0usize;

    for line in src.lines() {
        let line_start = byte_offset;
        let trimmed = line.split('#').next().unwrap_or(line).trim();
        byte_offset += line.len() + 1;

        if trimmed.is_empty() {
            continue;
        }

        let span = Span::new(line_start, line_start + trimmed.len());
        stmts.push(parse_line(trimmed, span)?);
    }

  apply_stmts(builder, stmts)
}

fn parse_line(line: &str, span: Span) -> Result<DefStmt, Diag> {
    if let Some(rest) = line.strip_prefix("dimension ") {
        let name = rest.trim();
        if name.is_empty() || !is_ident(name) {
            return Err(parse_err("expected dimension name", span));
        }
        return Ok(DefStmt::Dimension {
            name: name.into(),
            span,
        });
    }

    if let Some(rest) = line.strip_prefix("anchor ") {
        let (dim, unit) = rest
            .split_once('=')
            .ok_or_else(|| parse_err("expected `anchor Dim = unit`", span))?;
        let dim_name = dim.trim();
        let unit_name = unit.trim();
        if dim_name.is_empty() || unit_name.is_empty() {
            return Err(parse_err("invalid anchor statement", span));
        }
        return Ok(DefStmt::Anchor {
            dim_name: dim_name.into(),
            unit_name: unit_name.into(),
            span,
        });
    }

    if let Some(rest) = line.strip_prefix("define ") {
        if let Some((names_part, expr)) = rest.split_once('=') {
            let names = parse_names(names_part, span)?;
            return Ok(DefStmt::DefineLinear {
                names,
                expr: expr.trim().into(),
                span,
            });
        }
        if let Some((names_part, dim)) = rest.split_once(':') {
            let names = parse_names(names_part, span)?;
            let dim_name = dim.trim();
            if dim_name.is_empty() {
                return Err(parse_err("expected dimension name after `:`", span));
            }
            return Ok(DefStmt::DefinePrimary {
                names,
                dim_name: dim_name.into(),
                span,
            });
        }
        return Err(parse_err("expected `=` or `:` in define statement", span));
    }

    Err(parse_err(
        "expected `define`, `dimension`, or `anchor`",
        span,
    ))
}

fn parse_names(s: &str, span: Span) -> Result<Vec<String>, Diag> {
    let names: Vec<String> = s
        .split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect();
    if names.is_empty() || names.iter().any(|n| !is_ident(n)) {
        return Err(parse_err("expected comma-separated unit names", span));
    }
    Ok(names)
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(unicode_ident::is_xid_start(first) || matches!(first, '°' | '$' | '%' | 'Ω' | 'μ')) {
        return false;
    }
    chars.all(|c| {
        unicode_ident::is_xid_continue(c) || matches!(c, '°' | '$' | '%' | 'Ω' | 'μ' | '\'')
    })
}

fn apply_stmts(builder: &mut RegistryBuilder, stmts: Vec<DefStmt>) -> Result<(), Diag> {
    let mut dimensions = Vec::new();
    let mut primaries = Vec::new();
    let mut linears = Vec::new();
    let mut anchors = Vec::new();

    for stmt in stmts {
        match stmt {
            DefStmt::Dimension { name, span } => dimensions.push((name, span)),
            DefStmt::DefinePrimary { names, dim_name, span } => {
                primaries.push((names, dim_name, span))
            }
            DefStmt::DefineLinear { names, expr, span } => {
                linears.push((names, expr, span))
            }
            DefStmt::Anchor {
                dim_name,
                unit_name,
                span,
            } => anchors.push((dim_name, unit_name, span)),
        }
    }

    check_duplicate_names(builder, &dimensions, &primaries, &linears)?;

    for (name, span) in dimensions {
        builder
            .declare_dimension(&name)
            .map_err(|e| attach_span(e, span))?;
    }

    for (names, dim_name, span) in primaries {
        builder
            .apply_primary_define(&names, &dim_name, span)
            .map_err(|e| attach_span(e, span))?;
    }

    let pending: Vec<PendingDefine> = linears
        .iter()
        .map(|(names, expr, span)| PendingDefine {
            primary: names[0].clone(),
            aliases: names[1..].to_vec(),
            expr: expr.clone(),
            span: *span,
        })
        .collect();

    let order = match topo_sort(&pending) {
        Ok(o) => o,
        Err(diags) => return Err(diags.into_iter().next().unwrap()),
    };

    for idx in order {
        let (names, expr, span) = &linears[idx];
        builder
            .apply_linear_define(names, expr, *span)
            .map_err(|e| attach_span(e, *span))?;
    }

    for (dim_name, unit_name, span) in anchors {
        builder
            .apply_anchor_stmt(&dim_name, &unit_name, span)
            .map_err(|e| attach_span(e, span))?;
    }

    Ok(())
}

fn check_duplicate_names(
    builder: &RegistryBuilder,
    dimensions: &[(String, Span)],
    primaries: &[(Vec<String>, String, Span)],
    linears: &[(Vec<String>, String, Span)],
) -> Result<(), Diag> {
    let mut seen: Vec<(String, Span)> = Vec::new();

    for (name, span) in dimensions {
        if builder.custom_dims.contains_key(name) {
            return Err(dup_unit(name, *span, None));
        }
        if let Some((_, prev)) = seen.iter().find(|(n, _)| n == name) {
            return Err(dup_unit(name, *span, Some(*prev)));
        }
        seen.push((name.clone(), *span));
    }

    for (names, _, span) in primaries.iter().chain(linears.iter()) {
        for name in names {
            if builder.units.contains_key(name) {
                return Err(dup_unit(
                    name,
                    *span,
                    builder.unit_spans.get(name).copied(),
                ));
            }
            if let Some((_, prev)) = seen.iter().find(|(n, _)| n == name) {
                return Err(dup_unit(name, *span, Some(*prev)));
            }
            seen.push((name.clone(), *span));
        }
    }
    Ok(())
}

fn dup_unit(name: &str, span: Span, other: Option<Span>) -> Diag {
    let mut diag = Diagnostic::error(
        ErrorCode::DupUnit,
        format!("duplicate unit or dimension `{name}`"),
        span,
    );
    if let Some(other) = other {
        diag = diag.with_hints(vec![crate::diag::Hint::RelatedSpan(other)]);
    }
    Diag::new(diag)
}

fn parse_err(msg: impl Into<String>, span: Span) -> Diag {
    Diag::new(Diagnostic::error(ErrorCode::Parse, msg, span))
}

fn attach_span(mut err: Diag, span: Span) -> Diag {
    err.0.span = span;
    err
}

// Extend RegistryBuilder with apply methods - these will be in registry.rs
// For now defs calls builder methods we'll add

impl RegistryBuilder {
    pub(crate) fn declare_dimension(&mut self, name: &str) -> Result<(), Diag> {
        self.new_dimension(name)
    }

    pub(crate) fn apply_primary_define(
        &mut self,
        names: &[String],
        dim_name: &str,
        span: Span,
    ) -> Result<(), Diag> {
        let custom_id = self.custom_dims.get(dim_name).copied().ok_or_else(|| {
            Diag::new(Diagnostic::error(
                ErrorCode::Parse,
                format!("unknown dimension `{dim_name}`"),
                span,
            ))
        })?;
        let dim = Dimension::single(BaseDim::Custom(custom_id), num_rational::Ratio::from_integer(1));
        let primary = &names[0];
        let aliases: Vec<&str> = names[1..].iter().map(String::as_str).collect();
        self.insert_unit(primary, &aliases, dim, num_rational::Ratio::one(), false, span)?;
        let anchor = DimAnchor::Custom(custom_id);
        self.anchors.insert(anchor, primary.clone());
        Ok(())
    }

    pub(crate) fn apply_linear_define(
        &mut self,
        names: &[String],
        expr: &str,
        span: Span,
    ) -> Result<(), Diag> {
        if is_affine_name(&names[0]) {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::AffineDefine,
                format!("affine unit `{}` cannot be defined; use built-ins", names[0]),
                span,
            )));
        }
        let lookup = UnitLookup::from_builder(self);
        let qty = eval_def_expr(expr, &lookup)?;
        let primary = &names[0];
        let aliases: Vec<&str> = names[1..].iter().map(String::as_str).collect();
        self.insert_resolved_unit(primary, &aliases, qty, span)
    }

    pub(crate) fn apply_anchor_stmt(
        &mut self,
        dim_name: &str,
        unit_name: &str,
        span: Span,
    ) -> Result<(), Diag> {
        if let Some(base) = DimAnchor::parse_base_name(dim_name) {
            return self.set_anchor(base, unit_name).map_err(|mut e| {
                e.0.span = span;
                e
            });
        }
        if let Some(&custom_id) = self.custom_dims.get(dim_name) {
            if self.pending_anchors.contains_key(&DimAnchor::Custom(custom_id)) {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::DupAnchor,
                    format!("duplicate anchor for dimension `{dim_name}`"),
                    span,
                )));
            }
            let unit = self.units.get(unit_name).ok_or_else(|| {
                Diag::new(Diagnostic::error(
                    ErrorCode::AnchorInvalid,
                    format!("unknown anchor unit `{unit_name}`"),
                    span,
                ))
            })?;
            if unit.affine {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::AnchorAffine,
                    format!("affine unit `{unit_name}` cannot anchor a dimension"),
                    span,
                )));
            }
            let expected = Dimension::single(BaseDim::Custom(custom_id), num_rational::Ratio::one());
            if unit.dimension != expected {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::AnchorInvalid,
                    format!("unit `{unit_name}` is not a linear unit of `{dim_name}`"),
                    span,
                )));
            }
            self.pending_anchors
                .insert(DimAnchor::Custom(custom_id), unit_name.into());
            return Ok(());
        }
        Err(Diag::new(Diagnostic::error(
            ErrorCode::AnchorInvalid,
            format!("unknown dimension `{dim_name}`"),
            span,
        )))
    }
}

fn is_affine_name(name: &str) -> bool {
    matches!(name, "°F" | "°C" | "K" | "degC" | "degF" | "celsius" | "fahrenheit")
}
