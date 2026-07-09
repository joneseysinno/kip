# Implementation Plan — `kip`

**Status:** Draft 1 · Companion to `grammar-spec.md` (Draft 2)
**Scope:** The `kip` crate — a pure, thread-safe engineering-expression evaluator for US imperial units, with exact rational arithmetic, partial (symbolic) evaluation, empirical code equations as the default calling convention, and a user-extensible unit registry.

This plan is normative for crate structure, public API shape, concurrency contracts, and milestone ordering. The grammar itself is specified in `grammar-spec.md` and is not restated here except where it constrains implementation.

---

## 0. The three load-bearing requirements

Everything in this plan is arranged to serve three requirements, stated up front so no later design decision can quietly violate them:

**P1 — Parallel branch evaluation.** Any two independent branches of work — two subtrees of one expression, two expressions in a host's dependency graph, two what-if scenarios over the same AST — must be evaluable concurrently with no locks, no coordination, and bit-identical results. This is a *conformance requirement*, tested under Miri and loom, not a performance aspiration.

**P2 — Empirical engineering expressions are the default.** kip is built for code equations (`ACI.fr(fc: f'c, lambda: 1.0)`) and constant-dimensionalized empirical formulas first, and for calculator arithmetic second. The evaluator's dispatch path, the diagnostics, the equation-pack loader, and the docs all treat the empirical case as the main road, not a bolt-on.

**P3 — User-defined units anchored to the base system.** Users can define custom units (`define kip, kips = 1000 lbf`) and even custom base dimensions (`dimension Currency` → `define USD, $ : Currency`), and every custom unit resolves to an exact rational ratio against the built-in imperial anchors: **inch, lbf, second, Rankine** (plus the angle pseudo-dimension and any user base dimensions). No SI normalization ever occurs internally.

---

## 1. Crate layout

Single crate `kip`, organized as modules rather than a workspace for v1 (split later only if compile times demand it). Feature-gate the heavier optional pieces.

```
kip/
├── Cargo.toml
├── src/
│   ├── lib.rs            # public API surface, re-exports, crate docs
│   ├── lexer.rs          # tokenizer state machine (grammar §4); declares submodules below
│   ├── lexer/
│   │   ├── token.rs      # Token, Span, exact-value payloads for FEET/INCHES/FTIN
│   │   └── ftin.rs       # bounded-lookahead compound scanner + clean backtrack
│   ├── parser.rs         # Pratt parser (grammar §5.2), iterative/heap-recursing
│   ├── parser/
│   │   └── ast.rs        # Expr (immutable, Arc-shared), NodeId, spans
│   ├── dim.rs            # Dimension vector: SmallVec of (BaseDim, Ratio<i32>) exponents
│   ├── quantity.rs       # Quantity = Ratio<i128> magnitude + UnitExpr (as written) + Dimension
│   ├── registry.rs       # Registry (frozen), RegistryBuilder, generations (COW)
│   ├── registry/
│   │   ├── defs.rs       # define/dimension parser (grammar §6), order-free resolution
│   │   └── seed.rs       # built-in imperial seed data (anchors, derived units, affine temps)
│   ├── eval.rs           # Evaluator: pure fn of (Expr, &Registry, &dyn Resolver)
│   ├── eval/
│   │   ├── value.rs      # Value::Known(Quantity) | Value::Symbolic(SymExpr)
│   │   ├── constraint.rs # ConstraintSet: dimension unification over free symbols
│   │   ├── builtins.rs   # sqrt/trig/ln/min/max… with dimension semantics
│   │   └── partial.rs    # symbolic residual construction, simplification passes
│   ├── packs.rs          # TOML equation-pack loader → registry generation
│   ├── packs/
│   │   ├── contract.rs   # per-argument unit contracts, validity ranges, provenance
│   │   └── dimensionalize.rs # constant dimensionalization of empirical formulas
│   ├── diag.rs           # Diagnostic, ErrorCode/LintCode enums (grammar §9), spans
│   ├── fmt.rs            # display API: preferred units, precision policy
│   ├── fmt/
│   │   └── ftin.rs       # ft-in rendering with denominator snapping (1/16 etc.)
│   └── resolver.rs       # Resolver trait + EmptyResolver, MapResolver helpers
├── tests/
│   ├── conformance.rs    # grammar §8 table, one test per row
│   ├── packs.rs          # equation-pack loading, contract enforcement
│   └── concurrency.rs    # shared-Arc evaluation, loom models
├── fuzz/                 # cargo-fuzz targets: lexer, parser, eval
└── benches/              # criterion: lex, parse, eval, parallel scaling
```

**Module-style convention:** modern Rust 2018+ layout throughout — a module with children is `foo.rs` alongside a `foo/` directory of submodules. No `mod.rs` files anywhere in the crate; enforce with `clippy::mod_module_files` in CI (`#![warn(clippy::mod_module_files)]` plus `-D warnings`).

**Dependencies (deliberately few):**

| Crate | Why | Feature |
|---|---|---|
| `num-rational` / `num-traits` | `Ratio<i128>` exact arithmetic | core |
| `smallvec` | dimension vectors (≤ 8 exponents inline) | core |
| `serde` + `toml` | equation packs, registry defs as data | `packs` (default on) |
| `rayon` | parallel helper APIs (never required for correctness) | `parallel` (default on) |
| `unicode-ident` | letter classes incl. Greek block | core |
| `loom` | concurrency model tests | dev-only |

No `lazy_static`/global state anywhere. The absence of globals is what makes P1 cheap.

---

## 2. Core data model

### 2.1 `Dimension`

A dimension is a map from base dimension → rational exponent. Base dimensions are:

```rust
enum BaseDim {
    Length,        // default anchor: inch
    Force,         // default anchor: lbf
    Time,          // default anchor: second
    Temperature,   // default anchor: Rankine (absolute; °F/°C/K are affine views)
    Angle,         // pseudo-dimension; default anchor: radian — tracked to catch deg/rad bugs
    Custom(CustomDimId), // user-declared via `dimension`, e.g. Currency
}
```

**Anchors are per-registry data, not compile-time constants.** `BaseDim` names the dimension only; which unit anchors each dimension is a property of the `Registry` generation (an `anchors: Map<BaseDim, UnitId>` table). The defaults above are what the seed registry ships with, and the user can re-anchor any dimension through the builder (§4.2a) — e.g., anchor Length to `ft` for a sitework tool or Force to `kip` for a heavy-structural one. Because every unit stores an exact rational ratio to its dimension's anchor, re-anchoring is a pure rebasing of ratios and **cannot change any computed result** — it changes the internal reference point, `Ratio<i128>` magnitude growth characteristics (anchor near your working scale keeps numerators small), and the natural default for display fallbacks. Nothing in `Dimension`, `Quantity`, or the evaluator may assume a specific anchor unit; conformance includes an anchor-invariance property test (§8).

Exponents are `Ratio<i32>` (not integers) because `sqrt(4000 psi)` yields pressure^(1/2) and the spec makes `4000 psi^0.5` a legal literal so symbolic residuals round-trip through text. Stored as a sorted `SmallVec<[(BaseDim, Ratio<i32>); 8]>` — comparison, unification, and hashing are then trivial and allocation-free in the common case.

Note the **force-based** (gravitational) system: Force is a base dimension and mass is *derived* (slug = lbf·s²/ft). This matches structural-engineering practice and avoids gc constants leaking into user-visible math. Document this loudly; it is the most common source of confusion for SI-trained users.

### 2.2 `Quantity` — unit-preserving values

```rust
struct Quantity {
    magnitude: Ratio<i128>,      // exact where possible
    float_mag: Option<f64>,      // set when exactness was lost (trig, ln, non-dyadic roots)
    unit: UnitExpr,              // the unit AS WRITTEN — never normalized away
    dim: Dimension,              // derived, cached
}
```

The invariant is: **the unit the user wrote is the unit the value carries.** `12 ft - 6 in` produces `11.5 ft` (leftmost-wins), not 138 in and not some canonical base form. Conversion happens only at unification points (`+`, `-`, comparisons, function contracts) and converts the *right* operand into the *left* operand's unit via exact anchor ratios.

Exactness policy:
- Rational stays rational through `+ - * /`, integer `^`, and dyadic-exact roots.
- The first inexact operation (trig, `ln`, fractional powers of non-perfect ratios) flips the value to `float_mag`, and it stays float. A `L-EXACTNESS-LOST` lint (new, add to diagnostics inventory) records where.
- Overflow of `Ratio<i128>` mid-computation falls back to float with `L-RATIONAL-OVERFLOW` rather than panicking. (Checked arithmetic everywhere; panics are conformance failures.)

### 2.3 `Value` and symbolic residuals

```rust
enum Value {
    Known(Quantity),
    Symbolic(SymExpr),   // residual expression + free-symbol set + ConstraintSet
}
```

`SymExpr` is a simplified, immutable expression over free symbols and `Known` leaves. Partial evaluation folds every fully-known subtree eagerly, so `2*f_r + f'c` with `f_r = 450 psi` yields `Symbolic(900 psi + f'c)` with the constraint `dim(f'c) = pressure`. The `ConstraintSet` performs dimension unification and reports `E-DIM-MISMATCH` when unsatisfiable (`sqrt(f'c) + f'c` case), citing the constraining sites.

---

## 3. Parallel evaluation (P1) — the concurrency architecture

### 3.1 Immutability as the concurrency strategy

There are no locks in kip because there is nothing to lock:

| Object | Sharing model |
|---|---|
| `Expr` (AST) | frozen at parse; nodes in an arena owned by the `Expr`; the whole thing is `Arc<Expr>`, `Send + Sync` |
| `Registry` | frozen at build; shared as `Arc<Registry>`; extension = new generation (§4.3), never mutation |
| `Evaluator` | zero-sized / stateless; evaluation is a pure function `eval(&Expr, &Registry, &dyn Resolver) -> Result<Value, Diag>` |
| `Resolver` | caller-supplied trait object, required to be `Sync`; kip only reads through it |
| `Value`, `Diagnostic` | plain immutable data, `Send + Sync` |

The conformance suite includes the spec's own test: *the same `Expr` evaluated simultaneously from 8 threads against a shared `Arc<Registry>` — identical results, no data races under Miri/loom.*

### 3.2 Three levels of parallelism, layered

**Level 1 — Intra-expression (inside kip, `parallel` feature).**
The evaluator walks the AST iteratively with an explicit work stack (this also satisfies the 10,000-term stack-safety requirement). Independent siblings — the two operands of a binary op, the arguments of a call — are data-independent by construction, so with `parallel` enabled the evaluator hands sibling subtrees above a size threshold to `rayon::join`. Below the threshold it stays serial (a `2 kip * L` doesn't want a thread hop). Results are deterministic because floating-point operations are only combined in the fixed AST order regardless of which thread computed each side.

**Level 2 — Inter-expression (host-facing helper, `parallel` feature).**

```rust
/// Evaluate many independent expressions concurrently against one registry.
pub fn eval_batch<'a>(
    exprs: impl IntoParallelIterator<Item = &'a Expr>,
    registry: &Registry,
    resolver: &(dyn Resolver + Sync),
) -> Vec<Result<Value, Diag>>;
```

This is the primitive a sheet-layer host uses per topo-sort *level* of its dependency graph: everything in one level is independent, so one `eval_batch` call per level parallelizes the whole sheet. kip does not own the graph (grammar spec principle 6) but it supplies exactly the pure, `Send + Sync` evaluation the host's graph needs — and ships a documented example (`examples/sheet.rs`) showing the topo-sort-and-batch pattern end to end.

**Level 3 — Branch/scenario parallelism (the "different branches" case).**
Because ASTs and registries are shared immutably, N what-if scenarios are just N resolvers over the same `Arc<Expr>`/`Arc<Registry>`:

```rust
pub fn eval_scenarios(
    expr: &Expr,
    registry: &Registry,
    scenarios: impl IntoParallelIterator<Item = Box<dyn Resolver + Sync>>,
) -> Vec<Result<Value, Diag>>;
```

Parametric studies ("sweep f'c from 3000 to 8000 psi"), bracketed designs, and A/B registry generations (same expression against two unit-pack versions) all reduce to this shape with zero copying of the expression or registry. This is the cheapest useful parallelism in the whole design and it falls straight out of P1's immutability discipline.

**Additional exploitation of partial evaluation:** for a scenario sweep where most symbols are fixed and one varies, evaluate once with the fixed resolver to get a `Symbolic` residual in only the swept variable, then evaluate the (much smaller) residual per scenario. Expose this as `Value::bind(&self, resolver) -> Result<Value, Diag>` — residuals are themselves evaluable. This turns an O(scenarios × tree) sweep into O(tree + scenarios × residual).

### 3.3 Concurrency conformance tests

- Miri on the full conformance suite (CI, nightly job).
- loom models for: registry generation swap under concurrent readers; `eval_batch` over a shared registry.
- A determinism test: 8-thread repeated evaluation of a float-tainted expression must be bit-identical across 1,000 runs (guards against accidental reduction-order nondeterminism if rayon reductions ever creep in).
- `#![deny(unsafe_code)]` except in an explicitly-audited arena module, if one proves necessary at all (start with `Vec<Node>` + indices; measure before reaching for anything cleverer).

---

## 4. The registry: base anchors and user-defined units (P3)

### 4.1 Built-in seed data

The built-in generation-0 registry contains:

- **Default anchors** (ratio 1 to themselves in generation 0): `in`, `lbf`, `s`, `°R`, `rad`. These are defaults, not privileged units — the anchor table is registry data and the user can re-anchor any built-in dimension (§4.2a).
- **Derived imperial units**, each an exact rational against the anchors: `ft` (12 in), `yd`, `mi`, `mil`, `kip` (1000 lbf), `psi`, `ksi`, `psf`, `ksf`, `pcf`, `plf`, `klf`, `lbf·ft`, `kip·ft`, `kip·in`, `slug`, `lbm` (as lbf·s²·in⁻¹-derived mass with the exact standard-gravity ratio, clearly documented), `min`, `hr`, `deg`, plus the seed list finalized in the registry seed-data doc (open item — see §9).
- **Affine temperature units** `°F`, `°C`, `K` as built-in affine views over `°R` (offset + scale pairs, exact rationals). Affine units participate in `+`/`-` under documented affine rules (temperature difference vs. absolute temperature — model as two related units, `°F` and `Δ°F`, the standard trick that keeps affine arithmetic sound).
- **No SI working system.** A tiny read-only conversion table (m, mm, N, kN, MPa, kg) may ship *for display/`fmt` purposes only* behind a `si-display` feature, but SI units are not registered, not definable as anchors, and never appear in internal representation. (Users can still `define MPa = 145.038 psi`-style approximations themselves if they insist; kip won't stop them, but the seed data won't encourage it.)

### 4.2 User definitions

Exactly the grammar-spec §6 language:

```text
define kip, kips = 1000 lbf          # linear unit, exact rational vs. existing units
dimension Currency                    # new base dimension
define USD, $ : Currency              # primary unit (anchor) of the new dimension
define labor_rate = 85 $/hr           # composes across custom + built-in dimensions
```

`RegistryBuilder::parse_defs(src)` collects all lines, builds the small unit-to-unit dependency graph, resolves order-free, detects `E-DEF-CYCLE`/`E-DEF-SYMBOLIC`/`E-DUP-UNIT`/`E-AFFINE-DEFINE`, and reduces **every** unit to `(Dimension, Ratio<i128> anchor_ratio)`. That final reduction is the P3 guarantee: any custom unit, however many `define` hops deep, is one exact multiplication away from the anchors.

Builder API:

```rust
let mut b = RegistryBuilder::from_seed();       // generation 0, default anchors
b.parse_defs(user_defs_src)?;                    // text form
b.define("kip", &["kips"], qty!(1000, "lbf"))?;  // programmatic form (same checks)
b.new_dimension("Currency")?;
b.set_anchor(BaseDim::Length, "ft")?;            // re-anchor a built-in dimension (§4.2a)
let reg: Arc<Registry> = b.freeze();             // generation 1, immutable forever
```

### 4.2a User-selectable anchors

The user chooses each dimension's anchor; the defaults (`in`, `lbf`, `s`, `°R`, `rad`) apply only when they don't. Two forms:

**Programmatic:** `RegistryBuilder::set_anchor(dim, unit_name)`.

**Text**, as an extension to the definition language (to be folded into grammar-spec Draft 3, §6):

```text
anchor Length = ft            # re-anchor a built-in dimension
anchor Force  = kip
define USD, $ : Currency      # unchanged: `:` form already names the anchor of a NEW dimension
```

```ebnf
anchor_stmt = "anchor" IDENT "=" IDENT ;    (* dimension name = registered unit name *)
```

`anchor` joins `define`/`dimension` as a reserved word. Anchor statements resolve in the same order-free pass as `define` lines: the builder first resolves all units against the *current* anchors, then rebases every ratio to the requested anchors in one exact rational pass at `freeze()`.

**Rules:**

1. **The anchor must be a registered linear unit of exactly that dimension.** A compound (`kip·ft` for Length), a unit of another dimension, or an unknown name → `E-ANCHOR-INVALID` naming what was expected.
2. **Affine units cannot anchor.** `anchor Temperature = °F` → `E-ANCHOR-AFFINE` (anchors define the zero point; affine views like `°F`/`°C` don't own one). Any absolute temperature unit (e.g. a user-defined linear scale over `°R`) is fine.
3. **One anchor statement per dimension per generation** — a second is `E-DUP-ANCHOR`, both sites reported (mirrors `E-DUP-UNIT`).
4. **Re-anchoring is result-invariant.** Since all conversions are exact rationals composed through the anchor, moving the anchor rebases every stored ratio exactly and no expression's value, dimension, or display unit changes. This is enforced by a property test: evaluate the full conformance suite under default anchors and under a shuffled set (`ft`, `kip`, `hr`, `°R`, `deg`) and require identical `Value`s. What re-anchoring *does* legitimately affect: `Ratio<i128>` growth (anchor near the working scale keeps numerators/denominators small — a mile-scale sitework tool anchored to `mi` avoids ×63,360 blowups), and the `fmt` module's fallback unit when no preferred unit is set.
5. **Anchors are per-generation and frozen with the registry**, like everything else. A generation extension inherits the parent's anchors unless it re-anchors; two generations with different anchors can coexist and be A/B-compared through Level-3 scenario evaluation — and rule 4 guarantees the comparison is a pure performance/ergonomics test, never a numerics one.
6. **Custom dimensions already work this way** — `define USD, $ : Currency` names the anchor at dimension creation; `anchor Currency = EUR` in a later generation re-anchors it identically to the built-ins. One mechanism, no special cases.

New diagnostics: `E-ANCHOR-INVALID`, `E-ANCHOR-AFFINE`, `E-DUP-ANCHOR` (add to the §5.3 inventory extension).

### 4.3 Generations (copy-on-write)

`freeze()` produces an immutable `Registry` carrying a generation number. Extending is `Registry::extend() -> RegistryBuilder` which COW-shares the existing tables (interned names, `Arc`'d unit records) and only materializes the delta. Consequences:

- Readers never block; a "reload the unit pack" in a host app is: build gen N+1 in the background, swap the `Arc` (e.g. `arc-swap` in the *host*, not in kip), let in-flight evaluations finish on gen N.
- Diagnostics carry the generation they resolved against, so a stale-parse-vs-new-registry confusion is diagnosable.
- Two generations can be alive simultaneously — which is precisely what Level-3 scenario parallelism needs for A/B-testing a unit or equation-pack change.

### 4.4 Persistence of custom units

User units persist as data, not code: the same `define`/`dimension` text lives in TOML sidecars (shared format with equation packs, §5.4), so a host app stores the user's custom-unit file, and `RegistryBuilder` ingests it at startup. Round-trip guarantee: `Registry::dump_defs()` emits canonical `define` lines that re-parse to an identical registry (property-tested).

---

## 5. Empirical engineering expressions as the default (P2)

### 5.1 What "default" means concretely

1. **The evaluator's call syntax is designed around code equations.** Named arguments are *required* for pack equations (`E-CODE-POSITIONAL` otherwise) because empirical formulas have argument-specific unit contracts and transposing two pressures is a silent disaster. Positional args exist only for the math builtins.
2. **Constant dimensionalization is built in, not user-visible hackery.** `f_r = 7.5·λ·√f'c` (ACI 318, psi units) is dimensionally nonsense as written — the 7.5 secretly carries psi^(1/2). Pack equations declare per-argument and result units; the loader *dimensionalizes the constants automatically* so the equation is internally consistent, while the user still sees and cites the code's published form.
3. **Contracts convert, then compute, then convert back.** Calling `ACI.fr(fc: 4 ksi, lambda: 1.0)` converts 4 ksi → 4000 psi per the contract (exact ratio), computes, and returns the result in the contract's result unit (psi), which then participates in leftmost-wins like any quantity.
4. **Validity ranges are first-class.** Each argument may carry a range (`fc: 2500 psi ..= 10000 psi`); out-of-range inputs produce `L-RANGE` (lint, with the pack's cited limits) or `E-RANGE` per the pack's own declared severity. Empirical formulas are only true inside their fitted domain; kip should say so by default.
5. **Provenance rides along.** Every pack-equation result carries a provenance handle (pack id, equation id, edition, section citation) retrievable via the diagnostics/formatting API — calc sheets need to print "per ACI 318-19 §19.2.3.1."

### 5.2 Evaluation of a code-equation call

```
ACI.fr(fc: f'c, lambda: 1.0)
```

- Parses as a `path call_args` node (grammar §5.2); the path resolves against equation packs loaded into the registry generation.
- If `f'c` is a free symbol: the contract *pins its dimension immediately* (pressure), merging into the `ConstraintSet` before any value exists — so unit errors surface at parse-check time, not at final-number time. The call itself becomes part of the `Symbolic` residual.
- If all args are `Known`: convert per contract → evaluate the pack's expression body (which is itself a kip expression, parsed once at pack load and cached as `Arc<Expr>`) → attach result unit + provenance.
- Pack bodies are evaluated through the same pure evaluator, so they inherit P1's parallelism for free.

### 5.3 New diagnostics for the empirical layer

Extend the grammar-spec §9 inventory:

| Code | Meaning |
|---|---|
| `E-RANGE` / `L-RANGE` | argument outside the equation's declared validity range |
| `E-CONTRACT-DIM` | argument's dimension can't unify with the contract |
| `E-PACK-PARSE` | TOML pack malformed (with TOML span) |
| `E-PACK-BODY` | pack equation body fails to parse/type-check at load |
| `E-UNKNOWN-EQ` | path doesn't resolve to a loaded equation |
| `L-EXACTNESS-LOST` | value crossed from rational to float (§2.2) |
| `L-RATIONAL-OVERFLOW` | i128 rational overflow forced float fallback |

### 5.4 Equation-pack TOML format (v1)

```toml
[pack]
id        = "aci318"
title     = "ACI 318-19 selected equations"
edition   = "2019"
license   = "user-provided"        # kip ships no copyrighted pack content

[[equation]]
id        = "fr"                   # called as ACI.fr(...)
namespace = "ACI"
cite      = "§19.2.3.1"
result    = "psi"
body      = "7.5 * lambda * sqrt(fc)"   # kip expression, constants auto-dimensionalized

  [[equation.arg]]
  name  = "fc"
  unit  = "psi"
  range = { min = "2500 psi", max = "10000 psi", severity = "lint" }

  [[equation.arg]]
  name    = "lambda"
  unit    = "1"                    # dimensionless
  default = "1.0"                  # optional defaults allowed for dimensionless modifiers
```

The same TOML file format carries `[defs]` blocks of `define`/`dimension` lines (§4.4), so one sidecar can ship a pack *and* the custom units it needs (e.g., a cost pack shipping `dimension Currency`).

**kip ships the loader and the format spec, plus one liberally-licensed demo pack of textbook-generic equations for tests/docs.** Real code packs (ACI/AISC/ASCE text is copyrighted) are user- or vendor-supplied.

---

## 6. Public API sketch

```rust
// Parse (registry needed for unit-position resolution; resolver optional for shadow lints)
pub fn parse(src: &str, reg: &Registry) -> Result<Arc<Expr>, Vec<Diag>>;
pub fn parse_checked(src: &str, reg: &Registry, res: &dyn Resolver) -> ParseOutcome; // + lints

// Evaluate (pure, Send + Sync all the way down)
pub fn eval(expr: &Expr, reg: &Registry, res: &dyn Resolver) -> Result<Value, Diag>;
pub fn eval_batch(...) -> Vec<Result<Value, Diag>>;      // §3.2 level 2
pub fn eval_scenarios(...) -> Vec<Result<Value, Diag>>;  // §3.2 level 3

impl Value {
    pub fn bind(&self, res: &dyn Resolver) -> Result<Value, Diag>; // evaluate a residual further
    pub fn free_symbols(&self) -> &[Symbol];
    pub fn constraints(&self) -> &ConstraintSet;
    pub fn quantity(&self) -> Option<&Quantity>;
}

impl Quantity {
    pub fn convert_to(&self, unit: &UnitExpr, reg: &Registry) -> Result<Quantity, Diag>;
    pub fn display(&self, opts: &FmtOptions) -> String;  // incl. ft-in denominator snapping
}

pub trait Resolver: Sync {
    fn resolve(&self, name: &str) -> Option<Value>;      // Known or Symbolic both legal
}
```

Design rules for the API surface:
- Everything user-facing is `Send + Sync`; assert it with static `const _: () = assert_send_sync::<T>()` tests.
- No method takes `&mut` after freeze, anywhere.
- Diagnostics are values with spans, codes, and structured hints — hosts render them; kip never prints.

---

## 7. Milestones

Ordered so every milestone ends green on CI with the conformance rows implemented so far, and so P1/P2/P3 land as early skeletons rather than late retrofits.

**M0 — Skeleton + contracts (week 1).**
Crate scaffold, `Dimension`, `Quantity`, `Ratio<i128>` policies, `Value` enum stubs, `Diag` type, CI with Miri + `deny(unsafe_code)` + the send/sync static asserts. *P1's discipline is enforced from day one.*

**M1 — Lexer (weeks 1–2).**
Full §4 state machine including FTIN bounded lookahead + clean backtrack, prime/tick disambiguation (D1–D3), Unicode aliases (′ ″ · ×), digit separators, sci-notation tightness. All "Ticks and compounds", "Identifiers and primes", and "Numbers" conformance rows. Fuzz target `fuzz_lexer` running the adversarial seeds (never panics; incomplete compounds backtrack or clean-EOF).

**M2 — Registry core (week 2–3).**
Seed data (default anchors + derived + affine temps), `RegistryBuilder`, `parse_defs`, order-free resolution, cycle/dup/symbolic/affine diagnostics, **user-selectable anchors** (`set_anchor`, `anchor` text statement, rebasing pass, `E-ANCHOR-*` diagnostics, anchor-invariance property test), generations with COW extension, `dump_defs` round-trip property test (must round-trip anchor statements too). All "Registry definitions" conformance rows. *P3 lands here.*

**M3 — Parser (week 3–4).**
Pratt parser per §5.2, iterative (10,000-term stack-safety test), unit-expression attachment with the tight/spaced rule (W1, D4, D5), paths, named args, `E-EQ-IN-EXPR`, spans on every node, frozen `Arc<Expr>` output. Remaining pure-syntax conformance rows. Fuzz target `fuzz_parser`.

**M4 — Evaluator: known values (weeks 4–6).**
Pure eval of fully-known expressions: leftmost-wins unification, dimension composition, exactness policy + float fallback, builtins with dimension semantics (angle pseudo-dimension, `sqrt` halving exponents), affine temperature rules, `convert_to`. The 8-thread shared-`Arc` test and Miri job go green here. *P1's proof obligations start being discharged.*

**M5 — Partial evaluation + constraints (weeks 6–8).**
`Symbolic` residuals, eager folding, `ConstraintSet` unification with provenance-citing `E-DIM-MISMATCH`, `Value::bind`, residual simplification (constant folding, unit-consistent term merging — keep the simplifier conservative; correctness over prettiness). "Units, juxtaposition, and symbols" conformance rows complete.

**M6 — Equation packs (weeks 8–10).** *P2's main milestone.*
TOML loader, contracts, automatic constant dimensionalization, validity ranges, provenance, named-arg enforcement, pack-body pre-parse + cache, demo pack, `E-PACK-*`/`E-RANGE` diagnostics. `ACI.fr` conformance rows.

**M7 — Parallel helpers + formatting (weeks 10–12).**
`rayon` intra-expression join with threshold, `eval_batch`, `eval_scenarios`, residual-sweep pattern documented; determinism test; loom models. `fmt` module: preferred-unit display, ft-in denominator snapping, `FmtOptions`. `examples/sheet.rs` (topo-sort host pattern) and `examples/sweep.rs` (scenario parallelism over a residual).

**M8 — Hardening + release (weeks 12–14).**
Full fuzz corpus soak, benchmark suite (parse throughput, eval throughput, parallel scaling curve on a synthetic 1,000-node sheet), API docs with the force-based-system explainer front and center, crates.io publish as `kip` (name confirmed available), CHANGELOG, versioning policy (registry seed data changes = minor bump; grammar changes = major).

---

## 8. Testing strategy (summary)

| Layer | Method |
|---|---|
| Grammar conformance | one `#[test]` per §8 row, table-driven; rows are the spec's contract |
| Lexer/parser robustness | cargo-fuzz, adversarial seeds as corpus, "never panic" invariant |
| Exact arithmetic | proptest: round-trip `dump_defs`, conversion ratios compose exactly, `a.convert_to(u).convert_to(a.unit) == a` for rationals |
| Anchor invariance | full conformance suite run under default anchors and under a shuffled anchor set (`ft`, `kip`, `hr`, `°R`, `deg`) must produce identical `Value`s (§4.2a rule 4) |
| Concurrency | Miri (CI), loom models (generation swap, batch eval), 8-thread identity test, bit-identical determinism test |
| Empirical layer | pack fixture tests: contract conversion, range lints, constant dimensionalization equals hand-computed exponents |
| Performance | criterion benches; the parallel-scaling bench doubles as a regression tripwire for accidental serialization |

---

## 9. Open items carried forward

From the grammar spec, unchanged: check expressions (v1.1), affine `define`, vector/matrix literals, survey foot naming (must be settled in M2's seed-data doc — recommend `ft` = international, `sft` = survey, with a seed-data comment citing NIST's 2023 survey-foot deprecation), and `%` as a unit (recommend accepting: dimensionless, exact 1/100, decide in M2).

New in this plan:
1. **Simplifier depth** for symbolic residuals — v1 ships conservative folding only; algebraic rewriting (factoring, cancellation) is explicitly out of scope until there's a correctness story for it under dimensions.
2. **`si-display` feature** — display-only SI table: in or out? (Leaning in, behind a non-default feature, given P3's "no SI working system" stance.)
3. **Default arguments in packs** — v1 allows defaults for dimensionless modifiers only (as in the `lambda` example); dimensional defaults deferred, since a silent default pressure is exactly the class of bug named-args exist to prevent.
4. **Grammar-spec Draft 3 edit** — fold the `anchor_stmt` production (§4.2a) into grammar-spec §6, add `anchor` to the reserved-word list (§3.2), and add `E-ANCHOR-INVALID`/`E-ANCHOR-AFFINE`/`E-DUP-ANCHOR` to the §9 inventory.
