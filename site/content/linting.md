# Linting

`astli lint` catches common design bugs, bad synthesis patterns, and style discrepancies across your SystemVerilog code.

```sh
astli lint top.sv                   # Lint an individual source file
astli lint -f src.f                 # Lint all files enumerated in a filelist
astli lint                          # Lint the current package and its dependencies
astli lint --warn restriction rtl/*.sv  # Turn on a rule group as warnings
astli lint --list                   # List every available rule, group, and default level
```

Most lint rules run in **single-file mode** [as written](design.md): files are inspected without expanding preprocessor includes or macros, and every branch of conditional directives (like `` `ifdef ``) is checked.

A finding at the `deny` level is treated as a hard error and exits with code 1. A finding at `warn` prints a warning diagnostic without failing the run, and `allow` silences the rule.

---

## Automatically applying fixes (`--fix`)

Several lint rules include automated code fixes for reported violations. Pass `--fix` to apply these suggestions directly to source files on disk:

```sh
astli lint --fix rtl/*.sv
astli fmt rtl/*.sv          # Reformat source files after applying fixes
```

- **Semantics-preserving**: Fixes perform strictly equivalent source transformations (such as modernizing legacy keywords) without altering hardware design semantics.
- **Waiver-respecting**: Fixes only apply to active rules and are never applied to lines or files covered by waivers.
- **Minimal edits**: Fixes perform minimal surgical text replacements. Run `astli fmt` afterwards to ensure clean indentation and token alignment.

| Rule | Automated Fix |
| :--- | :--- |
| `forbid-reg` | Replaces legacy `reg` declarations with SystemVerilog `logic` |
| `uvm-macro-semicolon` | Removes redundant trailing semicolons after UVM macros |
| `endif-comment` | Appends `// <NAME>` comments matching the corresponding `` `ifdef ``/`` `ifndef `` condition |
| `legacy-generate-region` | Removes obsolete Verilog-2001 `generate` and `endgenerate` keyword wrappers |

When emitting machine-readable diagnostics with `--diag json`, each diagnostic record includes a `fix` array detailing the precise byte-offset replacements, enabling editor extensions to offer one-click quick-fixes.

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

You can override rule severities using `--allow`, `--warn`, and `--deny`. Specific rule flags take precedence over group flags:

```sh
# Silence all lowRISC style checks, but enforce strict parameter naming as warnings
astli lint --allow lowrisc --warn parameter-name-style rtl/*.sv
```

The level flags have no short forms, because `-D` defines a macro in `lint` just as it does in `check` and in simulators.

---

## Semantic and cross-file rules

Certain lint rules need to understand symbol visibility across the entire design (for instance, determining whether a signal or import is used anywhere). When these rules are active, `astli` runs in whole-design mode, resolving names across all package and dependency files:

- **`unused-signal`**: Flags signals declared inside modules or interfaces that are never read or written.
- **`undriven-signal`**: Detects signals that are read but never assigned a driver.
- **`unused-parameter`**: Finds parameters declared on a module that are never referenced within its body.
- **`unused-import`**: Detects `import pkg::*` or explicit package imports whose symbols are never consumed.

With `--top`, or a `[top.NAME]` table in `astli.toml` that a plain run builds from, these rules report only the files the tops reach: a file no top builds would be judged at its default parameters. Style rules still read every file.

To prevent noisy false positives, `astli` automatically suppresses warnings for names containing `unused`, signals created inside macro definitions, and signals accessed via hierarchical cross-module paths (e.g. `u_core.valid`). Where code constructs cannot be statically traced (such as members a UVM base class may declare), `astli` conservatively assumes the symbol is used.

---

## Elaboration-dependent width checks

Bit-width mismatches often depend on instance parameter overrides. Rules like `width-expand` and `width-trunc` evaluate your design during hierarchical elaboration and require a top-level module:

```sh
astli lint -f design.f --top chip_top
```

These rules detect implicit bit truncation or zero/sign extension during assignments, port connections, and comparisons, reporting the exact instantiation path where the mismatch occurs. A signal read through an instance, a generate block or an interface (`u_sub.y`, `g_lane[2].valid`, `bus.data`) is as wide as the instance, block or interface built there makes it.

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

By default, naming rules enforce the lowRISC SystemVerilog style guide conventions. If your team follows a different naming convention, configure rules under `[lint.rules.<rule>]` rather than disabling them entirely:

```toml
# Ports specify their direction prefix: i_data, o_valid, io_pad
[lint.rules.port-name-suffix]
input  = ["i_*", "ni_*"]
output = ["o_*"]
inout  = ["io_*"]

# Instances must start with u_
[lint.rules.instance-name-style]
patterns = ["u_*"]

# Signals must be camelCase
[lint.rules.signal-name-style]
case = ["camelCase"]
```

- **Wildcards**: Each pattern contains exactly one `*` representing the non-empty body of the identifier.
- **Casing conventions**: A name passes if the wildcard portion matches any of the specified cases. Supported casings: `lower_snake_case`, `ALL_CAPS`, `CamelCase`, and `camelCase`.
- **Precedence**: Specifying an option replaces its default configuration completely. To exempt vendored or legacy code that follows a different style, use `[lint.paths]`.

| Rule | Options and Defaults |
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

---

## Sharing lint styles across repositories (`extends`)

To maintain uniform code quality and naming conventions across multiple repositories, teams can publish a shared style configuration package.

A style package's `astli.toml` defines the base `[lint]` rules and naming options:

```toml
# acme-style/astli.toml
[package]
name = "acme-style"

[lint]
warn = ["restriction"]

[lint.rules.port-name-suffix]
input  = ["i_*"]
output = ["o_*"]
```

Downstream projects include the style package as a development dependency and extend it:

```toml
[dev-dependencies]
acme-style = { git = "https://git.acme.com/hw/style", version = "1.2" }

[lint]
extends = "acme-style"
allow = ["module-filename"]  # Local exceptions for this repository
```

- **Isolated dependencies**: Because the style package is declared under `[dev-dependencies]`, downstream designs consuming your IP will never fetch it.
- **Deterministic CI**: `astli.lock` locks the shared style package version, guaranteeing identical lint rules across all developer workstations and CI runners.
- **Local overrides**: Configurations in the local `astli.toml` take precedence over extended styles. Rule severities and custom options override the base style, and `[lint.paths]` waivers apply on top.
- **Monorepo & Submodule support**: `extends` also accepts relative paths to local directories or TOML files (e.g. `extends = "../common/lint.toml"`).

---

## Machine-readable diagnostics

For integration with IDEs, editors (like VS Code or Vim), and CI problem matchers, you can format diagnostics as compact single-line messages:

```sh
astli lint --diag short rtl/*.sv
```

This outputs standard `file:line:col: severity[code]: message` records. You can also make this the default across all `astli` commands by setting `export ASTLI_DIAGNOSTICS=short` in your environment.

For tooling and custom CI scripts, `--diag json` outputs each diagnostic as a single line of JSON on `stderr`. Records include precise source ranges (line, column, byte offsets), labels, notes, help messages, macro expansion traces, include hierarchies, and automated fix replacements:

```sh
astli lint --diag json 2>&1 | jq -r 'select(.severity == "error") | .path'
```

Every record provides a uniform schema for easy parsing with `jq` or Python. Non-source errors (such as unreadable files) emit records where `code`, `path`, `start`, and `end` are `null`.
