# Next steps

The working queue for the rest of M2, written so that picking it up cold costs
an afternoon rather than a week.

This is **not** a decision record. Anything durable belongs in
[`plan.md`](plan.md) or [`preprocessor.md`](preprocessor.md), and anything
knowingly traded away belongs in [`limitations.md`](limitations.md). Delete
this file when M2 closes.

---

## Where things stand

M1 is closed. Four and a half of M2's six rungs are done — the origin map out
of order, for the reason step 3b gave.

| Where | What |
| --- | --- |
| `src/lexer.rs` | Tracks the extent of a `` `define `` so that a `\` ending a `//` comment continues the definition instead of being swallowed by it. Not a lexer mode — one rule differs, so one bool suffices. |
| `src/preproc/directive.rs` | Recognises the 22 directives of 1800-2023 22.1 and parses the operands of `` `define ``, `` `undef ``, the conditionals and `` `include ``. The other six keep theirs as a token range. |
| `src/preproc/macros.rs` | The macro table and the reference half: arity, and argument lists delimited as balanced token soup. |
| `src/preproc/mod.rs` | `scan()` — one forward pass giving every directive and every macro reference as flat, non-overlapping `Item`s, plus the table they build. |
| `src/preproc/expand.rs` | Step 3b, done. Substitution, the two operators, and the renderer the differential reads. |
| `crates/svirig-text/` | Step 6, done. Files, spans, line/column, and the expansion chain. The first crate split. |
| `examples/` | `dump-tokens`, `dump-directives`, `dump-expanded`. |
| `tests/differential.rs` | The gate, running on what the two rungs left make comparable. |

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
`svirig-text` and `scan()` it is the shape both rungs below plug into.

---

## Step 3b — expansion — **done**

`expand()` takes a file's tokens and gives back `ExpandedToken`s: a kind and a
`TokenOrigin` rather than a kind and a byte range. Formals and their defaults,
the empty-argument-list case, nested calls, recursion, `` `" ``, ``` `` ```,
`` `__FILE__ `` and `` `__LINE__ `` are all in, and the design is recorded in
[`preprocessor.md`](preprocessor.md#substitution).

**The gate is met for what is comparable.** 1502 corpus files use neither an
`` `include `` nor a conditional; all 1502 agree with `slang -E --comments`
token for token. The normalisation the gate was expected to need turned out to
be one line — lex both sides and drop whitespace — which is *more* conservative
than a text comparison rather than less: separation that was genuinely needed
and not written shows up as two tokens fused into one.

What it does not do is diagnose. Seven separate error conditions in 22.5.1 are
recovered from silently, each in the direction that keeps the surrounding
tokens; they are tabulated in [`limitations.md`](limitations.md) and each one
becomes a diagnostic rather than a change of behaviour once there is a layer to
report to.

The scaffolding note from step 6 is still owed: `crates/svirig-text/tests/origins.rs`
builds its expansion records by hand, and `expand()` can now produce real ones.
Keep the cases, especially the argument one.

## Step 4 — `` `include ``

**Build.** Resolution for `IncludePath::Quoted` (relative to the including file,
then the include path) and `IncludePath::Angle` (the implementation's own
location). `IncludePath::Expanded` needed step 3 first, because its file name
arrives from a macro; that is no longer a blocker.

**The token indices are the work.** Everything in the table and in a
`MacroRef` addresses the one token slice the file was read from — a `MacroDef`
says so, and so does the recursion guard in `expand.rs`, which identifies a
definition by its own name token. A second file makes every one of those want a
`FileId` alongside. Doing that *first* and mechanically is cheaper than
resolving includes and then chasing the aliasing.

The other half is already there: `Origins::add_included` records the file and
the `` `include `` that pulled it in, and `include_trace` walks back up the
chain, which is what the depth limit and the cycle check both read.

**Expanded mode only.** The formatter never follows an include
([D6](plan.md#4-decisions)); each file is formatted alone.

**Traps.** Nesting must reach at least 15 levels (22.4), so a depth limit and a
cycle check are both needed. An `` `include `` may also sit *inside* a macro
body and resolve only where the macro is used — the corpus has exactly one,
`VECTOR_INCLUDE` in `google_riscv-dv`, and it is a good test.

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

**Traps.** An `` `endif `` with no opener, and a region that opens in one file
and closes in an included one — legal, and worth deciding about explicitly.

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

The tests build expansion records by hand, which step 3b can now produce for
real. `crates/svirig-syntax/tests/expand.rs` covers the same ground end to end,
including the argument case, so what is left in `origins.rs`'s tests is the map
tested without a preprocessor — worth keeping as that, since the crate is meant
to stand alone, but it should stop claiming expansion does not exist.

---

## The gate

**Met for expansion, and restricted by what is left.**
`crates/svirig-syntax/tests/differential.rs` runs `slang -E --comments` over
the corpus and compares token sequences; 1502 files are comparable and all
agree. 1880 more use an `` `include `` or a conditional and are skipped, which
is the number steps 4 and 5 turn into coverage. The test asserts on that count
as well as on agreement, because a file quietly *ceasing* to be comparable is
how this rots.

The reference declines another 2244 for want of a definition it has not been
told about. Reaching those means passing it the include paths a filelist or
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
