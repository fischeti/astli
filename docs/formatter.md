# Formatter

The style `svirig-fmt` produces, and the machinery behind it. The formatter is
the first consumer of the tree, and whatever it finds wrong with the tree's
shape, the trivia placement or the crate APIs is fixed where it is found. The
verbatim fallback lets it run ahead of the grammar: anything the parser does
not cover is passed through byte for byte. Rules lay out 89.1% of the corpus's
tokens (`cargo run --release -p svirig-fmt --example unformatted` shows what
is left, by node kind).

## Style

The target is
[lowRISC's style guide](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md),
which PULP follows too.

- **Branching directives sit at column 0**, nested or not: `` `ifdef ``,
  `` `ifndef ``, `` `elsif ``, `` `else ``, `` `endif ``. A branch's contents
  are indented as if the directives were absent. Every other directive, and
  every macro call, is indented like code.
- **A verbatim run moves as a block.** Its first line takes the indentation of
  where it stands, and every later line shifts by as much, stopping at column
  0. A run that starts mid-line and has a later line left of its start hangs
  off its line's indentation instead, and shifts by as much as that did. A
  line that starts inside a token (a string, a block comment) or inside a
  `` `define `` stays where it is.
- **Named connections align**, ports and parameters alike: every `(` in the
  column after the longest name, nothing inside the parentheses. A `.name`
  without parentheses stays in the table with nothing to pad.
- **Declaration names align** within a run of consecutive declarations; any
  other item ends the run. Initialisers are not aligned. A space goes around
  packed dimensions, and none between dimensions or before unpacked ones.
- **Parameter declarations align** in four columns, as the guide's header
  example does: the keyword, the type, the name, and the `=`.
- **A header's imports go on lines of their own**, one level in, and the
  parameter or port list starts the line after, as the guide has it and 231
  headers in the corpus do to 148 with the import on the `module` line.
- **Ports align** in three columns: the direction, the type and the name. The
  direction is padded so that types start together, as 1223 port lists in the
  corpus have it to 362 that follow it with one space.
- **An `enum`, `struct` or `union` has one entry per line**, as 1253 enums
  and 5474 structs in the corpus do to 84 and 27 on one line. Members line up
  as declarations do, variants on their `=`, and the name follows the `}`.
- **Trailing comments align** as the last column of their table, among the
  rows that have one. A run of items of a line or so each (`assign`s,
  statements, imports, macro calls) is a table of its comments alone; an item
  with a body, such as an `always` block, ends it. A line comment right below
  one, in the same column, continues it and moves with it.
- **Line endings are kept.** A file is written back with the ending it came
  with, since a CRLF `` `define `` body must stay byte for byte.

## Expressions

A statement first breaks inside itself at an expression. The guide allows two
forms: indent the continuation by four, or align it with the open `(` or `{`.
We take the second, and the first only where the second cannot fit.

- **A continued line aligns with what it continues**: under the first operand
  after the innermost open `(`, `{` or `'{`, or, with none open, under the
  start of the expression (after `assign x = `, or `if (`). The closer stays
  on the last line.
- **Too far right, the group indents instead.** If a line of the aligned
  layout would pass the width, or be aligned past half of it, that group
  breaks after its opener, indents by four and puts its closer on a line of
  its own, the guide's other form.
- **Inside `[…]`, binary operators take no space**, unless the tokens would
  run together: 27606 `[W-1:0]` to 181 `[W - 1:0]` in the corpus. Nor does
  `:`, 17732 to 72; `+:` and `-:` take one on either side, 927 to 618. A
  replication's count is written the same way (`{W-1{a}}`), since in
  `{W - 1{a}}` the `1` reads as the count; the corpus is split, 173 to 158.
- **Breaks go after an operator or a comma**, never before. The corpus puts
  `&&` at the end of a line 2830 times and at the start 140.
- **A chain of one operator is one group** (`a && b && c` breaks at every
  `&&`). An operand that binds tighter is a group of its own, and breaks only
  if it does not fit alone.
- **A broken list is packed**: arguments and the elements of a
  concatenation, as many on a line as fit, as the guide's examples do. An
  item that does not fit on a line of its own starts one and breaks inside.
  An assignment pattern instead ends its line with `'{`, puts one item per
  line a continuation in and closes on a line of its own, as a struct's body
  is laid out: 975 broken patterns in the corpus go one per line to 122
  packed, and 761 break after `'{` to 336 aligned under the first item.
- **Nothing breaks inside `[…]`, around `.` or `::`, or between a callee and
  its `(`.** Index, field and scope expressions are atoms.
- **A ternary chain through its else arms is one group**, a priority mux with
  `c ? a :` on each line and the final value on a line of its own, 274 broken
  chains in the corpus to 224 that keep it with the last condition. One
  ternary breaks the same way, after its `:`. A chain the input broke stays
  broken even if it fits, since that layout is what lets the guide drop its
  parentheses. Conditions are not padded to line up their `?`s: the corpus
  is split, 192 to 192.
- **A line comment inside an expression breaks every group around it.**

## Design

- **Line breaking uses a document IR**, Wadler/Prettier style: groups,
  indentation, soft and hard lines. The gap model in
  [`plan.md`](plan.md#3-formatter-model) handles separation between tokens,
  but not whether a 140-column port list breaks and where. `ruff_formatter`
  and `biome_formatter` are Rust implementations worth reading.
- **Alignment comes after line breaking** ([D4](plan.md#4-decisions)), after
  gofmt's `text/tabwriter`. A rule marks the ends of cells with `Doc::Cell`
  and the rows that align together with `Doc::Table`; the printer notes where
  each cell landed, and `align.rs` pads. This needs alignable constructs
  (connections, declarations) one per line, which the house styles require
  anyway.
- **Comment placement belongs to the formatter.** `comments.rs` maps each
  comment to the node it leads or trails, as Biome and ruff do, instead of
  relying on the same-line rule in `svirig-parse`'s tree builder. The tree
  keeps its trivia tokens as they are.

What the printer has for expressions, in `doc.rs`:

- **`Doc::Align`**: lines broken inside start at the column the printer is at.
  When padding moves the line an aligned group starts on, its later lines move
  by as much, recorded as `Continuation`s like a verbatim run's.
- **`Doc::Fill`**, Wadler's `fill`: each separator breaks only if the part
  after it does not fit. Whether it fits is measured up to its first break
  that cannot be flat, so a multi-line argument or a trailing comment does
  not push its part onto the next line.
- **`Doc::Prefer`**: the aligned form, unless a trial print of it passes the
  width or aligns a line past half of it; then the indented form.
- A verbatim run that starts on an aligned line hangs off the alignment
  column, which is that line's indentation, so the hanging rule needed no
  change.
