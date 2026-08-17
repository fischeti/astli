# svirig

SystemVerilog language tooling in Rust, built on one lossless syntax tree.
Swiss-German-ish for "difficult", which it is.

**Exploratory and unfinished.** There is nothing to use yet. See
[`docs/plan.md`](docs/plan.md) for what it is meant to become and
[`docs/grammar-coverage.md`](docs/grammar-coverage.md) for how far along it is.

The first product is a formatter. The reusable parts underneath it — a
preprocessor and a lossless syntax tree — are the point.

## Layout

| Path | |
| --- | --- |
| `crates/svirig-syntax` | Lexer, preprocessor, parser, CST |
| `docs/` | Design and planning |
| `scripts/fetch-corpus.sh` | Fetches real SystemVerilog into a gitignored `corpus/` |

## Licence

MIT or Apache-2.0, at your option.
