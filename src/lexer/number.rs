//! Decimal and scientific number parsing (grammar-spec §3.1).

use num_rational::Ratio;
use num_traits::One;

/// Number parse failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberError {
    /// Not a valid number at this position.
    Invalid,
    /// Empty or malformed.
    Empty,
}

/// Parse a tight `NUMBER` at the start of `slice`.
///
/// Returns `(value, consumed_bytes, had_underscore)`.
pub fn parse_number_at(slice: &str) -> Result<(Ratio<i128>, usize, bool), NumberError> {
    if slice.is_empty() {
        return Err(NumberError::Empty);
    }
    let b0 = slice.as_bytes()[0];
    if !b0.is_ascii_digit() && b0 != b'.' {
        return Err(NumberError::Invalid);
    }

    let mut pos = 0;
    let mut had_underscore = false;
    let bytes = slice.as_bytes();

    // INT or leading DECIMAL
    if b0 == b'.' {
        if bytes.len() < 2 || !bytes[1].is_ascii_digit() {
            return Err(NumberError::Invalid);
        }
    } else {
        while pos < bytes.len() {
            let b = bytes[pos];
            if b == b'_' {
                had_underscore = true;
                pos += 1;
                continue;
            }
            if !b.is_ascii_digit() {
                break;
            }
            pos += 1;
        }
    }

    // optional fraction
    if pos < bytes.len() && bytes[pos] == b'.' {
        pos += 1;
        let frac_start = pos;
        while pos < bytes.len() {
            let b = bytes[pos];
            if b == b'_' {
                had_underscore = true;
                pos += 1;
                continue;
            }
            if !b.is_ascii_digit() {
                break;
            }
            pos += 1;
        }
        if pos == frac_start {
            return Err(NumberError::Invalid);
        }
    }

    // tight SCI — no whitespace
    let mut exp: i32 = 0;
    let mut has_exp = false;
    if pos < bytes.len() && (bytes[pos] == b'e' || bytes[pos] == b'E') {
        has_exp = true;
        pos += 1;
        let mut sign: i32 = 1;
        if pos < bytes.len() && (bytes[pos] == b'+' || bytes[pos] == b'-') {
            if bytes[pos] == b'-' {
                sign = -1;
            }
            pos += 1;
        }
        let exp_start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == exp_start {
            return Err(NumberError::Invalid);
        }
        exp = sign
            * slice[exp_start..pos]
                .parse::<i32>()
                .map_err(|_| NumberError::Invalid)?;
    }

    if pos == 0 {
        return Err(NumberError::Invalid);
    }

    let text: String = slice[..pos]
        .chars()
        .filter(|c| *c != '_')
        .collect();

    let value = decimal_sci_to_rational(&text, has_exp, exp)?;
    Ok((value, pos, had_underscore))
}

fn decimal_sci_to_rational(text: &str, has_exp: bool, exp: i32) -> Result<Ratio<i128>, NumberError> {
    let (mantissa, exponent) = if let Some((m, e)) = text.split_once(['e', 'E']) {
        let e: i32 = e.parse().map_err(|_| NumberError::Invalid)?;
        (m, e)
    } else if has_exp {
        return Err(NumberError::Invalid);
    } else {
        (text, exp)
    };

    let mut ratio = parse_decimal_mantissa(mantissa)?;
    if exponent != 0 {
        let ten = Ratio::from_integer(10);
        let factor = ratio_pow_i32(ten, exponent.abs())?;
        ratio = if exponent.is_negative() {
            ratio / factor
        } else {
            ratio * factor
        };
    }
    Ok(ratio)
}

fn ratio_pow_i32(base: Ratio<i128>, exp: i32) -> Result<Ratio<i128>, NumberError> {
    if exp == 0 {
        return Ok(Ratio::one());
    }
    let mut result = Ratio::one();
    let mut b = base;
    let mut e = exp;
    while e > 0 {
        if e % 2 == 1 {
            result *= b;
        }
        b *= b;
        e /= 2;
    }
    Ok(result)
}

fn parse_decimal_mantissa(s: &str) -> Result<Ratio<i128>, NumberError> {
    if let Some((int_part, frac_part)) = s.split_once('.') {
        let int_s = if int_part.is_empty() { "0" } else { int_part };
        let int_val: i128 = int_s.parse().map_err(|_| NumberError::Invalid)?;
        if frac_part.is_empty() {
            return Ok(Ratio::from_integer(int_val));
        }
        let mut denom = 1i128;
        for _ in frac_part.chars() {
            denom = denom
                .checked_mul(10)
                .ok_or(NumberError::Invalid)?;
        }
        let frac_val: i128 = frac_part.parse().map_err(|_| NumberError::Invalid)?;
        let numer = int_val
            .checked_mul(denom)
            .and_then(|v| v.checked_add(frac_val))
            .ok_or(NumberError::Invalid)?;
        Ok(Ratio::new(numer, denom))
    } else {
        let int_val: i128 = s.parse().map_err(|_| NumberError::Invalid)?;
        Ok(Ratio::from_integer(int_val))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_traits::ToPrimitive;

    #[test]
    fn parses_int_and_sci() {
        let (v, n, _) = parse_number_at("1e3").unwrap();
        assert_eq!(n, 3);
        assert_eq!(v.to_i128(), Some(1000));
    }

    #[test]
    fn parses_decimal() {
        let (v, _, _) = parse_number_at(".5").unwrap();
        assert_eq!(v, Ratio::new(1, 2));
    }

    #[test]
    fn parses_underscores() {
        let (v, _, u) = parse_number_at("29_000").unwrap();
        assert!(u);
        assert_eq!(v.to_i128(), Some(29000));
    }
}
