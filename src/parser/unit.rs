//! Unit-expression parsing (grammar §5.1).

use crate::diag::{Diag, Diagnostic, ErrorCode, Hint, Span};
use crate::quantity::{UnitExpr, UnitExponent};
use crate::registry::Registry;
use crate::lexer::{SpannedToken, Token};

/// Parsed unit expression with source span covering the whole attachment.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedUnit {
    /// Structured unit expression.
    pub expr: UnitExpr,
    /// Span of the unit expression in source.
    pub span: Span,
}

/// Parser cursor over a token slice (shared with expression parser).
pub(crate) struct UnitParser<'a, 'b> {
    pub tokens: &'a [SpannedToken],
    pub pos: usize,
    pub registry: &'b Registry,
    pub errors: &'a mut Vec<Diag>,
    #[allow(dead_code)]
    pub lints: &'a mut Vec<Diag>,
}

impl<'a, 'b> UnitParser<'a, 'b> {
    pub fn peek(&self) -> Option<&SpannedToken> {
        self.tokens.get(self.pos)
    }

    pub fn bump(&mut self) -> SpannedToken {
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        t
    }

    pub fn span(&self) -> Span {
        self.peek()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::empty(0))
    }

    /// Whether the next token can start a unit expression (after optional W1 ws).
    pub fn can_start_unit_expr(&self) -> bool {
        matches!(self.peek().map(|t| &t.token), Some(Token::Ident(_)))
    }

    /// Parse a full unit expression; operators must be tight (no leading ws).
    pub fn parse_unit_expr(&mut self) -> Result<ParsedUnit, ()> {
        let start = self.span();
        let first = self.parse_unit_term()?;
        let mut parts = vec![first.expr];
        let mut end = first.span;

        while self.is_tight_unit_op() {
            let op = self.bump();
            end = op.span;
            let rhs = self.parse_unit_term()?;
            end = end.merge(rhs.span);
            parts = combine_unit_parts(parts, rhs.expr, &op.token);
        }

        let expr = fold_product(parts);
        Ok(ParsedUnit {
            span: start.merge(end),
            expr,
        })
    }

    fn is_tight_unit_op(&self) -> bool {
        let Some(t) = self.peek() else {
            return false;
        };
        if t.preceded_by_ws {
            return false;
        }
        matches!(t.token, Token::Star | Token::UnitMul | Token::Slash)
    }

    fn parse_unit_term(&mut self) -> Result<ParsedUnit, ()> {
        let ident_tok = self.bump();
        let (name, start) = match ident_tok.token {
            Token::Ident(name) => (name, ident_tok.span),
            _ => {
                self.push_error(
                    ErrorCode::Parse,
                    "expected unit identifier",
                    ident_tok.span,
                );
                return Err(());
            }
        };

        if self.registry.unit(&name).is_none() {
            self.errors.push(Diag::new(
                Diagnostic::error(
                    ErrorCode::UnknownUnit,
                    format!("unknown unit `{name}`"),
                    ident_tok.span,
                )
                .with_hints(vec![Hint::Note(
                    "if you meant multiplication, add spaces around `*`.".into(),
                )]),
            ));
            return Err(());
        }

        let mut expr = UnitExpr::named(name);
        let mut end = ident_tok.span;

        if matches!(self.peek().map(|t| &t.token), Some(Token::Caret)) {
            let caret = self.peek().unwrap();
            if caret.preceded_by_ws {
                return Ok(ParsedUnit { expr, span: start.merge(end) });
            }
            let caret_tok = self.bump();
            end = caret_tok.span;
            let exp = self.parse_unit_exp()?;
            end = end.merge(exp.span);
            expr = UnitExpr::Pow {
                base: Box::new(expr),
                exp: exp.value,
            };
        }

        Ok(ParsedUnit {
            expr,
            span: start.merge(end),
        })
    }

    fn parse_unit_exp(&mut self) -> Result<UnitExpParse, ()> {
        if matches!(self.peek().map(|t| &t.token), Some(Token::LParen)) {
            return self.parse_paren_unit_exp();
        }

        let mut neg = false;
        if matches!(self.peek().map(|t| &t.token), Some(Token::Minus)) {
            neg = true;
            self.bump();
        }

        let tok = self.bump();
        match tok.token {
            Token::Number { text, value, .. } => {
                if value.denom() != &1 {
                    Ok(UnitExpParse {
                        value: UnitExponent::Decimal(text),
                        span: tok.span,
                    })
                } else {
                    let n: i32 = (*value.numer()).try_into().map_err(|_| {
                        self.push_error(ErrorCode::Parse, "unit exponent out of range", tok.span);
                    })?;
                    let n = if neg { -n } else { n };
                    Ok(UnitExpParse {
                        value: UnitExponent::Int(n),
                        span: tok.span,
                    })
                }
            }
            _ => {
                self.push_error(ErrorCode::Parse, "expected unit exponent", tok.span);
                Err(())
            }
        }
    }

    fn parse_paren_unit_exp(&mut self) -> Result<UnitExpParse, ()> {
        let open = self.bump();
        let mut neg = false;
        if matches!(self.peek().map(|t| &t.token), Some(Token::Minus)) {
            neg = true;
            self.bump();
        }
        let num_tok = self.bump();
        let num: i32 = match num_tok.token {
            Token::Number { value, .. } if value.denom() == &1 => {
                (*value.numer()).try_into().map_err(|_| {
                    self.push_error(ErrorCode::Parse, "unit exponent out of range", num_tok.span);
                })?
            }
            _ => {
                self.push_error(ErrorCode::Parse, "expected integer numerator", num_tok.span);
                return Err(());
            }
        };
        if !matches!(self.peek().map(|t| &t.token), Some(Token::Slash)) {
            self.push_error(ErrorCode::Parse, "expected `/` in unit exponent fraction", self.span());
            return Err(());
        }
        self.bump();
        let den_tok = self.bump();
        let den = match den_tok.token {
            Token::Number { value, .. } if value.denom() == &1 => {
                let d: i32 = (*value.numer()).try_into().map_err(|_| {
                    self.push_error(ErrorCode::Parse, "unit exponent out of range", den_tok.span);
                })?;
                if d == 0 {
                    self.push_error(ErrorCode::Parse, "zero denominator in unit exponent", den_tok.span);
                    return Err(());
                }
                d
            }
            _ => {
                self.push_error(ErrorCode::Parse, "expected integer denominator", den_tok.span);
                return Err(());
            }
        };
        if !matches!(self.peek().map(|t| &t.token), Some(Token::RParen)) {
            self.push_error(ErrorCode::Parse, "expected `)` after unit exponent", self.span());
            return Err(());
        }
        let close = self.bump();
        let num = if neg { -num } else { num };
        Ok(UnitExpParse {
            value: UnitExponent::Ratio { num, den },
            span: open.span.merge(close.span),
        })
    }

    fn push_error(&mut self, code: ErrorCode, message: impl Into<String>, span: Span) {
        self.errors
            .push(Diag::new(Diagnostic::error(code, message, span)));
    }
}

struct UnitExpParse {
    value: UnitExponent,
    span: Span,
}

fn fold_product(mut parts: Vec<UnitExpr>) -> UnitExpr {
    if parts.len() == 1 {
        parts.pop().unwrap()
    } else {
        UnitExpr::Product(parts)
    }
}

fn combine_unit_parts(lhs_parts: Vec<UnitExpr>, rhs: UnitExpr, op: &Token) -> Vec<UnitExpr> {
    match op {
        Token::Star | Token::UnitMul => {
            let mut out = lhs_parts;
            out.push(rhs);
            out
        }
        Token::Slash => {
            let numerator = fold_product(lhs_parts);
            vec![UnitExpr::Quotient(Box::new(numerator), Box::new(rhs))]
        }
        _ => lhs_parts,
    }
}

/// Format a unit expression for unknown-unit diagnostics (`kip·L`).
pub fn unit_expr_display(expr: &UnitExpr) -> String {
    match expr {
        UnitExpr::Named(s) => s.clone(),
        UnitExpr::Dimensionless => "1".into(),
        UnitExpr::Product(parts) => parts
            .iter()
            .map(unit_expr_display)
            .collect::<Vec<_>>()
            .join("·"),
        UnitExpr::Quotient(num, den) => {
            format!("{}/{}", unit_expr_display(num), unit_expr_display(den))
        }
        UnitExpr::Pow { base, exp } => format!("{}^{exp:?}", unit_expr_display(base)),
    }
}
