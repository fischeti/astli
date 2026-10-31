# Check diagnostics reference

This reference documents the semantic diagnostics produced by [`astli check`](../checking.md). Each check corresponds to an explicit compliance requirement defined in IEEE 1800 (SystemVerilog).

When `astli` cannot statically determine certainty—such as inside unelaborated `generate` blocks—it downgrades diagnostics to warnings or suppresses them to avoid false positives.

| Diagnostic Code | Description |
| :--- | :--- |
| **`undeclared-name`** | A variable, wire, constant, or type identifier referenced in code that has not been declared in any accessible scope. |
| **`unknown-definition`** | An instantiation of a module, interface, or program that is not defined in any source file. |
| **`unknown-package`** | An explicit `import pkg::*` or qualified `pkg::item` reference targeting a package that does not exist. |
| **`not-in-package`** | A scoped reference `pkg::item` where `pkg` exists, but contains no declaration named `item`. (Note: symbols imported into a package are private to it unless explicitly re-exported). |
| **`not-in-class`** | A reference `C::item`, `this.item`, `super.item` or `handle.item` where the class and every class it extends are known, and none declares `item`. A class extending one no source file declares, such as a UVM base class, is never reported. |
| **`unknown-port`** | A named port connection (`.port_name(...)`) targeting a port that does not exist on the module definition. |
| **`unknown-parameter`** | A named parameter override (`#( .PARAM_NAME(...) )`) targeting a parameter that does not exist on the module definition. |
| **`too-many-ports`** | A positional port connection list providing more arguments than the total number of ports defined on the module. |
| **`too-many-parameters`** | A positional parameter list providing more override values than the module defines. |
| **`connected-twice`** | A port or parameter connected or overridden more than once within the same instantiation. |
| **`local-parameter`** | An illegal attempt to override a `localparam` from outside the module. |
| **`declared-twice`** | Multiple conflicting declarations for the same module, interface, program, or package across files in the active build hierarchy. A class declared outside any module or package in two files is a warning: each file is its own compilation unit, so each has its own class, but a tool compiling all files as one unit rejects it. Without `--top`, checked across root package files; with `--top`, checked across all files reached by the tops (including dependencies), matching [`astli files`](../filelists.md) resolution behavior. |
| **`multiple-drivers`** | A variable illegally driven by multiple concurrent sources (such as multiple procedural blocks `always_comb`/`always_ff`/`always_latch`, continuous `assign` statements, or module output ports). Bit-slice writes are flagged if assigned bit ranges overlap. |

---

## Hierarchical multi-driver validation with `--top`

When you provide a top-level module with `--top`, `astli check` verifies multi-driver rules across the fully elaborated hierarchy:

- Parameter overrides are propagated to calculate exact array dimensions and signal bit-widths.
- Loop variables (`genvar`) in `for`-generate constructs are unrolled and evaluated.
- Slices driving distinct bit ranges without overlap are confirmed as safe, while true collisions across procedural blocks or submodules are reported with their exact instantiation paths.

!!! note "Strict standard compliance"
    `astli check` strictly focuses on standard SystemVerilog semantics. If code passes `astli check`, you can be confident it will elaborate cleanly across major commercial and open-source simulators and synthesis compilers.
