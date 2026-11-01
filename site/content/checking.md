# Checking

`astli check` performs whole-design semantic analysis, checking your SystemVerilog source files with the rigor of a production compiler frontend.

```sh
astli check -f src.f                  # Check a design defined by a filelist
astli check +incdir+include rtl/*.sv  # Check files with include search paths
astli check -f src.f --top soc_top    # Hierarchically elaborate and check from a top module
astli check                           # Automatically check the current package and its dependencies
```

`astli check` compiles and links the design across all source files, identifying standard IEEE 1800 compliance violations:

- **Missing definitions**: References to undeclared signals, types, packages, package or class members, or missing module definitions; and names used above their declaration.
- **Port and parameter mismatches**: Port connections or parameter overrides that do not exist on the target module, positional argument counts exceeding definitions, or duplicate connections.
- **Illegal multiple drivers**: SystemVerilog variables concurrently driven by multiple procedural blocks (`always_comb`, `always_ff`), continuous `assign` statements, or module output ports.

A comprehensive list of diagnostics and error codes is available in the [Check Diagnostics Reference](reference/check.md).

---

## Hard macros and separately compiled modules

A hard macro, such as an SRAM or a PLL, is often linked only during synthesis, so no source file declares it. Declare its ports and parameters with `extern module` in a file of its own, and `astli check` checks every instance of it against them, instead of reporting the module as undefined:

```systemverilog
extern module sram_256x32 #(parameter int unsigned NumWords = 256)
  (input logic CLK, input logic CEN, input logic WEN,
   input logic [$clog2(NumWords)-1:0] A, input logic [31:0] D, output logic [31:0] Q);
```

A connection to a port the macro lacks, too many connections, or a value whose type does not convert is reported, and the macro's outputs count as drivers. `astli elab` shows the instance as a leaf marked `extern`. A file that also declares the module's body, such as a simulation model, takes its place, and the two are not reported as a module declared twice.

---

## Conservative error reporting ("Only what is certain")

A core design principle of `astli check` is **zero false positives**. It aims to report only genuine errors that would fail a conforming IEEE 1800 compiler, avoiding speculative warnings when code constructs cannot be fully evaluated.

Where static analysis lacks complete information, `astli` behaves conservatively:

- **Generate constructs without `--top`**: When running without an explicit `--top` module, `astli` cannot know which `generate` branches are active. Instantiations inside generate blocks are therefore checked leniently; missing module definitions within unelaborated generate branches emit warnings rather than fatal errors.
- **Hierarchical elaboration with `--top`**: When `--top` is specified, `astli` evaluates branch conditions and loop ranges (`genvar`). Modules in active generate branches are rigorously checked, while inactive branches and uninstantiated files (such as alternate testbenches) are safely excluded from reporting.
- **Classes with unknown bases**: A class extending one no source file declares, such as a UVM base class, may inherit any name, so names inside it that nothing declares are not reported.
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
