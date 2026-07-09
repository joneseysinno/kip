//! Immutable expression AST (Arc-shared, Send + Sync).

use std::sync::Arc;

use num_rational::Ratio;

use crate::diag::Span;
use crate::quantity::UnitExpr;

/// Stable node identifier within an expression arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// Binary operators in expression context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    /// Addition (`+`).
    Add,
    /// Subtraction (`-`).
    Sub,
    /// Multiplication (`*`).
    Mul,
    /// Division (`/`).
    Div,
    /// Exponentiation (`^`).
    Pow,
    /// Comparison (reserved v1.1).
    Cmp(CmpOp),
}

/// Comparison operators (reserved v1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmpOp {
    /// `>=`
    Gte,
    /// `<=`
    Lte,
    /// `>`
    Gt,
    /// `<`
    Lt,
    /// `==`
    EqEq,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// Negation (`-`).
    Neg,
}

/// Function or code-equation callee.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Callee {
    /// Built-in or user function (`sqrt`, `min`, …).
    Ident(String),
    /// Namespaced code equation (`ACI.fr`).
    Path(Vec<String>),
}

/// Call argument (positional or named).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CallArg {
    /// Positional argument.
    Positional(NodeId),
    /// Named argument (`fc: f'c`).
    Named {
        /// Parameter name.
        name: String,
        /// Argument expression.
        value: NodeId,
    },
}

/// Expression node kinds (grammar §5.2).
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Dimensionless numeric literal.
    Number {
        /// Exact rational value.
        value: Ratio<i128>,
        /// Normalized source text.
        text: String,
    },
    /// Numeric magnitude with attached unit expression.
    Quantity {
        /// Magnitude.
        magnitude: Ratio<i128>,
        /// Magnitude source text.
        mag_text: String,
        /// Attached unit expression.
        unit: UnitExpr,
    },
    /// Length literal from tick notation (`12'`, `6"`, `12'-6"`).
    Length {
        /// Exact total length in inches.
        inches: Ratio<i128>,
    },
    /// Free symbol or unresolved identifier.
    Ident {
        /// Identifier text.
        name: String,
    },
    /// Unary operator.
    Unary {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        operand: NodeId,
    },
    /// Binary operator.
    Binary {
        /// Operator.
        op: BinaryOp,
        /// Left operand.
        left: NodeId,
        /// Right operand.
        right: NodeId,
    },
    /// Function or code-equation call.
    Call {
        /// Callee.
        callee: Callee,
        /// Arguments.
        args: Vec<CallArg>,
    },
}

/// Frozen expression tree.
#[derive(Debug, Clone)]
pub struct Expr {
    /// Arena nodes.
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
    /// Lookup a node by id.
    pub fn node(&self, id: NodeId) -> &ExprNode {
        &self.nodes[id.0 as usize]
    }

    /// Root node.
    pub fn root_node(&self) -> &ExprNode {
        self.node(self.root)
    }

    /// Minimal placeholder expression for API wiring in M0.
    pub fn hole() -> Arc<Self> {
        use num_traits::Zero;
        Arc::new(Self {
            nodes: vec![ExprNode {
                id: NodeId(0),
                span: Span::empty(0),
                kind: ExprKind::Number {
                    value: Ratio::zero(),
                    text: "0".into(),
                },
            }],
            root: NodeId(0),
        })
    }
}

impl ExprKind {
    /// Whether this node is a quantity literal (including length ticks).
    pub fn is_quantity_like(&self) -> bool {
        matches!(self, Self::Quantity { .. } | Self::Length { .. })
    }
}

