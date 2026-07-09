//! Display formatting: preferred units, precision, ft-in snapping (M7).

pub mod ftin;

use crate::eval::value::Quantity;

/// Formatting options for quantity display.
#[derive(Debug, Clone)]
pub struct FmtOptions {
    /// Decimal places for float magnitudes.
    pub precision: usize,
    /// Prefer ft-in compound display for lengths.
    pub prefer_ft_in: bool,
    /// Ft-in denominator for snapping (e.g. 16 for sixteenths).
    pub ft_in_denominator: u32,
}

impl Default for FmtOptions {
    fn default() -> Self {
        Self {
            precision: 6,
            prefer_ft_in: false,
            ft_in_denominator: 16,
        }
    }
}

/// Format a quantity per options (M7 expands; M0 is minimal).
pub fn format_quantity(q: &Quantity, opts: &FmtOptions) -> String {
    if q.is_exact() {
        format!(
            "{} {} {}",
            q.magnitude.numer(),
            if q.magnitude.denom() != &1i128 {
                format!("/{}", q.magnitude.denom())
            } else {
                String::new()
            },
            q.unit.as_str()
        )
        .trim()
        .to_string()
    } else {
        format!("{:.prec$} {}", q.as_f64(), q.unit.as_str(), prec = opts.precision)
    }
}
