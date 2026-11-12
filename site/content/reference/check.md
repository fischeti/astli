# Check diagnostics

What [`astli check`](../checking.md) reports. Each diagnostic is something IEEE 1800 rejects. Where `check` cannot be sure, such as in a generate block without `--top`, it reports a warning or nothing ([only what is certain](../checking.md#only-what-is-certain)).

| Code | Reports |
| :--- | :--- |
| `undeclared-name` | A variable, net, parameter, or type identifier referenced in code that has not been declared in any accessible scope. |
| `used-before-declaration` | A symbol referenced before its declaration within the same scope, when not declared in an enclosing scope. (Tasks, functions, properties, sequences, and forward `typedef`s may legally be referenced prior to their definition according to IEEE 1800). |
| `unknown-definition` | An instantiation of a module, interface, or program that is not defined in any source file. (Modules declared via `extern module` are treated as defined and checked against their declared interface; declare a library's cells so, in a [stub](../checking.md#hard-macros-and-cell-libraries)). |
| `unknown-package` | An explicit `import pkg::*` or qualified `pkg::item` reference targeting a package that does not exist. |
| `not-in-package` | A scoped reference `pkg::item` where `pkg` exists, but contains no declaration named `item`. (Note: symbols imported into a package are private to it unless explicitly re-exported). |
| `not-in-class` | A member reference (`C::item`, `this.item`, `super.item`, or `handle.item`) where the class hierarchy is known, but does not declare `item`. Classes extending undeclared base types (such as external UVM classes) are skipped to prevent false positives. |
| `unknown-port` | A named port connection (`.port_name(...)`) targeting a port that does not exist on the module definition. |
| `unknown-parameter` | A named parameter override (`#( .PARAM_NAME(...) )`) targeting a parameter that does not exist on the module definition. |
| `too-many-ports` | A positional port connection list providing more arguments than the total number of ports defined on the module. |
| `too-many-parameters` | A positional parameter list providing more override values than the module defines. |
| `unset-parameter` | A parameter declared without a default (`parameter int unsigned Width`) that an instance in what `--top` builds gives no value, by an override or a `defparam`; or a top's, which its `[top.NAME]` table's `params` or `-G` must set. Without `--top`, nothing says what is built, so none is told. |
| `connected-twice` | A port or parameter connected or overridden more than once within the same instantiation. |
| `local-parameter` | An illegal attempt to override a `localparam` from outside the module. |
| `declared-twice` | A name declared more than once in the same scope (e.g. `logic v` and `wire v` in the same module), or conflicting module/interface/package declarations across files in the active build hierarchy. Without `--top`, checked across root package files; with `--top`, checked across all files reached by the tops. |
| `procedural-net-assignment` | An illegal procedural assignment to a net (`wire`, `input`, `inout`, or untyped port) inside an `initial`, `always`, function, or task block. In SystemVerilog, procedural assignments are only permitted on variables (`logic`, `reg`). Drive nets using continuous `assign`, or declare the signal as `output logic`. |
| `invalid-return` | An invalid `return` statement: returning a value from a `void` function or task, omitting a return value in a non-void function (except constructors), or using `return` inside a `fork...join` block. |
| `waits-in-function` | A function containing time-consuming statements (delays `#`, event controls `@`, `wait`, or blocking `fork...join`/`join_any`). In SystemVerilog, functions must execute in zero simulation time. Non-blocking `fork...join_none` is permitted. |
| `multiple-drivers` | A variable illegally driven by multiple concurrent sources (such as multiple procedural blocks `always_comb`/`always_ff`/`always_latch`, continuous `assign` statements, or module output ports). Bit-slice writes are flagged if assigned bit ranges overlap. |
| `incompatible-type` | A value assigned or passed where its type cannot implicitly convert to the destination type (assignments, initializers, parameter overrides, port connections, function arguments, or `return` values). Examples include assigning non-enum values or using compound operators like `+=` on an enum without an explicit cast, dimension mismatches in unpacked structs/arrays, or assigning incompatible types (numbers, strings, and class handles). |
| `packed-member-default` | A default member value specified in a packed struct (`struct packed { bit [3:0] lo = 5; … }`). Default values are legal only on unpacked struct members. |
| `specparam-outside-specify` | A `specparam` used outside a `specify` block or another `specparam` declaration. |
| `empty-select` | An indexed part-select (`[i+:w]` or `[i-:w]`) where the selected width `w` is less than 1 bit in elaborated code. |
| `invalid-enum-value` | An enum member initialized with an invalid value: a sized literal whose width does not match the base type (e.g. `4'h2` in an `enum logic [2:0]`), four-state values (`x` or `z`) in a two-state enum, or duplicate values. |
| `invalid-operand` | An operator applied to an incompatible operand type: using `real` with `%`, bitwise/reduction operators, part-selects, or edge events (`@(posedge real_var)`); applying unsupported operators to `string` types (other than comparisons); or invalid operations on class handles (other than equality checks `==` and `!=`). |
| `not-constant` | A hierarchical name (`blk.p`, `u_sub.W`) where a constant is needed: a parameter's value or override, a generate `if` condition, a generate `case` selector or label, or a generate `for` header. A parameter of the interface an interface port connects (`port.W`) is a constant. |
| `elaboration-error` | An elaboration-time `$error` or `$fatal` (one standing among module items, such as in a `generate` block) that runs in what `--top` builds, with the message its arguments format to there. Without `--top`, nothing says a module's defaults are what is built, so none runs. |
| `elaboration-warning` | An elaboration-time `$warning` that runs in what `--top` builds. An `$info` reports nothing wrong, so it is not shown. |

## Drivers with `--top`

With `--top`, `multiple-drivers` is checked per instance of what the tops build, with every parameter applied and every generate loop unrolled. Drivers of separate bits don't clash. A signal in an interface instance is checked across everything that drives it: the module that instantiates it, the interface itself, and every module it is connected to through an interface port. A clash is reported at the instance path where it occurs.

`check` reports a subset of what compilers reject, so code it passes may still fail elsewhere.
