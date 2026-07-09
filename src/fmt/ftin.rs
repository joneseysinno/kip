//! Ft-in rendering with denominator snapping (M7).

use num_rational::Ratio;
use num_traits::{Signed, Zero};

/// Render a length in inches as `ft'-in` with optional fractional snapping.
pub fn render_inches(total_inches: f64, denominator: u32) -> String {
    if total_inches.abs() < f64::EPSILON {
        return "0'".into();
    }
    let denom = denominator.max(1);
    let sign = if total_inches < 0.0 { "-" } else { "" };
    let abs = total_inches.abs();

    let mut feet = (abs / 12.0).floor();
    let inch_total = abs - feet * 12.0;

    let mut inch_whole = inch_total.floor();
    let frac = inch_total - inch_whole;
    let mut snap = (frac * f64::from(denom)).round() as u32;

    if snap >= denom {
        inch_whole += 1.0;
        snap = 0;
    }
    if inch_whole >= 12.0 {
        feet += (inch_whole / 12.0).floor();
        inch_whole %= 12.0;
    }

    let feet_i = feet as i64;
    let inch_i = inch_whole as i64;

    let mut out = String::new();
    out.push_str(sign);
    if feet_i != 0 || (inch_i == 0 && snap == 0) {
        out.push_str(&format!("{feet_i}'"));
    }
    if snap == 0 {
        if inch_i != 0 || feet_i == 0 {
            out.push_str(&format!("{inch_i}\""));
        }
    } else if inch_i != 0 {
        out.push_str(&format!("{inch_i} {snap}/{denom}\""));
    } else {
        out.push_str(&format!("{snap}/{denom}\""));
    }
    if out == sign {
        out.push_str("0'");
    }
    out
}

/// Exact rational inch rendering (no float) when the inch remainder is on-denom.
pub fn render_inches_exact(total_inches: Ratio<i128>, denominator: u32) -> Option<String> {
    let denom = i128::from(denominator.max(1));
    if total_inches.is_zero() {
        return Some("0'".into());
    }
    let neg = total_inches.is_negative();
    let abs = total_inches.abs();
    let twelve = Ratio::from_integer(12);
    let feet_i = (abs / twelve).to_integer();
    let feet = Ratio::from_integer(feet_i);
    let inch_rem = abs - feet * twelve;
    if inch_rem.denom() != &1 && inch_rem.denom() != &denom {
        return None;
    }
    let inch_whole = inch_rem.to_integer();
    let frac_num = if inch_rem.denom() == &denom {
        *inch_rem.numer() % denom
    } else {
        0
    };
    let sign = if neg { "-" } else { "" };
    let mut out = String::new();
    out.push_str(sign);
    if feet_i != 0 || (inch_whole == 0 && frac_num == 0) {
        out.push_str(&format!("{feet_i}'"));
    }
    if frac_num == 0 {
        if inch_whole != 0 || feet_i == 0 {
            out.push_str(&format!("{inch_whole}\""));
        }
    } else if inch_whole != 0 {
        out.push_str(&format!("{inch_whole} {frac_num}/{denom}\""));
    } else {
        out.push_str(&format!("{frac_num}/{denom}\""));
    }
    Some(out)
}
