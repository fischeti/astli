# Files and designs

`astli` operates on your SystemVerilog source code in two distinct modes, depending on the command being run.

---

## Analysis modes

### 1. Single-file mode

Used by `astli fmt` and purely syntactic `astli lint` rules.

In single-file mode, each file is processed completely independently as written on disk:

- Preprocessor `` `include `` directives are not expanded inline.
- Macro definitions (`+define+`) are not applied across file boundaries.
- Every branch of conditional compilation directives (such as `` `ifdef `` and `` `else ``) is preserved and inspected.

Because each file is evaluated in isolation, formatting and syntactic linting are blazing fast and deterministic. A file can be formatted and linted even if it does not compile or depends on external symbols.

### 2. Whole-design mode

Used by `astli check`, `astli elab`, `astli files`, `astli pickle`, and semantic lint rules (such as unused-signal or undriven-wire detection).

In whole-design mode, `astli` processes the codebase the same way an EDA compiler or simulator does:

- Files are preprocessed with all active `` `define `` macros and include search paths.
- Modules, interfaces, programs, and packages declared across all files are indexed into a global symbol table.
- Macro expansions that generate instantiations or port connections are fully evaluated.

!!! note "Separate compilation units"
    Each file represents its own compilation unit. Any declarations placed in the global compilation-unit scope outside of a `module`, `interface`, or `package` are private to that specific file and are not visible to other files.

---

## Defining a design

When you do not pass any explicit file arguments, `astli` constructs the design automatically from the current [package](packages.md) and its resolved [dependencies](dependencies.md).

Alternatively, you can supply files, filelists, or both directly via the command line:

```sh
astli check -f design.f                    # Load files from a filelist
astli check +incdir+include rtl/*.sv       # Pass files and an include directory
astli check -f design.f -I include -D SIM  # Combine a filelist with additional flags
```

### Filelists and flags

`astli` understands standard industry filelist conventions:

- **`-f <path>`**: Resolves relative paths within the filelist relative to the **current working directory**.
- **`-F <path>`**: Resolves relative paths within the filelist relative to the **filelist's own parent directory**.
- **`+incdir+<dir>` / `-I <dir>`**: Adds directories to the preprocessor include search path.
- **`+define+<NAME>=<VAL>` / `-D <NAME>=<VAL>`**: Defines preprocessor macros.
- **Environment variables**: Supports variable expansion such as `$ROOT` or `${PROJ_DIR}` inside filelists. If a referenced environment variable is unset, `astli` exits with a helpful error.

---

## Top-level modules and elaboration

A **top** is the root module, interface, or testbench from which elaboration begins. In a complete hardware design, this is typically the top-level SoC or chip harness that is not instantiated by anything else.

`astli` handles designs differently depending on whether `--top` is provided:

- **Without `--top`**: `astli check` verifies all modules in isolation using their default parameter values. Because no specific hierarchy is chosen, generate constructs are checked conservatively.
- **With `--top <module>`**: `astli check` performs full hierarchical elaboration starting from the specified root module, exactly as a simulator or synthesis tool would:

    - Every instance receives its actual propagated parameter overrides.
    - Generate conditionals and loop unrollings (`genvar`) are evaluated.
    - Modules or generate branches that are never instantiated in the active hierarchy are safely skipped.
    - Reporting is scoped strictly to the files reached by the specified tops. Unrelated testbenches or modules not in the active instance tree will not cause checks or CI jobs for that top to fail.

```sh
# Discover modules that are not instantiated by anything else in the design
astli files -f design.f --emit tops

# Inspect the elaborated instance tree for a top module
astli elab -f design.f --top soc_top

# Check the design with full hierarchical elaboration
astli check -f design.f --top soc_top
```

!!! tip "Configuring tops in `astli.toml`"
    If a top-level module requires specific preprocessor defines (such as `SIMULATION` or `SYNTHESIS`), declare it in your manifest under [`[top.NAME]`](reference/manifest.md#topname). Passing `--top NAME` automatically applies the configured defines, and setting `default-top` allows running `astli check` without repeating the flag.

!!! tip "Multiple tops"
    You can specify multiple `--top` arguments (e.g. `--top soc_top --top test_harness`) to elaborate and verify several disjoint hierarchies in a single run.
