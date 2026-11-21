# Packages

A package is a library, an IP or an SoC, with its configuration and its dependencies.

Named no files, a command works on the package around the current directory:

```sh
astli fmt --check   # Check all files in the current package
astli lint          # Lint package sources and dependencies
astli check         # Semantically check the full design including dependencies
```

!!! tip "Explicit files override package mode"
    Passing individual files (such as `astli lint rtl/alu.sv`) or filelists (`astli check -f design.f`) on the command line disables automatic package discovery. In that mode, `astli` analyzes only the explicitly specified files.

## Locating the package root

To find the package root, `astli` searches upward from your current directory for an `astli.toml` file, stopping at the repository boundary (e.g. `.git`). If no `astli.toml` is found, the root of the repository is treated as the package root, and default conventions apply.

All relative paths declared in `astli.toml` are resolved relative to the directory containing that manifest.

## Source file discovery

By default, every `.sv`, `.svh`, `.v` and `.vh` file below the package root is part of the design, except what `.gitignore` names. `astli.toml` says otherwise:

```toml
[package]
name = "my_soc"
sources = ["hw"]               # Design files (defaults to the package root)
include-dirs = ["hw/include"]  # Header search paths exposed to downstream dependents
exclude = ["hw/scripts/**"]    # Patterns to ignore, in addition to .gitignore
dev = ["hw/tb/**"]             # Testbenches used only within this package
```

`dev` and a [feature](features.md)'s `sources` take precedence over `[package] sources`: they claim their files wherever they are below the root, so a `tb/` beside `sources = ["hw"]` needs naming only in `dev`, and a `hw/tb/` inside it is still a testbench. `sources` takes whatever neither claims: with its default, every other file is design.

All three take glob patterns, and a pattern naming a directory names every file below it: `sources = ["rtl/*.sv"]` takes the files directly in `rtl/` and leaves `rtl/legacy/` out. A source whose directory does not exist, the part of its pattern before any wildcard, is an error. `exclude` removes a file whichever key names it.

### Dev sources

Files matched by the `dev` pattern are considered internal to the package—such as unit tests, testbenches, and verification harnesses. They are compiled and analyzed when working directly inside this package, but are automatically excluded when another package imports this project as an external dependency.

A `dev` pattern that matches no file is an error, so a typo cannot silently leave a testbench out or turn it into a design file.

!!! note "Nested sub-packages"
    Any subdirectory that contains its own `astli.toml` or `Bender.yml` is treated as an independent child package. Its files are excluded from the parent package's scope unless explicitly referenced as a dependency.

    A directory with a `Bender.yml` is still scanned where a pattern of `sources`, `dev` or a feature names it or a path inside it before any wildcard, for a Bender package that lists a nested package's files as its own. Inside it, only such patterns select files; `**/*.sv` or the root does not reach it:

    ```toml
    [package]
    sources = [".", "core/pmp/src"]   # core/pmp has a Bender.yml of its own
    ```

## Workspaces

A repository holding several packages, such as an SoC and its IP blocks, can make them one **workspace** with a `[workspace]` table in the `astli.toml` at the top. Its packages then share one lock and one version of each dependency, and `-p NAME` and `--workspace` run a command on any of them, or on all of them. Without `[workspace]`, a nested `astli.toml` is a separate package, with a lock of its own. See [Workspaces](workspaces.md).

## Headers and compilation units

In `astli`, each non-included `.sv` or `.v` file is treated as its own independent compilation unit. Any file included via an `` `include `` directive is classified as a header, regardless of whether its extension is `.svh`, `.vh`, or `.sv`.

??? info "How `astli` resolves `` `include `` directives"
    When expanding an `` `include "filename" `` directive, `astli` searches in the following order:

    1. **Include search paths**: Within directories declared in `include-dirs` or provided via `-I` / `+incdir+`.
    2. **Fuzzy package match**: If the file is still not found, `astli` searches across the package for any uniquely matching file whose path ends in that filename. If multiple candidates exist, `astli` raises an ambiguity error.

You can run `astli files --emit incdirs` to output all resolved include directories needed by third-party EDA tools to find every header without relying on fallback searches.

A tool searches those directories in order for every file in the list, while `astli` searches each package with its own. Where that would make a tool read a different header than `astli` did, for instance when two packages each hold a `defs.svh`, `astli files`, `astli build` and `astli check` warn with `include-differs`. Give one of the headers a more specific path, such as `` `include "ip/defs.svh" ``.

## Duplicate symbols and name collisions

If two files within the design define a module, interface, or package with the exact same name, `astli` raises an error if that symbol is instantiated or used by the design.

If an external dependency provides multiple conflicting implementations of the same module (for instance, ASIC vs. FPGA variants), you can use [features and overrides](features.md) to select only the intended implementation.
