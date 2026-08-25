# Next steps

The working queue for the rest of M2, written so that picking it up cold costs
an afternoon rather than a week.

This is **not** a decision record. Anything durable belongs in
[`plan.md`](plan.md) or [`preprocessor.md`](preprocessor.md), and anything
knowingly traded away belongs in [`limitations.md`](limitations.md). Delete
this file when M2 closes.

---

## Where things stand

M1 is closed. Three and a half of M2's six rungs are done — the origin map out
of order, for the reason [step 3b](#step-3b--expansion) gives:

| Where | What |
| --- | --- |
| `src/lexer.rs` | Tracks the extent of a `` `define `` so that a `\` ending a `//` comment continues the definition instead of being swallowed by it. Not a lexer mode — one rule differs, so one bool suffices. |
| `src/preproc/directive.rs` | Recognises the 22 directives of 1800-2023 22.1 and parses the operands of `` `define ``, `` `undef ``, the conditionals and `` `include ``. The other six keep theirs as a token range. |
| `src/preproc/macros.rs` | The macro table, and the reference half of step 3: arity from `` `define `` / `` `undef `` / `` `undefineall ``, and argument lists delimited as balanced token soup. **No expansion.** |
| `src/preproc/mod.rs` | `scan()` — one forward pass giving every directive and every macro reference as flat, non-overlapping `Item`s, plus the table they build. |
| `examples/dump-directives.rs` | The debugging aid step 2 left owing. `--table` prints the macro table. |
| `crates/svirig-text/` | Step 6, done. Files, spans, line/column, and the expansion chain. The first crate split. |

`scan()` reports nothing inside a `` `define `` body or inside a macro
argument. Both are *text*, processed where they are used; a nested call is
found by scanning the range that holds it. That boundary is the design, not an
omission: see
[Level B](preprocessor.md#level-b--macro-invocations-are-grammar-atoms).

The one design question that came up and was answered with a measurement:
**where arity is unknown, a `(` anywhere on the same line opens an argument
list.** Arity is unknown for 95% of references and permanently will be, since
raw mode never follows an include, so this fallback is the main path rather
than a corner. Written up in
[Level B](preprocessor.md#level-b--macro-invocations-are-grammar-atoms) and in
[`limitations.md`](limitations.md).

Start the next session by reading `svirig-text`'s crate doc and then
`src/preproc/mod.rs`; between them they are the shape everything below plugs
into.

---

## Step 3b — expansion

The table and the reference half are done; substitution is not. What is left is
everything that turns a `MacroRef` and a `MacroDef` into tokens.

**Read `Arity`, `Entry` and `MacroRef` first.** Argument delimitation, arity,
and the unknown state are all settled, so expansion starts from a call whose
arguments are already split.

**The output representation is decided.** That was step 6, done first for this
reason: expansion *is* the expanded mode, so the token type it emits is the
thing the origin map defines. A token carries an `Origin` — the `Span` its
bytes live at, plus the `Expansion` that placed it, if one did. Text that is in
no file goes in a synthesised buffer through `Origins::add_synthesised`, which
is what ``` `` ``` and `` `" `` need.

The expanded token type itself is **not** written yet, deliberately: it is one
struct, and writing it against a real substitution loop beats guessing at it.
`svirig-text` is what it will be made of.

**Traps.**

- **A `\`-newline in a body expands to a newline**, the backslash dropped —
  except inside a string literal, where both characters go (22.5.1).
- **A `//` comment is not part of the substituted text.** `MacroDef.body`
  already excludes a leading one because operand spans are trimmed, but one in
  the middle of a body is still in the range.
- **Stringification and pasting.** `MACRO_QUOTE`, `MACRO_ESCAPED_QUOTE` and
  `MACRO_PASTE` are already lexed; nothing consumes them yet.
- **Defaults.** `Formal::default` is parsed and unread. An omitted argument
  takes it; an argument that is present but empty does not.
- **`` `A() `` is one empty argument**, because the list is split on commas and
  nothing else. A macro with no formals has to read that as no arguments, which
  needs the arity and so belongs here rather than in the splitter.
- **Recursion detection**, and macros expanding to other macros.
- `` `__FILE__ `` and `` `__LINE__ `` expand here, against `Origins::path` and
  `Origins::line_col`. They occur 5 and 7 times in the corpus. `` `line `` would
  move those numbers and does not yet
  ([`limitations.md`](limitations.md)) — but it occurs zero times, so this is
  not the thing that blocks them.

**Done when** `slang -E` agrees on a slice of the corpus that uses macros
heavily — `axi` is the densest and the smallest, so start there.

## Step 4 — `` `include ``

**Build.** Resolution for `IncludePath::Quoted` (relative to the including file,
then the include path) and `IncludePath::Angle` (the implementation's own
location). `IncludePath::Expanded` has to go through step 3 first, because its
file name arrives from a macro.

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
expanding to a delimiter is one opaque token to a token-level pass. Re-running
the classifier once expansion works is the measurement M2 owes.

**Traps.** An `` `endif `` with no opener, and a region that opens in one file
and closes in an included one — legal, and worth deciding about explicitly.

## Step 6 — the origin map — **done**

`svirig-text` exists: `FileId`, `Span`, `Origins`, `Expansion`, `Origin`. Files,
included files, and synthesised buffers all go in one store; a token's
provenance is a span plus an optional expansion, and expansions chain through a
parent. The design and why it departs from the state of the art are in
[D9](plan.md#per-token-provenance) and
[preprocessor.md](preprocessor.md#two-output-modes).

What it does *not* have, and neither needs yet:

- **Diagnostic rendering.** `trace` and `reported_at` carry what a renderer
  needs; the renderer waits for a diagnostics layer to live in.
- **`` `line ``.** Recorded in [`limitations.md`](limitations.md). Zero
  occurrences in the corpus.
- **UTF-16 columns.** `LineCol` counts characters. An editor will want code
  units; nothing here has an editor.

The tests build expansion records by hand, because there is no expansion to
build them yet. Delete that scaffolding once step 3b can produce the real
thing — but keep the cases, especially the argument one.

---

## The gate

**`slang -E` differential over the corpus**, per
[plan.md M2](plan.md#5-milestones). The binary is already on `PATH` at
`/usr/local/bin/slang`.

Expect the comparison itself to need work: the two tools will disagree on
whitespace and on line-marker output long before they disagree on anything that
matters, so the differential needs a normalisation step and that step needs to
be conservative enough not to hide real differences.

```bash
cargo test && cargo clippy --all-targets && cargo fmt -- --check
```

## Left over from step 2

- The six `Unparsed` directives take the end of the line as their extent even
  though their syntax has a defined end. Recorded in
  [`limitations.md`](limitations.md). Small, and blocks nothing.
- ~~`examples/dump-tokens.rs` has no directive-level counterpart.~~ **Done:**
  `examples/dump-directives.rs`.
