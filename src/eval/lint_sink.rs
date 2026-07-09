//! Eval-time lint collection (R3).

use crate::diag::{Diag, Diagnostic, LintCode, Span};
use crate::eval::mag::TaintEvent;

/// Collects evaluation lints with de-duplication policy for exactness loss.
#[derive(Debug, Default)]
pub struct LintSink {
    lints: Vec<Diag>,
    exactness_lost: bool,
}

impl LintSink {
    /// Empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow collected lints.
    pub fn lints(&self) -> &[Diag] {
        &self.lints
    }

    /// Take collected lints.
    pub fn into_lints(self) -> Vec<Diag> {
        self.lints
    }

    /// Append lints from a subtree (parallel merge: left then right).
    pub fn extend(&mut self, other: &mut Self) {
        self.lints.append(&mut other.lints);
        self.exactness_lost |= other.exactness_lost;
    }

    /// Record a taint event from a `Mag` operation.
    pub fn record_mag_event(&mut self, event: TaintEvent, span: Span, message: impl Into<String>) {
        match event {
            TaintEvent::ExactnessLost => {
                if !self.exactness_lost {
                    self.exactness_lost = true;
                    self.lints.push(Diag::new(Diagnostic::lint(
                        LintCode::ExactnessLost,
                        message,
                        span,
                    )));
                }
            }
            TaintEvent::RationalOverflow => {
                self.lints.push(Diag::new(Diagnostic::lint(
                    LintCode::RationalOverflow,
                    message,
                    span,
                )));
            }
        }
    }

    /// Record an arbitrary lint.
    pub fn push(&mut self, lint: Diag) {
        if lint.diagnostic().code == LintCode::ExactnessLost.as_str() {
            if self.exactness_lost {
                return;
            }
            self.exactness_lost = true;
        }
        self.lints.push(lint);
    }
}
