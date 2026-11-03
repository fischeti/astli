# Checking

`astli check` performs whole-design semantic analysis, checking your SystemVerilog source files with the rigor of a production compiler frontend.

```sh
astli check -f src.f                  # Check a design defined by a filelist
astli check +incdir+include rtl/*.sv  # Check files with include search paths
astli check -f src.f --top soc_top    # Hierarchically elaborate and check from a top module
astli check                           # Automatically check the current package and its dependencies
```

`astli check` compiles and links the design across all source files, identifying standard IEEE 1800 compliance violations:

- **Missing definitions**: References to undeclared signals, types, packages, package or class members, or missing module definitions; symbols used before declaration; and duplicate definitions within a scope.
- **Port and parameter mismatches**: Port connections or parameter overrides that do not exist on the target module, positional argument counts exceeding definitions, or duplicate connections.
- **Procedural net assignments**: Illegal assignments to nets (`wire`, `input`, `inout`, or untyped ports) within procedural blocks (`initial`, `always`, functions, or tasks).
- **Illegal multiple drivers**: Variables concurrently driven by multiple procedural blocks (`always_comb`, `always_ff`), continuous `assign` statements, or module output ports.

A comprehensive list of diagnostics and error codes is available in the [Check Diagnostics Reference](reference/check.md).

---

## Hard macros and separately compiled modules

Hard macros (such as SRAMs, PLLs, or third-party IP) are often integrated as black boxes without source code during simulation or synthesis. You can declare their interface using `extern module` in a standalone header or source file:

```systemverilog
extern module sram_256x32 #(parameter int unsigned NumWords = 256)
  (input logic CLK, input logic CEN, input logic WEN,
   input logic [$clog2(NumWords)-1:0] A, input logic [31:0] D, output logic [31:0] Q);
```

When an `extern module` declaration is present, `astli check` verifies module instances against the declared ports and parameters, and treats the macro's output ports as valid signal drivers. In hierarchy visualization (`astli elab`), the module is displayed as an `extern` leaf. If a full module implementation is provided later (such as a simulation behavioral model), it cleanly supersedes the `extern` declaration without duplicate symbol errors.

---

## Conservative error reporting ("Only what is certain")

A core design principle of `astli check` is **zero false positives**. It aims to report only genuine errors that would fail a conforming IEEE 1800 compiler, avoiding speculative warnings when code constructs cannot be fully evaluated.

Where static analysis lacks complete information, `astli` behaves conservatively:

- **Generate constructs without `--top`**: When running without an explicit `--top` module, `astli` cannot know which `generate` branches are active. Instantiations inside generate blocks are therefore checked leniently; missing module definitions within unelaborated generate branches emit warnings rather than fatal errors.
- **Hierarchical elaboration with `--top`**: When `--top` is specified, `astli` evaluates branch conditions and loop ranges (`genvar`). Modules in active generate branches are rigorously checked, while inactive branches and uninstantiated files (such as alternate testbenches) are safely excluded from reporting.
- **Classes with external base classes**: Classes extending base classes from external libraries (such as UVM base classes) may inherit symbols declared outside the analyzed source tree. Unresolved member accesses in these classes are conservatively allowed to avoid false positives.
- **Unresolved includes or syntax errors**: If an `` `include `` file is missing or a file contains syntax errors, `astli` flags the error but avoids cascading false undeclared-variable errors for downstream code that might have been defined in that missing header.
- **Hierarchical scopes**: Deeply nested struct fields, dynamic class properties, and hierarchical identifiers (e.g. `u_sub.reg_field`) are checked at their root symbol without false alarms on unmodeled sub-properties.

!!! note "Multi-driver checking"
    Multi-driver checks account for bit-level slicing. Driving disjoint bit ranges of a vector from separate procedural assignments is recognized as legal, whereas overlapping bit ranges or full-variable collisions are flagged as errors.

---

## Inspecting the hierarchy with `astli elab`

You can visualize the elaborated module hierarchy using `astli elab`:

```sh
astli elab -f src.f --top soc_top
```

`astli elab` builds the full instance tree and prints a clean ASCII hierarchy showing:

- Every instantiated module, interface, and block instance path.
- Parameter values that have been overridden from their defaults.
- Active generate blocks, conditionally selected branches, and unrolled loops.
- Flagged instances referencing missing definitions or unresolved parameters.

!!! tip "Verifying chip hierarchy"
    Run `astli elab` during architectural bringup or integration to quickly verify that parameter propagation, bus widths, and submodule instantiations are hooking up exactly as intended.
