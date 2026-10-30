# astli

A modern SystemVerilog toolchain in a single binary, written in pure Rust. Much like `cargo` does for Rust, `astli` formats, lints, and checks your designs while managing dependencies—discovering your project structure automatically without requiring manual filelists.

```sh
astli fmt          # Format every file in the package (lowRISC style)
astli lint         # Catch common bugs and style issues (using Verible-compatible rule names)
astli check        # Verify undeclared names, port mismatches, and multi-driver conflicts
astli elab         # Inspect the elaborated instance hierarchy for a top-level module
astli files        # Generate a trimmed, topologically ordered filelist for a top module
astli pickle       # Flatten an entire design into a single self-contained source file
astli add URL      # Require a git repository at its latest release
astli update       # Resolve dependencies and pin them in astli.lock
astli fetch        # Download dependencies into a shared local cache
```

An `astli` package is simply a directory containing an `astli.toml` manifest. Instead of forcing you to maintain exhaustive lists of source files, `astli` automatically discovers your sources by convention and uses `astli.toml` to manage package settings, features, and dependencies.

> [!TIP]
> **Migrating from Bender?**
> If your project already has a `Bender.yml`, `astli` will automatically read its dependencies whenever none are explicitly defined in `astli.toml`. See the [Coming from Bender guide](https://fischeti.github.io/astli/bender/) for a smooth transition.

Comprehensive CLI documentation is available on the [documentation site](https://fischeti.github.io/astli/), and library APIs are documented on [docs.rs](https://docs.rs/astli).

---

## Why astli?

SystemVerilog workflows have traditionally relied on an assortment of disjoint utilities. A formatter, a linter, a synthesis tool, and a dependency manager often each bundle their own distinct parser, their own dialect quirks, and their own configuration files. As a result, developers are forced to repeatedly describe the same design across filelists, macro defines, and include search paths—and the tools often disagree on how that design should be interpreted.

Rust solved this fragmentation for its ecosystem with `cargo`: one tool, one package model, and one shared compiler frontend. `astli` brings that same experience to SystemVerilog:

- **A single shared frontend**: One preprocessor and one lossless concrete syntax tree (CST) power every stage—formatting, linting, semantic checking, and dependency resolution all share the exact same view of your code.
- **Pure Rust data structures**: While [slang](https://github.com/MikePopoloski/slang) is a phenomenal C++ reference frontend, integrating with it from Rust requires complex FFI bindings, opaque pointers, and hefty C++ toolchain dependencies. `astli` is 100% Rust: syntax nodes are native Rust types that can be walked, transformed, and emitted back to text with comments and formatting perfectly preserved.
- **Zero-configuration discovery**: Run `astli` inside a repository and it discovers your packages and source files on its own.

---

## Feature Comparison

Each of these tools excels in specific areas, and several remain mature industry standards. `astli` unifies their core workflows under a single parser and package manifest.

| Feature | astli | [slang](https://github.com/MikePopoloski/slang) | [verible](https://github.com/chipsalliance/verible) | [bender](https://github.com/pulp-platform/bender) | [sv-parser](https://github.com/dalance/sv-parser) |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Written in Rust** | ✅ | ❌ | ❌ | ✅ | ✅ |
| **Lossless syntax tree** | ✅ | ✅ | ✅ | ❌ | ✅ |
| **Preprocessor** | ✅ | ✅ | ◐ | ❌ | ✅ |
| **Full language coverage** | ◐ | ✅ | ◐ | ❌ | ✅ |
| **Code formatter** | ✅ | ❌ | ✅ | ❌ | ❌ |
| **Linter** | ✅ | ◐ | ✅ | ❌ | ❌ |
| **Name resolution & elaboration** | ✅ | ✅ | ❌ | ❌ | ❌ |
| **Filelist-free design discovery** | ✅ | ❌ | ❌ | ❌ | ❌ |
| **Fetches & locks dependencies** | ✅ | ❌ | ❌ | ✅ | ❌ |
| **Language server (LSP)** | ❌ | ◐ | ✅ | ❌ | ❌ |

*(Legend: ✅ Full support, ◐ Partial support, ❌ Not supported)*

> [!NOTE]
> `astli`'s parser covers modern synthesizable SystemVerilog, but preserves some niche verification constructs (such as `specify` blocks and UDPs) verbatim without full interior lowering. For detailed grammar status, see [`docs/grammar-coverage.md`](https://github.com/fischeti/astli/blob/main/docs/grammar-coverage.md).

---

## Scope & Roadmap

`astli` handles the end-to-end path from raw source code to a verified, elaborated design:

- Preprocessing with macro expansion and conditional compilation
- Lossless parsing into a Rowan-based syntax tree
- Cross-file name resolution, constant evaluation, and type checking
- Module elaboration and multi-driver checking
- Dependency resolution, caching, and lockfiles

`astli` is strictly an analysis, verification, and tooling platform; it is **not** a simulator or synthesis tool, nor does it aim to be one. A Language Server Protocol (LSP) implementation based on the existing semantic model is planned for upcoming releases.

> [!IMPORTANT]
> `astli` is currently pre-1.0. While active development is thoroughly validated against large real-world open-source designs via [`scripts/fetch-corpus.sh`](https://github.com/fischeti/astli/blob/main/scripts/fetch-corpus.sh), command flags and library APIs may evolve before stabilization.

---

## Installation

Precompiled binaries are available for Linux (x86_64, aarch64), macOS (Apple Silicon, Intel), and Windows.

### Shell Installer (Linux & macOS)

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.sh | sh
```

### Python Package (`uv` or `pip`)

```sh
# Install with uv (recommended)
uv tool install astli

# Or with pip / pipx
pip install astli
pipx install astli
```

### Cargo (`binstall` or build from source)

```sh
# Prebuilt binary via cargo-binstall
cargo binstall astli-cli

# Or compile from source
cargo install astli-cli
```

### Claude Code plugin

A skill that teaches the agent astli's commands, from this repository's plugin marketplace:

```sh
/plugin marketplace add fischeti/astli
/plugin install astli@astli
```

For more options, check the [Getting Started Guide](https://fischeti.github.io/astli/getting-started/).

---

## Using astli as a Library

All underlying crates are published on [crates.io](https://crates.io/crates/astli) under the umbrella `astli` crate:

```toml
[dependencies]
astli = "0.2"
```

```rust
use astli::parse::SyntaxTree;
use astli::syntax::ast::{AstNode, ModuleDecl};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read and parse a SystemVerilog source file
    let tree = SyntaxTree::read("top.sv")?;

    // Traverse the syntax tree and inspect module declarations
    for module in tree.root().descendants().filter_map(ModuleDecl::cast) {
        if let Some(name) = module.name() {
            println!("Found module: {}", name.text());
        }
    }

    // Format the tree according to lowRISC style conventions
    let formatted = astli::fmt::format(&tree, &Default::default())?;
    print!("{formatted}");

    Ok(())
}
```

`astli` re-exports its constituent crates (`astli-syntax`, `astli-parse`, `astli-fmt`, etc.) as modules. You can also depend on individual crates directly if you only need specific functionality. See [`docs/api.md`](https://github.com/fischeti/astli/blob/main/docs/api.md) for architectural details.

---

## Development & Contributing

Because `astli-cli` is the workspace's default root member, you can run the CLI directly without passing `-p`:

```sh
cargo run -- check --top soc_top
```

> [!TIP]
> Because of the default CLI member, cargo commands targeting the entire workspace require the `--workspace` flag:
> ```sh
> cargo nextest run --workspace
> ```
> To skip longer-running tests that fetch external open-source test corpora, run with `-P quick`:
> ```sh
> cargo nextest run --workspace -P quick
> ```
> Those tests alone, over the corpus `scripts/fetch-corpus.sh` fetches, run with `-P corpus`, in release:
> ```sh
> cargo nextest run --workspace --release -P corpus
> ```
> Git pre-commit hooks can be installed with [`prek`](https://github.com/j178/prek):
> ```sh
> prek install
> ```

<details>
<summary><strong>Inspecting intermediate compiler stages</strong></summary>

The CLI exposes hidden subcommands for developers to inspect the output of individual compilation stages:

```sh
# Inspect lexer token stream
cargo run -- lex top.sv

# Inspect preprocessor output
cargo run -- preprocess top.sv -I include --emit text

# Inspect parser syntax tree
cargo run -- parse top.sv --quiet
```
</details>

<details>
<summary><strong>Workspace Crate Structure</strong></summary>

| Path | Description |
| :--- | :--- |
| `crates/astli-text` | Source file storage, line indexing, and source span abstractions |
| `crates/astli-diag` | Diagnostic formatting with macro expansion and include traceback |
| `crates/astli-syntax` | Lexer definitions, `SyntaxKind`, and Rowan concrete syntax trees |
| `crates/astli-preproc` | Preprocessor directives, macro expansions, and include resolution |
| `crates/astli-parse` | Parser implementation and typed AST generation |
| `crates/astli-fmt` | Code formatter conforming to the lowRISC style guide |
| `crates/astli-index` | Fast indexing of symbols declared and referenced across files |
| `crates/astli-sema` | High-level Intermediate Representation (HIR), typing, and elaboration |
| `crates/astli-lint` | Syntactic and semantic lint rules |
| `crates/astli-package` | Package discovery and `astli.toml` manifest parser |
| `crates/astli-resolve` | Dependency resolution, version solver, and lockfile generation |
| `crates/astli` | Top-level library re-exporting all core components |
| `crates/astli-cli` | The main command-line executable (`astli`) |
| `docs/` | Internal architecture and planning documents |
| `site/` | Documentation website, generated via `scripts/site.sh` |
</details>

For more in-depth architectural notes, see [`docs/plan.md`](https://github.com/fischeti/astli/blob/main/docs/plan.md).

---

## Acknowledgements

- [slang](https://github.com/MikePopoloski/slang) by Mike Popoloski: The gold standard for SystemVerilog language compliance. `astli`'s preprocessor and parser behavior is validated directly against slang test suites.
- [rust-analyzer](https://github.com/rust-lang/rust-analyzer): Pioneered modern IDE-grade compiler design with Rowan lossless trees, event-based parsing, and ungrammar.
- [rowan](https://github.com/rust-analyzer/rowan), [logos](https://github.com/maciejhirsz/logos), and [ungrammar](https://github.com/rust-analyzer/ungrammar): The foundational libraries powering `astli`'s parsing engine.
- [Cargo](https://github.com/rust-lang/cargo) and [uv](https://github.com/astral-sh/uv): The models for `astli`'s package manager, from the manifest and lockfile to the shared cache and version resolution.
- [lowRISC Style Guides](https://github.com/lowRISC/style-guides): The basis for `astli`'s automated code formatter and style rules.
- The open-source silicon community, whose diverse real-world designs form our integration test suite.

---

## License

Licensed under either the [MIT License](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE) at your option. Unless explicitly stated otherwise, any contribution intentionally submitted for inclusion shall be dual-licensed as above without additional terms or conditions.
