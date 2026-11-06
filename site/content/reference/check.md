# Check diagnostics reference

This reference documents the semantic diagnostics produced by [`astli check`](../checking.md). Each check corresponds to an explicit compliance requirement defined in IEEE 1800 (SystemVerilog).

When `astli` cannot statically determine certainty—such as inside unelaborated `generate` blocks—it downgrades diagnostics to warnings or suppresses them to avoid false positives.

| Diagnostic Code | Description |
| :--- | :--- |
| **`undeclared-name`** | A variable, net, parameter, or type identifier referenced in code that has not been declared in any accessible scope. |
| **`used-before-declaration`** | A symbol referenced before its declaration within the same scope, when not declared in an enclosing scope. (Tasks, functions, properties, sequences, and forward `typedef`s may legally be referenced prior to their definition according to IEEE 1800). |
| **`unknown-definition`** | An instantiation of a module, interface, or program that is not defined in any source file. (Modules declared via `extern module` are treated as defined and checked against their declared interface). |
| **`unknown-package`** | An explicit `import pkg::*` or qualified `pkg::item` reference targeting a package that does not exist. |
| **`not-in-package`** | A scoped reference `pkg::item` where `pkg` exists, but contains no declaration named `item`. (Note: symbols imported into a package are private to it unless explicitly re-exported). |
| **`not-in-class`** | A member reference (`C::item`, `this.item`, `super.item`, or `handle.item`) where the class hierarchy is known, but does not declare `item`. Classes extending undeclared base types (such as external UVM classes) are skipped to prevent false positives. |
| **`unknown-port`** | A named port connection (`.port_name(...)`) targeting a port that does not exist on the module definition. |
| **`unknown-parameter`** | A named parameter override (`#( .PARAM_NAME(...) )`) targeting a parameter that does not exist on the module definition. |
| **`too-many-ports`** | A positional port connection list providing more arguments than the total number of ports defined on the module. |
| **`too-many-parameters`** | A positional parameter list providing more override values than the module defines. |
| **`connected-twice`** | A port or parameter connected or overridden more than once within the same instantiation. |
| **`local-parameter`** | An illegal attempt to override a `localparam` from outside the module. |
| **`declared-twice`** | A name declared more than once in the same scope (e.g. `logic v` and `wire v` in the same module), or conflicting module/interface/package declarations across files in the active build hierarchy. Without `--top`, checked across root package files; with `--top`, checked across all files reached by the tops. |
| **`procedural-net-assignment`** | An illegal procedural assignment to a net (`wire`, `input`, `inout`, or untyped port) inside an `initial`, `always`, function, or task block. In SystemVerilog, procedural assignments are only permitted on variables (`logic`, `reg`). Drive nets using continuous `assign`, or declare the signal as `output logic`. |
| **`invalid-return`** | An invalid `return` statement: returning a value from a `void` function or task, omitting a return value in a non-void function (except constructors), or using `return` inside a `fork...join` block. |
| **`waits-in-function`** | A function containing time-consuming statements (delays `#`, event controls `@`, `wait`, or blocking `fork...join`/`join_any`). In SystemVerilog, functions must execute in zero simulation time. Non-blocking `fork...join_none` is permitted. |
| **`multiple-drivers`** | A variable illegally driven by multiple concurrent sources (such as multiple procedural blocks `always_comb`/`always_ff`/`always_latch`, continuous `assign` statements, or module output ports). Bit-slice writes are flagged if assigned bit ranges overlap. |
| **`incompatible-type`** | A value assigned or passed where its type cannot implicitly convert to the destination type (assignments, initializers, parameter overrides, port connections, function arguments, or `return` values). Examples include assigning non-enum values or using compound operators like `+=` on an enum without an explicit cast, dimension mismatches in unpacked structs/arrays, or assigning incompatible types (numbers, strings, and class handles). |
| **`packed-member-default`** | A default member value specified in a packed struct (`struct packed { bit [3:0] lo = 5; … }`). Default values are legal only on unpacked struct members. |
| **`specparam-outside-specify`** | A `specparam` used outside a `specify` block or another `specparam` declaration. |
| **`empty-select`** | An indexed part-select (`[i+:w]` or `[i-:w]`) where the selected width `w` is less than 1 bit in elaborated code. |
| **`invalid-enum-value`** | An enum member initialized with an invalid value: a sized literal whose width does not match the base type (e.g. `4'h2` in an `enum logic [2:0]`), four-state values (`x` or `z`) in a two-state enum, or duplicate values. |
| **`invalid-operand`** | An operator applied to an incompatible operand type: using `real` with `%`, bitwise/reduction operators, part-selects, or edge events (`@(posedge real_var)`); applying unsupported operators to `string` types (other than comparisons); or invalid operations on class handles (other than equality checks `==` and `!=`). |
| **`elaboration-error`** | An elaboration-time `$error` or `$fatal` (one standing among module items, such as in a `generate` block) that runs in what `--top` builds, with the message its arguments format to there. Elaboration tasks are how a design rejects parameters it was not written for, so only bodies the tops build run them: without `--top`, nothing says a module's defaults are what is built. |
| **`elaboration-warning`** | An elaboration-time `$warning` that runs in what `--top` builds. An `$info` reports nothing wrong, so it is not shown. |

---

## Hierarchical multi-driver validation with `--top`

When you provide a top-level module with `--top`, `astli check` verifies multi-driver rules across the fully elaborated hierarchy:

- Parameter overrides are propagated to calculate exact array dimensions and signal bit-widths.
- Loop variables (`genvar`) in `for`-generate constructs are unrolled and evaluated.
- Slices driving distinct bit ranges without overlap are confirmed as safe, while true collisions across procedural blocks or submodules are reported with their exact instantiation paths.
- Signals inside interface instances are validated across all drivers: the instantiating module, the interface itself, and any submodules connected through interface ports across the hierarchy.

!!! note "Strict standard compliance"
    `astli check` strictly focuses on standard SystemVerilog semantics. If code passes `astli check`, you can be confident it will elaborate cleanly across major commercial and open-source simulators and synthesis compilers.
