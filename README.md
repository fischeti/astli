# astli

SystemVerilog language tooling in pure Rust: a preprocessor, a lossless syntax
tree, and a formatter built on them.

## Why

A Rust tool that wants to understand SystemVerilog has had to bind a C++
frontend. That works, but the FFI hands back opaque objects: walking a syntax
tree means writing a binding for every node and method you touch, and the C++
build is slow and hard to cache. [`bender`](https://github.com/pulp-platform/bender)
is where that friction showed up.

astli is the frontend as a Rust library. The tree is ordinary Rust data that
keeps every byte of the source, comments and whitespace included, so a tool can
walk it, query it, and print it back exactly.

The formatter is the first thing built on it. A linter, a language server, or
deeper analysis could follow.

## Scope

astli lexes, preprocesses and parses; it does not elaborate or type-check.
[slang](https://github.com/MikePopoloski/slang) is the state of the art for
SystemVerilog, a complete compiler, and the right tool whenever you need one.
astli does not try to replace it.

## Status

Pre-1.0, and the API still changes. What exists:

- **Preprocessor.** All directives, macro expansion, includes, and filelists
  with `+incdir+` and `+define+`.
- **Parser.** Design units, declarations, classes, instances, generate blocks,
  statements and expressions. Assertions, covergroups and a few rarer
  constructs are kept verbatim for now, so nothing is ever lost;
  [`docs/grammar-coverage.md`](https://github.com/fischeti/astli/blob/main/docs/grammar-coverage.md)
  has the detail.
- **Formatter.** The [lowRISC style](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md).
  It checks that every result preprocesses to the same thing as its input, and
  refuses a file rather than change what it means.
- **Filelists.** Trimmed to what a top needs, and put in dependency order.

All of it is tested against open-source designs, fetched by
[`scripts/fetch-corpus.sh`](https://github.com/fischeti/astli/blob/main/scripts/fetch-corpus.sh).

## Install

The `astli` command comes prebuilt for Linux, macOS and Windows. With the
installer script:

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.sh | sh
```

```
powershell -ExecutionPolicy Bypass -c "irm https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.ps1 | iex"
```

With [uv](https://docs.astral.sh/uv/), or to run it once without installing:

```
uv tool install astli
uvx astli fmt top.sv
```

With cargo, as a prebuilt binary through
[cargo-binstall](https://github.com/cargo-bins/cargo-binstall), or from source:

```
cargo binstall astli-cli
cargo install astli-cli
```

## Formatting

```
astli fmt top.sv            # print the formatted file
astli fmt -w rtl/*.sv       # rewrite in place
astli fmt --check -f src.f  # fail if any file in a filelist is unformatted
astli fmt --diff top.sv     # show what would change
astli fmt -                 # stdin to stdout
```

Each file is formatted on its own: includes are not followed and no
`+define+` reaches the formatter, so the output depends only on the file.

## Filelists

```
astli files -f design.f --top soc_top          # only what soc_top needs
astli files -f design.f --top soc_top --order  # packages before their users
astli files -f design.f --emit tops            # modules nothing instantiates
astli files -f design.f --top soc_top --why rtl/fifo.sv
```

Each file is expanded as it would be compiled, with the filelist's
`+incdir+`s and `+define+`s, so a module a macro instantiates counts. A name
used and declared in no file is a warning.

## As a library

```toml
[dependencies]
astli = "0.1"
```

```rust
use astli::parse::SyntaxTree;
use astli::syntax::ast::{AstNode, ModuleDecl};

let tree = SyntaxTree::read("top.sv")?;

for module in tree.root().descendants().filter_map(ModuleDecl::cast) {
    if let Some(name) = module.name() {
        println!("module {}", name.text());
    }
}

print!("{}", astli::fmt::format(&tree)?);
```

`astli` re-exports each `astli-*` crate as a module; they can also be used
on their own.
[`docs/api.md`](https://github.com/fischeti/astli/blob/main/docs/api.md)
explains the shape of the API.

## Development

`astli-cli` is the workspace's default member, so `cargo run` needs no `-p`.
Besides `fmt`, the driver has one subcommand per stage, each printing what that
stage made of a file:

```
cargo run -- lex        top.sv
cargo run -- preprocess top.sv -I include --emit text
cargo run -- parse      top.sv --quiet
```

The same default member means every other cargo command needs `--workspace`:
`cargo nextest run --workspace`, not `cargo nextest run`. `-P quick` skips the
tests that read the corpus. Hooks run through [`prek`](https://github.com/j178/prek):
`prek install`.

[`docs/plan.md`](https://github.com/fischeti/astli/blob/main/docs/plan.md)
has the architecture and the decisions behind it.

| Path | |
| --- | --- |
| `crates/astli-text` | Spans, the file store, reading a file |
| `crates/astli-diag` | Rendering a diagnostic, with its macro and include chain |
| `crates/astli-syntax` | `SyntaxKind`, the lexer, the `rowan` tree types |
| `crates/astli-preproc` | Directives, macros, includes |
| `crates/astli-parse` | The grammar, and the tree it builds |
| `crates/astli-fmt` | The formatter |
| `crates/astli-index` | The top-level names files declare and use |
| `crates/astli` | The umbrella: every library crate, as a module |
| `crates/astli-cli` | The driver, a binary named `astli` |
| `docs/` | Design and planning |

## Acknowledgements

- [slang](https://github.com/MikePopoloski/slang), the reference for how
  SystemVerilog behaves. The preprocessor is tested against it, file by file.
- [rust-analyzer](https://github.com/rust-lang/rust-analyzer), whose
  architecture this follows: an event-based parser, a lossless tree, and typed
  views generated from a tree grammar.
- [rowan](https://github.com/rust-analyzer/rowan),
  [logos](https://github.com/maciejhirsz/logos) and
  [ungrammar](https://github.com/rust-analyzer/ungrammar), which it is built
  on.
- [lowRISC's style guide](https://github.com/lowRISC/style-guides), which the
  formatter implements.
- The open-source designs in the test corpus, for being real code.

## Licence

MIT or Apache-2.0, at your option. Unless you state otherwise, any contribution
you submit is licensed the same way, without further terms.
