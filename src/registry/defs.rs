//! `define` / `dimension` / `anchor` text parser (grammar §6) — M2.

use crate::diag::{Diag, Diagnostic, ErrorCode, Span};
use crate::registry::RegistryBuilder;

/// Parse definition lines and apply to the builder.
///
/// M0 accepts only empty input; full order-free resolution lands in M2.
pub fn parse_defs(builder: &mut RegistryBuilder, src: &str) -> Result<(), Diag> {
    for (line_no, line) in src.lines().enumerate() {
        let trimmed = line.split('#').next().unwrap_or(line).trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("define ")
            || trimmed.starts_with("dimension ")
            || trimmed.starts_with("anchor ")
        {
            return Err(Diag::new(Diagnostic::error(
                ErrorCode::Parse,
                format!(
                    "registry definition parsing not yet implemented (M2); line {}",
                    line_no + 1
                ),
                Span::empty(0),
            )));
        }
        return Err(Diag::new(Diagnostic::error(
            ErrorCode::Parse,
            format!("unrecognized definition line: `{trimmed}`"),
            Span::empty(0),
        )));
    }
    let _ = builder;
    Ok(())
}
