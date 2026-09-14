# Next steps

The working queue for the rest of M2, written so that picking it up cold costs
an afternoon rather than a week.

This is **not** a decision record. Anything durable belongs in
[`plan.md`](plan.md) or [`preprocessor.md`](preprocessor.md), and anything
knowingly traded away belongs in [`limitations.md`](limitations.md). Delete
this file when M2 closes.

---

## Where things stand

M1 is closed. Five and a half of M2's six rungs are done — the origin map out
of order, for the reason step 3b gave. **Conditional evaluation is all that is
left.**

| Where | What |
| --- | --- |
| `src/lexer.rs` | Tracks the extent of a `` `define `` so that a `\` ending a `//` comment continues the definition instead of being swallowed by it. Not a lexer mode — one rule differs, so one bool suffices. |
| `src/preproc/directive.rs` | Recognises the 22 directives of 1800-2023 22.1 and parses the operands of `` `define ``, `` `undef ``, the conditionals and `` `include ``. The other six keep theirs as a token range. |
| `src/preproc/macros.rs` | The macro table and the reference half: arity, and argument lists delimited as balanced token soup. |
| `src/preproc/mod.rs` | `scan()` — one forward pass giving every directive and every macro reference as flat, non-overlapping `Item`s, plus the table they build. Raw mode's entry point; the expanded path walks tokens against its own running table instead. |
| `src/preproc/tokens.rs` | `TokenId`, `TokenSpan`, `Input` — addressing a token when there is more than one file. |
| `src/preproc/expand.rs` | Step 3b, done. Substitution, the two operators, and the renderer the differential reads. |
| `src/preproc/include.rs` | Step 4, done. The search path, and the trait the files are read through. |
| `crates/svirig-text/` | Step 6, done. Files, spans, line/column, and the expansion chain. The first crate split. |
| `examples/` | `dump-tokens`, `dump-directives`, `dump-expanded`. |
| `tests/include.rs` | The search path, the cross-file table, cycles, and the depth the standard asks for. |
| `tests/differential.rs` | The gate, running on what the one rung left makes comparable. |

`scan()` reports nothing inside a `` `define `` body or inside a macro
argument. Both are *text*, processed where they are used; a nested call is
found by scanning the range that holds it. That boundary is the design, not an
omission: see
[Level B](preprocessor.md#level-b--macro-invocations-are-grammar-atoms).

Two design questions came up and were answered with a measurement. **Where
arity is unknown, a `(` anywhere on the same line opens an argument list** —
arity is unknown for 95% of references and permanently will be, so this
fallback is the main path rather than a corner. And **the differential compares
token sequences, not text**, because the reference separates `)` from an
identifier where we do not and both mean the same thing. Both are written up in
[`preprocessor.md`](preprocessor.md) and in
[`limitations.md`](limitations.md).

Start the next session by reading `src/preproc/expand.rs`'s module doc. With
`svirig-text` and `scan()` it is the shape the last rung plugs into.

---

## Step 3b — expansion — **done**

`expand()` takes a file's tokens and gives back `ExpandedToken`s: a kind and a
`TokenOrigin` rather than a kind and a byte range. Formals and their defaults,
the empty-argument-list case, nested calls, recursion, `` `" ``, ``` `` ```,
`` `__FILE__ `` and `` `__LINE__ `` are all in, and the design is recorded in
[`preprocessor.md`](preprocessor.md#substitution).

**The gate was met for what was comparable then:** 1502 corpus files used
neither an `` `include `` nor a conditional, and all 1502 agreed with
`slang -E --comments` token for token. See [the gate](#the-gate) for where that
count stands now. The normalisation the gate was expected to need turned out to
be one line — lex both sides and drop whitespace — which is *more* conservative
than a text comparison rather than less: separation that was genuinely needed
and not written shows up as two tokens fused into one.

What it does not do is diagnose. Seven separate error conditions in 22.5.1 are
recovered from silently, each in the direction that keeps the surrounding
tokens; they are tabulated in [`limitations.md`](limitations.md) and each one
becomes a diagnostic rather than a change of behaviour once there is a layer to
report to.

~~The scaffolding note from step 6.~~ **Settled:** `svirig-text`'s own tests go
on building their records by hand, because the crate is meant to stand alone
and what it can express has to be answerable without a preprocessor.
`crates/svirig-syntax/tests/expand.rs` and `tests/include.rs` cover the same
ground with real expansions behind them.

## Step 4 — `` `include `` — **done**

Expanded mode only, as [D6](plan.md#4-decisions) requires. An included file is
spliced in where the directive was and walked at the top level however deeply
nested it is; what pulled it in lives on the file rather than on each token,
which is the `include_trace` axis the origin map already had. The design is in
[`preprocessor.md`](preprocessor.md#following-an-include).

The token indices were the work, and they went in first and mechanically:
`TokenId` and `TokenSpan` carry a `FileId` alongside every index the table, a
`MacroRef` and the recursion guard hold, so a body read from one file and an
argument read from another can no longer be confused. That commit changed no
behaviour, which is what made it cheap.

Following includes is also what lets the macro table cross a file boundary, so
the expanded path now walks tokens against its own running table rather than
against a per-file `scan()`. That answers the arity question on this path
outright — see [`limitations.md`](limitations.md).

The corpus's own macro-carried include is
`` `define include_file(f) `include `"f`" `` in `google_riscv-dv`, vendored
into `ibex` and `opentitan`. It is defined and never used, so `tests/include.rs`
carries the shape instead.

## Step 5 — conditional evaluation

**Build.** Nest the flat directive list into regions:
`` `ifdef ``/`` `ifndef `` … `` `elsif `` … `` `else `` … `` `endif ``. Expanded
mode evaluates them against the table and drops inactive branches. Raw mode
keeps every branch and classifies each region self-delimiting or ragged
([Level C](preprocessor.md#level-c--conditionals-as-structured-regions)).

**Rebase the example.** `examples/conditionals.rs` re-implements directive
scanning by hand in 604 lines. It should sit on `scan()` and on the real region
nesting once they exist.

**Re-measure.** The 96.4% self-delimiting figure is a *lower bound*: a macro
expanding to a delimiter is one opaque token to a token-level pass. Expansion
exists now, so the classifier can be re-run against expanded text — this is
the measurement M2 owes and nothing blocks it any more.

**It is also what the oracle is waiting on.** 859 corpus files use a conditional
somewhere in the tree they pull in and are skipped for it; that is the number
this rung turns into coverage.

**Traps.** An `` `endif `` with no opener, and a region that opens in one file
and closes in an included one — legal, and worth deciding about explicitly, and
reachable now that includes are followed. An include guard is a conditional, so
until this lands a header reached twice is read twice.

## Step 6 — the origin map — **done**

`svirig-text` exists: `FileId`, `Span`, `Origins`, `Expansion`, and
`TokenOrigin`. Files, included files, and synthesised buffers all go in one
store; a token's provenance is a span plus an optional expansion, and
expansions chain through a parent. The design and why it departs from the
state of the art are in [D9](plan.md#per-token-provenance) and
[preprocessor.md](preprocessor.md#two-output-modes).

What it does *not* have, and neither needs yet:

- **Diagnostic rendering.** `trace` and `reported_at` carry what a renderer
  needs; the renderer waits for a diagnostics layer to live in.
- **`` `line ``.** Recorded in [`limitations.md`](limitations.md). Zero
  occurrences in the corpus.
- **UTF-16 columns.** `LineCol` counts characters. An editor will want code
  units; nothing here has an editor.

Its tests go on building their expansion records by hand, and should: the crate
is meant to stand alone, so what its model can express has to be answerable
without a preprocessor. `crates/svirig-syntax/tests/expand.rs` and
`tests/include.rs` cover the same ground with real expansions behind them.

---

## The gate

**Met for expansion and includes, and restricted by the one rung left.**
`crates/svirig-syntax/tests/differential.rs` runs `slang -E --comments` over
the corpus and compares token sequences; 1507 files are comparable and all
agree. Comparability is asked of every file an expansion *read*, not only of
the one named — a source with no conditional of its own routinely includes a
header that chooses its contents with one — and 859 files fail that test, which
is the number step 5 turns into coverage. The test asserts on the count as well
as on agreement, because a file quietly *ceasing* to be comparable is how this
rots.

The reference declines another 3260 for want of an include path it has not been
told about. Reaching those means passing *both* sides what a filelist or
`bender` would give us, which is work for the driver rather than for M2.

```bash
cargo test && cargo clippy --all-targets && cargo fmt -- --check
```

## Left over from step 2

- The six `Unparsed` directives take the end of the line as their extent even
  though their syntax has a defined end. Recorded in
  [`limitations.md`](limitations.md). Small, and blocks nothing.
- ~~`examples/dump-tokens.rs` has no directive-level counterpart.~~ **Done:**
  `examples/dump-directives.rs`.
