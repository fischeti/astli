# astli

A modern SystemVerilog toolchain in a single fast binary. Much like `cargo` does for Rust, `astli` formats, lints, and checks your designs while managing dependencies—without requiring manual filelists.

```sh
astli fmt     # Format every file in the package (lowRISC style)
astli lint    # Catch common bugs, syntax pitfalls, and style violations
astli check   # Check undeclared names, port mismatches, and multi-driver conflicts
astli fetch   # Fetch and cache project dependencies
```

Run `astli` in your repository and it automatically discovers your package layout. An `astli.toml` file declares project metadata, include paths, and external dependencies, giving every command a unified view of your design.

!!! tip "Zero-configuration discovery"
    You don't need to specify filelists or include flags for standard workflows. `astli` finds your source files, figures out their package boundaries, and loads the design automatically.

## Core capabilities

**Single-file operations**, running quickly without cross-file compilation:

- [`astli fmt`](formatting.md) automatically formats code according to the [lowRISC style guide](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md). It verifies that formatting never alters semantic meaning; if a formatting transformation could change code semantics, the file is safely left untouched.
- [`astli lint`](linting.md) identifies suspicious constructs, common bugs, and coding style issues using familiar Verible-compatible rule names.

**Whole-design analysis**, understanding the full hierarchy just like a compiler:

- [`astli check`](checking.md) resolves symbols across all files, validates module instantiations against their definitions, and detects multiple-driver conflicts on variables.
- [`astli elab`](checking.md#inspecting-the-hierarchy-with-astli-elab) inspects the elaborated module hierarchy starting from a top-level module.
- [`astli files` and `astli pickle`](filelists.md) extract and topologically order only the files required to build a given top module, generating clean filelists or a single self-contained source file.

**Dependency management**, powered by reproducible resolution:

- [`astli update` and `astli fetch`](dependencies.md) resolve git repositories and local path dependencies into an exact, reproducible `astli.lock` file and check them out into a shared global cache.

## How it works

Every `astli` command shares a unified preprocessor and a lossless Rowan-based parser. Because tools share the same parser, your formatter, linter, and semantic checker always agree on what the code means. 

Unrecognized or non-standard syntax constructs are preserved verbatim in the syntax tree, ensuring the formatter never drops comments, macros, or unmodeled code.

!!! note "Design philosophy"
    `astli` focuses on developer ergonomics, fast feedback loops, and design verification. It is an analysis, linting, and packaging tool—not a simulator or logic synthesis engine.

The underlying crates are published on [crates.io](https://crates.io/crates/astli) and documented on [docs.rs](https://docs.rs/astli).

[Get started](getting-started.md){ .md-button .md-button--primary }
