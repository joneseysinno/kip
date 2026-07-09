//! Bounded-lookahead feet-inch compound scanner (grammar-spec §3.3, §4).

use num_rational::Ratio;
use num_traits::Zero;

use super::number::{parse_number_at, NumberError};

/// Result of a successful FTIN compound scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtInScan {
    /// Total length in inches.
    pub inches: Ratio<i128>,
    /// End byte offset (exclusive).
    pub end: usize,
    /// Whether a spaced hyphen was used (lint `L-FTIN-SPACED`).
    pub spaced_hyphen: bool,
    /// Inch component (for `L-INCH-GE-12`).
    pub inch_part: Ratio<i128>,
}

/// Result of a successful standalone inches literal scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InchesScan {
    /// Length in inches.
    pub inches: Ratio<i128>,
    /// End byte offset (exclusive).
    pub end: usize,
    /// Inch component ≥ 12 (lint `L-INCH-GE-12`).
    pub inch_ge_12: bool,
}

/// Scan error inside inch parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InchScanError {
    /// Zero denominator in fraction.
    DivZero,
}

/// Try to scan `inch_val dquote` starting at `start` (grammar §3.3).
///
/// Does not cross newlines. Returns `None` if the pattern does not complete.
pub fn try_scan_inches_literal(src: &str, start: usize) -> Result<Option<InchesScan>, InchScanError> {
    if start >= src.len() {
        return Ok(None);
    }
    let nl = src[start..].find('\n').map(|i| start + i).unwrap_or(src.len());
    let slice = &src[start..nl];
    let (inches, consumed, inch_ge_12) = match parse_inch_val(slice)? {
        Some(v) => v,
        None => return Ok(None),
    };
    let rest = &slice[consumed..];
    let dquote_len = match peek_dquote(rest) {
        Some(len) => len,
        None => return Ok(None),
    };
    Ok(Some(InchesScan {
        inches,
        end: start + consumed + dquote_len,
        inch_ge_12,
    }))
}

/// After a `NUMBER '`, try the FTIN compound (grammar §3.3 R2).
pub fn try_scan_ftin(
    src: &str,
    after_prime: usize,
    feet_value: Ratio<i128>,
) -> Result<Option<FtInScan>, InchScanError> {
    let nl = src[after_prime..]
        .find('\n')
        .map(|i| after_prime + i)
        .unwrap_or(src.len());
    let slice = &src[after_prime..nl];
    let mut pos = 0usize;
    let mut spaced_hyphen = false;

    let ws_before = skip_ws(slice, &mut pos);

    if pos < slice.len() && slice.as_bytes()[pos] == b'-' {
        if ws_before {
            spaced_hyphen = true;
        }
        pos += 1;
        let ws_after = skip_ws(slice, &mut pos);
        if ws_after {
            spaced_hyphen = true;
        }
    }

    let inch_slice = &slice[pos..];
    let (inch_part, inch_consumed, _) = match parse_inch_val(inch_slice)? {
        Some(v) => v,
        None => return Ok(None),
    };
    pos += inch_consumed;

    let dquote_len = match peek_dquote(&slice[pos..]) {
        Some(len) => len,
        None => return Ok(None),
    };
    pos += dquote_len;

    let total = feet_value * Ratio::from_integer(12) + inch_part;

    Ok(Some(FtInScan {
        inches: total,
        end: after_prime + pos,
        spaced_hyphen,
        inch_part,
    }))
}

fn skip_ws(slice: &str, pos: &mut usize) -> bool {
    let start = *pos;
    while *pos < slice.len() && matches!(slice.as_bytes()[*pos], b' ' | b'\t') {
        *pos += 1;
    }
    *pos > start
}

/// Parse `inch_val` — mixed | frac | NUMBER (grammar §3.3).
fn parse_inch_val(slice: &str) -> Result<Option<(Ratio<i128>, usize, bool)>, InchScanError> {
    // mixed: INT (ws | "-") frac
    if let Some((whole, mut pos)) = parse_int_at(slice) {
        let sep_pos = pos;
        if pos < slice.len() {
            let b = slice.as_bytes()[pos];
            if b == b'-' {
                pos += 1;
                let ws = skip_ws(slice, &mut pos);
                if let Some((num, den, frac_len)) = parse_frac_at(&slice[pos..]) {
                    if den.is_zero() {
                        return Err(InchScanError::DivZero);
                    }
                    let total = Ratio::from_integer(whole) + Ratio::new(num, den);
                    let inch_ge_12 = total >= Ratio::from_integer(12);
                    return Ok(Some((total, pos + frac_len, inch_ge_12)));
                }
                let _ = ws;
            } else if matches!(b, b' ' | b'\t') {
                let mut p = pos;
                skip_ws(slice, &mut p);
                if let Some((num, den, frac_len)) = parse_frac_at(&slice[p..]) {
                    if den.is_zero() {
                        return Err(InchScanError::DivZero);
                    }
                    let total = Ratio::from_integer(whole) + Ratio::new(num, den);
                    let inch_ge_12 = total >= Ratio::from_integer(12);
                    return Ok(Some((total, p + frac_len, inch_ge_12)));
                }
            }
        }
        let _ = sep_pos;
    }

    // frac: INT "/" INT
    if let Some((num, den, len)) = parse_frac_at(slice) {
        if den.is_zero() {
            return Err(InchScanError::DivZero);
        }
        let val = Ratio::new(num, den);
        let inch_ge_12 = val >= Ratio::from_integer(12);
        return Ok(Some((val, len, inch_ge_12)));
    }

    // NUMBER
    match parse_number_at(slice) {
        Ok((val, len, _)) => {
            let inch_ge_12 = val >= Ratio::from_integer(12);
            Ok(Some((val, len, inch_ge_12)))
        }
        Err(NumberError::Invalid | NumberError::Empty) => Ok(None),
    }
}

fn parse_int_at(slice: &str) -> Option<(i128, usize)> {
    if slice.is_empty() || !slice.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    let mut pos = 0;
    let mut value: i128 = 0;
    let mut any = false;
    while pos < slice.len() {
        let b = slice.as_bytes()[pos];
        if b == b'_' {
            pos += 1;
            continue;
        }
        if !b.is_ascii_digit() {
            break;
        }
        any = true;
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as i128))?;
        pos += 1;
    }
    if !any {
        return None;
    }
    Some((value, pos))
}

fn parse_frac_at(slice: &str) -> Option<(i128, i128, usize)> {
    let (num, mut pos) = parse_int_at(slice)?;
    if pos >= slice.len() || slice.as_bytes()[pos] != b'/' {
        return None;
    }
    pos += 1;
    let (den, den_len) = parse_int_at(&slice[pos..])?;
    pos += den_len;
    Some((num, den, pos))
}

fn peek_dquote(slice: &str) -> Option<usize> {
    if slice.is_empty() {
        return None;
    }
    let ch = slice.chars().next()?;
    if ch == '"' || ch == '\u{2033}' {
        Some(ch.len_utf8())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::ToPrimitive;

    #[test]
    fn inches_frac() {
        let r = try_scan_inches_literal(r#"1/2""#, 0).unwrap().unwrap();
        assert_eq!(r.inches, Ratio::new(1, 2));
    }

    #[test]
    fn inches_mixed() {
        let r = try_scan_inches_literal(r#"6 1/2""#, 0).unwrap().unwrap();
        assert_eq!(r.inches, Ratio::new(13, 2));
    }

    #[test]
    fn ftin_basic() {
        let src = r#"12'-6""#;
        let (feet, num_len, _) = parse_number_at("12").unwrap();
        let after_prime = num_len + 1; // skip ASCII '
        let r = try_scan_ftin(src, after_prime, feet).unwrap().unwrap();
        assert_eq!(r.inches.to_i128(), Some(150));
    }
}
