# Packages

In `astli`, a package represents a cohesive SystemVerilog module library, core, or SoC together with its configuration and external dependencies.

When you invoke `astli` without naming specific files or filelists, it automatically operates on the package enclosing your current working directory:

```sh
astli fmt --check   # Check all files in the current package
astli lint          # Lint package sources and dependencies
astli check         # Semantically check the full design including dependencies
```

!!! tip "Explicit files override package mode"
    Passing individual files (such as `astli lint rtl/alu.sv`) or filelists (`astli check -f design.f`) on the command line disables automatic package discovery. In that mode, `astli` analyzes only the explicitly specified files.

---

## Locating the package root

To find the package root, `astli` searches upward from your current directory for an `astli.toml` file, stopping at the repository boundary (e.g. `.git`). If no `astli.toml` is found, the root of the repository is treated as the package root, and default conventions apply.

All relative paths declared in `astli.toml` are resolved relative to the directory containing that manifest.

---

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

---

## Headers and compilation units

In `astli`, each non-included `.sv` or `.v` file is treated as its own independent compilation unit. Any file included via an `` `include `` directive is classified as a header, regardless of whether its extension is `.svh`, `.vh`, or `.sv`.

??? info "How `astli` resolves `` `include `` directives"
    When expanding an `` `include "filename" `` directive, `astli` searches in the following order:

    1. **Adjacent directory**: Directly alongside the file containing the `` `include `` directive.
    2. **Include search paths**: Within directories declared in `include-dirs` or provided via `-I` / `+incdir+`.
    3. **Fuzzy package match**: If the file is still not found, `astli` searches across the package for any uniquely matching file whose path ends in that filename. If multiple candidates exist, `astli` raises an ambiguity error.

You can run `astli files --emit incdirs` to output all resolved include directories needed by third-party EDA tools to find every header without relying on fallback searches.

---

## Duplicate symbols and name collisions

If two files within the design define a module, interface, or package with the exact same name, `astli` raises an error if that symbol is instantiated or used by the design.

If an external dependency provides multiple conflicting implementations of the same module (for instance, ASIC vs. FPGA variants), you can use [features and overrides](features.md) to select only the intended implementation.
