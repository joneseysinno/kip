#![allow(non_snake_case)]

//! Lexer conformance tests (grammar-spec §8: Ticks, Identifiers, Numbers).

use kip::{lex, lex_checked, ErrorCode, LintCode, Token};
use num_rational::Ratio;
use num_traits::ToPrimitive;

fn inches(src: &str) -> Ratio<i128> {
    let tokens = lex(src).expect("lex");
    match &tokens[0].token {
        Token::Feet { inches } | Token::Inches { inches } | Token::FtIn { inches } => *inches,
        other => panic!("expected length literal, got {other:?}"),
    }
}

fn first_number(src: &str) -> Ratio<i128> {
    let tokens = lex(src).expect("lex");
    match &tokens[0].token {
        Token::Number { value, .. } => *value,
        other => panic!("expected number, got {other:?}"),
    }
}

fn first_ident(src: &str) -> String {
    let tokens = lex(src).expect("lex");
    match &tokens[0].token {
        Token::Ident(s) => s.clone(),
        other => panic!("expected ident, got {other:?}"),
    }
}

fn token_kinds(src: &str) -> Vec<String> {
    lex(src)
        .expect("lex")
        .into_iter()
        .map(|t| match t.token {
            Token::Eof => "Eof".into(),
            Token::Number { .. } => "Number".into(),
            Token::Ident(_) => "Ident".into(),
            Token::Feet { .. } => "Feet".into(),
            Token::Inches { .. } => "Inches".into(),
            Token::FtIn { .. } => "FtIn".into(),
            Token::Plus => "Plus".into(),
            Token::Minus => "Minus".into(),
            Token::Star => "Star".into(),
            Token::Slash => "Slash".into(),
            Token::Caret => "Caret".into(),
            Token::Comma => "Comma".into(),
            Token::LParen => "LParen".into(),
            Token::RParen => "RParen".into(),
            _ => format!("{:?}", t.token),
        })
        .collect()
}

fn err_code(src: &str) -> String {
    lex(src).unwrap_err().diagnostic().code.clone()
}

fn has_lint(src: &str, code: &str) -> bool {
    lex_checked(src)
        .lints
        .iter()
        .any(|l| l.diagnostic().code == code)
}

// --- Ticks and compounds (grammar-spec §8) ---

#[test]
fn lex_3_feet() {
    assert_eq!(inches("3'"), Ratio::from_integer(36));
}

#[test]
fn lex_0_dash_6_ftin() {
    assert_eq!(inches(r#"0'-6""#), Ratio::from_integer(6));
}

#[test]
fn lex_12_dash_0_ftin() {
    assert_eq!(inches(r#"12'-0""#), Ratio::from_integer(144));
}

#[test]
fn lex_12_dash_6_ftin() {
    assert_eq!(inches(r#"12'-6""#), Ratio::from_integer(150));
}

#[test]
fn lex_12_space_6_ftin() {
    assert_eq!(inches(r#"12' 6""#), Ratio::from_integer(150));
}

#[test]
fn lex_12_spaced_hyphen_ftin_lint() {
    assert_eq!(inches(r#"12' - 6""#), Ratio::from_integer(150));
    assert!(has_lint(r#"12' - 6""#, LintCode::FtInSpaced.as_str()));
}

#[test]
fn lex_12_dash_6_half_ftin() {
    // 12 ft + 6 1/2 in = 144 + 13/2 = 301/2 in (grammar §3.3 mixed = six and a half)
    assert_eq!(inches(r#"12'-6 1/2""#), Ratio::new(301, 2));
}

#[test]
fn lex_12_dash_6_dash_half_ftin() {
    assert_eq!(inches(r#"12'-6-1/2""#), Ratio::new(301, 2));
}

#[test]
fn lex_half_inch_frac() {
    assert_eq!(inches(r#"1/2""#), Ratio::new(1, 2));
}

#[test]
fn lex_six_and_half_inches() {
    assert_eq!(inches(r#"6 1/2""#), Ratio::new(13, 2));
}

#[test]
fn lex_2_star_12_dash_6_tokens() {
    assert_eq!(
        token_kinds(r#"2*12'-6""#),
        vec!["Number", "Star", "FtIn", "Eof"]
    );
}

#[test]
fn lex_paren_subtraction_tokens() {
    assert_eq!(
        token_kinds(r#"(2*12') - 6""#),
        vec!["LParen", "Number", "Star", "Feet", "RParen", "Minus", "Inches", "Eof"]
    );
}

#[test]
fn lex_tick_space_error() {
    assert_eq!(err_code("12 '"), ErrorCode::TickSpace.as_str());
}

#[test]
fn lex_12_dash_L_backtrack() {
    assert_eq!(
        token_kinds("12' - L"),
        vec!["Feet", "Minus", "Ident", "Eof"]
    );
}

#[test]
fn lex_12_dash_x_quote_bare_tick() {
    assert_eq!(err_code(r#"12'-x""#), ErrorCode::BareTick.as_str());
}

#[test]
fn lex_5_dash_13_inch_ge_12_lint() {
    assert_eq!(inches(r#"5'-13""#), Ratio::from_integer(73));
    assert!(has_lint(r#"5'-13""#, LintCode::InchGe12.as_str()));
}

// --- Identifiers and primes ---

#[test]
fn lex_fc_prime() {
    assert_eq!(first_ident("f'c"), "f'c");
}

#[test]
fn lex_f_double_prime() {
    assert_eq!(first_ident("f''"), "f''");
}

#[test]
fn lex_L_prime() {
    assert_eq!(first_ident("L'"), "L'");
}

#[test]
fn lex_f_prime_space_c_two_idents() {
    assert_eq!(token_kinds("f' c"), vec!["Ident", "Ident", "Eof"]);
}

#[test]
fn lex_greek_phi_b() {
    assert_eq!(first_ident("φ_b"), "φ_b");
}

#[test]
fn lex_lambda_ident() {
    assert_eq!(first_ident("lambda"), "lambda");
}

#[test]
fn lex_delta_max() {
    assert_eq!(first_ident("Δ_max"), "Δ_max");
}

#[test]
fn lex_bare_tick_foo() {
    assert_eq!(err_code("'foo"), ErrorCode::BareTick.as_str());
}

#[test]
fn lex_2L_prime_attachment_tokens() {
    assert_eq!(token_kinds("2L'"), vec!["Number", "Ident", "Eof"]);
}

// --- Numbers ---

#[test]
fn lex_underscore_separator() {
    assert_eq!(first_number("29_000 ksi").to_i128(), Some(29000));
}

#[test]
fn lex_comma_group_lint() {
    let kinds = token_kinds("29,000 ksi");
    assert_eq!(kinds[0], "Number");
    assert_eq!(kinds[1], "Comma");
    assert_eq!(kinds[2], "Number");
    assert!(has_lint("29,000 ksi", LintCode::CommaGroup.as_str()));
}

#[test]
fn lex_sci_tight() {
    assert_eq!(first_number("1e3 lbf").to_i128(), Some(1000));
}

#[test]
fn lex_sci_spaced_two_tokens() {
    assert_eq!(token_kinds("1 e3"), vec!["Number", "Ident", "Eof"]);
}

#[test]
fn lex_leading_decimal() {
    assert_eq!(first_number(".5 in"), Ratio::new(1, 2));
}

#[test]
fn lex_caret_chain() {
    assert_eq!(token_kinds("2^-3"), vec!["Number", "Caret", "Minus", "Number", "Eof"]);
}

#[test]
fn lex_unary_minus_caret() {
    assert_eq!(token_kinds("-2^2"), vec!["Minus", "Number", "Caret", "Number", "Eof"]);
}

// --- Adversarial lexer seeds (grammar-spec §8) ---

#[test]
fn adv_12_prime_eof() {
    assert_eq!(token_kinds("12'"), vec!["Feet", "Eof"]);
}

#[test]
fn adv_12_dash_eof() {
    assert_eq!(token_kinds("12'-"), vec!["Feet", "Minus", "Eof"]);
}

#[test]
fn adv_12_dash_6_no_quote() {
    assert_eq!(token_kinds("12'-6"), vec!["Feet", "Minus", "Number", "Eof"]);
}

#[test]
fn adv_incomplete_frac() {
    // R2 backtrack: `12'` FEET, then `- 6 1/` re-lexed as MINUS NUMBER SLASH
    assert_eq!(
        token_kinds(r#"12'-6 1/"#),
        vec!["Feet", "Minus", "Number", "Number", "Slash", "Eof"]
    );
}

#[test]
fn adv_incomplete_mixed_no_quote() {
    assert_eq!(
        token_kinds("12'-6 1/2"),
        vec!["Feet", "Minus", "Number", "Number", "Slash", "Number", "Eof"]
    );
}

#[test]
fn adv_div_zero_inch_frac() {
    assert_eq!(err_code(r#"1/0""#), ErrorCode::DivZeroLiteral.as_str());
}

#[test]
fn adv_fc_prime_tick() {
    assert_eq!(first_ident("f'c'"), "f'c'");
}

#[test]
fn adv_unicode_prime_alias() {
    assert_eq!(inches("3\u{2032}"), Ratio::from_integer(36));
}

#[test]
fn adv_double_bare_ticks() {
    assert_eq!(err_code("''"), ErrorCode::BareTick.as_str());
}

#[test]
fn lex_never_panics_on_garbage() {
    for s in ["\0", "🙂", "\u{2032}\u{2032}", "12'\u{ffff}"] {
        let _ = lex_checked(s);
    }
}
