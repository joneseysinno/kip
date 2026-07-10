# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.1.0] - 2026-07-09

First public release. All milestones M0–M8 complete.

### Added

- **Lexer (M1)** — grammar §3–§4: numbers, identifiers/primes, FTIN compounds, Unicode tick aliases, diagnostics, `fuzz_lexer`.
- **Registry (M2)** — order-free `define` / `dimension` / `anchor`, imperial seed data, `dump_defs`, anchor rebase.
- **Parser (M3)** — Pratt parser, unit attachment, code-equation paths, `fuzz_parser`.
- **Evaluator (M4)** — exact rational eval, leftmost-wins unification, builtins, affine temperatures.
- **Partial eval (M5)** — `Symbolic` residuals, `ConstraintSet`, `Value::bind`, eager folding.
- **Equation packs (M6)** — TOML loader, contracts, auto-dimensionalization, `ACI.fr` demo pack, provenance.
- **Parallel + fmt (M7)** — `rayon` intra-expression join, `eval_batch`, `eval_scenarios`, `FmtOptions`, ft-in display.
- **Hardening (M8)** — criterion benches, proptest properties, fuzz corpus, anchor-invariance eval tests, loom CI.

### API surface

- `lex`, `parse`, `eval`, `Registry`, `RegistryBuilder`, `Resolver`, `Value`, `Quantity`, `Diag`.
- Optional features: `packs` (default), `parallel` (default), `si-display` (stub).

### CI

- `cargo test --all-features`, clippy `-D warnings`, Miri, loom models, fuzz smoke, bench compile.

[Unreleased]: https://github.com/joneseysinno/kip/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/joneseysinno/kip/releases/tag/v0.1.0
