# M4 — Formatter v0

The working queue. Delete this file when M4 closes.

The formatter is the first consumer of the tree, and whatever it finds wrong
with the tree's shape, the trivia placement or the crate APIs is fixed where it
is found. It comes first, ahead of more grammar or preprocessor work. The
verbatim fallback is what allows that: anything the grammar does not cover is
passed through byte for byte.

## Queue

1. *Done.* **`svirig-fmt`, first slice.** Design units with their parameter
   and port lists, conditional regions, `assign`, procedural blocks and
   `begin`/`end`, `if`/`case`, instantiations. A node without a rule moves as
   a block. Rules lay out 26.9% of the corpus's tokens.
2. **Widen the slice** according to what the corpus shows is unformatted most
   often: `cargo run --release -p svirig-fmt --example unformatted`. One
   construct per commit, with `.sv` cases under `svirig-fmt/tests/data`.
   Classes, functions and tasks, loops and variable declarations are done;
   rules lay out 39.9%, and expressions and parameter declarations hold the
   largest share of the rest.
3. **Revisit the crate APIs** with the formatter as their first real caller.
   Each library crate stays usable on its own, as `svirig preprocess` already
   uses `svirig-preproc` without the grammar.
4. **The six nodes outside the grammar.** `SHAPE_RATCHET` in
   `svirig-parse/tests/gates.rs` counts corpus nodes whose children are not
   what `svirig.ungram` names, and each is a parser inconsistency: `typedef
   name;` builds a `TYPE_REF` where `typedef class C;` builds a `DECLARATOR`;
   the `?` digit of a casez pattern written in pieces (`2'b 1?`) is read as a
   conditional; a macro standing for an `inside` list is left a bare token;
   and a struct member comes out incomplete. Fix them and take the ratchet to
   zero, ahead of whichever formatter rule meets them.

## Style

The target is lowRISC's style guide, which PULP follows too; a copy is in
`reference/lowrisc-verilog-style.md`.

- **Branching directives sit at column 0**, nested or not: `` `ifdef ``,
  `` `ifndef ``, `` `elsif ``, `` `else ``, `` `endif ``. A branch's contents
  are indented as if the directives were absent. Every other directive, and
  every macro call, is indented like code.
- **A verbatim run moves as a block.** Its first line takes the indentation of
  where it stands, and every later line shifts by as much, stopping at column
  0. A line that starts inside a token (a string, a block comment) or inside a
  `` `define `` stays where it is.
- **Named connections align**, ports and parameters alike: every `(` in the
  column after the longest name, nothing inside the parentheses. A `.name`
  without parentheses stays in the table with nothing to pad.
- **Declaration names align** within a run of consecutive declarations; any
  other item ends the run. Initialisers are not aligned. A space goes around
  packed dimensions, and none between dimensions or before unpacked ones.

- **Line endings are kept.** A file is written back with the ending it came
  with, since a CRLF `` `define `` body must stay byte for byte.

## Formatter design, to settle in step 1

- **Line breaking needs an IR.** The gap model in
  [`plan.md`](plan.md#3-formatter-model) handles separation between tokens,
  but not whether a 140-column port list breaks and where. Use a Wadler/Prettier
  document IR: groups, indentation, soft and hard lines. `ruff_formatter` and
  `biome_formatter` are Rust implementations worth reading.
- **Alignment after line breaking** ([D4](plan.md#4-decisions)), after gofmt's
  `text/tabwriter`. A rule marks the ends of cells with `Doc::Cell` and the
  rows that align together with `Doc::Table`; the printer notes where each
  cell landed, and `align.rs` pads. A later line of a verbatim run moves with
  its first. This needs alignable constructs (connections, declarations) one
  per line, which the house styles require anyway.
- **Comment placement belongs to the formatter.** Build a map of leading,
  trailing and dangling comments per node from the tree, as Biome and ruff do,
  instead of relying on the same-line rule in `svirig-parse`'s tree builder.
  The tree keeps its trivia tokens as they are.

## Deferred on purpose

- **Rollback of whole constructs**, in
  [`limitations.md`](limitations.md#a-construct-missing-its-closer-falls-back-whole).
- **Preprocessor performance** (shared store, single-unit mode), which waits
  until something consumes expanded mode
  ([`preprocessor.md`](preprocessor.md#cost-of-a-table-per-file)).
- **Tests in a hook or in CI.** The quick profile takes 2.6 s, so adding it to
  pre-push costs almost nothing when it is wanted.
