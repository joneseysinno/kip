# kip

Pure, thread-safe engineering expression evaluator for **US imperial units**, with exact rational arithmetic, partial (symbolic) evaluation, empirical code equations, and a user-extensible unit registry.

**Status: 0.1.0** — M0–M8 complete (lexer through release hardening).

```rust
use kip::{eval, parse, RegistryBuilder, EmptyResolver};

let reg = RegistryBuilder::from_seed().freeze();
let expr = parse("12 ft - 6 in", &reg).unwrap();
let value = eval(expr.as_ref(), &reg, &EmptyResolver).unwrap();
println!("{}", value.quantity().unwrap().display(&reg, &kip::FmtOptions::calc_sheet()));
```

## Three load-bearing requirements

1. **P1 — Parallel branch evaluation.** Immutable ASTs, registries, and pure evaluation — no globals, no locks.
2. **P2 — Empirical engineering expressions as default.** Code equations (`ACI.fr(fc: f'c, lambda: 1.0)`) are the main road.
3. **P3 — User-defined units anchored to inch, lbf, second, Rankine** (plus angle and custom base dimensions).

## Force-based system

kip uses a **force-based** (gravitational) dimensional system common in structural engineering: **Force** is fundamental (`lbf`), mass is derived (`slug = lbf·s²/ft`). There is no hidden *g*<sub>c</sub> in user-visible math. SI-trained users: do not expect mass to be fundamental here.

## Quick start

```toml
[dependencies]
kip = "0.1.0"
```

```rust
use kip::RegistryBuilder;

let mut b = RegistryBuilder::from_seed();
b.parse_defs("define tonf, tons = 2000 lbf").unwrap();
let reg = b.freeze();
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `packs` | yes | TOML equation-pack loader |
| `parallel` | yes | `rayon` batch/scenario helpers + intra-expr join |
| `si-display` | no | Display-only SI table (reserved) |

## Examples

- `examples/sheet.rs` — topo-sort host pattern with `eval_batch`
- `examples/sweep.rs` — partial eval + parallel `Value::bind` sweep

## Development

```bash
cargo test --all-features
cargo bench                    # criterion throughput + parallel scaling
cd fuzz && cargo fuzz run fuzz_lexer -- -runs=2048
```

See [VERSIONING.md](VERSIONING.md) for release policy.

## License

Licensed under either of MIT or Apache-2.0 at your option.
