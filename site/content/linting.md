# Linting

`astli lint` reports likely bugs, constructs that synthesize differently from how they simulate, and departures from a naming style.

```sh
astli lint top.sv                       # Lint one file
astli lint -f src.f                     # Lint the files a filelist names
astli lint                              # Lint the current package and its dependencies
astli lint --warn restriction rtl/*.sv  # Turn on a rule group as warnings
astli lint --list                       # List every rule, its group, and its level
```

Most rules read each file [as written](design.md): without expanding includes or macros, and in every branch of an `` `ifdef ``.

A finding at `deny` is an error and fails the run with exit code 1. A finding at `warn` is printed and does not fail the run. `allow` turns the rule off.

## Rule groups and levels

Every rule belongs to a group, which gives it its default level:

| Group | Default | What it holds |
| :--- | :---: | :--- |
| `correctness` | `deny` | Definite bugs, such as a blocking assignment in `always_ff`, or a `case` item listed twice. |
| `suspicious` | `warn` | Legal code that is usually a mistake, such as `always @*`, or a `case` without a `default`. |
| `lowrisc` | `warn` | The naming and layout conventions of the [lowRISC style guide](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md). |
| `restriction` | `allow` | Policies a project may choose, such as one module per file, or `begin`/`end` around every body. |

A rule that matches a check in [Verible](https://github.com/chipsalliance/verible) has Verible's name. The [lint rule reference](reference/lint-rules.md) lists every rule.

`--allow`, `--warn` and `--deny` take a group or a rule. A rule named on its own wins over its group:

```sh
# Silence lowRISC style, but keep parameter naming as a warning
astli lint --allow lowrisc --warn parameter-name-style rtl/*.sv
```

## Applying fixes

Some rules carry a fix, and `--fix` writes it into the file:

```sh
astli lint --fix rtl/*.sv
astli fmt rtl/*.sv          # Lay out what the fixes changed
```

A fix doesn't change what the code means. It only rewrites the text it has to, so run `astli fmt` afterwards. A rule that is allowed or waived applies no fix.

| Rule | Fix |
| :--- | :--- |
| `forbid-reg` | Replaces `reg` with `logic` |
| `uvm-macro-semicolon` | Removes the `;` after a UVM macro |
| `endif-comment` | Adds `// NAME` after an `` `endif `` |
| `legacy-generate-region` | Removes `generate` and `endgenerate` |

## Rules that read the whole design

Some rules need to know whether a name is used anywhere in the design. When any of them is on, `astli lint` reads the whole design, as [`astli check`](checking.md) does:

- `unused-signal`: a signal nothing reads or writes.
- `undriven-signal`: a signal something reads and nothing drives.
- `unused-parameter`: a parameter its module never uses.
- `unused-import`: an import nothing uses.

With `--top`, or a [`[top.NAME]`](reference/manifest.md#topname) table that a plain run builds from, these rules report only on the files the tops reach. Style rules still read every file.

These rules skip a name that contains `unused`, a signal a macro declares, and a signal used through a hierarchical path such as `u_core.valid`. If a name might be declared somewhere `astli` can't see, such as a UVM base class, it counts as used.

In this mode, code that doesn't parse is a `not-parsed` error, as it is in `astli check`.

## Width checks

`width-expand` and `width-trunc` report an assignment, connection or comparison that widens or truncates a value. A width often depends on the parameters an instance is given, so these rules need a top:

```sh
astli lint -f design.f --top chip_top
```

Each finding names the instance path where the mismatch occurs. A signal reached through an instance, a generate block or an interface, such as `u_sub.y`, `g_lane[2].valid` or `bus.data`, has the width that instance gives it.

Some code is left alone on purpose:

- A small literal takes the width of the expression it is in, as in `count <= count + 1` or `data = 0`.
- A concatenation that catches the carry, as in `{carry, sum} = a + b`.
- A sized left shift into a wider destination.

## Waivers

`(* astli_allow *)` waives rules for the construct it is attached to and everything inside it:

```systemverilog
(* astli_allow = "always-ff-non-blocking" *)
always_ff @(posedge clk_i) begin
  q = d;
end
```

It takes a comma-separated list of rules and groups, such as `(* astli_allow = "lowrisc, unused-signal" *)`. A name that isn't a rule or a group is a warning.

For code you can't annotate, such as vendored IP or generated files, `[lint.paths]` in `astli.toml` sets levels by path:

```toml
[lint]
warn = ["restriction"]

[lint.paths]
"hw/vendor/**" = { allow = ["lowrisc"] }
"hw/dv/**/dv_macros.svh" = { allow = ["macro-name-style"] }
```

Paths are relative to the directory that holds `astli.toml`.

## Naming styles

The naming rules default to the lowRISC conventions. To use another convention, configure the rule under `[lint.rules.<rule>]` rather than turning it off:

```toml
# Ports start with their direction: i_data, o_valid, io_pad
[lint.rules.port-name-suffix]
input  = ["i_*", "ni_*"]
output = ["o_*"]
inout  = ["io_*"]

[lint.rules.instance-name-style]
patterns = ["u_*"]

[lint.rules.signal-name-style]
case = ["camelCase"]
```

A pattern has exactly one `*`, which stands for the rest of the name. That part must not be empty, and it must be in one of the cases listed: `lower_snake_case`, `ALL_CAPS`, `CamelCase` or `camelCase`. An option you set replaces its default instead of adding to it.

| Rule | Defaults |
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

## Sharing a style (`extends`)

A team can keep its lint configuration in a package of its own:

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

A project depends on that package as a dev-dependency and extends it:

```toml
[dev-dependencies]
acme-style = { git = "https://git.acme.com/hw/style", version = "1.2" }

[lint]
extends = "acme-style"
allow = ["module-filename"]
```

As a dev-dependency, the style isn't fetched by packages that depend on the project, and `astli.lock` pins its version. Anything the project's own `astli.toml` sets wins over the style, and `[lint.paths]` applies on top of both. `extends` also takes a path to a directory or a TOML file, such as `extends = "../common/lint.toml"`.

## Output for tools

`--diag short` prints one `file:line:col: severity[code]: message` line per finding. `--diag json` writes one JSON record per finding, including any fix. See [Diagnostic formats](reference/diagnostics.md).
