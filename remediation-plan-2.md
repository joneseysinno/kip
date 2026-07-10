# Remediation Plan 2 — `kip` builtins exactness hardening

**Status:** Draft 1 · Companion to `remediation-plan.md` (R-series), `plan.md`, and `grammar-spec.md` (Draft 2)
**Scope:** Fixes for the defects found in the pre-publish review of the R0–R6 remediation: the float-based `integer_sqrt` (overflow + missed exactness), always-float `floor`/`ceil`/`round`, f64 comparison in `min`/`max`, the lint-coverage gap across the trig family, the `Mag`-laundering return of `magnitude_in_anchor_units` (plus an unchecked `Ratio` multiply discovered at the same site), and the unfinished half of R6. Stage numbers are S0–S6 to avoid collision with the R-series.

No stage adds a diagnostic code; grammar-spec §9 is untouched. One stage adds documented semantics (rounding tie policy) to plan §2.2. **All stages land before the crates.io 0.1.0 publish** — the CHANGELOG's `[0.1.0] - 2026-07-09` entry is staged, not shipped (verified: `kip` does not exist on crates.io as of this draft), so the API and `builtins.rs` internals can still change at zero semver cost. Refresh the release date when the gate clears.

---

## 0. Root-cause summary

| ID | Defect | Severity | Root cause |
|---|---|---|---|
| **N1** | `integer_sqrt` uses `(n as f64).sqrt().round()` + unchecked `root * root`: overflow panic/wrap near `i128::MAX`; true perfect squares above ~2^106 silently fall to float | **Blocker** — violates "checked arithmetic everywhere; panics are conformance failures" and the exactness promise | R4 deviated from its own spec (`i128::isqrt`) |
| **N2** | `eval_rounding` goes through `as_f64()` unconditionally: exact input → float output with **no lint**, and wrong integers for exacts beyond 2^53 | **Blocker** — silent exactness loss + wrong answers; F1-class in spirit | Builtins were outside the R0 taint property's reach |
| **N3** | `eval_min_max` compares via `as_f64()`: exact operands closer than f64 resolution tie, accumulator keeps the wrong one | **High** — wrong selection at scale | Same blind spot; `mag_cmp` existed but wasn't used |
| **N4** | `eval_trig` / `eval_inverse_trig` / `eval_atan2` produce `Float` from `Exact` inputs with no `L-EXACTNESS-LOST`, while `sqrt` and the transcendentals emit it | **Medium** — spec §9 defines the lint as "first transition from exact rational to float"; these are transitions | Lint wiring (R3) stopped at the functions R4 touched |
| **N5** | `magnitude_in_anchor_units` launders `Mag::Float` through `f64_to_ratio_approx` into a `Ratio<i128>` that looks exact; **and** its `Exact` branch does an unchecked `r * factor` `Ratio` multiply (panic on overflow) | **Medium** — no active bug at current call sites, but it is the sentinel pattern's ghost, plus one live unchecked-arithmetic violation | Function predates R0 and was never migrated to return `Mag` |
| **N6** | R6 remainder: did-you-mean hint on unknown pack args missing; `parse_defs` CRLF/indent span fix and `eval_ten_thousand_term_sum` rename unverified | **Low** | R6 landed partially |

The unifying root cause: **the R0 taint-propagation property covers binary ops, not builtins.** N1's silent fallback, N2, and N4 are one class, and one property kills the class. That property goes first.

---

## 1. S0 — Builtin exactness property (the class-killer)

**Fixes:** nothing by itself; **prevents** N1/N2/N4 recurring and any future builtin repeating them.
**Touches:** `tests/` only (new `builtin_exactness.rs` or rows in `remediation_conformance.rs`).

### 1.1 The property

For every builtin, for arbitrary dimension-valid `Exact` inputs, evaluate via `eval_checked` and assert:

> the result is `Known` with `q.is_exact()`, **or** the lint stream contains `L-EXACTNESS-LOST` / `L-RATIONAL-OVERFLOW`.

Plus the NaN-hygiene companion (should already hold via `Mag::float`, assert anyway): no builtin ever returns a value whose `as_f64()` is non-finite; non-finite intermediates are `E-EVAL` at the op site.

### 1.2 Strategy design (this is where the value is)

Proptest's default rational strategies will never reach the regimes where N1 and N2 actually break. Use a custom `Mag`-strategy that mixes, with explicit weights:

- small integers and simple fractions (the happy path),
- the **2^53 neighborhood** (f64 integer-precision boundary — discriminates N2),
- the **`i128::isqrt(i128::MAX)` neighborhood** (~1.3 × 10^19) and squares thereof (discriminates N1's overflow and missed-exactness modes),
- negatives and zero.

These regimes also become fixed regression rows, so the property's coverage doesn't depend on proptest's RNG in CI.

### 1.3 Landing mechanics

Land the property **first, red**, with a `const EXPECTED_VIOLATIONS: &[&str]` exception table naming the currently failing builtins (`floor`, `ceil`, `round`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, plus the large-square `sqrt` rows). Each subsequent stage deletes its entries. The final commit of this plan asserts the table is empty and removes it. This is the S-series analogue of R0's "let the compiler find the sites": let the property enumerate the debt, then burn it down visibly.

**Effort:** ~half a day. The strategy design is the deliverable; the assertion is ten lines.

---

## 2. S1 — `integer_sqrt` via `i128::isqrt` (N1)

**Touches:** `eval/builtins.rs` (one function), `Cargo.toml` (MSRV).

Replace the float round-trip:

```rust
fn integer_sqrt(n: i128) -> Option<i128> {
    if n < 0 { return None; }
    let root = n.isqrt();                       // floors, exact, no f64
    (root.checked_mul(root)? == n).then_some(root)
}
```

`i128::isqrt` floors by construction, so the `checked_mul` verification is exact and cannot overflow into UB-adjacent territory — if `root * root` would overflow, `checked_mul` returns `None` and the value correctly falls to float with the existing `L-EXACTNESS-LOST` path. (In fact `isqrt(n)² ≤ n ≤ i128::MAX` always, so the `?` is belt-and-braces; keep it — checked everywhere.)

**MSRV decision required:** `i128::isqrt` stabilized in Rust 1.84. Set `rust-version = "1.84"` in `Cargo.toml`. Pre-1.0 this costs nothing; the alternative (hand-rolled Newton iteration) is more code to get wrong for zero benefit. If a lower MSRV ever becomes a hard requirement, revisit then.

Gate tests:

- `sqrt(10^38)` → exact `10^19` (large perfect square the f64 path missed).
- `sqrt` of a `Ratio` with both numerator and denominator large perfect squares → exact.
- `sqrt(i128::MAX as literal)` → `Float` + `L-EXACTNESS-LOST`, **no panic** — run under both debug and release profiles in CI (the old bug's behavior differed between them; the row stays as a permanent tombstone).
- Property (folds into S0): for random `k ≤ isqrt(i128::MAX)`, including the top of the range, `sqrt(k²)` is exact and equals `k`. Delete the sqrt rows from `EXPECTED_VIOLATIONS`.

**Effort:** ~1 hour + CI profile plumbing.

---

## 3. S2 — Exact `floor`/`ceil`/`round` (N2)

**Touches:** `eval/builtins.rs` (`eval_rounding`), plan §2.2 (tie-policy doc).

New behavior, matching on `Mag`:

- `Mag::Exact(r)` → `Ratio::floor` / `Ratio::ceil` / `Ratio::round`, result stays **`Exact`**, **no lint** (nothing was lost — the result is an exact integer rational).
- `Mag::Float(f)` → f64 path unchanged, **no lint** (input already tainted; no *transition* occurs).

Note the pleasant consequence: after S2, `eval_rounding` needs no lint sink at all, and drops out of N4's scope entirely.

**Tie policy decision (make it explicit, then it's free):** `Ratio::round` rounds half-away-from-zero — which is also exactly what `f64::round` does. So the exact and float paths agree by default, and the policy is: **`round` is half-away-from-zero on both paths.** Document it in the `round` rustdoc and plan §2.2; add `round(5/2) = 3` and `round(-5/2) = -3` as conformance rows so the choice can never drift silently. (Banker's rounding is a plausible future builtin — `round_even` — not a change to `round`.)

Gate tests:

- `floor(7/2 ft)` → exact `3 ft`, unit preserved as written, zero lints.
- **The discriminator:** `round((2^60) + 1/2)` → exact, correct integer. The f64 path cannot represent this input; this row fails on the old code and is the permanent proof the detour is gone.
- `ceil(-3/2)` → exact `-1`; the tie rows above.
- `floor` of a `Float` input → `Float`, zero lints.
- Delete `floor`/`ceil`/`round` from `EXPECTED_VIOLATIONS`.

**Effort:** ~half a day, mostly tests and the doc sentence.

---

## 4. S3 — Lint wiring for the trig family (N4)

**Touches:** `eval/builtins.rs` (`eval_trig`, `eval_inverse_trig`, `eval_atan2` — thread the `&mut LintSink` already in scope at the dispatch site).

Emit `TaintEvent::ExactnessLost` at the call-site span, message naming the function (`"`sin` produced an inexact result"`), when the transition actually happens:

- Unary trig / inverse trig: input `is_exact()` → record.
- `atan2`: record iff **both** inputs exact (mirrors `Mag::add`'s rule — a `Float` operand means exactness was already lost upstream, and the sink's first-flip de-dup already recorded it there).
- One subtlety to pin in a test: for `sin(30 deg)`, the flip technically happens inside the degrees→radians conversion (π), before the `sin` itself. The recorded site is the **builtin call span** and the message names the builtin, not the internal conversion — that's what the user wrote and what they can act on. Assert the span in the gate test so this doesn't regress into two lints or a conversion-internal span.

Deliberately out of scope (record in plan §2.2's deferred notes): exact special values (`sin(30°) = 1/2`, `tan(45°) = 1`, `atan2(0, 1) = 0`). That is an exact-special-angle-table *feature*, not remediation; conflating it here would delay the publish gate.

Gate tests: `sin(30 deg)` → `Float` + exactly one `L-EXACTNESS-LOST` at the call span; `sin` of a `Float` input → zero new lints; `atan2(exact, float)` → zero new lints; `atan2(exact, exact)` → one. Delete the trig rows from `EXPECTED_VIOLATIONS`.

**Effort:** ~half a day.

---

## 5. S4 — `min`/`max` via `mag_cmp` (N3)

**Touches:** `eval/builtins.rs` (`eval_min_max`).

Replace the `as_f64()` comparisons with `mag_cmp` (exact-exact compares rationally; mixed drops to f64 per the established R0 rule). Leftmost-wins conversion of the RHS into the accumulator's unit is unchanged — and since exact conversion preserves exactness, exact-vs-exact comparisons are fully rational end to end.

`mag_cmp` returns `Option<Ordering>`; `None` is unreachable because non-finite `Mag::Float` is unrepresentable since R0. Do **not** `unwrap` — map `None` to an `E-EVAL` internal error ("non-comparable magnitudes"), consistent with the panics-are-conformance-failures rule. Unreachable errors are cheap; unreachable panics are not.

Gate tests:

- Two exact operands at magnitude ~10^20 differing by 10^-30 (identical in f64) → `min` returns the truly smaller, `max` the truly larger.
- Identity property (S0 harness): `min`/`max` over exact args returns one of the args **bit-identical** (same `Mag`, same as-written unit — leftmost's unit for converted picks), and agrees with rational ordering.

**Effort:** ~1 hour.

---

## 6. S5 — `magnitude_in_anchor_units` returns `Mag` (N5)

**Touches:** `eval/units.rs` (signature + the unchecked multiply), `fmt.rs`, `convert_quantity`'s `Exact` branch.

Two defects at one site:

1. **The sentinel ghost.** The `Float` branch returns `f64_to_ratio_approx(...)` as a bare `Ratio<i128>` — indistinguishable from exact to any caller. Current call sites happen to be safe; the signature is the hazard. Change the return type to carry taint: `Result<MagOpResult, Diag>` (anchor-space magnitude plus any event), or `Result<Mag, Diag>` with the caller-side event handled as below. The `Float` branch returns `Mag::Float(anchor_f)` — the approximation-to-ratio conversion is deleted, and `f64_to_ratio_approx` loses its last caller (delete it too, R0-style: the symbol should not exist in the tree).
2. **The unchecked multiply.** The `Exact` branch's `r * factor` is a raw `Ratio` multiply — panic on overflow, a live violation of checked-everywhere that survived R0 because this function was never migrated. Route it through `Mag::mul` (or `checked_mul` with float fallback + `TaintEvent::RationalOverflow`), forwarding the event to the caller's sink where one exists; where none exists (`fmt`), the event is display-only and may be dropped **at the boundary with a comment**, matching the R0 convention.

Caller adaptations: `convert_quantity`'s `Exact` branch matches on the returned `Mag` (trivial); `fmt.rs`'s ft-in path *simplifies* — its own `match in_q.mag` duplication collapses into using the returned `Mag` directly.

Gate tests:

- A `Float` psi value queried in anchor space never resurfaces as a claimed-exact `Ratio` anywhere downstream (compile-time guaranteed by the signature; the test asserts the display path).
- Huge exact magnitude × large anchor factor → `Float` + overflow event, **no panic** (debug profile row).
- `grep` gate in CI: `f64_to_ratio_approx` does not exist in the tree.

**Effort:** ~half a day.

---

## 7. S6 — R6 remainder + micro-polish

Small, independent, batched into one PR:

1. **Did-you-mean hint on unknown pack args.** The unknown-name loop is already hoisted (good); add the edit-distance hint against `eq.args` keys using the existing `E-UNKNOWN-UNIT` hint machinery. Test (verbatim from the R-series plan): `ACI.fr(fcc: 4 ksi, lambda: 1)` → `E-EVAL` naming `fcc`, hinting `fc`.
2. **Audit-then-fix the two unverified R6 items:** `parse_defs` span accuracy under indentation and CRLF, and the `eval_ten_thousand_term_sum` truth-in-labeling. They may already be done — check first, and either way add the R6-specified tests so "done" is machine-checked rather than remembered.
3. **`check_range` double conversion.** `check_range` converts to contract units and `prepare_arg` immediately converts again. Have `check_range` return the converted `Quantity` (or accept the pre-converted one). Behavior-neutral; existing tests gate it.
4. **Recorded, deferred:** `Mag::float() -> Result<Self, ()>` discards the failure reason. Fine internally; revisit with a real error type only if `Mag` construction becomes a stable public entry point.

**Effort:** ~half a day.

---

## 8. Structural guard (do this once, keep it forever)

The S-series exists because unchecked/f64-shortcut arithmetic kept reappearing in modules the R0 property didn't watch. Add a mechanical guard so the *next* one is a CI failure, not a review finding:

```rust
// in eval/ modules
#![deny(clippy::arithmetic_side_effects)]
```

scoped to `eval/` (and `packs/call.rs`), with explicit `#[allow]` + justification comments at the handful of sites doing f64 arithmetic on already-tainted values. Every raw `+`/`-`/`*`/`/` on `Ratio<i128>` or `i128` in evaluation code then needs either a checked form or a signed waiver. This is the same philosophy as R0 — make the illegal pattern loud — applied to the operator level. Budget it inside S5 (they touch the same files).

---

## 9. Sequencing and gates

```
S0  Builtin exactness property (lands RED, exception-listed) ──┐
                                                               │
S1  isqrt (+ MSRV 1.84)            ── independent ─────────────┤
S2  Exact floor/ceil/round         ── independent ─────────────┤   each stage deletes
S3  Trig lint wiring               ── after S2 (same file;     │   its EXPECTED_VIOLATIONS
                                      S2 removes rounding      │   rows
                                      from S3's scope)         │
S4  min/max via mag_cmp            ── independent ─────────────┤
S5  magnitude_in_anchor_units → Mag (+ clippy guard)  ─────────┤
S6  R6 remainder + polish          ── anytime ─────────────────┘
                                                               ▼
Final commit: EXPECTED_VIOLATIONS asserted empty and deleted;
CHANGELOG 0.1.0 date refreshed; crates.io publish unblocked.
```

S1, S2+S3, S4, S5, S6 are parallelizable across branches; merge order S2 → S3 only to avoid `builtins.rs` conflicts.

## 10. Definition of done

- The builtin exactness + NaN-hygiene property is a permanent conformance row with the 2^53 and `isqrt(i128::MAX)` regime rows fixed in CI, and the exception table **does not exist in the tree**.
- Neither `f64_to_ratio_approx` nor any float-based integer sqrt exists in the tree.
- `clippy::arithmetic_side_effects` is denied across `eval/` with justified allows only.
- `round`'s half-away-from-zero tie policy is documented in rustdoc and plan §2.2 and pinned by conformance rows.
- `rust-version = "1.84"` is declared and CI tests the MSRV.
- All of S0–S6 land before the crates.io 0.1.0 publish; the CHANGELOG release date reflects the actual publish day.

**Total estimated effort:** S0–S6 ≈ 2–3 working days, dominated by S0's strategy design and S5's signature migration.
