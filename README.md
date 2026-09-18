# svirig

SystemVerilog language tooling in Rust, built on one lossless syntax tree.
Swiss-German-ish for "difficult", which it is.

**Exploratory and unfinished.** There is nothing to use yet. See
[`docs/plan.md`](docs/plan.md) for what it is meant to become and
[`docs/grammar-coverage.md`](docs/grammar-coverage.md) for how far along it is.

The first product is a formatter. The reusable parts underneath it — a
preprocessor and a lossless syntax tree — are the point.

## Running it

One subcommand per stage of the pipeline, each printing what that stage made of
a file. `svirig` is the workspace's default member, so from the root:

```
cargo run -- lex        top.sv
cargo run -- preprocess top.sv -I include --emit text
cargo run -- parse      top.sv --quiet
cargo run -- fmt        top.sv          # declared; not implemented
```

Files are read a thread at a time and printed in the order they were named;
`-j1` reads them one at a time, which is what a timing run wants. `-q` drops
the dump and leaves the summary the run ends with.

`-f design.f` or `-F design.f` reads a filelist instead — sources, `+incdir+`
and `+define+` — differing in whether a relative path inside it is relative to
the working directory or to the filelist.

`cargo run` needing no `-p` is why every command that takes a package needs
`--workspace` spelled out: `cargo nextest run --workspace`, not `cargo nextest
run`.

## Layout

| Path | |
| --- | --- |
| `crates/svirig-text` | Spans, the file store, reading a file |
| `crates/svirig-syntax` | `SyntaxKind`, the lexer, the `rowan` tree types |
| `crates/svirig-preproc` | Directives, macros, includes |
| `crates/svirig-parse` | The grammar, and the tree it builds |
| `crates/svirig` | The driver binary |
| `docs/` | Design and planning |
| `scripts/fetch-corpus.sh` | Fetches real SystemVerilog into a gitignored `corpus/` |

## Licence

MIT or Apache-2.0, at your option.
