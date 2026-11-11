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

By default, `astli` recursively scans for all `.sv`, `.svh`, `.v`, and `.vh` files within the package directories, automatically respecting `.gitignore`. You can customize where `astli` searches and exclude unwanted paths in `astli.toml`:

```toml
[package]
name = "my_soc"
sources = ["hw"]               # Directories to scan (defaults to the package root)
include-dirs = ["hw/include"]  # Header search paths exposed to downstream dependents
exclude = ["hw/scripts/**"]    # Patterns to ignore, in addition to .gitignore
dev = ["hw/tb/**"]             # Testbenches/scripts used only within this package
```

### Dev sources

Files matched by the `dev` pattern are considered internal to the package—such as unit tests, testbenches, and verification harnesses. They are compiled and analyzed when working directly inside this package, but are automatically excluded when another package imports this project as an external dependency.

!!! note "Nested sub-packages"
    Any subdirectory that contains its own `astli.toml` is treated as an independent child package. Its files are excluded from the parent package's scope unless explicitly referenced as a dependency.

## Workspaces

A repository holding several packages can resolve them together, with one lock. Add a `[workspace]` table, even an empty one, to the `astli.toml` at the top: every `astli.toml` below it in the same repository is then a member of the workspace.

```toml
[package]
name = "soc"

[workspace]
exclude = ["vendor/**"]     # directories whose astli.toml is not a member

[dependencies]
dma = { path = "ip/dma" }   # a member, required as any path dependency is
```

- **Finding members**: `astli` walks down from the workspace root. It leaves out what a `.gitignore` names, what `exclude` matches, and any directory holding a `.git` of its own, such as a submodule. A member may sit inside another. Two packages in a workspace may not share a name, and a workspace inside another is an error unless the outer one excludes it.
- **What the root holds**: `astli.lock`, `[resolve] pinned`, patches, `[described.NAME]`, `[build]`, `[profile.NAME]` and `.astli/build` are the workspace root's alone. A member holding one of these tables, or patches of its own, gets a warning that they are not read.
- **What a member keeps**: its `[package]`, dependencies, features, `[overrides]`, `[top.NAME]`, `[fmt]` and `[lint]`. Each member is a design of its own, and is in another's design only if that one requires it by `path`.
- **One resolution**: every run resolves the whole workspace, wherever it runs, so the lock is the same whichever member writes it, and the workspace holds one version of each package. Run inside a member, a command builds that member's design.
- **Workspace commands**: `astli update`, `astli fetch`, `astli patch` and `astli clean` act on the whole workspace from any member.

Without `[workspace]`, a nested `astli.toml` is a separate package, with a lock of its own.

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
