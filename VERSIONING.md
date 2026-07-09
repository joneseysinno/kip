# Versioning policy

kip follows [Semantic Versioning](https://semver.org/) for public releases on [crates.io](https://crates.io/crates/kip).

## What triggers a major bump (breaking)

- **Grammar changes** — lexer, parser, or expression semantics described in `grammar-spec.md`.
- **Diagnostic code meaning changes** — stable `E-*` / `L-*` strings that change severity or when they fire.
- **Public API removals or signature changes** on stable types and functions.

## What triggers a minor bump (compatible)

- **Registry seed data changes** — new built-in units, anchor defaults, or exact rational ratio adjustments.
- **New equation-pack format fields** with backward-compatible defaults.
- **New optional features**, builtins, or diagnostics that do not alter existing behavior.
- **Performance improvements** that preserve evaluation results.

## What triggers a patch bump

- Bug fixes with no seed or grammar impact.
- Documentation and test-only changes.

## Pre-1.0 note

While at `0.y.z`, minor versions may still carry small incompatible API adjustments; grammar and seed-data rules above apply once we reach `1.0.0`.
