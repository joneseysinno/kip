# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added (M1)

- Full lexer per `grammar-spec.md` §3–§4: numbers, identifiers/primes, `FEET`/`INCHES`/`FTIN` with bounded lookahead and clean backtrack.
- Unicode tick aliases (`′` `″`), unit-expression `·`/`×`, digit separators, tight scientific notation.
- Lexer diagnostics: `E-TICK-SPACE`, `E-BARE-TICK`, `E-DIV-ZERO-LITERAL`, `L-FTIN-SPACED`, `L-INCH-GE-12`, `L-COMMA-GROUP`.
- `lex()` and `lex_checked()` public API; 42 lexer conformance tests; `fuzz/fuzz_lexer` target.

## [0.1.0] - 2026-07-09

### Added

- **M0 skeleton** per `plan.md`: crate layout, module structure, and public API contracts.
- Core types: `Dimension`, `Quantity`, `Value`, `ConstraintSet`, `UnitExpr`.
- Diagnostics inventory: `ErrorCode`, `LintCode`, `Diagnostic`, `Span`.
- `Registry` / `RegistryBuilder` with generation-0 imperial seed data (anchors, derived units, affine temp stubs).
- `Resolver` trait with `EmptyResolver` and `MapResolver`.
- Rational arithmetic overflow policy with `L-RATIONAL-OVERFLOW` lint path.
- `Send + Sync` static assertions for public types.
- CI: `cargo test`, `clippy -D warnings`, Miri nightly job.
- Stub APIs for lexer, parser, eval, packs, and parallel helpers (return structured `Diag` until their milestones land).

### Not yet implemented

- M1 lexer, M2 full registry defs, M3 parser, M4 evaluator, M5 partial eval, M6 equation packs, M7 parallel/fmt.
