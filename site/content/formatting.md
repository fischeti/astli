# Formatting

`astli fmt` automatically reformats your SystemVerilog files according to the [lowRISC style guide](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md).

```sh
astli fmt rtl/*.sv          # Format specified files in place
astli fmt                   # Format all files in the current package
astli fmt --check -f src.f  # Verify formatting in CI (fails if any file differs)
astli fmt --diff top.sv     # Show a unified diff without modifying the file
astli fmt -                 # Format standard input and write to standard output
```

Each file is formatted [independently](design.md) in single-file mode: preprocessor includes are not traversed, and macro defines are not expanded. This ensures formatting is instantaneous and completely deterministic.

!!! note "Semantic safety guarantee"
    `astli` verifies every formatting transformation by checking that the preprocessed token stream of the formatted output matches the input byte-for-byte (ignoring whitespace and comments). If any formatting operation could potentially alter the semantic meaning of your code, `astli` leaves the file untouched and emits a diagnostic alert.

---

## Configuration

Project-wide formatting rules can be specified in the `[fmt]` table of your `astli.toml`:

```toml
[fmt]
width = 100   # Maximum line length in columns (default: 100)
indent = 2    # Spaces per indentation level (default: 2)
max-pad = 12  # Maximum padding spaces inserted to vertically align tokens (default: 12)
```

`astli fmt` automatically reads configuration from the nearest `astli.toml` found in the directory hierarchy, or from a specific manifest passed via `--config <path>`. All formatting settings reside in configuration files rather than CLI flags, ensuring that developer machines, editor integrations, and CI pipelines format code consistently.

??? info "Understanding `max-pad` alignment thresholds"
    `max-pad` controls how far `astli` will push tokens to achieve vertical alignment:

    - Setting `max-pad = 0` restricts alignment only to tokens that naturally end on the same column.
    - Setting `max-pad` to a value equal to or greater than `width` allows alignment across lines regardless of column distance.
    - The default value (`12`) strikes a balance: it neatly aligns related variables without pushing entire blocks far to the right when a single outlier identifier is exceptionally long.

---

## Vertical column alignment

`astli` automatically aligns consecutive statements and declarations into neat visual columns:

| Element | Alignment Anchor |
| :--- | :--- |
| **Signal declarations** | Variable name |
| **Port lists** | Direction (`input`/`output`), data type, and port name |
| **Parameter lists** | Parameter keyword, data type, parameter name, and `=` operator |
| **Assignments (`assign`, `<=`, `=`)** | Assignment operator |
| **Named instantiations (`.name(...)`)** | Opening parenthesis `(` |
| **Trailing comments** | Comment marker `//` |

```systemverilog
logic        valid_q;
logic [7:0]  data_q;
int unsigned count;

assign ready_o = ~full;
assign data_o  = data_q; // registered output
```

### Grouping and breaking alignment runs

Vertical alignment runs across a continuous group of related lines. You can control where alignment boundaries start and stop:

- **Blank lines break alignment**: Inserting a blank line creates separate alignment blocks. This allows you to group related signals or ports without forcing unrelated signals into awkward column widths:

=== "Input"

    ```systemverilog
    assign a = x;
    assign data_q = y;

    assign counter = z;
    ```

=== "Formatted Output"

    ```systemverilog
    assign a      = x;
    assign data_q = y;

    assign counter = z;
    ```

- **Standalone comments preserve alignment**: A comment line between declarations does **not** break the alignment block, letting you add sectional comments without disrupting visual columns:

```systemverilog
logic        a;
logic [7:0]  data_q;
// Status counter
int unsigned counter;
```

Alignment runs also terminate naturally when:
- Statements transition to a different syntax type (e.g. from declarations to continuous assignments, or entering an `if` block).
- Operators differ (e.g. mixing blocking `=` and non-blocking `<=` assignments).
- The padding distance exceeds `max-pad`.

---

## Disabling formatting (`(* astli_fmt_skip *)`)

If you have hand-crafted tables, complex state matrices, or ASCII art that you want preserved exactly as written, attach the `(* astli_fmt_skip *)` attribute to the item, statement, or module:

```systemverilog
(* astli_fmt_skip *)
assign out = sel ? a
                 : b;
```

The formatted file preserves the interior text verbatim, only adjusting the outer block indentation level. Unrecognized attributes are safely ignored by standard synthesis and simulation tools.

---

## Pre-commit integration

You can run `astli fmt` automatically before every Git commit using [pre-commit](https://pre-commit.com) with the official hook:

```yaml
repos:
  - repo: https://github.com/fischeti/astli-pre-commit
    rev: v0.3.0
    hooks:
      - id: astli-fmt
```

!!! tip "File type filters"
    By default, the hook formats `.sv`, `.svh`, `.v`, and `.vh` files. If you want to restrict formatting exclusively to SystemVerilog, add `types_or: [system-verilog]` under the hook definition.
