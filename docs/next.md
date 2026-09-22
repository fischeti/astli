# M4 — Formatter v0

The working queue. Delete this file when M4 closes.

The formatter is the first consumer of the tree, and whatever it finds wrong
with the tree's shape, the trivia placement or the crate APIs is fixed where it
is found. It comes first, ahead of more grammar or preprocessor work. The
verbatim fallback is what allows that: anything the grammar does not cover is
passed through byte for byte.

## Queue

1. **`svirig-fmt`, first slice.** Module and interface headers, parameter and
   port lists, `assign`, `always_*`, `if`/`case`, instantiations. Everything
   else is emitted verbatim. From the first version:
   - the transparency check as an assertion inside `format`
     ([`preprocessor.md`](preprocessor.md#the-transparency-invariant));
   - idempotency over the corpus;
   - no build input of any kind ([D15](plan.md#4-decisions)).
2. **Widen the slice** according to what the corpus shows is unformatted most
   often.
3. **Revisit the crate APIs** with the formatter as their first real caller.
   Each library crate stays usable on its own, as `svirig preprocess` already
   uses `svirig-preproc` without the grammar.
4. **The eight nodes outside the grammar.** `SHAPE_RATCHET` in
   `svirig-parse/tests/gates.rs` counts corpus nodes whose children are not
   what `svirig.ungram` names, and each is a parser inconsistency: `typedef
   name;` builds a `TYPE_REF` where `typedef class C;` builds a `DECLARATOR`;
   the `?` digit of a casez pattern written in pieces (`2'b 1?`) is read as a
   conditional; a macro standing for an `inside` list is left a bare token;
   and a struct member and an index expression come out incomplete. Fix them
   and take the ratchet to zero, ahead of whichever formatter rule meets them.

## Formatter design, to settle in step 1

- **Line breaking needs an IR.** The gap model in
  [`plan.md`](plan.md#3-formatter-model) handles separation between tokens,
  but not whether a 140-column port list breaks and where. Use a Wadler/Prettier
  document IR: groups, indentation, soft and hard lines. `ruff_formatter` and
  `biome_formatter` are Rust implementations worth reading.
- **Alignment after line breaking** ([D4](plan.md#4-decisions)). The precedent
  is gofmt's `text/tabwriter`: rules emit cell separators, and a post-pass
  aligns runs of consecutive lines that carry cells, broken by blank lines and
  lines without cells. This works with the IR as long as alignable constructs
  (port connections, declarations) are always one per line, which the house
  styles require anyway.
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
