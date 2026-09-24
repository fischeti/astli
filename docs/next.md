# M4 — Formatter v0

The working queue. Delete this file when M4 closes. The formatter's style and
design are in [`formatter.md`](formatter.md).

## Now

Fix the parser inconsistencies behind `SHAPE_RATCHET` in
`svirig-parse/tests/gates.rs` (corpus nodes whose children are not what
`svirig.ungram` names), ahead of whichever formatter rule meets them, and take
the ratchet to zero:

1. `typedef name;` builds a `TYPE_REF` where `typedef class C;` builds a
   `DECLARATOR`.
2. The `?` digit of a casez pattern written in pieces (`2'b 1?`) is read as a
   conditional.
3. A macro standing for an `inside` list is left a bare token.
4. A struct member comes out incomplete.

Then the rest of the M4 gate. Idempotency and transparency hold over the
corpus (`svirig-fmt/tests/gates.rs`); `slang --parse-only` agreeing before and
after is not checked yet:

5. A `corpus_*` test after `svirig-preproc/tests/differential.rs`, which
   already runs `slang` over the corpus and counts the files it declines.

## Then

- **Revisit the crate APIs** with the formatter as their first real caller.
  Each library crate stays usable on its own, as `svirig preprocess` already
  uses `svirig-preproc` without the grammar. Needs a list of concrete changes
  before it is a step.
- **The shapes rules fall back on.** `PAREN_EXPR` 0.4%, `ARG_LIST` 0.3% and
  `CALL_EXPR` 0.1% are left unformatted where their rules give up: look at
  which shapes before writing more.
