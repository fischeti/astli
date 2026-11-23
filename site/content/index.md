# astli

A SystemVerilog toolchain in one binary. As `cargo` does for Rust, `astli` formats, lints, checks, builds and tests your designs, and manages their dependencies, without a hand-written filelist.

```sh
astli fmt     # Format every file in the package (lowRISC style)
astli lint    # Catch common bugs, syntax pitfalls, and style violations
astli check   # Check undeclared names, port mismatches, and multi-driver conflicts
astli test    # Build and run every testbench the package marks, with Verilator
```

Run `astli` in a repository and it finds the package's files itself. An optional `astli.toml` names the package, its include directories and its dependencies, and every command reads the design from it.

## Core capabilities

**One file at a time**, without reading the rest of the design:

- [`astli fmt`](formatting.md) lays out code in the [lowRISC style](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md), and checks that the output preprocesses exactly as the input did.
- [`astli lint`](linting.md) reports likely bugs and style issues, under Verible's rule names where Verible has the rule.

**The whole design**, as a compiler reads it:

- [`astli check`](checking.md) resolves names across files, holds each instance to its definition, and reports variables driven from more than one place.
- [`astli elab`](checking.md#the-instance-tree-astli-elab) prints the instance tree a top builds.
- [`astli files` and `astli pickle`](filelists.md) write the files a top needs, in compile order, as a filelist or as one source file.

**Simulation**, with Verilator:

- [`astli build` and `astli run`](building.md) compile a top from the files it needs, and run the simulation.
- [`astli test`](testing.md) builds and runs every testbench the package marks `(* astli_test *)`, across the parameter values it sweeps, in parallel, and reports each one on its own.

**Dependencies**:

- [`astli add`, `astli update` and `astli fetch`](dependencies.md) resolve Git and path dependencies, record the chosen commits in `astli.lock`, and check them out into a shared cache.
- [Workspaces](workspaces.md) hold several packages in one repository, with one lock between them.

**In the editor**:

- [`astli lsp`](editors.md) shows what `lint` and `check` report as you type, formats, and adds go to definition, references, hover and inlay hints, in VS Code, Zed, Neovim, Emacs and Claude Code.

## How it works

Every command reads code through the same preprocessor and the same lossless parser, so the formatter, the linter, the checker and the language server agree on what the code says. What the parser doesn't recognize stays in the tree as it was written, so no comment, macro or construct is ever dropped.

`astli` is not a simulator or a synthesis tool: `astli build` hands the design to Verilator.

The underlying crates are published on [crates.io](https://crates.io/crates/astli) and documented on [docs.rs](https://docs.rs/astli).

[Get started](getting-started.md){ .md-button .md-button--primary }
