//! Display formatting: preferred units, precision, ft-in snapping (M7).

pub mod ftin;

use num_rational::Ratio;
use num_traits::{One, Zero};

use crate::dim::BaseDim;
use crate::eval::mag::Mag;
use crate::eval::units::magnitude_in_anchor_units;
use crate::eval::value::Quantity;
use crate::quantity::UnitExpr;
use crate::registry::Registry;

/// Formatting options for quantity display.
#[derive(Debug, Clone)]
pub struct FmtOptions {
    /// Decimal places for float magnitudes.
    pub precision: usize,
    /// Prefer ft-in compound display for lengths.
    pub prefer_ft_in: bool,
    /// Ft-in denominator for snapping (e.g. 16 for sixteenths).
    pub ft_in_denominator: u32,
    /// Convert to this unit before display when set.
    pub preferred_unit: Option<String>,
}

impl Default for FmtOptions {
    fn default() -> Self {
        Self {
            precision: 6,
            prefer_ft_in: false,
            ft_in_denominator: 16,
            preferred_unit: None,
        }
    }
}

impl FmtOptions {
    /// Options for engineering calc sheets: ft-in lengths, sixteenths.
    pub fn calc_sheet() -> Self {
        Self {
            precision: 4,
            prefer_ft_in: true,
            ft_in_denominator: 16,
            preferred_unit: None,
        }
    }
}

/// Format a quantity per options (never mutates the input quantity).
pub fn format_quantity(q: &Quantity, registry: &Registry, opts: &FmtOptions) -> String {
    let display = if let Some(pref) = &opts.preferred_unit {
        q.convert_to(&UnitExpr::named(pref), registry)
            .unwrap_or_else(|_| q.clone())
    } else {
        q.clone()
    };

    if opts.prefer_ft_in && is_length(&display.dim) {
        if let Ok(s) = format_length_ft_in(&display, registry, opts) {
            return s;
        }
    }

    format_magnitude_unit(&display, opts)
}

fn is_length(dim: &crate::dim::Dimension) -> bool {
    dim == &crate::dim::Dimension::single(BaseDim::Length, Ratio::one())
}

fn format_length_ft_in(
    q: &Quantity,
    registry: &Registry,
    opts: &FmtOptions,
) -> Result<String, crate::Diag> {
    let inches_unit = UnitExpr::named("in");
    let in_q = q.convert_to(&inches_unit, registry)?;
    if in_q.is_exact() {
        if let Some(r) = in_q.exact_ratio() {
            if let Some(s) = ftin::render_inches_exact(r, opts.ft_in_denominator) {
                return Ok(s);
            }
        }
    }
    let anchor = magnitude_in_anchor_units(&in_q, registry)?;
    let total = anchor.as_f64();
    Ok(ftin::render_inches(total, opts.ft_in_denominator))
}

fn format_magnitude_unit(q: &Quantity, opts: &FmtOptions) -> String {
    let unit = format!("{}", q.unit);
    match q.mag {
        Mag::Float(f) => format!("{f:.prec$} {unit}", prec = opts.precision),
        Mag::Exact(r) => {
            if r.is_zero() {
                format!("0 {unit}")
            } else if r.denom() == &1i128 {
                format!("{} {unit}", r.numer())
            } else {
                format!("{}/{} {unit}", r.numer(), r.denom())
            }
        }
    }
}
