# Formatting

```sh
astli fmt top.sv            # print the formatted file
astli fmt -w rtl/*.sv       # rewrite in place
astli fmt --check -f src.f  # fail if any file in a filelist is unformatted
astli fmt --diff top.sv     # show what would change
astli fmt -                 # stdin to stdout
```

Each file is formatted on its own: includes are not followed and no
`+define+` reaches the formatter, so the output depends only on the file.

Every result is checked to preprocess to the same thing as its input. A file
that would not is left alone and reported, rather than changed in meaning.

## Configuring

The `[fmt]` table of an `astli.toml` sets the layout for a project. Each key
is optional; these are the defaults:

```toml
[fmt]
width = 100   # columns a line may take
indent = 2    # spaces per level; a continuation takes two levels
max-pad = 12  # most spaces a cell is padded by to line up
```

`astli fmt` reads the first `astli.toml` it finds from the current directory
up, or the file `--config` names. There is no flag for each setting, so every
run in the project, in an editor, a hook or CI, formats the same.

`max-pad = 0` lines up only cells that already end together. A value at or
above `width` never limits alignment. Indentation is always spaces.

## Alignment

Consecutive lines of the same kind line up in columns:

| What | Lines up on |
| --- | --- |
| Declarations | the name |
| Ports | the direction, the type and the name |
| Parameters | the keyword, the type, the name and the `=` |
| `assign`s, and statements like `a <= b;` | the operator |
| Named connections, `.name(...)` | the `(` |
| Trailing `//` comments | the `//` |

```systemverilog
logic        valid_q;
logic [7:0]  data_q;
int unsigned count;

assign ready_o = ~full;
assign data_o  = data_q; // registered
```

An initialiser is not aligned: in `logic a = 0;` only the name is.

### Breaking a run

A column lines up across a *run* of lines, and you decide where a run ends.
**A blank line ends it**, so to align two groups separately, put a blank line
between them:

=== "Input"

    ```systemverilog
    assign a = x;
    assign data_q = y;

    assign counter = z;
    ```

=== "Output"

    ```systemverilog
    assign a      = x;
    assign data_q = y;

    assign counter = z;
    ```

The same holds inside a port list: each group of ports separated by a blank
line gets its own columns.

**A comment on its own line does not end a run**, so a group can carry a
heading and still align as one:

```systemverilog
logic        a;
logic [7:0]  data_q;
// the counter
int unsigned counter;
```

A run also ends at:

- **a different kind of line**, such as a declaration after `assign`s, or an
  `if` among nonblocking assignments;
- **a different operator**: `a = 1;` and `b <= 2;` do not align with each
  other;
- **a cell too far from the others**: no cell is padded by more than 12
  spaces, or [`max-pad`](#configuring). A declaration with a much longer type
  than its neighbours starts a new run rather than pushing every name far to
  the right.

Named connections and trailing comments are exempt from that limit.

Blank lines are kept where you put them; several in a row become one.

## Keeping code as written

Put the attribute `(* astli_fmt_skip *)` on an item, statement or member. It
is left as written and only moved to its indentation:

```systemverilog
(* astli_fmt_skip *)
assign out = sel ? a
               : b;
```

Other tools ignore an attribute they do not know.

## With pre-commit

[astli-pre-commit](https://github.com/fischeti/astli-pre-commit) runs the
formatter as a [pre-commit](https://pre-commit.com) hook, rewriting the
`.sv`, `.svh`, `.v` and `.vh` files you commit:

```yaml
repos:
  - repo: https://github.com/fischeti/astli-pre-commit
    rev: v0.2.0
    hooks:
      - id: astli-fmt
```

Each tag installs the astli release of the same version, so `rev` picks the
formatter and `pre-commit autoupdate` upgrades it. To leave Verilog files
alone, add `types_or: [system-verilog]` to the hook.

## In CI

`--check` prints the files that are not formatted and fails if there are any.
In a GitHub Actions workflow:

=== "uv"

    ```yaml
    - uses: astral-sh/setup-uv@v7
    - run: uvx astli@0.2.0 fmt --check -f src.f
    ```

=== "pipx"

    GitHub's runners come with [pipx](https://pipx.pypa.io), so this needs no
    setup step:

    ```yaml
    - run: pipx run --spec astli==0.2.0 astli fmt --check -f src.f
    ```

Pin the version: a new minor version may format differently, and would fail
the check on code that has not changed.
