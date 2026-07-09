//! Immutable expression AST (Arc-shared, Send + Sync).

use std::sync::Arc;

use crate::diag::Span;

/// Stable node identifier within an expression arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// Expression node kinds (M3 fills this out per grammar §5.2).
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Placeholder root until parser lands.
    Hole,
}

/// Frozen expression tree.
#[derive(Debug, Clone)]
pub struct Expr {
    /// Arena nodes (M3).
    pub nodes: Vec<ExprNode>,
    /// Root node id.
    pub root: NodeId,
}

/// Single AST node with span.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprNode {
    /// Node id.
    pub id: NodeId,
    /// Source span.
    pub span: Span,
    /// Node kind.
    pub kind: ExprKind,
}

impl Expr {
    /// Minimal placeholder expression for API wiring in M0.
    pub fn hole() -> Arc<Self> {
        Arc::new(Self {
            nodes: vec![ExprNode {
                id: NodeId(0),
                span: Span::empty(0),
                kind: ExprKind::Hole,
            }],
            root: NodeId(0),
        })
    }
}
