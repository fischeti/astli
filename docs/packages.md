# Packages

> **Status:** discovery and selection, phases 1 and 2, are built (M10,
> [`plan.md`](plan.md#5-milestones)); the rest is designed.

`astli` should find a design by itself, without a filelist or `+incdir+`, and
in time resolve dependencies the way Cargo does. A package is a directory with
an `astli.toml`; it says what its files are, what a dependent sees of them,
and what it may replace in its dependencies. It never lists files.

## The manifest

```toml
[package]
name = "my_soc"
sources = ["hw"]                 # where the walk looks; default ["."]
include-dirs = ["hw/include"]    # header roots a dependent searches; default ["include"]
exclude = ["hw/scripts/**"]      # on top of .gitignore
dev = ["hw/tb/**"]               # never seen by a dependent

[dependencies]                   # located by bender; path, version and git later
common_cells = { features = ["asic"] }

[dev-dependencies]
axi = { path = "../axi", features = ["vip"] }

[features]
default = ["vip"]                # what a dependent gets without asking
vip  = { sources = ["hw/vip/**"], include-dirs = ["hw/vip/include"], dependencies = ["common_verification"] }
asic = { sources = ["hw/tech/asic/**"], dependencies = ["tech_cells_gf22"] }

[overrides]                      # names this package may replace in a dependency; `*` any run
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
| P1 | The package root is the directory holding `astli.toml`, found walking up from the current directory as far as the git root; without one, the git root. Files or `-f` on the command line turn discovery off | A dependency in a shared store has no `.git`. A manifest above the repository is not its own. One base for every path in the file. |
| P2 | `sources` narrows the walk and stays inside the root; the root itself does not move | Covers a `hw/` beside `sw/`. The root is what a package publishes, so `sources` cannot reach outside it. Not called `include`, which SystemVerilog uses for headers. |
| P3 | The walk yields `.sv`, `.svh`, `.v` and `.vh` files, honours the `.gitignore` files at or below the root, with or without a repository, and stops at a nested `astli.toml` | Walking is cheap (80 ms over OpenTitan's 13.7k files, the same files `git ls-files` names); parsing is the cost. A dependency then reads the same from a shared store, and a package checked out where another repository ignores it is still walked. A global ignore file and `.git/info/exclude` differ between machines. A nested manifest is another package, such as vendored IP, which would otherwise clash with the dependency it copies. |
| P4 | A header is any file another includes, whatever its extension and in whichever branch; a unit is a file with a unit extension (`.sv`, `.v`) that nothing includes | `ibex`, `cva6` and OpenTitan include `.sv` files, 3,000 times in the corpus. A file included only where a build defines a macro is still not meant to be compiled alone. |
| P5 | `` `include `` resolves beside the including file, then in `include-dirs`, then within the package by unique path suffix; two candidates are an error where the expansion reaches the directive, and each is a header. A dependent searches only `include-dirs`, so an exported file whose include resolves only by suffix is an error. `include-dirs` defaults to `include` where it exists | No configuration inside a package; a header added to a dependency cannot make a downstream include ambiguous. Most IP already exports `include`, so one without a manifest still exports its headers. |
| P6 | Discovery finds no order and needs none; an output that wants one takes `Index::ordered` | Each file is its own unit ([D17](plan.md#4-decisions)). A defines file that must come first gets an explicit define instead. |
| P7 | A referenced name two selected files declare is an error; an unreferenced one is a warning | Repositories hold several `tb_top`s nothing instantiates. |
| P8 | `lint` and `check` load the dependencies and report on the root package only; `fmt` formats every file the root holds, dev and feature sources included | `check` needs the dependencies' definitions, and `lint`'s design rules their port directions; a dependency's findings are its authors' to fix, as with clippy. |
| P9 | Features are additive: one adds sources, include directories and dependencies, and enables a dependency's features (`axi/vip`), never removes any. A file a feature names is walked only when a feature naming it is enabled. A package's own runs enable all its features; `--no-features` turns them off and `--features` names more, its own or a dependency's. A dependent gets `default` and what it asks for | Any combination is then a valid design, so all of them together is too, and that is the run that reads every file. Negation (`not(synthesis)`) is what made bender's targets hard to read. A feature's own include directory keeps it out of what every dependent searches. `default` decides only what a dependent gets, which is all a default buys when nothing conflicts. |
| P10 | A variant is an override. `[overrides]` names what a package may replace in a dependency; a replacement takes effect when a selected file of the package declares the name. Two packages declaring one name without an override, or overriding the same one, is an error | An override follows the feature that selects its file, with no condition of its own. Dependencies stay read-only. Covers generic and technology cells and regenerated register files alike. |
| P11 | A file is kept or dropped whole, so a dependency file declaring both a replaced and a kept name is an error | As in file selection ([M6](plan.md#m6-astli-files-and-astli-pickle)); splitting a file is a rewrite. |
| P12 | Tool support is not modelled: `` `ifdef VERILATOR ``. No cfg attributes, and a package sets no defines | Every tool predefines its macro. A tool has to parse a file to ignore an attribute in it, so an attribute selects only where astli writes the file list or the pickle, and within a file `` `ifdef `` already works in every tool. A file list can only define a macro for every file, so a define is the consumer's. |
| P13 | Generators come last: declared by the package whose files they write, parameters from the dependent, output in the dependent's `target/`, cached by inputs and parameters. The output replaces the package's checked-in default without an override, and a missing output is reported | A `build.rs` without the code: dependencies stay read-only and shareable between projects. Until then, regenerating into the root package and overriding does the same by hand. |
| P14 | The bridge to bender asks it only which dependencies there are, which each depends on, and where each is checked out (`bender packages`, `bender path`). Targets, file lists and defines in a `Bender.yml` are not read | bender fetches until astli does (phase 4), and none of its file model reaches astli: what an IP exports and leaves out is its `astli.toml`'s to say, and the bridge leaves no other way to say it. |
| P15 | Selection is per design: features, overrides and each dependency's version are settled once. Two dependents asking for different features of a package get both; asking for different versions, or overriding one name, is an error naming both. No feature excludes another | Every name is global to the design, so a dependency cannot be two ways at once. What varies per instance is a parameter; a configuration package such as `cva6`'s cannot. Until names are mangled, a second copy is `astli pickle --prefix`. |
| P16 | A private package depends on the public one it builds on, by `path = ".."` when checked out inside it. A dependent never resolves a package's dev-dependencies, so what the private side reuses, such as synthesis wrappers or a simulation fixture, is a feature, not `dev` | A public manifest then names nothing its users cannot fetch, and the path follows the surrounding checkout, so neither repository updates a link when the other changes. The other direction needed a stub manifest for the private package. |
| P17 | A dependency is a package, read as a root is: its manifest or the defaults, its walk, its headers and units, its own build. A package searches its own `include-dirs`, then those of every package it depends on, directly or not. Where a dependency's directory comes from is the resolver's: bender now, `path` in phase 3, git and versions in phase 4 | One model whatever fetched it, so dropping bender changes only where the directories come from. Searching what a dependency's dependencies export spares listing a package only for its headers. |
| P18 | `dev` patterns match as `exclude`'s do. A dependent's walk of the package skips them, so a dev file is neither compiled nor found by an include, while the package's own runs read it as any other | Otherwise each design built on a package compiles its testbenches too: common_cells' 28 under `test/` in every design with axi. A header only a testbench includes is not the dependent's to find. |
| P19 | An override replaces a unit, never a header. What a header declares is replaced through the unit that includes it | A dependency searches its own include directories first and never its dependents', so nothing a dependent ships can shadow its headers, and `[overrides]` holds names, not paths. |

A variant is never a feature, since all features together must be a design.
cva6 ships 19 configurations, each a file declaring `cva6_config_pkg`, so
enabling them all would declare it 19 times. Instead cva6 keeps one default,
and each other configuration is a package nested in it, such as
`core/include/configs/cv64a6_imafdc_sv39/astli.toml`, depending on cva6 by
`path` and overriding `cva6_config_pkg`. cva6's walk stops at the nested
manifests (P3), so its own runs read the default; `check` inside a
configuration checks cva6 under it; and a design picks one by depending on
it. Without path dependencies, cva6 excludes all but one, and a dependent
wanting another copies it and overrides.

Large monorepositories are not a target: OpenTitan declares 209 names more
than once under `hw/`, one per top, which only separate packages can tell
apart.

## Phases

1. **Discovery (M10).** P1–P8 without features: `fmt`, `lint`, `check` and
   `files` run with no arguments. The bender bridge (P14) locates the
   dependencies, each read as a package (P17), and reporting is the root
   package's.
2. **Selection.** `dev`, features, overrides.
3. **Path dependencies.** `[dependencies]` and `[dev-dependencies]` with
   `path`, a second way to locate what P17 reads, and a feature's
   dependency named alone, which only that feature brings into the design.
4. **Package manager.** Versions, git sources, a lockfile, a shared store,
   generators.
