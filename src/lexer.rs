//! Tokenizer state machine (grammar-spec §4).

pub mod ftin;
pub mod number;
pub mod token;

use crate::diag::{
    Diag, Diagnostic, ErrorCode, LintCode, Severity, Span,
};
use ftin::{try_scan_ftin, try_scan_inches_literal, InchScanError};
use number::parse_number_at;
pub use token::{SpannedToken, Token};

/// Lexer span type (alias for [`crate::diag::Span`]).
pub type LexSpan = crate::diag::Span;

/// Outcome of lexing with non-fatal lints.
#[derive(Debug, Clone)]
pub struct LexOutcome {
    /// Produced tokens (always ends with `Eof` on success).
    pub tokens: Vec<SpannedToken>,
    /// Hard errors (lexer stops at first).
    pub errors: Vec<Diag>,
    /// Non-fatal lints.
    pub lints: Vec<Diag>,
}

/// Lex source into tokens; returns error on first hard lex failure.
pub fn lex(src: &str) -> Result<Vec<SpannedToken>, Diag> {
    let outcome = lex_checked(src);
    if let Some(err) = outcome.errors.first() {
        return Err(err.clone());
    }
    Ok(outcome.tokens)
}

/// Lex source, collecting lints and continuing past non-fatal issues where possible.
pub fn lex_checked(src: &str) -> LexOutcome {
    let mut lexer = Lexer::new(src);
    lexer.run();
    LexOutcome {
        tokens: lexer.tokens,
        errors: lexer.errors,
        lints: lexer.lints,
    }
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<SpannedToken>,
    errors: Vec<Diag>,
    lints: Vec<Diag>,
    ftin_spaced_emitted: bool,
    comma_group_emitted: bool,
    /// Whitespace seen since the previous emitted token.
    next_preceded_by_ws: bool,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
            errors: Vec::new(),
            lints: Vec::new(),
            ftin_spaced_emitted: false,
            comma_group_emitted: false,
            next_preceded_by_ws: false,
        }
    }

    fn run(&mut self) {
        while self.pos < self.bytes.len() {
            if self.skip_ws() {
                self.next_preceded_by_ws = true;
                continue;
            }
            if self.skip_comment() {
                continue;
            }
            if self.pos >= self.bytes.len() {
                break;
            }
            if self.errors.iter().any(|e| e.diagnostic().severity == Severity::Error) {
                break;
            }
            self.lex_next();
            self.next_preceded_by_ws = false;
        }
        if self.errors.is_empty() {
            self.push(Token::Eof, self.pos, self.pos);
        }
    }

    fn lex_next(&mut self) {
        let start = self.pos;
        let b = self.bytes[self.pos];

        if b.is_ascii_digit() || (b == b'.' && self.dot_starts_number()) {
            self.lex_number_or_length(start);
            return;
        }

        if self.peek_char().is_some_and(is_ident_start) {
            self.lex_ident(start);
            return;
        }

        if matches!(b, b'\'' | b'"') || self.peek_char() == Some('\u{2032}') || self.peek_char() == Some('\u{2033}')
        {
            self.error(
                ErrorCode::BareTick,
                "tick mark cannot start a token",
                Span::new(start, start + self.char_len_at(start)),
            );
            self.pos += self.char_len_at(start);
            return;
        }

        if b == b'#' {
            self.skip_comment();
            return;
        }

        // multi-char operators
        if self.try_two_char(b'=', b'=', Token::EqEq) {
            return;
        }
        if self.try_two_char(b'>', b'=', Token::Gte) {
            return;
        }
        if self.try_two_char(b'<', b'=', Token::Lte) {
            return;
        }
        if self.try_two_char(b':', b':', Token::ColonColon) {
            return;
        }
        if self.bytes[self.pos] == b':' {
            let start = self.pos;
            self.pos += 1;
            self.push(Token::Colon, start, self.pos);
            return;
        }

        let (token, len) = match b {
            b'+' => (Token::Plus, 1),
            b'-' => (Token::Minus, 1),
            b'*' => (Token::Star, 1),
            b'/' => (Token::Slash, 1),
            b'^' => (Token::Caret, 1),
            b'(' => (Token::LParen, 1),
            b')' => (Token::RParen, 1),
            b',' => {
                self.pos += 1;
                self.maybe_comma_group_lint();
                self.push(Token::Comma, start, self.pos);
                return;
            }
            b'=' => (Token::Eq, 1),
            b'.' => (Token::Dot, 1),
            b'<' => (Token::Lt, 1),
            b'>' => (Token::Gt, 1),
            _ => {
                if let Some(ch) = self.peek_char() {
                    if ch == '\u{00B7}' || ch == '\u{00D7}' {
                        (Token::UnitMul, ch.len_utf8())
                    } else {
                        self.error(
                            ErrorCode::Parse,
                            format!("unexpected character `{ch}`"),
                            Span::new(start, start + ch.len_utf8()),
                        );
                        self.pos += ch.len_utf8();
                        return;
                    }
                } else {
                    self.error(
                        ErrorCode::Parse,
                        "unexpected character",
                        Span::new(start, start + 1),
                    );
                    self.pos += 1;
                    return;
                }
            }
        };
        self.pos += len;
        self.push(token, start, self.pos);
    }

    fn try_two_char(&mut self, a: u8, b: u8, tok: Token) -> bool {
        if self.bytes[self.pos] == a
            && self.pos + 1 < self.bytes.len()
            && self.bytes[self.pos + 1] == b
        {
            let start = self.pos;
            self.pos += 2;
            self.push(tok, start, self.pos);
            true
        } else {
            false
        }
    }

    fn lex_number_or_length(&mut self, start: usize) {
        // Standalone inch literal: `1/2"`, `6 1/2"`, etc.
        match try_scan_inches_literal(self.src, start) {
            Ok(Some(scan)) => {
                if scan.inch_ge_12 && !self.lint_emitted(LintCode::InchGe12) {
                    self.lint(
                        LintCode::InchGe12,
                        "inch part ≥ 12; intentional?",
                        Span::new(start, scan.end),
                    );
                }
                self.pos = scan.end;
                self.push(
                    Token::Inches {
                        inches: scan.inches,
                    },
                    start,
                    scan.end,
                );
                return;
            }
            Err(InchScanError::DivZero) => {
                self.error(
                    ErrorCode::DivZeroLiteral,
                    "zero denominator in fraction literal",
                    Span::new(start, start + 3),
                );
                return;
            }
            Ok(None) => {}
        }

        let slice = &self.src[start..];
        let (value, num_len, _) = match parse_number_at(slice) {
            Ok(v) => v,
            Err(_) => {
                self.error(
                    ErrorCode::Parse,
                    "invalid number literal",
                    Span::new(start, start + 1),
                );
                self.pos += 1;
                return;
            }
        };
        let num_end = start + num_len;

        // whitespace before tick → E-TICK-SPACE (R1, state 8)
        if self.is_ws_at(num_end) {
            let tick_pos = self.skip_ws_pos(num_end);
            if self.is_tick_at(tick_pos) {
                self.push_number(value, &slice[..num_len], start, num_end);
                self.error(
                    ErrorCode::TickSpace,
                    "whitespace between number and tick mark",
                    Span::new(tick_pos, tick_pos + 1),
                );
                return;
            }
        }

        // `'` immediately after number
        if self.is_prime_tick_at(num_end) {
            let prime_len = self.tick_len_at(num_end);
            let after_prime = num_end + prime_len;
            match try_scan_ftin(self.src, after_prime, value) {
                Ok(Some(scan)) => {
                    if scan.spaced_hyphen && !self.ftin_spaced_emitted {
                        self.ftin_spaced_emitted = true;
                        self.lint(
                            LintCode::FtInSpaced,
                            "interpreted as feet-inch compound; write `12 ft - 6 in` for subtraction",
                            Span::new(start, scan.end),
                        );
                    }
                    let inch_ge_12 = scan.inch_part >= num_rational::Ratio::from_integer(12);
                    if inch_ge_12 && !self.lint_emitted(LintCode::InchGe12) {
                        self.lint(
                            LintCode::InchGe12,
                            "inch part ≥ 12; intentional?",
                            Span::new(start, scan.end),
                        );
                    }
                    self.pos = scan.end;
                    self.push(
                        Token::FtIn {
                            inches: scan.inches,
                        },
                        start,
                        scan.end,
                    );
                    return;
                }
                Ok(None) => {
                    let feet_inches = value * num_rational::Ratio::from_integer(12);
                    self.pos = after_prime;
                    self.push(
                        Token::Feet {
                            inches: feet_inches,
                        },
                        start,
                        after_prime,
                    );
                    return;
                }
                Err(InchScanError::DivZero) => {
                    self.error(
                        ErrorCode::DivZeroLiteral,
                        "zero denominator in fraction literal",
                        Span::new(start, after_prime),
                    );
                    return;
                }
            }
        }

        // `"` immediately after number → INCHES
        if self.is_dquote_at(num_end) {
            let tick_len = self.tick_len_at(num_end);
            let end = num_end + tick_len;
            self.pos = end;
            self.push(
                Token::Inches { inches: value },
                start,
                end,
            );
            return;
        }

        self.pos = num_end;
        self.push_number(value, &slice[..num_len], start, num_end);
        self.maybe_comma_group_lint_after_number();
    }

    fn push_number(&mut self, value: num_rational::Ratio<i128>, text: &str, start: usize, end: usize) {
        let normalized: String = text.chars().filter(|c| *c != '_').collect();
        self.push(
            Token::Number {
                text: normalized,
                value,
            },
            start,
            end,
        );
    }

    fn lex_ident(&mut self, start: usize) {
        let mut pos = start;
        while pos < self.bytes.len() {
            let ch = self.char_at(pos);
            if is_ident_continue(ch) {
                pos += ch.len_utf8();
            } else if ch == '\'' || ch == '\u{2032}' {
                // prime inside identifier (R1: after letter)
                pos += ch.len_utf8();
            } else {
                break;
            }
        }
        let text = &self.src[start..pos];
        self.pos = pos;
        self.push(Token::Ident(text.to_string()), start, pos);
    }

    fn maybe_comma_group_lint_after_number(&mut self) {
        if self.comma_group_emitted {
            return;
        }
        let p = self.pos;
        if p >= self.bytes.len() || self.bytes[p] != b',' {
            return;
        }
        let after_comma = p + 1;
        if looks_like_comma_group(&self.src[after_comma..]) {
            self.comma_group_emitted = true;
            self.lint(
                LintCode::CommaGroup,
                "did you mean digit separator `_` instead of comma?",
                Span::new(p, after_comma + 3),
            );
        }
    }

    fn maybe_comma_group_lint(&mut self) {
        // handled after number; comma token itself doesn't need action
    }

    fn skip_ws(&mut self) -> bool {
        let start = self.pos;
        while self.pos < self.bytes.len() && is_ws_byte(self.bytes[self.pos]) {
            self.pos += 1;
        }
        self.pos > start
    }

    fn skip_comment(&mut self) -> bool {
        if self.bytes.get(self.pos) != Some(&b'#') {
            return false;
        }
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
            self.pos += 1;
        }
        true
    }

    fn is_ws_at(&self, pos: usize) -> bool {
        self.bytes.get(pos).copied().is_some_and(is_ws_byte)
    }

    fn skip_ws_pos(&self, mut pos: usize) -> usize {
        while pos < self.bytes.len() && is_ws_byte(self.bytes[pos]) {
            pos += 1;
        }
        pos
    }

    fn is_tick_at(&self, pos: usize) -> bool {
        self.is_prime_tick_at(pos) || self.is_dquote_at(pos)
    }

    fn is_prime_tick_at(&self, pos: usize) -> bool {
        matches!(
            self.peek_char_at(pos),
            Some('\'') | Some('\u{2032}')
        )
    }

    fn is_dquote_at(&self, pos: usize) -> bool {
        matches!(
            self.peek_char_at(pos),
            Some('"') | Some('\u{2033}')
        )
    }

    fn tick_len_at(&self, pos: usize) -> usize {
        self.char_at(pos).len_utf8()
    }

    fn char_len_at(&self, pos: usize) -> usize {
        self.char_at(pos).len_utf8()
    }

    fn char_at(&self, pos: usize) -> char {
        self.src[pos..].chars().next().unwrap()
    }

    fn peek_char(&self) -> Option<char> {
        self.peek_char_at(self.pos)
    }

    fn peek_char_at(&self, pos: usize) -> Option<char> {
        self.src[pos..].chars().next()
    }

    fn dot_starts_number(&self) -> bool {
        let next = self.pos + 1;
        self.bytes
            .get(next)
            .copied()
            .is_some_and(|b| b.is_ascii_digit())
    }

    fn push(&mut self, token: Token, start: usize, end: usize) {
        self.tokens.push(SpannedToken {
            token,
            span: Span::new(start, end),
            preceded_by_ws: self.next_preceded_by_ws,
        });
    }

    fn error(&mut self, code: ErrorCode, message: impl Into<String>, span: Span) {
        self.errors.push(Diag::new(Diagnostic::error(code, message, span)));
    }

    fn lint(&mut self, code: LintCode, message: impl Into<String>, span: Span) {
        self.lints.push(Diag::new(Diagnostic::lint(code, message, span)));
    }

    fn lint_emitted(&self, code: LintCode) -> bool {
        self.lints.iter().any(|l| l.diagnostic().code == code.as_str())
    }
}

fn is_ws_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn is_ident_start(ch: char) -> bool {
    unicode_ident::is_xid_start(ch)
        || matches!(ch, '°' | '$' | '%' | 'Ω' | 'μ')
}

fn is_ident_continue(ch: char) -> bool {
    unicode_ident::is_xid_continue(ch)
        || matches!(ch, '°' | '$' | '%' | 'Ω' | 'μ')
}

fn looks_like_comma_group(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit() && bytes[2].is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::ToPrimitive;

    fn tok_kind(src: &str) -> Vec<&'static str> {
        lex(src)
            .unwrap()
            .into_iter()
            .map(|t| match t.token {
                Token::Eof => "Eof",
                Token::Number { .. } => "Number",
                Token::Ident(_) => "Ident",
                Token::Feet { .. } => "Feet",
                Token::Inches { .. } => "Inches",
                Token::FtIn { .. } => "FtIn",
                Token::Plus => "Plus",
                Token::Star => "Star",
                Token::Minus => "Minus",
                Token::Comma => "Comma",
                _ => "Other",
            })
            .collect()
    }

    #[test]
    fn feet_and_ftin() {
        let t = lex("3'").unwrap();
        assert!(matches!(t[0].token, Token::Feet { .. }));
        if let Token::Feet { inches } = t[0].token {
            assert_eq!(inches.to_i128(), Some(36));
        }

        let t = lex(r#"12'-6""#).unwrap();
        assert!(matches!(t[0].token, Token::FtIn { .. }));
        if let Token::FtIn { inches } = t[0].token {
            assert_eq!(inches.to_i128(), Some(150));
        }
    }

    #[test]
    fn ident_primes() {
        let t = lex("f'c").unwrap();
        assert!(matches!(&t[0].token, Token::Ident(s) if s == "f'c"));
    }

    #[test]
    fn tick_space_error() {
        assert!(lex("12 '").is_err());
    }

    #[test]
    fn bare_tick_error() {
        assert!(lex("'foo").is_err());
    }

    #[test]
    fn comma_group_lint() {
        let o = lex_checked("29,000");
        assert!(o.lints.iter().any(|l| l.diagnostic().code == "L-COMMA-GROUP"));
        assert_eq!(tok_kind("29,000"), vec!["Number", "Comma", "Number", "Eof"]);
    }
}
