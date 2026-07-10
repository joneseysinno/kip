//! UnitExpr algebraic simplification: collect like-named terms, cancel,
//! and rebuild a canonical numerator/denominator form.

use indexmap::IndexMap;
use num_rational::Ratio;
use num_traits::Zero;

use crate::quantity::{UnitExpr, UnitExponent};

/// Simplify a `UnitExpr` by collecting all named-unit exponents, summing them,
/// cancelling zero-exponent terms, and rebuilding a canonical
/// `numer / denom` form.
///
/// This is purely syntactic — it never touches `Dimension` or the registry.
/// Invariant: the returned `UnitExpr` is dimensionally identical to the input.
pub fn simplify_unit_expr(expr: &UnitExpr) -> UnitExpr {
    let mut map: IndexMap<String, Ratio<i32>> = IndexMap::new();
    collect(expr, Ratio::from_integer(1), &mut map);
    map.retain(|_, e| !e.is_zero());

    if map.is_empty() {
        return UnitExpr::Dimensionless;
    }

    let mut numer: Vec<(String, Ratio<i32>)> = map
        .iter()
        .filter(|(_, e)| **e > Ratio::zero())
        .map(|(n, e)| (n.clone(), *e))
        .collect();
    let mut denom: Vec<(String, Ratio<i32>)> = map
        .iter()
        .filter(|(_, e)| **e < Ratio::zero())
        .map(|(n, e)| (n.clone(), -*e)) // flip sign — will go in denominator
        .collect();

    // Stable sort so output is deterministic.
    numer.sort_by(|a, b| a.0.cmp(&b.0));
    denom.sort_by(|a, b| a.0.cmp(&b.0));

    let numer_expr = if numer.is_empty() {
        UnitExpr::Dimensionless
    } else {
        build_product(numer)
    };
    if denom.is_empty() {
        numer_expr
    } else {
        UnitExpr::Quotient(Box::new(numer_expr), Box::new(build_product(denom)))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Recursively walk `expr`, accumulating `unit_name → net_exponent` into `map`.
/// `scale` carries the effective exponent multiplier from enclosing `Pow` nodes.
fn collect(expr: &UnitExpr, scale: Ratio<i32>, map: &mut IndexMap<String, Ratio<i32>>) {
    match expr {
        UnitExpr::Named(name) => {
            *map.entry(name.clone()).or_insert_with(Ratio::zero) += scale;
        }
        UnitExpr::Dimensionless => {}
        UnitExpr::Product(parts) => {
            for p in parts {
                collect(p, scale, map);
            }
        }
        UnitExpr::Quotient(num, den) => {
            collect(num, scale, map);
            collect(den, -scale, map); // denominator inverts exponents
        }
        UnitExpr::Pow { base, exp } => {
            let e = unit_exponent_to_ratio(exp);
            collect(base, scale * e, map);
        }
    }
}

/// Convert a `UnitExponent` to a `Ratio<i32>` for arithmetic.
/// Decimal exponents that don't parse cleanly default to 1 (should not occur
/// in practice for unit expressions built by the evaluator).
fn unit_exponent_to_ratio(exp: &UnitExponent) -> Ratio<i32> {
    match exp {
        UnitExponent::Int(n) => Ratio::from_integer(*n),
        UnitExponent::Ratio { num, den } => Ratio::new(*num, *den),
        UnitExponent::Decimal(s) => s
            .parse::<f64>()
            .ok()
            .and_then(|f| {
                // Only exact halves appear in engineering unit exponents.
                if (f - f.round()).abs() < 1e-9 {
                    Some(Ratio::from_integer(f.round() as i32))
                } else if (f * 2.0 - (f * 2.0).round()).abs() < 1e-9 {
                    Some(Ratio::new((f * 2.0).round() as i32, 2))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| Ratio::from_integer(1)),
    }
}

/// Build a `UnitExpr` from a list of `(name, positive_exponent)` pairs.
/// Assumes all exponents are > 0 (denominator pairs have already been flipped
/// by the caller).
fn build_product(terms: Vec<(String, Ratio<i32>)>) -> UnitExpr {
    let parts: Vec<UnitExpr> = terms
        .into_iter()
        .map(|(name, exp)| {
            if exp == Ratio::from_integer(1) {
                UnitExpr::Named(name)
            } else if *exp.denom() == 1 {
                UnitExpr::Pow {
                    base: Box::new(UnitExpr::Named(name)),
                    exp: UnitExponent::Int(*exp.numer()),
                }
            } else {
                UnitExpr::Pow {
                    base: Box::new(UnitExpr::Named(name)),
                    exp: UnitExponent::Ratio {
                        num: *exp.numer(),
                        den: *exp.denom(),
                    },
                }
            }
        })
        .collect();

    if parts.len() == 1 {
        parts.into_iter().next().unwrap()
    } else {
        UnitExpr::Product(parts)
    }
}
