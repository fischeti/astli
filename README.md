# astli

The SystemVerilog toolchain in one binary, written in pure Rust: format, lint,
check, elaborate and pick a design apart, the way `cargo` does for Rust. Run
`astli` in a repository and it finds the design itself, with no filelist.

```
astli fmt          # format every file of the package
astli lint         # likely bugs and style, with verible's rule names
astli check        # undeclared names, instances that do not match their module
astli elab         # the instance tree a design builds from each top
astli files        # trim and order a filelist by what a top needs
astli pickle       # one source file from a whole design
```

A package is a directory with an `astli.toml`, which says what its files are
and never lists them. Dependencies are located through
[`bender`](https://github.com/pulp-platform/bender) today. Fetching and
versioning them is the goal, so that astli replaces bender in time.

The [documentation](https://fischeti.github.io/astli/) covers the `astli`
command; the libraries are documented on [docs.rs](https://docs.rs/astli).

## Why

SystemVerilog tooling is a set of separate programs. A formatter, a linter, a
compiler and a dependency manager each carry their own parser, their own idea
of which files make a design, and their own configuration. The same design is
described again for every one of them, as filelists, defines and include
paths, and the tools can disagree about what it contains.

Rust's answer to this was one tool, `cargo`, over one model of a package. astli
is that for SystemVerilog: one frontend and one lossless tree under every
stage, one manifest, one command.

[slang](https://github.com/MikePopoloski/slang) is the state of the art
frontend, and written in C++. A Rust tool that builds on it goes through an FFI
that returns opaque objects, needs a binding for every node and method it
touches, and brings a slow C++ build along. astli is a frontend in Rust, so
the tree is ordinary Rust data that a tool can walk, query and print back
exactly, comments and whitespace included.

## How it compares

Each of these is good at what it was made for, and several are the better
choice today. astli tries to cover the ground between them with one parser and
one manifest.

| | astli | [slang](https://github.com/MikePopoloski/slang) | [verible](https://github.com/chipsalliance/verible) | [bender](https://github.com/pulp-platform/bender) | [sv-parser](https://github.com/dalance/sv-parser) |
| --- | :-: | :-: | :-: | :-: | :-: |
| Written in Rust | ✅ | ❌ | ❌ | ✅ | ✅ |
| Lossless syntax tree | ✅ | ✅ | ✅ | ❌ | ✅ |
| Preprocessor | ✅ | ✅ | ◐ | ❌ | ✅ |
| Full language coverage | ◐ | ✅ | ◐ | ❌ | ✅ |
| Formatter | ✅ | ❌ | ✅ | ❌ | ❌ |
| Linter | ✅ | ◐ | ✅ | ❌ | ❌ |
| Name resolution and elaboration | ✅ | ✅ | ❌ | ❌ | ❌ |
| Finds a design without a filelist | ✅ | ❌ | ❌ | ❌ | ❌ |
| Fetches dependencies | ❌ | ❌ | ❌ | ✅ | ❌ |
| Language server | ❌ | ◐ | ✅ | ❌ | ❌ |

✅ yes, ◐ partly, ❌ no. astli locates dependencies through bender but does not
fetch them yet, and its grammar leaves some rarer constructs verbatim. The
others are my reading of them and may be out of date; corrections welcome.

## Scope

astli covers the path from source to a checked design: preprocessing, parsing,
name resolution, constants, types and elaboration. It is not a simulator or a
synthesis tool and does not aim to be. A dependency manager and a language
server are what could come next, both built on the same tree.

## Status

Pre-1.0, and the API and the output still change. What exists:

- **Package discovery.** `astli.toml` and `.gitignore` find a design's files and
  headers, and dependencies are located through bender. Path dependencies,
  features, overrides, and then versions, git sources and a lockfile follow
  ([`docs/packages.md`](https://github.com/fischeti/astli/blob/main/docs/packages.md)).
- **Preprocessor.** All directives, macro expansion, includes, and filelists
  with `+incdir+` and `+define+`.
- **Parser.** Design units, declarations, classes, instances, generate blocks,
  statements and expressions, constraints, assertions, covergroups and
  clocking blocks. Rarer constructs such as `specify`, `checker`, gate
  primitives and UDPs are kept verbatim for now, so nothing is ever lost;
  [`docs/grammar-coverage.md`](https://github.com/fischeti/astli/blob/main/docs/grammar-coverage.md)
  has the detail.
- **Formatter.** The [lowRISC style](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md).
  It checks that every result preprocesses to the same thing as its input, and
  refuses a file rather than change what it means.
- **Linter.** Rules for likely bugs and for the lowRISC style, with verible's
  names where verible has the rule, waived by attribute or by path.
- **Checker and elaborator.** Names are resolved across a design, constants
  evaluated and the instance tree built, and instances are checked against the
  ports and parameters of their definitions.
- **Filelists.** Trimmed to what a top needs, put in dependency order, or
  pickled into one source.

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

With pip, or [pipx](https://pipx.pypa.io) to install it in an environment of
its own or run it once:

```
pip install astli
pipx install astli
pipx run astli fmt top.sv
```

With cargo, as a prebuilt binary through
[cargo-binstall](https://github.com/cargo-bins/cargo-binstall), or from source:

```
cargo binstall astli-cli
cargo install astli-cli
```

## A package

`astli.toml` at the root marks a package. Without one, the root of the git
repository is, and the defaults apply. The commands above, run with no files
named, read the whole package; a file or `-f filelist` on the command line turns
discovery off.

```toml
[package]
name = "my_soc"
sources = ["hw"]               # where the walk looks; default ["."]
include-dirs = ["hw/include"]  # headers a dependent searches; default ["include"]
exclude = ["hw/scripts/**"]    # on top of .gitignore
```

## Formatting

```
astli fmt rtl/*.sv          # rewrite in place
astli fmt                   # every file of the package here
astli fmt --check -f src.f  # fail if any file in a filelist is unformatted
astli fmt --diff top.sv     # show what would change
astli fmt -                 # stdin to stdout
```

Each file is formatted on its own: includes are not followed and no
`+define+` reaches the formatter, so the output depends only on the file.

[astli-pre-commit](https://github.com/fischeti/astli-pre-commit) runs it as a
[pre-commit](https://pre-commit.com) hook.

An `astli.toml` sets the width, the indentation and how far a column may pad
to line up; these are the defaults:

```toml
[fmt]
width = 100
indent = 2
max-pad = 12
```

To keep an item, statement or member as you wrote it, put the attribute
`(* astli_fmt_skip *)` on it. It is left as written and only moved to its
indentation:

```systemverilog
(* astli_fmt_skip *)
assign out = sel ? a
               : b;
```

## Linting

```
astli lint -f src.f                 # report what the rules find
astli lint -W correctness rtl/*.sv  # report a group as warnings
astli lint --list                   # every rule, its group and its level
```

Rules come in groups: `correctness` fails the run, `suspicious` and `lowrisc`
warn, `restriction` is off. To turn a rule off for one construct, put an
attribute on it:

```systemverilog
(* astli_allow = "always-ff-non-blocking" *)
always_ff @(posedge clk_i) q = d;
```

An `astli.toml` sets levels for a project, and per path for what no attribute
reaches:

```toml
[lint.paths]
"hw/vendor/**" = { allow = ["lowrisc"] }
```

## Checking

```
astli check                      # the package, against its dependencies
astli check -f src.f --top soc   # elaborate from soc and check what it builds
astli elab --top soc             # print the instance tree
```

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

`astli pickle` takes the same selection and writes the files as one source,
each as written with its headers inlined. With `--expand`, each is written as
a compiler reads it instead, and `--prefix` and `--suffix` can rename every
module, interface, program, package and class the files declare, wherever it
is named, so that two designs can share a compilation:

```sh
astli pickle -f design.f --top soc_top --order > soc.sv
astli pickle -f design.f --top soc_top --order --expand --prefix v2_ > soc.sv
```

`--exclude-rename NAME` keeps a name as it is.

## As a library

```toml
[dependencies]
astli = "0.2"
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

print!("{}", astli::fmt::format(&tree, &Default::default())?);
```

`astli` re-exports each `astli-*` crate as a module; they can also be used
on their own.
[`docs/api.md`](https://github.com/fischeti/astli/blob/main/docs/api.md)
explains the shape of the API.

## Development

`astli-cli` is the workspace's default member, so `cargo run` needs no `-p`.
The driver also has one subcommand per stage, each printing what that stage
made of a file:

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
| `crates/astli-sema` | The HIR a tree lowers to, and what it means |
| `crates/astli-lint` | Lint rules over one file's tree |
| `crates/astli-package` | Finding a package's root and reading its `astli.toml` |
| `crates/astli-resolve` | Fetching dependencies into the shared cache |
| `crates/astli` | The umbrella: every library crate but `astli-resolve`, as a module |
| `crates/astli-cli` | The driver, a binary named `astli` |
| `docs/` | Design and planning |
| `site/` | The documentation site, built by `scripts/site.sh` |

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
