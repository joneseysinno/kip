# Remediation Plan — `kip` correctness review

**Status:** Draft 1 · Companion to `plan.md` (Draft 1) and `grammar-spec.md` (Draft 2)
**Scope:** Fixes for the defects found in the post-M6 code review: the float-sentinel leak class, the `Symbolic − Known` rejection, always-float `sqrt`, the missing eval-time lint channel, affine addition semantics, and a diagnostics-polish batch. Ordered so each stage mechanically exposes or unblocks the next.

This plan is normative for the fix order, API changes, and the tests that gate each stage. Nothing here changes the grammar; two stages add diagnostic codes that must be back-ported into grammar-spec §9.

---

## 0. Root-cause summary

Five findings, one root cause dominating:

| ID | Defect | Severity | Root cause |
|---|---|---|---|
| **F1** | `unify_add`/`unify_sub` (and their affine branches) do rational arithmetic on the sentinel `magnitude = 1` of float-tainted operands and stamp `float_mag: None` on the result | **Blocker** — silent wrong answers that claim exactness | Representable illegal state: `(Ratio<i128>, Option<f64>)` pair + `effective_magnitude()` |
| **F2** | `check_range` compares the sentinel against range bounds for float-valued args; `eval_sqrt` negativity check reads the sentinel (negative floats → NaN) | **Blocker** — wrong range verdicts, NaN escape | Same as F1 |
| **F3** | `add_like` rejects `Symbolic − Known` (`f'c - 100 psi` errors; `100 psi - f'c` works) | **High** — kills the most common partial-eval shape | Missed match arm in the flip logic |
| **F4** | `sqrt(4)` goes float; no `L-EXACTNESS-LOST`, `L-RATIONAL-OVERFLOW`, or lint-severity `L-RANGE` can surface because `eval_known` returns `Result<Value, Diag>` with no lint channel | **High** — violates the plan §2.2 exactness policy and §5.1 range philosophy | Missing plumbing; API returns errors only |
| **F5** | Affine `+` has delta semantics same-unit (`32 °F + 10 °F = 42 °F`) and absolute semantics cross-unit (via Rankine) | **Design** — same operation, two meanings | Unresolved semantics decision |

Plus a polish batch (F6) and a deferred perf batch (F7).

The order below is deliberate: **R0 (the `Mag` enum) goes first because the compiler then finds every F1/F2 site for us.** Fixing call sites piecemeal before R0 would mean auditing by eye the same code the type system can audit exhaustively.

---

## 1. R0 — Make the illegal state unrepresentable (`Mag` enum)

**Fixes:** the root cause of F1/F2.
**Touches:** `eval/value.rs`, then every arithmetic call site (`eval/units.rs`, `eval/builtins.rs`, `eval/partial.rs`, `eval/affine.rs`, `packs/call.rs`, `fmt.rs`, `registry/eval_expr.rs`).

### 1.1 The type change

```rust
/// Magnitude of a quantity: exact rational, or float after exactness loss.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mag {
    Exact(Ratio<i128>),
    Float(f64),
}

pub struct Quantity {
    pub mag: Mag,                 // replaces magnitude + float_mag
    pub unit: UnitExpr,
    pub dim: Dimension,
    pub provenance: Option<Arc<EquationProvenance>>,
}
```

Rules:

- **`effective_magnitude()` is deleted, not deprecated.** Nothing may extract a `Ratio<i128>` without matching on `Mag`. The only accessors are `as_f64()` (always valid, lossy for `Exact`), `exact() -> Option<Ratio<i128>>`, and `is_exact()`.
- **Field rename is the migration tool.** Renaming `magnitude` → `mag` guarantees every touch point is a compile error; migrate module-by-module in the order `units` → `builtins` → `partial` → `affine` → `packs` → `fmt` → `registry`. No `#[allow]` escapes; the branch does not merge with any remaining `todo!()`.
- **Taint propagation lives on `Mag`, once.** Implement `Mag::add/sub/mul/div/pow_int` as the single source of truth: `Exact ⊕ Exact` → checked rational (overflow → `Float` + overflow lint, see R3), anything involving `Float` → `Float`. `unify_add`, `unify_sub`, `combine_mul`, `combine_div`, `combine_pow`, and the affine paths all delegate to these — which retroactively deletes the hand-rolled float branch in `combine_mul` too, so there is exactly one place taint rules can be wrong.
- **`Mag` ops return `(Mag, Option<TaintEvent>)`** (or a small result struct) so callers can forward exactness-loss/overflow events into the lint sink once R3 lands. Until R3, events are collected and dropped at the boundary with a `// R3` marker — not silently omitted at the op site.

### 1.2 Gate tests

- **Taint-propagation property test** (proptest): for arbitrary quantity pairs where at least one operand is `Float`, every binary op yields `Float`. This is the test that would have caught F1 on day one; it becomes a permanent conformance row.
- **Regression: `ln(2) * 1 ft + 1 ft`** — must yield `Float(≈1.693…) ft`, never `Exact(2) ft`.
- **Regression: float through `convert_quantity`** — a `Float` psi value converted to ksi and range-checked against `2500 psi ..= 10000 psi` must be judged on its real value (this is the F2/`check_range` fix falling out of R0: with the sentinel gone, `check_range` is forced to match on `Mag`; compare exactly for `Exact`, via `f64` with both bounds lowered to `f64` for `Float`).
- **Regression: `sqrt` of a negative `Float`** — must error `E-EVAL` (negative sqrt), not return NaN.
- **NaN hygiene:** any op producing a non-finite `f64` (`NaN`, `±inf`) is an eval error at the op site, never a stored `Mag::Float`. Property-test this; it closes the NaN-escape class permanently, not just the sqrt instance.
- The affine `unify_add`/`unify_sub` branches switch from raw `+`/`-` to the checked `Mag` ops (the "checked arithmetic everywhere; panics are conformance failures" rule was being violated there).

**Effort:** ~2–3 days. Mechanical but wide; the property tests are the real deliverable.

---

## 2. R1 — `Symbolic − Known` (F3)

**Fixes:** the missed match arm in `add_like`.
**Touches:** `eval/partial.rs` only.

The `(Symbolic, Known)` + subtraction case currently errors ("cannot subtract a known quantity from a symbolic residual" — a message that is both wrong and backwards). Replace the error branch: build `SymNode::Binary { op: Sub, left: s.root, right: Known(k) }`, run it through the same `unify_additive_terms` constraint step the other three add/sub shapes already use (so `f'c - 100 psi` pins `dim(f'c) = pressure` exactly like `100 psi - f'c` does), then `simplify` + `finish_symbolic`.

Gate tests:

- `f'c - 100 psi` produces a residual with constraint `dim(f'c) = pressure`; binding `f'c = 4000 psi` yields exact `3900 psi`.
- Symmetry property: for random known `k` and symbol `x`, binding after `x - k` and negating `k - x` agree exactly.
- `f'c - 100 lbf` fails at **bind time** with `E-DIM-MISMATCH` citing both sites (constraint provenance already supports this).

**Effort:** ~half a day. Independent of R0; can land in parallel.

---

## 3. R3 — Eval-time lint channel

**Fixes:** the structural half of F4 (and unblocks R4's lint emission and R5's affine lint).
**Touches:** `eval/known.rs` public surface, `eval/partial.rs`, `eval/units.rs`, `packs/call.rs`, `lib.rs`.

> Numbered R3 before R4 because R4's exact-sqrt work *emits* lints and needs the channel to exist. R2 does not exist; the review's finding numbering and the remediation numbering diverge here intentionally — stages are ordered by dependency, not by finding.

### 3.1 Design

Mirror the existing lexer precedent (`lex` / `lex_checked`) rather than inventing a new shape:

```rust
pub struct EvalOutcome {
    pub value: Result<Value, Diag>,
    pub lints: Vec<Diag>,
}

pub fn eval(expr, registry, resolver) -> Result<Value, Diag>       // unchanged, thin wrapper
pub fn eval_checked(expr, registry, resolver) -> EvalOutcome       // new
```

Internally, thread a `&mut Vec<Diag>` sink through `eval_tree`/`eval_node`/`eval_binary` and the pack-call path. **Parallel determinism constraint (P1):** under `rayon::join`, each branch collects into its own local `Vec`, and merge order is fixed as *left subtree lints, then right subtree lints, then the joining node's own lints* — tree order, never completion order. Add a determinism test: same expression, `PARALLEL_THRESHOLD` forced to 1 vs `usize::MAX`, byte-identical lint sequences.

De-duplication policy: `L-EXACTNESS-LOST` fires **once per evaluation at the first flip site** (matching the spec's "records where"), not once per subsequent float op. `L-RATIONAL-OVERFLOW` and `L-RANGE` fire per occurrence.

### 3.2 Emissions wired in this stage

- `L-EXACTNESS-LOST` — from the `Mag` taint events R0 left parked at the boundary. The span is the operation site, and the message names the operation ("`ln` produced an irrational result").
- `L-RATIONAL-OVERFLOW` — the `rational::*` helpers already construct this `Diag` and hand it back in their `Err((f64, Diag))`; today callers discard it. They now forward it to the sink and continue with the float value (fallback semantics unchanged).
- `L-RANGE` — `check_range` with `severity = lint` currently returns `Ok(())` silently, a straight contradiction of plan §5.1 ("kip should say so by default"). Emit the lint carrying the pack's cited limits and the provenance cite string, same message shape as the `E-RANGE` error.

Gate tests: one per emission; the parallel-determinism test; and a pack fixture with `severity = "lint"` asserting the lint text includes the citation.

**Effort:** ~1–2 days. The API addition should land **before any crates.io publish** (pre-release, so no semver cost — but only if it lands now).

---

## 4. R4 — Exact `sqrt` for perfect squares (F4, arithmetic half)

**Fixes:** the plan §2.2 promise that dyadic-exact roots stay rational.
**Touches:** `eval/builtins.rs` (+ a small integer-sqrt helper).

`eval_sqrt` currently does `q.as_f64().sqrt()` unconditionally, so `sqrt(4)` and `sqrt(144 in²)` come back float — off-brand for a library whose identity is exactness. New behavior for `Mag::Exact(r)`:

1. Reduce `r` (`Ratio` keeps it reduced); attempt integer sqrt on numerator and denominator independently (`i128::isqrt`, verify `s*s == n` since `isqrt` floors).
2. Both perfect squares → `Mag::Exact(s_n / s_d)`, no lint. `sqrt(4) = 2` exact, `sqrt(9/4) = 3/2` exact.
3. Otherwise → `Mag::Float(r.to_f64().sqrt())` + `L-EXACTNESS-LOST` through the R3 sink.
4. `Mag::Float` input → float path as today (with the R0 negativity/NaN fixes already in force).

Dimension halving (`Ratio<i32>` exponents) is already correct and untouched. Note the unit representation stays `UnitExpr::Pow { exp: 1/2 }` as written — exactness applies to the magnitude, not to collapsing `(in²)^(1/2)` to `in`; unit simplification is out of scope here (it belongs to `fmt`, if anywhere).

Gate tests: the four cases above, plus `sqrt(2'^2)` (compound-literal-derived exact area) → exact `24 in` magnitude with `pow(1/2)` unit, and a property test: for random `n`, `sqrt(n²)` is exact and equals `n`.

**Effort:** ~half a day.

---

## 5. R5 — Affine addition semantics (F5) · **decision required**

**Touches:** `eval/units.rs` affine branches, `eval/affine.rs`, grammar-spec §9, plan §2.2 docs.

Today the *same operator* means two things: same display unit → naive magnitude add (RHS treated as a **delta**: `32 °F + 10 °F = 42 °F`), different display units → Rankine **absolute** addition (`32 °F + 5 °C ≈ 533 °F`). A drafter expects the first; nobody expects the pair to coexist.

Three options, with a recommendation:

| Option | Behavior | Cost |
|---|---|---|
| **A. Delta types** (`Δ°F` distinct from `°F`) | The correct long-term answer; subtraction of absolutes yields a delta, absolute+delta legal, absolute+absolute illegal | Grammar + type-system change; too large for a remediation pass |
| **B. Restrict + lint** *(recommended for v1)* | Same-unit `T + T` keeps delta semantics **with new lint `L-AFFINE-DELTA`** ("interpreted as 32 °F + Δ10 °F; absolute-temperature addition is rarely meaningful"); cross-unit affine `+` becomes **error `E-AFFINE-MIXED`** ("convert explicitly: `32 °F + (5 °C -> °F)` or state a delta"); affine `−` unchanged (T − T is the physically meaningful ΔT) | Small; removes the inconsistent Rankine-add path entirely |
| **C. Uniform absolute** | All affine `+` through Rankine | Breaks the drafting expectation the same-unit rule was built for; `32 °F + 10 °F = 533 °F` is a support-ticket generator |

Under Option B, subtraction's result unit continues to display as the leftmost unit (documented as "delta expressed in °F"); true `Δ` display is reserved for the v1.1 delta-types work, and `L-AFFINE-DELTA`'s wording should already use the Δ vocabulary so the v1.1 migration reads as a formalization, not a reversal.

Spec debt this stage must pay: add `L-AFFINE-DELTA` and `E-AFFINE-MIXED` to grammar-spec §9's inventory; reserve the `Δ`/delta design note in plan §2.2; add conformance rows (`32 °F + 10 °F` → 42 + lint; `32 °F + 5 °C` → error; `70 °F - 20 °C` → exact delta in °F).

**Effort:** ~1 day once the decision is made. **This stage blocks on your call between A-deferred/B/C** — everything else in this plan proceeds without it.

---

## 6. R6 — Diagnostics polish batch (F6)

Small, independent, batched into one PR:

1. **`eval_equation_call` argument-check order.** Unknown-argument detection currently runs *after* the required-argument loop, so a typo'd name (`fcc:` for `fc:`) reports "missing required argument `fc`" and never mentions the typo. Hoist the unknown-name loop to the top; add a did-you-mean hint via edit distance against `eq.args` keys (the machinery matches the existing `E-UNKNOWN-UNIT` hint pattern). Test: `ACI.fr(fcc: 4 ksi, lambda: 1)` → `E-EVAL` naming `fcc`, hinting `fc`.
2. **`parse_defs` span accuracy.** Spans start at `line_start` even for indented lines (start too early, end too short) and the `line.len() + 1` offset advance is wrong under CRLF. Compute the trimmed slice's true byte offset within the line; advance by the actual terminator length. Test: an indented `define` with a deliberate error under both `\n` and `\r\n`, asserting exact span bytes.
3. **Test truth-in-labeling.** `eval_ten_thousand_term_sum` builds 100 terms. Either make it 10,000 (matching the parser-side test and the M4 stack-safety claim) or rename it; prefer the former — eval-side stack safety at depth is exactly what the iterative postorder design promises.
4. **`Token` doc comments.** `Gte`'s doc lists all five comparison operators (copy-paste artifact); give each variant its own operator.
5. **Message audit.** The `add_like` error message deleted by R1 was backwards; grep the remaining eval-error strings for direction/operand-order errors while in the neighborhood.

**Effort:** ~half a day total.

---

## 7. R7 — Perf batch · **deferred to M8, correctness-gated**

Not remediation, but recorded here so it isn't lost:

- **`Arc` the `SymNode` tree.** Children are `Box`; every `values.get(id).cloned()` on a symbolic value deep-clones the residual tree, and residual-heavy sheets pay it per parent node. `Arc` children make clones pointer-bumps. Do this *after* R1/R3 settle `partial.rs`, and only with a benchmark showing the win (M8's residual-sweep benchmark is the natural gate).
- **Reduce `Value` cloning in `eval_node`.** Children could be *taken* from the map (each node has exactly one parent in this AST) rather than cloned — but verify the pack-call path, which reads arg values by `NodeId`, before assuming single-consumption.
- Neither item may change any conformance result; both run under the existing determinism/Miri gates.

---

## 8. Sequencing and gates

```
R0  Mag enum + taint property tests          ──┐  (blocker class F1/F2 dies here)
R1  Symbolic − Known                         ──┤  parallel with R0
                                               ▼
R3  Lint channel (eval_checked, sink, P1-deterministic merge)
                                               ▼
R4  Exact sqrt (+ L-EXACTNESS-LOST emission)
R5  Affine decision → implement B (or A/C)   ── blocks only on the decision
R6  Polish batch                             ── anytime after R1
────────────────────────────────────────────────
R7  Perf (M8, benchmark-gated)
```

Definition of done for the remediation as a whole:

- The taint-propagation and NaN-hygiene property tests are permanent conformance rows.
- `grammar-spec.md` §9 lists `L-AFFINE-DELTA` and `E-AFFINE-MIXED` (pending R5 decision) and the conformance table gains the rows named in R2–R5.
- `plan.md` §2.2's exactness policy is true again (exact dyadic roots, `L-EXACTNESS-LOST` actually emitted) rather than aspirational.
- No `effective_magnitude` — the symbol does not exist in the tree.
- All of R0–R6 land before any crates.io publish, so `eval_checked` and the `Quantity` layout never need a breaking release.

**Total estimated effort:** R0–R6 ≈ 5–7 working days, dominated by R0's migration and R3's plumbing.
