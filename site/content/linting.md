# Linting

`astli lint` catches common design bugs, bad synthesis patterns, and style discrepancies across your SystemVerilog code.

```sh
astli lint top.sv                   # Lint an individual source file
astli lint -f src.f                 # Lint all files enumerated in a filelist
astli lint                          # Lint the current package and its dependencies
astli lint -W correctness rtl/*.sv  # Promote a specific rule group to warnings
astli lint --list                   # List every available rule, group, and default level
```

Most lint rules run in **single-file mode** [as written](design.md): files are inspected without expanding preprocessor includes or macros, and every branch of conditional directives (like `` `ifdef ``) is checked.

A finding at the `deny` level is treated as a hard error and exits with code 1. A finding at `warn` prints a warning diagnostic without failing the run, and `allow` silences the rule.

---

## Rule groups and severity levels

Every rule belongs to a logical group with a default severity:

| Group | Default Level | Description |
| :--- | :---: | :--- |
| **`correctness`** | `deny` | Definite bugs or dangerous synthesis mismatches (e.g. blocking assignments inside `always_ff`, duplicate `case` items). |
| **`suspicious`** | `warn` | Legal SystemVerilog constructs that are usually unintentional or error-prone (e.g. legacy `always @*`, missing `default` in `case`). |
| **`lowrisc`** | `warn` | Style and naming conventions from the [lowRISC style guide](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md) (e.g. signal casing, suffix conventions). |
| **`restriction`** | `allow` | Restrictive project policies you can selectively enable (e.g. forbidding multiple modules per file, requiring explicit `begin...end` blocks). |

!!! tip "Verible compatibility"
    Rules that correspond to checks in [Verible](https://github.com/chipsalliance/verible) share identical rule names and semantic checks, making migration between tools effortless. See the complete [Lint Rule Reference](reference/lint-rules.md).

### Overriding levels on the command line

You can override rule severities using `-A` (`allow`), `-W` (`warn`), and `-D` (`deny`). Specific rule flags take precedence over group flags:

```sh
# Silence all lowRISC style checks, but enforce strict parameter naming as warnings
astli lint -A lowrisc -W parameter-name-style rtl/*.sv
```

---

## Semantic and cross-file rules

Certain lint rules need to understand symbol visibility across the entire design (for instance, determining whether a signal or import is used anywhere). When these rules are active, `astli` runs in whole-design mode, resolving names across all package and dependency files:

- **`unused-signal`**: Flags signals declared inside modules or interfaces that are never read or written.
- **`undriven-signal`**: Detects signals that are read but never assigned a driver.
- **`unused-parameter`**: Finds parameters declared on a module that are never referenced within its body.
- **`unused-import`**: Detects `import pkg::*` or explicit package imports whose symbols are never consumed.

With `--top`, or a `[top.NAME]` table in `astli.toml` that a plain run builds from, these rules report only the files the tops reach: a file no top builds would be judged at its default parameters. Style rules still read every file.

To prevent noisy false positives, `astli` automatically suppresses warnings for names containing `unused`, signals created inside macro definitions, and signals accessed via hierarchical cross-module paths (e.g. `u_core.valid`). Where code constructs cannot be statically traced (such as dynamic class handles), `astli` conservatively assumes the symbol is used.

---

## Elaboration-dependent width checks

Bit-width mismatches often depend on instance parameter overrides. Rules like `width-expand` and `width-trunc` evaluate your design during hierarchical elaboration and require a top-level module:

```sh
astli lint -f design.f --top chip_top
```

These rules detect implicit bit truncation or zero/sign extension during assignments, port connections, and comparisons, reporting the exact instantiation path where the mismatch occurs.

??? info "Intelligent suppression of width noise"
    Unlike legacy linters that emit hundreds of spurious width warnings on idiomatic constructs, `astli` incorporates smart heuristics:

    - Small integer literals adapt to the expression width without warning (e.g. `count <= count + 1` or `data = 0`).
    - Compound concatenations accounting for carries (e.g. `{carry, sum} = a + b`) are recognized as intentional.
    - Sized left-shifts targeting wider destination registers are permitted.

---

## In-code waivers

To suppress a lint rule for a specific statement, block, declaration, or module, annotate the construct with the `(* astli_allow *)` attribute:

```systemverilog
// Waive blocking assignment check for this specific always_ff block
(* astli_allow = "always-ff-non-blocking" *)
always_ff @(posedge clk_i) begin
  q = d;
end
```

The attribute applies to the annotated construct and all nested child elements. You can waive multiple rules or entire groups by separating names with commas (e.g. `(* astli_allow = "lowrisc, unused-signal" *)`).

If an unrecognized rule name is passed in an attribute, `astli` generates an informative warning so typos don't silently mask problems.

---

## Path-based configuration

For third-party vendor code, auto-generated headers, or verification directories where in-code attributes cannot easily be added, configure rule severities by file path in `astli.toml`:

```toml
[lint]
# Enable restriction group checks across the project
warn = ["restriction"]

[lint.paths]
# Relax style rules on third-party vendor IP
"hw/vendor/**" = { allow = ["lowrisc"] }

# Allow non-standard macro naming in UVM verification utilities
"hw/dv/**/dv_macros.svh" = { allow = ["macro-name-style"] }
```

Globs are resolved relative to the directory containing `astli.toml`.

---

## Naming styles

The naming rules follow lowRISC's style by default. If your team names things differently, configure the rule in `[lint.rules.<rule>]` instead of turning it off:

```toml
# Ports say their direction first: i_data, o_valid, io_pad
[lint.rules.port-name-suffix]
input  = ["i_*", "ni_*"]
output = ["o_*"]
inout  = ["io_*"]

# Instances start with u_
[lint.rules.instance-name-style]
patterns = ["u_*"]

# Signals in camelCase
[lint.rules.signal-name-style]
case = ["camelCase"]
```

A pattern has exactly one `*`, which stands for the rest of the name and can't be empty. A name passes if it matches any one of the patterns and the part the `*` stands for is in any one of the cases. The cases are `lower_snake_case`, `ALL_CAPS`, `CamelCase` and `camelCase`.

| Rule | Options and their defaults |
| :--- | :--- |
| `signal-name-style` | `case = ["lower_snake_case"]`, `patterns = ["*"]` |
| `instance-name-style` | `case = ["lower_snake_case"]`, `patterns = ["*"]` |
| `parameter-name-style` | `case = ["CamelCase", "ALL_CAPS"]`, `patterns = ["*"]` |
| `macro-name-style` | `case = ["ALL_CAPS"]`, `patterns = ["*"]` (UVM's `uvm_*` macros always pass) |
| `enum-name-style` | `case = ["lower_snake_case"]`, `patterns = ["*_e", "*_t"]` |
| `struct-union-name-style` | `case = ["lower_snake_case"]`, `patterns = ["*_t"]` |
| `interface-name-style` | `case = ["lower_snake_case"]`, `patterns = ["*_if"]` |
| `constraint-name-style` | `case = ["lower_snake_case"]`, `patterns = ["*_c"]` |
| `generate-label-prefix` | `patterns = ["gen_*", "g_*"]` |
| `port-name-suffix` | `input = ["*_i", "*_ni", "*_pi"]`, `output = ["*_o", "*_no", "*_po"]`, `inout = ["*_io", "*_nio", "*_pio"]` |

Setting an option replaces its default entirely. Options apply to the whole project; for code that follows another style, such as vendored IP, use `[lint.paths]` to allow the rule.

---

## Machine-readable diagnostics

For integration with IDEs, editors (like VS Code or Vim), and CI problem matchers, you can format diagnostics as compact single-line messages:

```sh
astli lint --diag short rtl/*.sv
```

This outputs standard `file:line:col: severity[code]: message` records. You can also make this the default across all `astli` commands by setting `export ASTLI_DIAGNOSTICS=short` in your environment.

For a script, `--diag json` writes each diagnostic as one line of JSON, with its start and end, label, notes, and the macro calls and includes it came through. Every key is in every record, and stderr holds nothing else, so it can be piped to `jq`:

```sh
astli lint --diag json 2>&1 | jq -r 'select(.severity == "error") | .path'
```

An error with no place in a file, such as a file that cannot be read, is a record whose `code`, `path`, `start` and `end` are `null`.
