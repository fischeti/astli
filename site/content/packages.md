# Packages

A package is a library, an IP or an SoC, with its configuration and its dependencies.

Named no files, a command works on the package around the current directory:

```sh
astli fmt --check   # Check all files in the current package
astli lint          # Lint package sources and dependencies
astli check         # Semantically check the full design including dependencies
```

!!! tip "Named files replace the package"
    Given files (`astli lint rtl/alu.sv`) or filelists (`astli check -f design.f`), a command reads those alone, not the rest of the package.

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

`sources`, `dev` and `exclude` take glob patterns, and a pattern naming a directory names every file below it: `sources = ["rtl/*.sv"]` takes the files directly in `rtl/` and leaves `rtl/legacy/` out. A source whose directory does not exist, the part of its pattern before any wildcard, is an error. `exclude` removes a file whichever key names it.

### Dev sources

The files `dev` matches are the package's own: its testbenches and verification harnesses. A command run in the package reads them, and a package that depends on this one never sees them.

A `dev` pattern that matches no file is an error, so a typo cannot silently leave a testbench out or turn it into a design file.

!!! note "Nested sub-packages"
    A directory with an `astli.toml` or a `Bender.yml` of its own is another package. Its files are not the parent's, and reach the parent's design only as a dependency.

    A directory with a `Bender.yml` is still scanned where a pattern of `sources`, `dev` or a feature names it or a path inside it before any wildcard, for a Bender package that lists a nested package's files as its own. Inside it, only such patterns select files; `**/*.sv` or the root does not reach it:

    ```toml
    [package]
    sources = [".", "core/pmp/src"]   # core/pmp has a Bender.yml of its own
    ```

## Workspaces

A repository holding several packages, such as an SoC and its IP blocks, can make them one **workspace** with a `[workspace]` table in the `astli.toml` at the top. Its packages then share one lock and one version of each dependency, and `-p NAME` and `--workspace` run a command on any of them, or on all of them. Without `[workspace]`, a nested `astli.toml` is a separate package, with a lock of its own. See [Workspaces](workspaces.md).

## Headers and compilation units

Each `.sv` or `.v` file that nothing includes is a compilation unit of its own. A file that an `` `include `` names is a header, whatever its extension.

??? info "How `astli` resolves `` `include `` directives"
    For `` `include "filename" ``, `astli` searches:

    1. **The include directories**: those `include-dirs` declares, and those `-I` or `+incdir+` gives.
    2. **The package**: failing those, the one file in the package whose path ends in the name. Two such files are an ambiguity error.

`astli files --emit incdirs` writes the include directories another tool needs to find every header without that second search.

A tool searches those directories in order for every file in the list, while `astli` searches each package with its own. Where that would make a tool read a different header than `astli` did, for instance when two packages each hold a `defs.svh`, `astli files`, `astli build` and `astli check` warn with `include-differs`. Give one of the headers a more specific path, such as `` `include "ip/defs.svh" ``.

## Two definitions of one name

Two files of a design may declare a module, interface or package of the same name, and it is an error only once the design uses that name.

Where a dependency has several implementations of one module, such as an ASIC and an FPGA variant, [features and overrides](features.md) choose the one to build.
