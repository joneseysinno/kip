# kip

Pure, thread-safe engineering expression evaluator for **US imperial units**, with exact rational arithmetic, partial (symbolic) evaluation, empirical code equations, and a user-extensible unit registry.

## Status: 0.1.0 (M0 + M1 lexer + M2 registry)

M0 skeleton, **M1 lexer** (grammar §3–§4), and **M2 registry** (§6) are implemented.
Parser (M3), evaluator (M4), and equation packs (M6) follow.

```rust
use kip::RegistryBuilder;

let mut b = RegistryBuilder::from_seed();
b.parse_defs("define tonf, tons = 2000 lbf").unwrap();
let reg = b.freeze();
```

## Three load-bearing requirements

1. **P1 — Parallel branch evaluation.** Immutable ASTs, registries, and pure evaluation — no globals, no locks.
2. **P2 — Empirical engineering expressions as default.** Code equations (`ACI.fr(fc: f'c, lambda: 1.0)`) are the main road.
3. **P3 — User-defined units anchored to inch, lbf, second, Rankine** (plus angle and custom base dimensions).

## Force-based system

kip uses a **force-based** (gravitational) dimensional system: **Force** is fundamental (`lbf`), mass is derived (`slug = lbf·s²/ft`). No hidden *g*<sub>c</sub> in user-visible math.

## Quick start

```toml
[dependencies]
kip = "0.1.0"
```

```rust
use kip::{RegistryBuilder, Dimension, BaseDim};
use num_rational::Ratio;
use num_traits::One;

let reg = kip::RegistryBuilder::from_seed().freeze();
assert!(reg.unit("ft").is_some());

let length = kip::Dimension::single(kip::BaseDim::Length, Ratio::one());
assert!(!length.is_dimensionless());
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `packs` | yes | TOML equation-pack loader (M6) |
| `parallel` | yes | `rayon` batch/scenario helpers |
| `si-display` | no | Display-only SI conversion table |

## License

Licensed under either of MIT or Apache-2.0 at your option.
