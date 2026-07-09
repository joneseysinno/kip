//! Iterative Pratt expression parser (grammar §5.2).

use num_rational::Ratio;

use crate::diag::{Diag, Diagnostic, ErrorCode, Hint, LintCode, Span};
use crate::lexer::{lex_checked, SpannedToken, Token};
use crate::quantity::UnitExpr;
use crate::registry::Registry;
use crate::resolver::Resolver;

use super::ast::{
    BinaryOp, Callee, CallArg, CmpOp, Expr, ExprKind, ExprNode, NodeId, UnaryOp,
};
use super::unit::{unit_expr_display, ParsedUnit, UnitParser};

/// Binding power for Pratt infix operators `(left_bp, right_bp)`.
struct InfixBp {
    left: u8,
    right: u8,
}

const BP_ADD: InfixBp = InfixBp { left: 10, right: 11 };
const BP_MUL: InfixBp = InfixBp { left: 20, right: 21 };
const BP_POW: InfixBp = InfixBp { left: 40, right: 39 }; // right-assoc
const BP_CMP: InfixBp = InfixBp { left: 5, right: 6 };

pub(crate) struct PrattParser<'a, 'b> {
    tokens: Vec<SpannedToken>,
    pos: usize,
    registry: &'a Registry,
    resolver: Option<&'b dyn Resolver>,
    nodes: Vec<ExprNode>,
    errors: Vec<Diag>,
    lints: Vec<Diag>,
}

impl<'a, 'b> PrattParser<'a, 'b> {
    pub fn parse(
        src: &str,
        registry: &'a Registry,
        resolver: Option<&'b dyn Resolver>,
    ) -> (Option<Expr>, Vec<Diag>, Vec<Diag>) {
        let lex = lex_checked(src);
        let mut errors = lex.errors;
        let mut lints = lex.lints;
        if errors.iter().any(|e| e.diagnostic().severity == crate::diag::Severity::Error) {
            return (None, errors, lints);
        }

        let mut parser = Self {
            tokens: lex.tokens,
            pos: 0,
            registry,
            resolver,
            nodes: Vec::new(),
            errors: Vec::new(),
            lints: Vec::new(),
        };

        let root = match parser.parse_expr(0) {
            Ok(id) => id,
            Err(()) => {
                errors.extend(parser.errors);
                lints.extend(parser.lints);
                return (None, errors, lints);
            }
        };

        if !parser.at_eof() {
            if matches!(parser.peek_token(), Some(Token::Eq)) {
                let tok = parser.bump();
                parser.errors.push(Diag::new(
                    Diagnostic::error(
                        ErrorCode::EqInExpr,
                        "`=` is not allowed in expressions; bindings are an application-layer feature",
                        tok.span,
                    )
                    .with_hints(vec![Hint::Note(
                        "use your sheet host to bind names like `M = …`.".into(),
                    )]),
                ));
            } else {
                parser.error(
                    ErrorCode::Parse,
                    format!("unexpected token {:?}", parser.peek_token()),
                    parser.span(),
                );
            }
        }

        errors.extend(parser.errors);
        lints.extend(parser.lints);

        if errors.iter().any(|e| e.diagnostic().severity == crate::diag::Severity::Error) {
            return (None, errors, lints);
        }

        (
            Some(Expr {
                nodes: parser.nodes,
                root,
            }),
            errors,
            lints,
        )
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<NodeId, ()> {
        let mut lhs = self.parse_prefix()?;

        loop {
            let Some((op, bp)) = self.infix_binding_power() else {
                break;
            };
            if bp.left < min_bp {
                break;
            }

            if matches!(op, BinaryOp::Pow) {
                let caret = self.peek().unwrap();
                if caret.preceded_by_ws {
                    self.lint(
                        LintCode::SpacedCaret,
                        "spaced `^` binds at expression level, not as a unit exponent",
                        caret.span,
                    );
                }
            }

            self.bump();
            let rhs = self.parse_expr(bp.right)?;
            let span = self.nodes[lhs.0 as usize]
                .span
                .merge(self.nodes[rhs.0 as usize].span);
            lhs = self.alloc(
                ExprKind::Binary {
                    op,
                    left: lhs,
                    right: rhs,
                },
                span,
            );
        }

        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<NodeId, ()> {
        if matches!(self.peek_token(), Some(Token::Minus)) {
            let tok = self.bump();
            let operand = self.parse_expr(BP_POW.right)?;
            let span = tok.span.merge(self.nodes[operand.0 as usize].span);
            return Ok(self.alloc(ExprKind::Unary { op: UnaryOp::Neg, operand }, span));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<NodeId, ()> {
        let tok = self.peek().cloned();
        let Some(tok) = tok else {
            self.error(ErrorCode::Parse, "unexpected end of input", Span::empty(0));
            return Err(());
        };

        match &tok.token {
            Token::Number { value, text } => {
                self.bump();
                if self.can_start_unit_attachment() {
                    match self.parse_unit_attachment(tok.span, *value, text.clone()) {
                        Ok(id) => return Ok(id),
                        Err(()) => return Err(()),
                    }
                }
                Ok(self.alloc(
                    ExprKind::Number {
                        value: *value,
                        text: text.clone(),
                    },
                    tok.span,
                ))
            }
            Token::Feet { inches } | Token::Inches { inches } | Token::FtIn { inches } => {
                self.bump();
                Ok(self.alloc(ExprKind::Length { inches: *inches }, tok.span))
            }
            Token::Ident(_) => self.parse_ident_or_path(),
            Token::LParen => {
                self.bump();
                let inner = self.parse_expr(0)?;
                if !matches!(self.peek_token(), Some(Token::RParen)) {
                    self.error(ErrorCode::Parse, "expected `)`", self.span());
                    return Err(());
                }
                let close = self.bump();
                let span = tok.span.merge(close.span);
                self.nodes[inner.0 as usize].span = span;
                Ok(inner)
            }
            Token::Eq => {
                self.bump();
                self.errors.push(Diag::new(
                    Diagnostic::error(
                        ErrorCode::EqInExpr,
                        "`=` is not allowed in expressions; bindings are an application-layer feature",
                        tok.span,
                    )
                    .with_hints(vec![Hint::Note(
                        "use your sheet host to bind names like `M = …`.".into(),
                    )]),
                ));
                Err(())
            }
            Token::Gte | Token::Lte | Token::Gt | Token::Lt | Token::EqEq => {
                self.error(
                    ErrorCode::Parse,
                    "comparison operators are reserved for v1.1",
                    tok.span,
                );
                Err(())
            }
            _ => {
                self.error(
                    ErrorCode::Parse,
                    format!("unexpected token {:?}", tok.token),
                    tok.span,
                );
                Err(())
            }
        }
    }

    fn parse_ident_or_path(&mut self) -> Result<NodeId, ()> {
        let first = self.bump();
        let Token::Ident(first_name) = first.token else {
            unreachable!();
        };

        let mut segments = vec![first_name];
        let mut span = first.span;

        while matches!(
            self.peek_token(),
            Some(Token::Dot) if matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.token),
                Some(Token::Ident(_))
            )
        ) {
            self.bump(); // dot
            let seg = self.bump();
            if let Token::Ident(name) = seg.token {
                segments.push(name);
                span = span.merge(seg.span);
            }
        }

        if matches!(self.peek_token(), Some(Token::LParen)) {
            let callee = if segments.len() == 1 {
                Callee::Ident(segments[0].clone())
            } else {
                Callee::Path(segments)
            };
            return self.parse_call(callee, span);
        }

        if segments.len() == 1 {
            let name = segments.pop().unwrap();
            return Ok(self.alloc(ExprKind::Ident { name }, first.span));
        }

        self.error(
            ErrorCode::Parse,
            format!("dotted path `{}` requires a call", segments.join(".")),
            span,
        );
        Err(())
    }

    fn parse_call(&mut self, callee: Callee, span: Span) -> Result<NodeId, ()> {
        self.bump(); // (
        let mut args = Vec::new();
        if !matches!(self.peek_token(), Some(Token::RParen)) {
            loop {
                args.push(self.parse_arg()?);
                if matches!(self.peek_token(), Some(Token::Comma)) {
                    self.bump();
                    if matches!(self.peek_token(), Some(Token::RParen)) {
                        self.error(ErrorCode::Parse, "trailing comma in argument list", self.span());
                        return Err(());
                    }
                    continue;
                }
                break;
            }
        }
        if !matches!(self.peek_token(), Some(Token::RParen)) {
            self.error(ErrorCode::Parse, "expected `)` after arguments", self.span());
            return Err(());
        }
        let close = self.bump();
        let span = span.merge(close.span);
        Ok(self.alloc(ExprKind::Call { callee, args }, span))
    }

    fn parse_arg(&mut self) -> Result<CallArg, ()> {
        let start = self.pos;
        if let Some(Token::Ident(name)) = self.peek_token().cloned() {
            if matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.token),
                Some(Token::Colon)
            ) {
                self.bump();
                self.bump(); // colon
                let value = self.parse_expr(0)?;
                return Ok(CallArg::Named { name, value });
            }
            let _ = start; // ident may start expr
        }
        let value = self.parse_expr(0)?;
        Ok(CallArg::Positional(value))
    }

    fn parse_unit_attachment(
        &mut self,
        mag_span: Span,
        magnitude: Ratio<i128>,
        mag_text: String,
    ) -> Result<NodeId, ()> {
        let saved_pos = self.pos;
        let mut unit_parser = UnitParser {
            tokens: &self.tokens,
            pos: self.pos,
            registry: self.registry,
            errors: &mut self.errors,
            lints: &mut self.lints,
        };

        let parsed = match unit_parser.parse_unit_expr() {
            Ok(u) => u,
            Err(()) => {
                self.pos = unit_parser.pos;
                // Improve unknown-unit message for compounds like `kip*L`.
                if let Some(last) = self.errors.last() {
                    if last.diagnostic().code == ErrorCode::UnknownUnit.as_str() {
                        let partial = self.reconstruct_partial_unit(saved_pos);
                        let display = unit_expr_display(&partial);
                        let span = last.diagnostic().span;
                        self.errors.pop();
                        self.errors.push(Diag::new(
                            Diagnostic::error(
                                ErrorCode::UnknownUnit,
                                format!("unknown unit `{display}`"),
                                span,
                            )
                            .with_hints(vec![Hint::Note(
                                "if you meant multiplication, add spaces around `*`.".into(),
                            )]),
                        ));
                    }
                }
                return Err(());
            }
        };

        self.pos = unit_parser.pos;
        self.maybe_unit_shadow_lint(&parsed, mag_span);

        let span = mag_span.merge(parsed.span);
        Ok(self.alloc(
            ExprKind::Quantity {
                magnitude,
                mag_text,
                unit: parsed.expr,
            },
            span,
        ))
    }

    fn reconstruct_partial_unit(&self, start: usize) -> UnitExpr {
        let mut parts = Vec::new();
        let mut pos = start;
        while pos < self.tokens.len() {
            let t = &self.tokens[pos];
            match &t.token {
                Token::Ident(name) => {
                    parts.push(UnitExpr::named(name.clone()));
                    pos += 1;
                }
                Token::Star | Token::UnitMul if !t.preceded_by_ws => {
                    pos += 1;
                    continue;
                }
                Token::Slash if !t.preceded_by_ws => {
                    if parts.len() == 1 {
                        if let Some(Token::Ident(den)) = self.tokens.get(pos + 1).map(|t| &t.token) {
                            return UnitExpr::Quotient(
                                Box::new(parts.pop().unwrap()),
                                Box::new(UnitExpr::named(den.clone())),
                            );
                        }
                    }
                    break;
                }
                _ => break,
            }
        }
        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            UnitExpr::Product(parts)
        }
    }

    fn maybe_unit_shadow_lint(&mut self, unit: &ParsedUnit, _mag_span: Span) {
        let Some(resolver) = self.resolver else {
            return;
        };
        let unit_names = collect_unit_names(&unit.expr);
        for name in unit_names {
            if self.registry.unit(&name).is_some() && resolver.resolve(&name).is_some() {
                self.lint(
                    LintCode::UnitShadow,
                    format!("`{name}` resolves as a registered unit here; resolver also binds this symbol"),
                    unit.span,
                );
            }
        }
    }

    fn can_start_unit_attachment(&self) -> bool {
        UnitParser {
            tokens: &self.tokens,
            pos: self.pos,
            registry: self.registry,
            errors: &mut Vec::new(),
            lints: &mut Vec::new(),
        }
        .can_start_unit_expr()
    }

    fn infix_binding_power(&self) -> Option<(BinaryOp, InfixBp)> {
        let tok = self.peek()?;
        if tok.preceded_by_ws && matches!(tok.token, Token::Caret) {
            // spaced ^ handled as expression-level with lint in parse_expr
        }
        match &tok.token {
            Token::Plus => Some((BinaryOp::Add, BP_ADD)),
            Token::Minus => Some((BinaryOp::Sub, BP_ADD)),
            Token::Star | Token::UnitMul => Some((BinaryOp::Mul, BP_MUL)),
            Token::Slash => Some((BinaryOp::Div, BP_MUL)),
            Token::Caret => Some((BinaryOp::Pow, BP_POW)),
            Token::Gte => Some((BinaryOp::Cmp(CmpOp::Gte), BP_CMP)),
            Token::Lte => Some((BinaryOp::Cmp(CmpOp::Lte), BP_CMP)),
            Token::Gt => Some((BinaryOp::Cmp(CmpOp::Gt), BP_CMP)),
            Token::Lt => Some((BinaryOp::Cmp(CmpOp::Lt), BP_CMP)),
            Token::EqEq => Some((BinaryOp::Cmp(CmpOp::EqEq), BP_CMP)),
            _ => None,
        }
    }

    fn alloc(&mut self, kind: ExprKind, span: Span) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(ExprNode { id, span, kind });
        id
    }

    fn peek(&self) -> Option<&SpannedToken> {
        self.tokens.get(self.pos)
    }

    fn peek_token(&self) -> Option<&Token> {
        self.peek().map(|t| &t.token)
    }

    fn bump(&mut self) -> SpannedToken {
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        t
    }

    fn span(&self) -> Span {
        self.peek().map(|t| t.span).unwrap_or_else(|| Span::empty(0))
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_token(), Some(Token::Eof) | None)
    }

    fn error(&mut self, code: ErrorCode, message: impl Into<String>, span: Span) -> Diagnostic {
        let diag = Diagnostic::error(code, message, span);
        self.errors.push(Diag::new(diag.clone()));
        diag
    }

    fn lint(&mut self, code: LintCode, message: impl Into<String>, span: Span) {
        self.lints
            .push(Diag::new(Diagnostic::lint(code, message, span)));
    }
}

fn collect_unit_names(expr: &UnitExpr) -> Vec<String> {
    match expr {
        UnitExpr::Named(s) => vec![s.clone()],
        UnitExpr::Dimensionless => Vec::new(),
        UnitExpr::Product(parts) => {
            parts.iter().flat_map(collect_unit_names).collect()
        }
        UnitExpr::Quotient(num, den) => {
            let mut v = collect_unit_names(num);
            v.extend(collect_unit_names(den));
            v
        }
        UnitExpr::Pow { base, .. } => collect_unit_names(base),
    }
}
