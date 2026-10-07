# Packages

> **Status:** designed, nothing built. Discovery is M10
> ([`plan.md`](plan.md#5-milestones)); the rest follows in the phases below.

`astli` should find a design by itself, without a filelist or `+incdir+`, and
in time resolve dependencies the way Cargo does. A package is a directory with
an `astli.toml`; it says what its files are, what a dependent sees of them,
and what it may replace in its dependencies. It never lists files.

## The manifest

```toml
[package]
name = "my_soc"
sources = ["hw"]                 # where the walk looks; default ["."]
include-dirs = ["hw/include"]    # header roots a dependent searches
exclude = ["hw/scripts/**"]      # on top of .gitignore
dev = ["hw/tb/**"]               # never seen by a dependent

[dependencies]                   # path first; version and git later
common_cells = { path = "../common_cells" }

[dev-dependencies]
axi = { path = "../axi", features = ["vip"] }

[features]
vip  = { sources = ["hw/vip/**"], dependencies = ["common_verification"] }
asic = { sources = ["hw/tech/asic/**"], dependencies = ["tech_cells_gf22"] }

[overrides]                      # names this package may replace in a dependency
tech_cells_generic = ["tc_sram", "tc_clk_*"]

[fmt]                            # as now
[lint]                           # as now
```

Every path is relative to the file. Without `[package]`, discovery uses the
defaults; `name` is needed once something depends on the package.

## What a file is

| Class | A dependent sees it | Selected by | Corpus example |
| --- | --- | --- | --- |
| Design | always | `sources`, less `exclude`, `dev` and feature sources | `axi_xbar.sv` |
| Verification IP | when it enables the feature | a feature's `sources` | `axi_test.sv` |
| Testbench | never | `dev` | `tb_axi_xbar.sv` |
| Header | through `include-dirs` | being included | `axi/typedef.svh` |

## Decisions

| # | Decision | Why |
| --- | --- | --- |
| P1 | The package root is the directory holding `astli.toml`, found walking up from the current directory; without one, the git root. Files or `-f` on the command line turn discovery off | A dependency in a shared store has no `.git`. One base for every path in the file. |
| P2 | `sources` narrows the walk and stays inside the root; the root itself does not move | Covers a `hw/` beside `sw/`. The root is what a package publishes, so `sources` cannot reach outside it. Not called `include`, which SystemVerilog uses for headers. |
| P3 | The walk honours `.gitignore` and stops at a nested `astli.toml` | Walking is cheap (44 ms over OpenTitan's 13.7k files); parsing is the cost. A nested manifest is another package, such as vendored IP, which would otherwise clash with the dependency it copies. |
| P4 | A header is any file another includes, whatever its extension; a unit is a file with a unit extension (`.sv`, `.v`) that nothing includes | `ibex`, `cva6` and OpenTitan include `.sv` files, 3,000 times in the corpus. |
| P5 | `` `include `` resolves beside the including file, then in `include-dirs`, then within the package by unique path suffix; two candidates are an error. A dependent searches only `include-dirs`, so an exported file whose include resolves only by suffix is an error | No configuration inside a package; a header added to a dependency cannot make a downstream include ambiguous. |
| P6 | Discovery finds no order and needs none; an output that wants one takes `Index::ordered` | Each file is its own unit ([D17](plan.md#4-decisions)). A defines file that must come first gets an explicit define instead. |
| P7 | A referenced name two selected files declare is an error; an unreferenced one is a warning | Repositories hold several `tb_top`s nothing instantiates. |
| P8 | `lint` and `check` load the dependencies and report on the root package only; `fmt` formats every file the root holds, dev and feature sources included | `check` needs the dependencies' definitions, and `lint`'s design rules their port directions; a dependency's findings are its authors' to fix, as with clippy. |
| P9 | Features are additive: one adds sources and dependencies, and enables a dependency's features, never removes any. A package's own `lint` and `check` enable all its features | Any combination is then a valid design, so all of them together is too. Negation (`not(synthesis)`) is what made bender's targets hard to read. |
| P10 | A variant is an override. `[overrides]` names what a package may replace in a dependency; a replacement takes effect when a selected file of the package declares the name. Two packages declaring one name without an override is an error | An override follows the feature that selects its file, with no condition of its own. Dependencies stay read-only. Covers generic and technology cells and regenerated register files alike. |
| P11 | A file is kept or dropped whole, so a dependency file declaring both a replaced and a kept name is an error | As in file selection ([M6](plan.md#m6-astli-files-and-astli-pickle)); splitting a file is a rewrite. |
| P12 | Tool support is not modelled: `` `ifdef VERILATOR ``. No cfg attributes | Every tool predefines its macro. A tool has to parse a file to ignore an attribute in it, so an attribute selects only where astli writes the file list or the pickle, and within a file `` `ifdef `` already works in every tool. |
| P13 | Generators come last: declared by the package whose files they write, parameters from the dependent, output in the dependent's `target/`, cached by inputs and parameters. The output replaces the package's checked-in default without an override | A `build.rs` without the code: dependencies stay read-only and shareable between projects. Until then, regenerating into the root package and overriding does the same by hand. |
| P14 | The bridge to bender reads `bender sources`, which keeps each file's package | A flat filelist loses package boundaries, and with them which files to report on and whose include directories apply. |

Large monorepositories are not a target: OpenTitan declares 209 names more
than once under `hw/`, one per top, which only separate packages can tell
apart.

## Phases

1. **Discovery (M10).** P1–P8 without dependencies and features: `fmt`,
   `lint`, `check` and `files` run with no arguments. The bender bridge
   (P14) supplies dependencies, and reporting is the root package's.
2. **Selection.** `dev`, features, overrides.
3. **Path dependencies.** `[dependencies]` and `[dev-dependencies]` with
   `path`, each discovered by its own `astli.toml`.
4. **Package manager.** Versions, git sources, a lockfile, a shared store,
   generators.

## Open questions

- **Where discovery lives.** A library crate, since an LSP needs it as much as
  the CLI, or the driver until that second reader exists.
- **Header overrides.** The dependent's `include-dirs` searched first would
  replace a dependency's header, but `[overrides]` holds names, not paths.
- **Naming features on the command line.** `--features`, and whether a run
  can turn off the root's own.
