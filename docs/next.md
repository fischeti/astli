# Next steps

The working queue for the rest of M2, written so that picking it up cold costs
an afternoon rather than a week.

This is **not** a decision record. Anything durable belongs in
[`plan.md`](plan.md) or [`preprocessor.md`](preprocessor.md), and anything
knowingly traded away belongs in [`limitations.md`](limitations.md). Delete
this file when M2 closes.

---

## Where things stand

M1 is closed. Two of M2's six rungs are done:

| Where | What |
| --- | --- |
| `src/lexer.rs` | Tracks the extent of a `` `define `` so that a `\` ending a `//` comment continues the definition instead of being swallowed by it. Not a lexer mode — one rule differs, so one bool suffices. |
| `src/preproc/directive.rs` | Recognises the 22 directives of 1800-2023 22.1 and parses the operands of `` `define ``, `` `undef ``, the conditionals and `` `include ``. The other six keep theirs as a token range. |

`scan()` deliberately leaves macro *references* alone — around four in five
`` ` `` tokens in real code — because their arguments cannot be delimited
before step 3 exists. That boundary is the design, not an omission: see
[Level B](preprocessor.md#level-b--macro-invocations-are-grammar-atoms).

Start the next session by reading `src/preproc/directive.rs`; it is short, and
it is the shape everything below plugs into.

---

## Step 3 — the macro table and expansion

The big one, and the only step with real subtlety.

**Build.** A table from name to definition, fed by `Operands::Define` and
emptied by `` `undef `` / `` `undefineall ``. Then, beside `directive.rs`, the
*reference* half: given a `DIRECTIVE` token that is not a directive, consult the
table to decide whether a following `(` opens an argument list, delimit the
arguments, substitute, and re-scan the result.

**Both modes need the table.** Raw mode never expands, but it cannot shape a
`MACRO_CALL` node without knowing the arity, so it builds one too.

**Traps.**

- **Arguments are balanced token soup**, matched on `()`, `[]`, `{}` and
  nothing more. A macro argument is text; parsing it as an expression is wrong.
- **Arity is not always knowable.** A macro from an unfollowed header has no
  entry, and with every `` `ifdef `` branch present at once two branches may
  define one name with different formals. The table needs an *unknown* state,
  and the fallback is to treat an immediately adjacent `(` as an argument list.
- **A `\`-newline in a body expands to a newline**, the backslash dropped —
  except inside a string literal, where both characters go (22.5.1).
- **A `//` comment is not part of the substituted text.** `MacroDef.body`
  already excludes a leading one because operand spans are trimmed, but one in
  the middle of a body is still in the range.
- **Stringification and pasting.** `MACRO_QUOTE`, `MACRO_ESCAPED_QUOTE` and
  `MACRO_PASTE` are already lexed; nothing consumes them yet.
- **Recursion detection**, and macros expanding to other macros.
- `` `__FILE__ `` and `` `__LINE__ `` expand here, and interact with `` `line ``.

**Done when** `slang -E` agrees on a slice of the corpus that uses macros
heavily — `axi` is the densest and the smallest, so start there.

## Step 4 — `` `include ``

**Build.** Resolution for `Include::Quoted` (relative to the including file,
then the include path) and `Include::Angle` (the implementation's own
location). `Include::Expanded` has to go through step 3 first, because its file
name arrives from a macro.

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

## Step 6 — the origin map

`svirig-text`, and the first crate split. Definition location versus expansion
location, so a diagnostic can say "this token came from `` `FOO `` expanded at
line 40, defined at line 12".

**This is the one hard ordering constraint in M2:** it must exist *before the
expanded mode has any users*, because retrofitting it means touching everything
that already consumes tokens. Give it a name that is not `SourceManager`.

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

Small, and neither blocks anything:

- The six `Unparsed` directives take the end of the line as their extent even
  though their syntax has a defined end. Recorded in
  [`limitations.md`](limitations.md).
- `examples/dump-tokens.rs` has no directive-level counterpart. A
  `dump-directives` example would make the next three steps much easier to
  debug by eye.
