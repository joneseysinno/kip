# Grammar Specification — `kip`

**Status:** Draft 2 · Reference document for the tokenizer and parser (`kip::parse`) of the imperial engineering-expression engine.
**Scope change from Draft 1:** kip is a **pure expression evaluator**, not a worksheet engine. This spec is now normative for (a) the expression language and (b) the registry-definition language (`define`/`dimension`). The sheet-layer statement grammar (bindings, annotations, `show`, `options`) is **non-normative** and preserved in Appendix A as a recommended convention for applications built on kip, so the ecosystem shares one notation even though kip does not implement it.
Semantics (dimension inference, partial evaluation, leftmost-wins conversion, registry generations) are specified elsewhere; this document notes them only where they constrain syntax.

---

## 1. Design principles

1. **Notation follows the drawing set, not the programming language.** `12'-6"`, `f'c`, `6 1/2"`, and `kip·ft` are first-class syntax. Where drafting convention and programming convention conflict, drafting wins.
2. **The characters `'` and `"` are permanently spent on unit notation and identifier primes.** They will never denote string or character literals in the expression language. String data (paths, titles, pack metadata) lives in TOML sidecar files only.
3. **Juxtaposition means unit attachment, never multiplication.** `2 L` is "2 of unit L" and is an error if `L` is not a registered unit. Multiplication is always explicit (`2*L`). This single rule eliminates the largest ambiguity class in the language.
4. **Whitespace is meaningful in exactly three places** (unit-expression continuation, tick attachment, scientific-notation attachment) and nowhere else. Each is specified below; everything else is whitespace-insensitive.
5. **Deterministic longest-match lexing with bounded lookahead.** The lexer may look ahead a bounded distance to resolve the feet-inch compound, and must backtrack cleanly when the compound fails to complete.
6. **kip has no statement order, no bindings, no sheet.** An expression is parsed and evaluated against a caller-supplied symbol resolver; `define`/`dimension` lines are input to the registry builder, resolved order-free among themselves (unit-to-unit references form their own small dependency graph). Everything above that — binding names to expressions, dependency graphs, display directives — is the host application's concern (Appendix A).
7. **Everything the parser produces is immutable and shareable.** The grammar makes no demand for mutation during parse or evaluation; ASTs are frozen on construction so evaluation can run concurrently across threads with no coordination.

---

## 2. Character classes

| Class | Definition |
|---|---|
| `letter` | Unicode categories Lu/Ll/Lt/Lm/Lo; Greek block U+0370–U+03FF explicitly supported (`φ`, `λ`, `Δ`, …) |
| `digit` | `0`–`9` |
| `prime` | `'` (U+0027). Typographic prime `′` (U+2032) is accepted as an alias and normalized by the lexer. |
| `dquote` | `"` (U+0022); double prime `″` (U+2033) accepted as alias |
| `ws` | space, tab. **Newline terminates a registry-definition line; inside an expression it is ws** (see §6.2 for the continuation rule). |
| `dot_op` | `·` (U+00B7) and `×` (U+00D7) are accepted aliases for `*` **inside unit expressions only** |

---

## 3. Tokens

### 3.1 Numbers

```ebnf
INT      = digit { digit | "_" } ;
DECIMAL  = INT "." { digit | "_" }
         | "." digit { digit | "_" } ;
SCI      = ( INT | DECIMAL ) ("e"|"E") [ "+" | "-" ] INT ;   (* NO whitespace anywhere inside *)
NUMBER   = SCI | DECIMAL | INT ;
```

- `_` is the digit-group separator: `29_000 ksi`. **Commas are not digit separators** — `f(1,000)` must unambiguously be a two-argument call. If the lexer sees `digit "," digit digit digit` it emits hint lint `L-COMMA-GROUP` ("did you mean 29_000?") but tokenizes as two numbers separated by a comma.
- Scientific notation binds tightly: `1e3` is 1000; `1 e3` is `NUMBER(1)` followed by `IDENT(e3)` (a unit-attachment attempt on `e3` — almost certainly an error, reported as unknown unit).
- Numbers are lexed as exact decimal strings and converted to rational where representable (`0.5` → 1/2); the evaluator decides rational-vs-float, not the lexer.

### 3.2 Identifiers (variables, unit names, function names)

```ebnf
IDENT = letter { letter | digit | "_" | prime } ;
```

- Prime is legal **anywhere after the first character**: `f'c`, `f''`, `L'`, `x'_max`.
- An identifier never *starts* with a prime or a digit.
- Unit names and variable names share this token type; they are distinguished **only by syntactic position** (§5.1) and live in separate namespaces. Registered unit names may additionally contain `°`, `$`, `%`, `Ω`, `μ` (`°F`, `$`, `μin`) — legal only when the name is in unit position or introduced by `define`.
- Reserved words: `define`, `dimension` (kip registry language); `show`, `in`, `as`, `options` (reserved on behalf of the sheet-layer convention, Appendix A, so identifiers stay portable across kip-based applications). None are usable as variable names.

### 3.3 Feet, inches, and the compound literal

These are **single atomic tokens** produced by the lexer, not parse-tree constructs. This is what makes `2*12'-6"` mean `2 × 12.5 ft` with no precedence gymnastics.

```ebnf
frac      = INT "/" INT ;                                (* 1/2, 3/16 *)
mixed     = INT ( ws | "-" ) frac ;                      (* 6 1/2  or  6-1/2 *)
inch_val  = mixed | frac | NUMBER ;

FEET      = NUMBER prime ;                               (* 12'   — no ws before ' *)
INCHES    = inch_val dquote ;                            (* 6", 6.5", 1/2", 6 1/2", 6-1/2" *)
FTIN      = NUMBER prime [ws] [ "-" [ws] ] inch_val dquote ;
                                                         (* 12'-6", 12' 6", 12'-6 1/2", 12' - 6" *)
```

Rules, in priority order:

- **R1 (attachment):** `'` or `"` following a digit **with no intervening whitespace** is a length tick. `'` following a letter inside an identifier is a prime. `12 '` (space before tick) is lex error `E-TICK-SPACE`.
- **R2 (compound, longest match):** after a candidate `FEET`, the lexer scans forward through optional ws, optional `-`, optional ws, for `inch_val dquote`. If the full pattern completes, one `FTIN` token is emitted. If it does not complete (e.g. the `-` is followed by an identifier, or the number lacks a trailing `"`), the lexer **backtracks** and emits `FEET`, then re-lexes from the `-` normally (as MINUS).
- **R3 (spaced hyphen is still the compound):** `12' - 6"` lexes as `FTIN` = 12.5 ft, matching drafting convention, **with lint `L-FTIN-SPACED`** on first occurrence per parse session ("interpreted as 12'-6" = 12.5 ft; write `12 ft - 6 in` for subtraction"). To force subtraction, use unit keywords or parentheses: `12 ft - 6 in`, or `(12') - 6"`.
- **R4 (no bare ticks):** `'` and `"` never begin a token. `'foo'` is a lex error.
- **R5 (exactness):** `FEET`, `INCHES`, `FTIN` carry exact rational values in inches: `12'-6 1/2"` → `299/2 in`. Fractional inches never round-trip through floats.
- **R6 (mixed-number scope):** the mixed forms `6 1/2` and `6-1/2` are legal **only** inside `INCHES`/`FTIN` (immediately closed by `"`). In ordinary expressions, `6 1/2` is a syntax error (juxtaposed numbers) and `1/2` is ordinary division — numerically identical, so nothing is lost.

### 3.4 Operators and punctuation

```
+  -  *  /  ^  (  )  ,  =  .  #
·  ×          (unit-expression aliases for *)
::            (reserved for the sheet-layer annotation convention, Appendix A)
>=  <=  >  <  ==   (reserved for check expressions, v1.1)
```

`#` begins a comment running to end of line. `=` appears only in the registry-definition language (§6) and the sheet-layer convention (Appendix A); it is not an expression operator.

---

## 4. Tokenizer state machine

`emit(T)` produces token T; `BT` is backtrack to a saved position. The `FTIN?` scan never crosses a newline.

| # | State | Input | Action → Next state |
|---|---|---|---|
| 0 | `START` | ws | skip → `START` |
| 1 | `START` | digit or `.`digit | begin number → `NUM` |
| 2 | `START` | letter | begin ident → `IDENT` |
| 3 | `START` | `'` or `"` | **error `E-BARE-TICK`** (R4) |
| 4 | `START` | `#` | consume to EOL → `START` |
| 5 | `NUM` | digit, `_`, first `.`, tight `e/E±`digits | accumulate → `NUM` |
| 6 | `NUM` | `'` (no ws) | save position → `FTIN?` |
| 7 | `NUM` | `"` (no ws) | emit `INCHES` → `START` |
| 8 | `NUM` | ws, then lookahead sees `'`/`"` | emit `NUMBER`; **error `E-TICK-SPACE`** (R1) |
| 9 | `NUM` | any other | emit `NUMBER` → `START` (re-read char) |
| 10 | `FTIN?` | scan `[ws] ["-"] [ws] inch_val "` completes | emit `FTIN` → `START`; spaced hyphen → lint `L-FTIN-SPACED` (R3) |
| 11 | `FTIN?` | scan fails at any point | `BT`; emit `FEET` → `START` (R2) |
| 12 | `IDENT` | letter, digit, `_`, `'` | accumulate → `IDENT` (R1: prime after letter is interior) |
| 13 | `IDENT` | any other | emit `IDENT` → `START` (re-read char) |

The tokenizer is a pure function of its input string; it holds no state between calls and is trivially usable from multiple threads.

---

## 5. Expression grammar (normative — the kip language)

### 5.1 Quantity literals and unit expressions

A unit expression attaches to a `NUMBER` by juxtaposition (`FEET`/`INCHES`/`FTIN` arrive pre-dimensioned). **Whitespace rule W1:** the number and the first unit identifier may be separated by ws (`2 kip` and `2kip` both legal), but operators *inside* a unit expression must be **tight** — no surrounding ws. A spaced operator terminates the unit expression and returns to ordinary expression parsing.

```ebnf
quantity   = NUMBER [ unit_expr ]                       (* juxtaposition = attachment *)
           | FEET | INCHES | FTIN ;

unit_expr  = unit_term { tight_op unit_term } ;         (* tight_op: * · × /  with NO ws *)
unit_term  = UNIT_IDENT [ "^" unit_exp ] ;              (* ^ also tight *)
unit_exp   = [ "-" ] INT
           | "(" [ "-" ] INT "/" INT ")"                (* psi^(1/2) *)
           | [ "-" ] DECIMAL ;                          (* psi^0.5 *)
```

Consequences:

- `2 kip*ft` → quantity `2 kip·ft`.
- `2 kip * L` → `(2 kip) * L` — the spaced `*` ends the unit expression; `L` resolves as a symbol via the caller's resolver.
- `2 kip*L` → attachment attempt on `kip·L`; if `L` is not a registered unit → `E-UNKNOWN-UNIT` with hint *"if you meant multiplication, write `2 kip * L`."*
- `120 lbf/ft^2` → one quantity. `120 lbf / A` → `(120 lbf) / A`.
- `UNIT_IDENT` is an `IDENT` resolved against the **frozen `Registry` supplied to the parse call**. Because the registry is immutable per generation, resolution is deterministic and thread-safe; a re-parse against a newer registry generation may legally resolve differently (the host application owns generation swaps). Resolution failure is a name error, never a re-parse.
- **Shadow lint `L-UNIT-SHADOW`:** if a name is a registered unit *and* the caller's symbol resolver reports it as a known symbol, every use is flagged with its resolved meaning (unit, by the juxtaposition rule). Emitting this lint requires the resolver to be consulted at parse-check time; it is optional for hosts that parse without a resolver.

### 5.2 Operator precedence (Pratt table)

| Level | Operators | Assoc | Notes |
|---|---|---|---|
| 1 (loosest) | `>= <= > < ==` | none | **reserved, v1.1** — pass/fail check expressions; operands must unify dimensionally |
| 2 | `+ -` (binary) | left | dimension unification; leftmost-wins unit for the result |
| 3 | `* /` (expression context) | left | dimension composition, no conversion |
| 4 | `-` (unary) | prefix | binds looser than `^`: `-2^2 = -(2^2) = -4` |
| 5 (tightest) | `^` | right | `2^3^2 = 2^(3^2)`; exponent must be dimensionless (symbolic exponents allowed only if inferably dimensionless) |
| atom | quantity, IDENT, call, `( expr )` | — | |

```ebnf
expr      = cmp ;
cmp       = add [ cmp_op add ] ;               (* v1.1, reserved *)
add       = mul { ("+" | "-") mul } ;
mul       = unary { ("*" | "/") unary } ;
unary     = "-" unary | pow ;
pow       = atom [ "^" unary ] ;               (* right assoc; unary allowed in exponent: x^-2 *)
atom      = quantity
          | IDENT [ call_args ]                (* function call *)
          | path call_args                     (* namespaced code equation: ACI.fr(...) *)
          | "(" expr ")" ;
path      = IDENT { "." IDENT } ;
call_args = "(" [ arg { "," arg } ] ")" ;
arg       = IDENT ":" expr                     (* named — REQUIRED for code equations *)
          | expr ;                             (* positional — allowed for math functions *)
```

Built-in functions (lexically ordinary `IDENT`s): `sqrt`, `abs`, `min`, `max`, `floor`, `ceil`, `round`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `ln`, `log10`, `exp`. Trig requires the pseudo-dimension *angle* (`deg`, `rad` are its units — kept as a tracked dimension to catch deg/rad bugs); `ln/log10/exp` require dimensionless arguments; `sqrt` halves dimension exponents. Semantic constraints, not grammar.

Code-equation calls (`ACI.fr(fc: f'c, lambda: 1.0)`) parse identically to function calls; the named-argument requirement and unit-contract conversion are enforced by the evaluator against the equation pack loaded into the registry. Positional args to a code equation → `E-CODE-POSITIONAL`.

### 5.3 Free symbols

Any `IDENT` in expression position that is neither a built-in function, a code-equation path, nor resolved to a value by the caller's symbol resolver is a **free symbol**. This is not an error: evaluation returns a `Symbolic` residual with the free symbols intact, plus the dimension constraints the expression structure imposes on them. Whether an unresolved symbol is acceptable, an error, or a "still waiting on input" state is host-application policy.

---

## 6. Registry-definition grammar (normative — input to `RegistryBuilder`)

kip accepts `define`/`dimension` lines as text so that host applications and equation packs share one syntax for custom units. These lines are **not expressions**; they are consumed by the registry builder (e.g. `RegistryBuilder::parse_defs(src)`), which resolves them order-free (unit-to-unit references form a small dependency graph; cycles are `E-DEF-CYCLE`) and then freezes into an immutable `Registry`.

```ebnf
def_src        = { def_line } ;
def_line       = [ def_stmt ] [ comment ] NEWLINE ;
def_stmt       = define_stmt | dimension_stmt ;

define_stmt    = "define" IDENT { "," IDENT } "=" expr    (* define kip, kips = 1000 lbf *)
               | "define" IDENT { "," IDENT } ":" IDENT ; (* primary unit of new dimension:
                                                             define USD, $ : Currency *)
dimension_stmt = "dimension" IDENT ;                      (* dimension Currency *)
```

Constraints:

- The `expr` on the right of a linear `define` must evaluate to a `Known` quantity using only previously-defined or built-in units — no free symbols (`E-DEF-SYMBOLIC`).
- **Affine units (`°F`, `°C`, `°R`, `K`) are built-in, not definable in v1.** Linear `define` only; an affine form is reserved (`E-AFFINE-DEFINE` points at the built-ins).
- Duplicate unit names within one builder generation → `E-DUP-UNIT`, both sites reported. (This replaces Draft 1's sheet-level `E-DUP-BINDING`, which is now an application concern.)
- A registry, once frozen, is immutable; adding definitions produces a *new generation* via copy-on-write extension. The grammar is unaffected, but diagnostics carry the generation they were resolved against.

### 6.1 Comments

`#` to end of line, in both expression and definition source.

### 6.2 Newlines and continuation

A single expression string handed to `kip::parse` may span lines: a newline is ordinary whitespace when parentheses are unbalanced or the previous line ends with an infix operator (`+ - * / ^ ,`); otherwise it is end-of-input for the expression. In definition source, each `define`/`dimension` occupies one logical line under the same continuation rule. No backslash continuation.

---

## 7. Disambiguation rules — consolidated

| ID | Rule |
|---|---|
| **D1** | `'`/`"` after a digit, no ws → length tick. `'` after a letter within an identifier → prime. Ws before a tick → error. |
| **D2** | `NUMBER ' [ws] [- [ws]] inch "` lexes as one atomic `FTIN` token (longest match, bounded lookahead, clean backtrack). |
| **D3** | Spaced hyphen inside the compound is still the compound (`12' - 6"` = 12.5 ft) + lint. Subtraction requires unit keywords or parens. |
| **D4** | Juxtaposition (`NUMBER IDENT`) is unit attachment only — never implicit multiplication. Unknown unit → error with "did you mean `*`" hint. |
| **D5** | Unit-expression operators are tight; spaced operators return to expression context. |
| **D6** | Scientific notation is tight; `1 e3` is not scientific. |
| **D7** | Mixed numbers (`6 1/2`, `6-1/2`) exist only immediately before `"`. |
| **D8** | Unit and symbol namespaces are separate; syntactic position resolves; shadowing lints. |
| **D9** | `·`/`×` are `*` aliases inside unit expressions only. |

---

## 8. Edge-case test list (lexer + parser conformance suite)

Every entry states input → expected result. These are the day-one regression tests. Unless marked otherwise, entries are single expressions evaluated with an empty symbol resolver against the built-in imperial registry.

### Ticks and compounds

| Input | Expect |
|---|---|
| `3'` | `FEET`, exact 36 in |
| `0'-6"` | `FTIN`, 6 in |
| `12'-0"` | `FTIN`, 144 in (drawings write this constantly) |
| `12'-6"` | `FTIN`, 150 in |
| `12' 6"` | `FTIN`, 150 in |
| `12' - 6"` | `FTIN`, 150 in + lint `L-FTIN-SPACED` |
| `12'-6 1/2"` | `FTIN`, exact 299/2 in |
| `12'-6-1/2"` | `FTIN`, exact 299/2 in (dash-fraction form) |
| `1/2"` | division? **No** — `1/2` followed tightly by `"`: `INCHES`, exact 1/2 in (inch scan takes precedence when the `"` closes it) |
| `6 1/2"` | `INCHES`, exact 13/2 in |
| `2*12'-6"` | `2 * FTIN(12.5 ft)` = 25 ft (atomic token wins) |
| `(2*12') - 6"` | 24 ft − 6 in = 23.5 ft (parens force subtraction) |
| `12 ft - 6 in` | 11.5 ft (leftmost-wins) |
| `12 '` | error `E-TICK-SPACE` |
| `12' - L` | `FEET(12 ft)` minus free symbol `L` → `Symbolic`, constraint dim(L) = length (compound scan fails: no trailing `"` → backtrack) |
| `12'-x"` | scan fails at `x` → backtrack → `FEET MINUS IDENT(x)` then bare `"` → `E-BARE-TICK` with a diagnostic pointing at the whole construct |
| `5'-13"` | `FTIN`, 73 in — **legal**; lint `L-INCH-GE-12` ("inch part ≥ 12; intentional?") |

### Identifiers and primes

| Input | Expect |
|---|---|
| `f'c` | one `IDENT` |
| `f''` | one `IDENT` |
| `L'` | one `IDENT` (L-prime) |
| `f' c` | `IDENT(f')` `IDENT(c)` → juxtaposed identifiers → syntax error (no implicit mult) |
| `φ_b`, `lambda`, `Δ_max` | single identifiers |
| `'foo` | error `E-BARE-TICK` |
| `2L'` | attachment attempt on unit `L'` → `E-UNKNOWN-UNIT` (+ hint) unless such a unit is defined |

### Numbers

| Input | Expect |
|---|---|
| `29_000 ksi` | 29000 ksi |
| `29,000 ksi` | `NUMBER(29) COMMA NUMBER(000)` + lint `L-COMMA-GROUP` suggesting `_` |
| `1e3 lbf` | 1000 lbf |
| `1 e3` | attachment attempt on `e3` → `E-UNKNOWN-UNIT` |
| `.5 in` | exact 1/2 in |
| `2^-3` | 1/8, dimensionless |
| `-2^2` | −4 |

### Units, juxtaposition, and symbols

| Input | Expect |
|---|---|
| `2 kip*ft` | quantity 2 kip·ft |
| `2 kip * L` | `(2 kip) * L`, `L` free symbol → `Symbolic` |
| `2 kip*L` | `E-UNKNOWN-UNIT(kip·L)` + "did you mean `2 kip * L`" (assuming no unit `L`) |
| `120 lbf/ft^2` | one quantity |
| `4000 psi^0.5` | **legal literal**, dimension pressure^(1/2) — required so symbolic residuals round-trip through text |
| `sqrt(4000 psi)` | ≈ 63.246 psi^(1/2) |
| `9 in^2` vs `9 in ^2` | first: 9 in²; second: `(9 in)^2` = 81 in² — tight-`^` rule; lint `L-SPACED-CARET` on the second |
| `1 ft + 6 in` | 1.5 ft (leftmost-wins) |
| `6 in + 1 ft` | 18 in |
| `5 s` with a resolver that also knows symbol `s` | unit `s` (juxtaposition position) + lint `L-UNIT-SHADOW` |
| `2*f_r + f'c`, resolver knows `f_r = 450 psi` | `Symbolic(900 psi + f'c)`, constraint dim(f'c) = pressure |
| `M = …` style input | **not an expression** — `=` is rejected in expression source (`E-EQ-IN-EXPR`, hint: bindings are an application feature; see sheet-layer convention) |
| `sqrt(f'c) + f'c` (both free) | `E-DIM-MISMATCH` from constraint unification — no dimension assignment satisfies d^(1/2) + d |
| `ACI.fr(fc: f'c, lambda: 1.0)` | code-equation call node; `f'c` pinned to pressure by the contract before any value exists |
| `ACI.fr(4000 psi, 1.0)` | `E-CODE-POSITIONAL` |

### Registry definitions (input to `RegistryBuilder::parse_defs`)

| Input | Expect |
|---|---|
| `define kip, kips = 1000 lbf` | two aliases, exact ratio |
| `dimension Currency` · `define USD, $ : Currency` then expression `12 $/ft^2` | new base dimension flows through |
| `define a = 2 b` · `define b = 3 a` | `E-DEF-CYCLE`, both sites reported (order-free resolution, cycle detected) |
| `define x = 2 * L` (`L` not a unit) | `E-DEF-SYMBOLIC` — definitions must be fully known |
| `define kip = 1000 lbf` twice in one generation | `E-DUP-UNIT`, both sites reported |
| `define degC = …` | `E-AFFINE-DEFINE` pointing at built-ins |

### Adversarial / fuzz seeds

`12'` at EOF · `12'-` · `12'-6` · `12'-6 1/` · `12'-6 1/2` (incomplete compounds → clean backtrack or clean EOF error, never a panic) · `''` · `""` · `2''` · `2""` · `3'4'` · `1/0"` (zero denominator → `E-DIV-ZERO-LITERAL`) · `f'c'` (legal identifier) · parens nested 64 deep · `2^2^2^2` chains · a 10 000-term sum (parser must be iterative or heap-recursing — stack safety is a conformance requirement, not a nicety) · the same `Expr` evaluated simultaneously from 8 threads against a shared `Arc<Registry>` (results identical, no data races under Miri/loom — the concurrency contract is part of conformance).

---

## 9. Diagnostics inventory

| Code | Meaning |
|---|---|
| `E-TICK-SPACE` | whitespace between number and `'`/`"` |
| `E-BARE-TICK` | `'`/`"` starting a token |
| `E-UNKNOWN-UNIT` | juxtaposed identifier not in the supplied registry (carries "did you mean `*`" hint when the resolver knows a same-named symbol) |
| `E-EQ-IN-EXPR` | `=` encountered in expression source; bindings are an application-layer feature |
| `E-DIM-MISMATCH` | value or constraint set is dimensionally unsatisfiable; message cites the constraining site and the inferred dimension's provenance |
| `E-CODE-POSITIONAL` | positional args passed to a code equation |
| `E-AFFINE-DEFINE` | attempt to `define` an affine unit |
| `E-DUP-UNIT` | unit name defined twice in one registry generation |
| `E-DEF-CYCLE` | circular unit definitions |
| `E-DEF-SYMBOLIC` | unit definition references a free symbol |
| `E-DIV-ZERO-LITERAL` | zero denominator in a fraction literal |
| `L-FTIN-SPACED` | spaced hyphen interpreted as feet-inch compound |
| `L-INCH-GE-12` | inch part ≥ 12 inside a compound |
| `L-UNIT-SHADOW` | name is both a registered unit and a resolver-known symbol |
| `L-SPACED-CARET` | ws-separated `^` changed binding relative to the tight form |
| `L-COMMA-GROUP` | comma used as an apparent digit-group separator |

Removed from Draft 1: `E-DUP-BINDING` (sheet concern — see Appendix A, which recommends the equivalent policy to host applications).

---

## 10. Open items (deliberately deferred)

1. **Check expressions** (`>=` etc., level-1 precedence) — grammar slot reserved; semantics (pass/fail objects, demand/capacity reporting) belong to the code-equation layer design.
2. **Affine `define` form** for user-defined temperature-like scales.
3. **Vector/matrix literals** — out of scope for v1; `[` `]` remain unclaimed so nothing is foreclosed.
4. **Survey foot naming** — registry concern, not grammar; US survey foot ≠ international foot, `sft`/`ft_survey` to be settled in the registry seed-data doc.
5. **Percent (`%`)** — proposal: a dimensionless unit with exact ratio 1/100 (`5% = 1/20`), pending the decision on `%` in the unit-name character set.

---

## Appendix A — Sheet-layer statement convention (non-normative)

kip does not implement statements, bindings, or sheets. This appendix preserves Draft 1's statement grammar as a **recommended convention** for applications built on kip (worksheet apps, calc-pad UIs, CLIs), so that user-facing notation stays consistent across the ecosystem. An application adopting it parses these forms itself and hands the right-hand-side expressions to kip.

```ebnf
sheet        = { line } ;
line         = [ statement ] [ comment ] NEWLINE ;

statement    = binding
             | annotation
             | show_stmt
             | options_stmt
             | def_stmt ;                               (* forwarded to RegistryBuilder *)

binding      = IDENT [ "::" unit_expr ] "=" expr ;      (* annotation may ride along *)
annotation   = IDENT "::" unit_expr ;                   (* dimension/display constraint, no value *)

show_stmt    = "show" IDENT "in" unit_expr
             | "show" IDENT "as" "ftin" [ "(" frac ")" ] ;   (* show L as ftin(1/16) *)

options_stmt = "options" "(" IDENT ":" option_val { "," IDENT ":" option_val } ")" ;
option_val   = IDENT | NUMBER | frac ;
```

Recommended semantics for conforming applications:

- **Bindings are order-free and unique.** Build a dependency graph from free symbols; topo-sort; evaluate ready nodes (in parallel if desired — kip's evaluator is pure and `Send + Sync`). A second binding of a name is a hard error (Draft 1's `E-DUP-BINDING`), both sites reported.
- **`::` feeds kip's constraint machinery.** `f'c :: psi` becomes an externally-supplied dimension constraint merged into the `ConstraintSet` the application folds across its graph. A later value that conflicts surfaces kip's `E-DIM-MISMATCH`, and the application should cite the `::` site.
- **`show` and `options` are display policy** — preferred output units, ft-in rendering with denominator snapping — implemented via kip's formatting API, never by mutating stored values.
- **Reserved words and the `::`/`=` tokens** are reserved in kip's lexer (§3.2, §3.4) precisely so this convention can exist without colliding with expression syntax.
