# Formatting

`astli fmt` lays out SystemVerilog in the [lowRISC style guide](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md).

```sh
astli fmt rtl/*.sv          # Format specified files in place
astli fmt                   # Format all files in the current package
astli fmt --check -f src.f  # Fail if any file would change, as in CI
astli fmt --diff top.sv     # Print what would change as a diff
astli fmt -                 # Format standard input and write to standard output
```

Each file is formatted [as written](design.md), on its own: includes aren't followed and macros aren't expanded, so the output depends on the file alone.

Before writing a file, `astli fmt` checks that the output preprocesses as the input does, under any set of definitions: the same tokens and comments, the same lines in every directive, and every `` `define `` body as it was. Output that fails the check is a bug in the formatter, and the file is left as it was, with an error that says where.

## Configuration

Project-wide formatting rules can be specified in the `[fmt]` table of your `astli.toml`:

```toml
[fmt]
width = 100   # Maximum line length in columns (default: 100)
indent = 2    # Spaces per indentation level (default: 2)
max-pad = 12  # Maximum padding spaces inserted to vertically align tokens (default: 12)
exclude = ["hw/vendor", "hw/legacy/**/*.sv"]  # Files to leave as they are (default: none)
```

A comment at the end of a line doesn't count towards `width`, so code that fits stays on one line and its comment runs past the margin.

`astli fmt` reads the nearest `astli.toml` above the file, or the one `--config <path>` names. The layout has no command-line flags, so every machine, editor and CI job formats the same way.

### Excluding files from formatting

`exclude` lists globs, relative to `astli.toml`, of files to leave as they are:

```toml
[fmt]
exclude = ["hw/vendor", "hw/legacy/**/*.sv"]
```

A directory excludes everything under it. An excluded file is left alone even when it is named on the command line, so a pre-commit hook that passes the staged files agrees with `astli fmt --check` in CI. Standard input (`astli fmt -`) is always formatted.

`exclude` also lets an existing project adopt `astli fmt` a directory at a time: exclude what isn't formatted yet, turn on `astli fmt --check` in CI, and shorten the list as you go.

`max-pad` is the most padding `astli` inserts to line a token up with the lines around it. `0` aligns only what already lines up, and a value of `width` or more aligns regardless of distance. The default, 12, aligns related declarations without pushing a whole block to the right because one name is long.

## Vertical column alignment

Consecutive declarations and statements of one kind are aligned in columns:

| Element | Alignment Anchor |
| :--- | :--- |
| Signal declarations | Variable name |
| Port lists | Direction (`input`/`output`), data type, and port name |
| Parameter lists | Parameter keyword, data type, parameter name, and `=` operator |
| Assignments (`assign`, `<=`, `=`) | Assignment operator |
| Named instantiations (`.name(...)`) | Opening parenthesis `(` |
| Trailing comments | Comment marker `//` |

```systemverilog
logic        valid_q;
logic [7:0]  data_q;
int unsigned count;

assign ready_o = ~full;
assign data_o  = data_q; // registered output
```

### Grouping and breaking alignment runs

A blank line ends a run of aligned lines, so groups separated by one are aligned on their own:

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

A comment on a line of its own doesn't end the run:

```systemverilog
logic        a;
logic [7:0]  data_q;
// Status counter
int unsigned counter;
```

A run also ends where the kind of line changes (from declarations to `assign`s, say), where the operator changes (`=` to `<=`), and where aligning would need more than `max-pad`.

## Disabling formatting (`(* astli_fmt_skip *)`)

To keep a hand-laid table or diagram as written, attach `(* astli_fmt_skip *)` to the item, statement or module:

```systemverilog
(* astli_fmt_skip *)
assign out = sel ? a
                 : b;
```

Its text is kept as written, and only its indentation follows the code around it. Other tools ignore an attribute they don't know.

## Pre-commit integration

The [pre-commit](https://pre-commit.com) hook runs `astli fmt` on the files a commit stages:

```yaml
repos:
  - repo: https://github.com/fischeti/astli-pre-commit
    rev: v0.5.0
    hooks:
      - id: astli-fmt
```

It formats `.sv`, `.svh`, `.v` and `.vh` files. To format SystemVerilog alone, add `types_or: [system-verilog]` to the hook.
