//! Evaluate definition RHS expressions to known quantities (grammar §6).

use num_rational::Ratio;
use num_traits::One;

use crate::diag::{Diag, Diagnostic, ErrorCode, Hint, Span};
use crate::dim::{BaseDim, Dimension};
use crate::eval::value::Quantity;
use crate::lexer::{lex, SpannedToken, Token};
use crate::quantity::UnitExpr;
use crate::registry::UnitLookup;

/// Evaluate a registry-definition expression against known units.
pub fn eval_def_expr(src: &str, units: &UnitLookup) -> Result<Quantity, Diag> {
    let tokens = lex(src)?;
    let mut parser = DefExprParser::new(tokens, units);
    let qty = parser.parse_quantity()?;
    if parser.peek().is_some() {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Parse,
            "unexpected tokens after definition expression",
            parser.span(),
        )));
    }
    Ok(qty)
}

/// Collect unit identifiers referenced in a definition expression.
pub fn def_expr_dependencies(src: &str) -> Result<Vec<String>, Diag> {
    let tokens = lex(src)?;
    let mut deps = Vec::new();
    for t in tokens {
        if let Token::Ident(name) = t.token {
            if !["define", "dimension", "anchor"].contains(&name.as_str()) && !deps.contains(&name) {
                deps.push(name);
            }
        }
    }
    Ok(deps)
}

struct DefExprParser<'a> {
    tokens: Vec<SpannedToken>,
    pos: usize,
    units: &'a UnitLookup,
}

impl<'a> DefExprParser<'a> {
    fn new(tokens: Vec<SpannedToken>, units: &'a UnitLookup) -> Self {
        let tokens: Vec<_> = tokens
            .into_iter()
            .filter(|t| !matches!(t.token, Token::Eof))
            .collect();
        Self {
            tokens,
            pos: 0,
            units,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.token)
    }

    fn bump(&mut self) -> SpannedToken {
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        t
    }

    fn span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span)
            .unwrap_or_else(|| Span::empty(0))
    }

    fn parse_quantity(&mut self) -> Result<Quantity, Diag> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek(), Some(Token::Star | Token::UnitMul | Token::Slash)) {
            let op = self.bump().token;
            let right = self.parse_unary()?;
            left = combine_quantities(left, right, &op)?;
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Quantity, Diag> {
        if matches!(self.peek(), Some(Token::Minus)) {
            self.bump();
            let mut q = self.parse_pow()?;
            q.magnitude = -q.magnitude;
            return Ok(q);
        }
        self.parse_pow()
    }

    fn parse_pow(&mut self) -> Result<Quantity, Diag> {
        let mut left = self.parse_atom()?;
        if matches!(self.peek(), Some(Token::Caret)) {
            self.bump();
            let right = self.parse_unary()?;
            if !right.dim.is_dimensionless() {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::DefSymbolic,
                    "unit exponent must be dimensionless",
                    self.span(),
                )));
            }
            let exp = ratio_to_i32(&right.magnitude)?;
            left.dim = left.dim.pow(exp);
            let exp_i = *exp.numer();
            if exp_i >= 0 {
                left.magnitude = left.magnitude.pow(exp_i);
            } else {
                left.magnitude = Ratio::one() / left.magnitude.pow(-exp_i);
            }
        }
        Ok(left)
    }

    fn parse_atom(&mut self) -> Result<Quantity, Diag> {
        match self.peek() {
            Some(Token::Number { value, .. }) => {
                let n = *value;
                self.bump();
                if matches!(
                    self.peek(),
                    Some(Token::Ident(_))
                        | Some(Token::Feet { .. })
                        | Some(Token::Inches { .. })
                        | Some(Token::FtIn { .. })
                ) {
                    let (unit_expr, dim) = self.parse_unit_expr()?;
                    return Ok(Quantity::new(n, unit_expr, dim));
                }
                Ok(Quantity::new(n, UnitExpr::one(), Dimension::dimensionless()))
            }
            Some(Token::Feet { inches, .. }) => {
                let inches = *inches;
                self.bump();
                Ok(Quantity::new(
                    inches,
                    UnitExpr::named("ft"),
                    Dimension::single(BaseDim::Length, Ratio::one()),
                ))
            }
            Some(Token::Inches { inches, .. }) => {
                let inches = *inches;
                self.bump();
                Ok(Quantity::new(
                    inches,
                    UnitExpr::named("in"),
                    Dimension::single(BaseDim::Length, Ratio::one()),
                ))
            }
            Some(Token::FtIn { inches, .. }) => {
                let inches = *inches;
                self.bump();
                Ok(Quantity::new(
                    inches,
                    UnitExpr::named("ft"),
                    Dimension::single(BaseDim::Length, Ratio::one()),
                ))
            }
            Some(Token::Ident(_)) => {
                let (unit_expr, dim) = self.parse_unit_expr()?;
                Ok(Quantity::new(Ratio::one(), unit_expr, dim))
            }
            Some(Token::LParen) => {
                self.bump();
                let q = self.parse_quantity()?;
                if !matches!(self.peek(), Some(Token::RParen)) {
                    return Err(Diag::new(Diagnostic::error(
                        ErrorCode::Parse,
                        "expected `)`",
                        self.span(),
                    )));
                }
                self.bump();
                Ok(q)
            }
            _ => Err(Diag::new(Diagnostic::error(
                ErrorCode::Parse,
                "expected quantity in definition expression",
                self.span(),
            ))),
        }
    }

    fn parse_unit_expr(&mut self) -> Result<(UnitExpr, Dimension), Diag> {
        let mut acc = self.parse_unit_term()?;
        while matches!(self.peek(), Some(Token::Star | Token::UnitMul | Token::Slash)) {
            let op = self.bump().token;
            let right = self.parse_unit_term()?;
            acc = match op {
                Token::Star | Token::UnitMul => (
                    compose_unit_expr(&acc.0, &right.0, true),
                    acc.1.mul(&right.1),
                ),
                Token::Slash => (
                    compose_unit_expr(&acc.0, &right.0, false),
                    acc.1.div(&right.1),
                ),
                _ => unreachable!(),
            };
        }
        Ok(acc)
    }

    fn parse_unit_term(&mut self) -> Result<(UnitExpr, Dimension), Diag> {
        let ident = match self.bump().token {
            Token::Ident(name) => name,
            other => {
                return Err(Diag::new(Diagnostic::error(
                    ErrorCode::Parse,
                    format!("expected unit identifier, found {other:?}"),
                    self.span(),
                )));
            }
        };
        let record = self.units.get(&ident).ok_or_else(|| {
            Diag::new(
                Diagnostic::error(
                    ErrorCode::DefSymbolic,
                    format!("unknown unit or symbol `{ident}` in definition"),
                    self.span(),
                )
                .with_hints(vec![Hint::Note(
                    "definitions must be fully known — no free symbols".into(),
                )]),
            )
        })?;
        if record.affine {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::AffineDefine,
                format!("affine unit `{ident}` cannot be used in definitions"),
                self.span(),
            )));
        }
        let mut dim = record.dimension.clone();
        let mut unit = UnitExpr::named(ident);
        if matches!(self.peek(), Some(Token::Caret)) {
            self.bump();
            let exp_qty = self.parse_unary()?;
            let exp = ratio_to_i32(&exp_qty.magnitude)?;
            dim = dim.pow(exp);
            unit = UnitExpr::Compound(vec![unit]);
        }
        Ok((unit, dim))
    }
}

fn combine_quantities(left: Quantity, right: Quantity, op: &Token) -> Result<Quantity, Diag> {
    match op {
        Token::Star | Token::UnitMul => Ok(Quantity::new(
            left.magnitude * right.magnitude,
            compose_unit_expr(&left.unit, &right.unit, true),
            left.dim.mul(&right.dim),
        )),
        Token::Slash => Ok(Quantity::new(
            left.magnitude / right.magnitude,
            compose_unit_expr(&left.unit, &right.unit, false),
            left.dim.div(&right.dim),
        )),
        _ => Err(Diag::new(Diagnostic::error(
            ErrorCode::Parse,
            "invalid operator between quantities",
            Span::empty(0),
        ))),
    }
}

fn compose_unit_expr(lhs: &UnitExpr, rhs: &UnitExpr, mul: bool) -> UnitExpr {
    if mul {
        match (lhs, rhs) {
            (UnitExpr::Dimensionless, u) | (u, UnitExpr::Dimensionless) => u.clone(),
            _ => UnitExpr::Compound(vec![lhs.clone(), rhs.clone()]),
        }
    } else {
        UnitExpr::Compound(vec![lhs.clone(), rhs.clone()])
    }
}

fn ratio_to_i32(r: &Ratio<i128>) -> Result<Ratio<i32>, Diag> {
    if r.denom() != &1i128 {
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::DefSymbolic,
            "non-integer exponent in definition",
            Span::empty(0),
        )));
    }
    let n: i32 = (*r.numer()).try_into().map_err(|_| {
        Diag::new(Diagnostic::error(
            ErrorCode::DefSymbolic,
            "exponent out of range",
            Span::empty(0),
        ))
    })?;
    Ok(Ratio::from_integer(n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RegistryBuilder;

    #[test]
    fn eval_simple_define_rhs() {
        let reg = RegistryBuilder::from_seed().freeze();
        let lookup = UnitLookup::from_registry(&reg);
        let q = eval_def_expr("1000 lbf", &lookup).unwrap();
        assert_eq!(q.magnitude, Ratio::from_integer(1000));
    }
}
