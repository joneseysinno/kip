//! Order-free resolution of unit definitions with cycle detection.

use std::collections::{BTreeSet, HashMap, VecDeque};

use crate::diag::{Diag, Diagnostic, ErrorCode, Hint, Span};

/// A pending linear `define` statement.
#[derive(Debug, Clone)]
pub struct PendingDefine {
    /// Primary unit name.
    pub primary: String,
    /// Alias names (diagnostics).
    #[allow(dead_code)]
    pub aliases: Vec<String>,
    /// RHS expression source.
    pub expr: String,
    /// Source span.
    pub span: Span,
}

/// Dependency graph over pending define names.
pub fn topo_sort(defs: &[PendingDefine]) -> Result<Vec<usize>, Vec<Diag>> {
    let names: BTreeSet<_> = defs.iter().map(|d| d.primary.as_str()).collect();
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    let mut indegree: HashMap<String, usize> = HashMap::new();

    for d in defs {
        graph.entry(d.primary.clone()).or_default();
        indegree.entry(d.primary.clone()).or_insert(0);
        let deps = super::eval_expr::def_expr_dependencies(&d.expr).unwrap_or_default();
        for dep in deps {
            if names.contains(dep.as_str()) {
                graph.entry(dep.clone()).or_default().push(d.primary.clone());
                *indegree.entry(d.primary.clone()).or_insert(0) += 1;
            }
        }
    }

    let mut queue: VecDeque<String> = indegree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(k, _)| k.clone())
        .collect();
    queue.make_contiguous().sort();

    let mut order = Vec::new();
    let mut seen = BTreeSet::new();

    while let Some(n) = queue.pop_front() {
        if !seen.insert(n.clone()) {
            continue;
        }
        if let Some(idx) = defs.iter().position(|d| d.primary == n) {
            order.push(idx);
        }
        if let Some(edges) = graph.get(&n) {
            for m in edges {
                if let Some(deg) = indegree.get_mut(m) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(m.clone());
                    }
                }
            }
        }
    }

    if order.len() != defs.len() {
        return Err(cycle_diagnostics(defs, &names));
    }
    Ok(order)
}

fn cycle_diagnostics(defs: &[PendingDefine], names: &BTreeSet<&str>) -> Vec<Diag> {
    let mut diags = Vec::new();
    for d in defs {
        let deps = super::eval_expr::def_expr_dependencies(&d.expr).unwrap_or_default();
        if deps.iter().any(|dep| names.contains(dep.as_str())) {
            diags.push(Diag::new(
                Diagnostic::error(
                    ErrorCode::DefCycle,
                    format!("circular unit definition involving `{}`", d.primary),
                    d.span,
                )
                .with_hints(vec![Hint::Note(
                    "order-free resolution detected a dependency cycle".into(),
                )]),
            ));
        }
    }
    if diags.is_empty() {
        diags.push(Diag::new(Diagnostic::error(
            ErrorCode::DefCycle,
            "circular unit definitions",
            Span::empty(0),
        )));
    }
    diags
}
