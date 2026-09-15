# Next steps

The working queue for M3, written so that picking it up cold costs an afternoon
rather than a week.

This is **not** a decision record. Anything durable belongs in
[`plan.md`](plan.md) or [`preprocessor.md`](preprocessor.md), and anything
knowingly traded away belongs in [`limitations.md`](limitations.md). Delete
this file when M3 closes.

---

## Where things stand

M2 is closed, and steps 1 to 8 of the list below are done — the skeleton, the
preprocessor's own structure built on it, expressions, and declarations. What
is left is the constructs that *hold* declarations: module, package and class
shells, and the statements inside them.

The gate's metric stands at **98.19%** of 7,009,174 tokens still verbatim,
recorded as `RATCHET` in `tests/verbatim.rs`. Steps 7 and 8 barely moved it,
which is the order working as designed rather than a problem: everything
inside a module is one fallback run until step 9 opens it, and then both
rungs' work is reached at once.

| Where | What |
| --- | --- |
| `src/kind.rs` | One flat `#[repr(u16)]` enum: tokens up to `EOF`, then nodes, then `LAST` bounding them. `from_raw` is the way back from what the tree stores. |
| `src/tree.rs` | `SystemVerilog`, the `rowan` `Language`, and the `SyntaxNode` / `SyntaxToken` / `SyntaxElement` names everything downstream uses. |
| `src/parser/event.rs` | What a rule emits and how it takes it back: `Events`, `Marker`, `Completed`, `Snapshot`, and `resolve` to ordinary nesting. A rollback undoes a `precede`'s forward parent before it truncates. |
| `src/parser/mod.rs` | `Parser`, which joins the events to the tokens and is all a rule ever takes, and `parse`, the entry point. |
| `src/parser/verbatim.rs` | The fallback: a balanced run of what no rule could make sense of, bounded by a `Position` where the caller knows the extent. |
| `src/parser/preprocessor.rs` | The rules for what a `` ` `` introduces: `MACRO_CALL`, `DIRECTIVE`, `CONDITIONAL_REGION`. Reachable from the top level and from inside a run alike. |
| `src/parser/expr.rs` | Precedence climbing over Table 11-2. `expr` answers `None` when nothing there starts one, and otherwise stops at the first token it cannot use. |
| `src/parser/decl.rs` | Data types and declarations, and `type_names`, the forward pass that decides whether `foo bar;` is one. All or nothing: a declaration that misses its `;` rolls back. |
| `src/parser/build.rs` | The events walked against the file's own tokens, with the trivia put back around them. `parse` is the entry point, and gives one `VERBATIM` until there are rules. |
| `src/parser/source.rs` | `Tokens`, the trait the grammar is generic over, with `Raw` and `Expanded` behind it. Trivia is already stepped over; `Position` is the token half of a rollback. `macro_call`, `directive` and `region` report what the preprocessor found, in grammar tokens — and `Expanded` answers `None` to all three. |
| `grammar/` | `annex-a.bnf` to grep, gitignored; `productions.txt`, the 747 names, committed. Both from `scripts/extract-grammar.sh`. |
| `src/lexer.rs` | A gapless token stream: every byte in exactly one token, `EOF` terminated. |
| `src/keyword.rs` | Annex B, as data, selected by `KeywordVersion`. |
| `src/preproc/mod.rs` | `scan()` — every directive and macro reference in one forward pass, flat and non-overlapping, plus the table they build. **Raw mode's entry point, and the parser's.** |
| `src/preproc/conditional.rs` | `region()` / `regions()` — the flat directive list nested into `` `ifdef `` regions with branch children, each classified self-delimiting or ragged. |
| `src/preproc/expand.rs` | Expanded mode: `ExpandedToken`, a kind plus a `TokenOrigin`. |
| `crates/svirig-text/` | Files, spans, line/column, the expansion chain. |
| `tests/differential.rs` | The M2 gate, against `slang -E --comments` over the corpus. |

The two things to read before starting are `src/preproc/mod.rs`'s module doc —
`scan()` is what the raw token source is built out of — and
[Level B](preprocessor.md#level-b--macro-invocations-are-grammar-atoms) and
[Level C](preprocessor.md#level-c--conditionals-as-structured-regions), which
are the two places the parser has to do something no other SystemVerilog parser
does.

## The shape of the work

Ten rungs, in an order chosen so that **the gate's metric exists after rung 4
and only ever falls**. The alternative order — expressions first, because
everything needs them — has no number to show until most of the grammar is in,
which is how a months-long milestone loses its way.

Steps 1–5 are the skeleton: no grammar at all, a file parsing to one `VERBATIM`
node, and a round-trip over the corpus proving the tree is lossless. Steps 6–9
are the grammar, each one converting some verbatim tokens into structure.

---

## Step 1 — the tree, and the kinds it is built from — **done**

**`rowan` plumbing.** `src/tree.rs`: an uninhabited `SystemVerilog` carrying
the `Language` impl, and the three type names that have the parameter already
applied so that no two callers can disagree about it.

`SyntaxKind::from_raw` is the way back from the `u16` a green node stores. It
is a bounds check and a `transmute`, which is **the only `unsafe` in the
crate**: the alternative is a 750-arm match, generated by a macro that would
have to own the enum's declaration -- doc comments, `logos` attributes and all.
The soundness condition is that the discriminants are `0..LAST` with no holes,
and `tests/tree.rs` walks the whole range, so pinning one by hand fails there
rather than in the field.

**Two node kinds, not nine.** `SOURCE_FILE` and `VERBATIM` are what steps 4
and 5 build; `MACRO_CALL`, `CONDITIONAL_REGION` and the rest arrive at step 6
with the rules that construct them, which is
[D11](plan.md#node-kinds-are-not-annex-as-productions) applied to our own
planning rather than to Annex A. Deferring them also defers a name collision
worth deciding deliberately: `DIRECTIVE` is already the *token* for a
`` `name ``, so the node holding one plus its operands needs either a
different name or a rename of the token to what it actually is -- a backtick
and an identifier, which is a directive or a macro call depending on a table.
(Step 6 took the rename: the token is `TICK_IDENT`.)

**The grammar extracts, and the extraction has one real trap.**
`scripts/extract-grammar.sh <pdf>` gives 747 productions over 2243 lines, with
all 56 footnotes kept as prose at the end. Their *markers* are not: a
superscript flattens into a digit glued to the name it marks, and 31 names in
Annex A really do end in a digit. Stripping the digits off a whole identifier
is not enough either -- matching the digit run directly finds `t01` inside
`t01_path_delay_expression` and silently leaves `t_path_delay_expression`,
which the script now takes the trouble to avoid. A production no longer points
at its footnote; the constraints are still there to read.

## Step 2 — events, markers, rollback — **done**

`Events` is the flat list; `Marker` opens a node before its kind is known and
`complete` writes the kind in; `Completed::precede` reopens a finished node
from the outside, which is what a left-associative operator needs. Rollback is
a truncate to a `Snapshot`, and that is the whole reason the events exist
([D2](plan.md#4-decisions)) — a `Checkpoint` wraps retroactively but cannot
undo, and **retrofitting this later is a parser rewrite.**

Four things came out differently from the sketch:

- **`resolve` lives here, not in the builder.** Turning a forward parent into
  ordinary nesting is the only interesting thing about the event list, and
  doing it at this end leaves step 4 with nothing to do but walk a flat
  sequence and put the trivia back. It also makes `precede` testable now
  rather than two rungs from now.
- **A tombstone is its own variant**, not a reserved `SyntaxKind`. An opened
  marker and an abandoned one are the same state — a slot with no kind yet —
  which is why `start` needs no kind and `abandon` mostly needs to do nothing.
- **A lost marker panics**, through a drop bomb, because the failure it
  otherwise causes is a misshapen tree built much later somewhere else.
- **A rollback across an open marker is refused.** `Events` counts open
  markers and a `Snapshot` carries the count, so undoing half a node fails
  where it is written. The discipline that buys is that a speculative rule
  finishes what it starts before it can be undone.

No `Error` event and no token-splitting count: there is no diagnostics layer
to report to, and a number lexed in pieces (`8 'h FF`) wants a node over its
tokens rather than one fused token, so neither has a caller yet.

## Step 3 — the token source — **done**

`Tokens` is the trait the grammar is generic over, and `Raw` and `Expanded`
are both behind it, which is what makes the claim testable rather than merely
asserted: `the_two_streams_read_a_plain_file_identically` runs a file with
nothing for the preprocessor to do through both and compares what a rule would
see. Anything that made the expanded path drop or add a token the grammar can
see fails there.

**Trivia is not in the grammar view**, so no rule has to remember to skip it,
and a `Position` counts grammar tokens rather than bytes or raw indices --
the one coordinate both implementations can offer. Putting the trivia back is
step 4's job, out of the original token list.

**`EOF` is a token, and looking past the end gives it forever.** A rule that
has run off the end then behaves like one that reached it, which is what every
rule wants and none would remember to write.

**A macro call answers with a length**, not with its structure. `scan()`
already split the arguments, but a length is what both streams can express --
expanded mode answers `None`, because an expansion leaves no reference behind
-- and a rule that walks the call splitting on commas at depth zero needs
nothing more. That a rule handling calls correctly in raw mode does *nothing*
in expanded mode, without asking why, is the abstraction working.

**Conditional regions are not in the trait yet**, and that is the one place
this departs from the sketch. The question a rule will ask at step 6 is not
"is a region here" but "where do its branches start and end", and the answer
has to be in grammar positions while `Region` is written in raw token spans.
Guessing that shape now would be guessing; the source grows a method when
step 6 knows what it wants, which is a method rather than a rewrite.

## Step 4 — the builder, and where trivia lands — **done**

The rule is `rdlfmt`'s, and it is the one piece of it that carries over
unchanged: leading trivia belongs to the item that follows, a comment on the
same line as the token before it stays with that token, and whitespace alone
never attaches backwards — it carries no signal, and the formatter asks for
the separation it wants rather than reading it here.

It is applied at **both** ends of a node, which the sketch did not say. A node
about to close takes its own trailing comment with it, or `endmodule // top`
would leave the comment outside the module; a node about to open takes none of
what belongs to the token before it. Same split, opposite directions.

Two things the tests found. Nothing may be written before the root opens, or a
file beginning with a comment puts it outside the tree and `rowan` refuses the
second root; and end-of-file trivia lands in `SOURCE_FILE` rather than in the
last node, which is where a trailing newline belongs anyway.

**The gate's first test is in place.** `corpus_round_trips_through_the_tree`
parses all 5626 files and compares the tree's text with the file: 53 MB, byte
for byte, in 1.9 s in release. There is no grammar, so what it proves is the
trip through events and back — and it is the invariant no later rung may
break.

`examples/dump-cst.rs` prints a tree, and `--stats` answers the `rowan`
question [plan.md](plan.md#8-open-questions) parked here. The short version:
the largest corpus file, 298k tokens, builds in 15.5 ms at 29.6 MB resident,
and what that measures is the token layer rather than the node layer that does
not exist yet.

## Step 5 — `VERBATIM`, and resync — **done**

The [highest-leverage decision in the plan](plan.md#the-verbatim-fallback),
and it was cheap to build before there was anything to fall back *from*.

**Two contexts, not four.** An item, a member and a statement all end at their
own `;` and at anything that closes what encloses them, and the delimiter
stack already knows the second half of that — so they are one context, and a
list element, which ends at its `,`, is the other. A finer one is worth adding
when a rule needs it.

**The stack holds what opened, not a count**, and that is the whole safety
net: a closer that does not match the top ends the run instead of being
swallowed, so a run cannot escape past the `endmodule` of the module it
started in. It is also what bounds the cost of the guesses below.

**Five keywords only sometimes open a body** — `function`, `class`,
`interface`, `property`, `sequence` — and getting one wrong is what makes a run
escape. `extern function f();`, `typedef class C;`, `virtual interface i vif;`
and `assert property (…)` all have no `end…` to find. Each is decided by a
test on the tokens around it, and all of it is written up in
[`limitations.md`](limitations.md), because none of these tests is right in
general: deciding properly means knowing whether a declaration has a body,
which is the thing the parser could not work out to begin with.

A run also ends when a **keyword** closer empties the stack, which a bracket
does not: `(a + b) + c` carries on, but the token after `endmodule` belongs to
the next run. Without that, two modules in one file are one run.

**The metric is in place and it is the number M3 is graded on.**
`corpus_verbatim_rate_does_not_rise` walks every corpus file and counts
grammar tokens inside a `VERBATIM` against all of them: **100.0% of 7,009,174
tokens**, which is right, because there is no grammar. `RATCHET` in that file
is the recorded number, the assertion only allows it to fall, and

```bash
cargo nextest run --release -E 'test(corpus_verbatim_rate)' --no-capture
```

prints the per-repo table. The report lives in the test rather than in an
example because it is the same walk as the assertion; duplicating it to have a
separate reporter would be exactly the kind of thing D11 argues against.

## Step 6 — macro calls, directives, and regions as structure — **done**

[Level B](preprocessor.md#level-b--macro-invocations-are-grammar-atoms) and the
structural half of
[Level C](preprocessor.md#level-c--conditionals-as-structured-regions). This is
what most SystemVerilog tooling gets wrong, and the corpus makes it unavoidable
rather than optional: a `` ` `` token is a macro reference four times out of
five.

`MACRO_CALL` over the introducer and an optional `MACRO_ARG_LIST` of
`MACRO_ARG`s; `DIRECTIVE` over a directive and its operands, with a
`` `define ``'s body in a `MACRO_BODY` of its own; `CONDITIONAL_REGION` with a
`CONDITIONAL_BRANCH` per branch and every branch present. The rules are in
`src/parser/preprocessor.rs`, and the shapes they read — extents in grammar
tokens, worked out once when the stream is built — are in
`src/parser/source.rs`.

**The ratchet fell from 100.0% to 98.30%**, 118,817 tokens of 7,009,174. That
is the file-level preprocessor and nothing else: `` `define ``, `` `include ``
and the conditional scaffolding are structure now rather than fallback, and
everything inside a module is still a run.

Five things came out differently from the sketch.

- **The token had to be renamed, and it was the right rename.** `DIRECTIVE` was
  the *token* for a `` `name ``, which the node needed. The token is now
  `TICK_IDENT` — named for how it is written, like the operators and for the
  same reason: the lexer cannot know whether it introduces a directive or a
  macro call, because that is a table lookup. The old name was an apology, and
  four times out of five it was simply wrong.
- **The rules run inside a `VERBATIM` run, not only at the top level.** A macro
  call is written in statement and member position, which at this rung is the
  middle of a fallback run — so the run defers to these rules instead of
  swallowing their tokens, and the preprocessor's structure lands everywhere
  rather than only where the grammar already reaches. It costs the metric
  nothing either way, because a node inside a run is still inside a run.
- **Reading a region as one atom is what protects the delimiter stack.** The
  run never looks inside, so a ragged region hands it nothing unbalanced, and
  what a ragged region costs is the one construct enclosing it rather than the
  rest of the file. That was not the reason for doing it and it is the better
  half of the result.
- **A branch body needs a bound, and `verbatim` grew one.** Without it the
  first branch of a ragged region goes looking for the `end` that the *second*
  branch writes, and swallows the rest of the region. A `Position` is the
  bound, which is what step 3 built it for.
- **The self-delimiting classification was deferred to step 9.** The sketch had
  the region node carry it. It is not in the library — it lives in
  `examples/conditionals.rs`, measured rather than built — and lifting it here
  would add a computation with no reader, which is
  [D11](plan.md#node-kinds-are-not-annex-as-productions)'s argument applied to
  a field rather than to a variant. Step 9 is the consumer, and what it will
  want is the answer in grammar positions, which is a shape that can only be
  designed once something asks.

`RegionShape::closed` went the same way and for the same reason: the tree shows
the `` `endif `` or it does not, so nothing needed telling.

What this step does *not* do is parse inside a branch: every region is verbatim
for now. Parsing the self-delimiting ones is step 9, once there is a grammar
worth running on them — but the region is in the tree from here, because
everything built later is built on this shape.

## Step 7 — expressions — **done**

Precedence climbing over Table 11-2 in `src/parser/expr.rs`. Primaries, unary
and binary operators, `?:`, concatenation and replication, streaming, ranges,
calls and method calls, hierarchical and class-scoped references, system
tasks, assignment patterns, `inside` and `dist`.

**The metric did not move, and could not.** Nothing calls `expr` yet: the
things that hold expressions are declarations and statements, which are steps
8 and 9. The ratchet stays at 98.30%. This is the one rung in the order that
buys no number, which is worth saying out loud rather than discovering later
and suspecting a bug.

**So it was measured against its own oracle instead.** Every `assign` right-
hand side in the corpus -- 47,317 of them -- fed to `expr` directly: **47,247
parse whole**, and all 70 that do not are accounted for. 61 are `` `` ``
token-paste fragments out of macro bodies, which are text and never reach an
expression rule; 2 are macro references whose arity is unknown, which is the
[recorded limitation](limitations.md); and 7 are the probe's own fault, from
`assign a = b, c = d;` splitting wrong. Nothing is left. The probe was
throwaway -- `tests/expr.rs` is what stays -- but it is how the last three
bugs were found, and it is the argument for an oracle over a test suite all
over again.

Four things worth recording.

- **Rollback across `precede` was broken, and step 8 would have hit it.** A
  reopened node points *forward* at the marker that reopened it, which is the
  one pointer in a flat event list that a truncate can leave dangling --
  `resolve` then followed it into whatever landed at that index next. `Events`
  now remembers which events a `precede` wrote into, and a rollback undoes
  those before it truncates. This is what
  [D2](plan.md#4-decisions)'s speculative parse rests on, so finding it here
  rather than in the middle of the type-versus-expression ambiguity was luck
  worth banking.
- **`None` has to mean nothing was taken.** A prefix operator with no operand
  used to leave the operator emitted, so a caller's fallback would start after
  the tokens it was meant to fall back on. Both failure paths now roll back.
- **A separated literal's digits may be several tokens.** `'h 4a43_f880` lexes
  as a base, an integer and an identifier, because the run starts as a number
  and stops being one. What joins them is that nothing separates them, so
  `Tokens` grew `adjacent` -- the one question a rule may ask about the trivia
  it otherwise cannot see, and it is asked for the one reason that survives.
- **Assignment is deliberately not a binary operator.** Putting `=` at the
  bottom of the table would have the expression rule swallow the right-hand
  side of every assignment *statement*, which is precisely what step 9 needs
  to see for itself. Written up in [`limitations.md`](limitations.md); the
  corpus has zero parenthesised assignments.

`(* … *)` is told from a parenthesised expression by lookahead, as the sketch
said it would have to be. The one wrinkle the sketch did not have: a value
inside an attribute would read `1 *)` as a multiplication, so the binary loop
stops at a `*` that is followed by `)` -- which is no multiplication anywhere.

## Step 8 — types, declarations, and the ambiguity — **done**

Data types, packed and unpacked dimensions, `typedef`, `enum`, `struct`,
`union`, nets, variables and parameters, in `src/parser/decl.rs`. And with
them the load-bearing problem: `foo bar;` is a declaration only if `foo` names
a type.

**The ratchet fell from 98.30% to 98.19%.** A small move, and the expected
one: a declaration is reached at the top level and inside a conditional
branch, and everything inside a module is still one fallback run. Step 9 opens
those, and it is where this rung's real return arrives.

**Measured against its own oracle in the meantime.** Every declaration-shaped
span in the corpus -- from a declaration keyword at the start of a line to the
`;` that ends it, 114,568 of them -- fed to the rule: **114,342 parse whole**.
Of the 226 that do not, none is an unresolvable type name. 108 are macro-body
text, 84 are the ragged conditional regions `ibex` is known for, 20 are casts
the rule correctly declines, 8 are DPI function prototypes that belong to step
9, and 6 are the probe's own fault.

**The ambiguity turned out to be three questions about shape and one about
meaning**, and only the last one needs the type-name set.

- A **qualifier** carries the declaration: after `const`, `var` or a net type,
  what follows is a type whether or not the name can be resolved.
- A **scope** settles it: nothing but a declaration is written `pkg::t x;`.
- **Brackets** are read by what comes after them. `cfg_t [N-1:0] Configs;` and
  `regs [4];` are the same three shapes of token, and the difference is
  whether a name follows the dimensions. This one was not in the sketch and is
  the most useful of the three -- it works for types this file has never heard
  of, which is exactly where the set has nothing to say.

What is left is the bare `unknown_t x;`, which is genuinely undecidable:
`my_module inst ();` has the same shape until its port list. Written up in
[`limitations.md`](limitations.md), as the sketch said it must be.

Four more things worth recording.

- **The set is built by a forward pass, not as the parse goes.** Raw mode
  keeps every branch of every conditional, so a `typedef` inside an
  `` `ifdef `` names a type whichever branch a build takes. A running set
  would answer differently depending on where in the file it was asked.
- **A declaration is all or nothing.** One that does not reach its own `;` was
  read wrongly, and it rolls back rather than leaving a node over a prefix --
  which would put the fallback in the middle of what it misread. This is the
  first real user of the rollback that step 7 fixed.
- **A macro may be the name being declared.** `logic [31:0] `X(mcause);` is one
  declaration whose declarator is written entirely by an expansion, and a
  macro in an enum body brings its own commas with it. Level B keeps being
  right about positions nobody thinks of.
- **The literal rule grew again.** `32'h`DM_ADDR` takes its digits from a
  macro, and `13'h 1e0` lexes them as a *real*, because those digits are also
  how scientific notation is written. Neither is a number to the lexer; what
  makes them one is the base in front.

## Step 9 — module shells, items, statements, and live branches

The RTL subset M3 is named for, in the order that shrinks the metric fastest:

1. `module` / `endmodule` shells, ANSI and non-ANSI headers, parameter port
   lists, port declarations.
2. Module items: continuous assign, instantiation with named and positional
   connections, `always*` / `initial` / `final`.
3. Statements: `begin`/`end`, `if`, `case`, loops, blocking and nonblocking
   assignment, event control.
4. `generate` / `for` / `if` / `case`, which the corpus uses heavily.
5. `package`, `interface`, `modport`, then `class` members, functions and
   tasks.

Then the second half of Level C: a region whose branches are all
self-delimiting has each branch parsed in the enclosing context; a ragged one
stays verbatim. 96.4% of regions classify self-delimiting, so this is where a
large block of verbatim tokens turns into structure in one move — and it is the
thing `verible-verilog-format` handles worst, which is the point.

Concurrent assertions and `specify` sections stay verbatim on purpose. They are
large, they are rare in this corpus, and they are the fallback earning its
keep.

## Step 10 — measure, and close

Fill in [`grammar-coverage.md`](grammar-coverage.md)'s metrics table per repo
and per commit, quoting the corpus commits beside the numbers as
[plan.md §6](plan.md#6-corpus-and-testing) requires. Add the fuzzer: random
token sequences must never panic and must always round-trip.

---

## The gate

**Parses the corpus with a measured, decreasing verbatim-fallback rate.**
Concretely, three `corpus_*` tests:

1. **Round-trip.** The tree's text equals the input, byte for byte, for every
   file. Available from step 4 and never allowed to regress.
2. **No panics, no `ERROR` nodes reaching the top.** A file that cannot be
   parsed falls back; it does not fail.
3. **The ratchet.** Verbatim rate per repo, asserted against a recorded
   number. Down is a new number to record; up is a bug.

```bash
cargo nextest run && cargo clippy --all-targets && cargo fmt -- --check
```

`cargo nextest run -P quick` stays the tight loop; the corpus tests are the
slow ones and are named so they can be left out.

## Housekeeping, while M3 is open

Done. [`grammar-coverage.md`](grammar-coverage.md)'s preprocessor section now
describes M2 as closed and separates the preprocessor's own coverage from the
parser's view of it; its header no longer promises to be generated, which
[D11](plan.md#node-kinds-are-not-annex-as-productions) ruled out; and
[`plan.md`](plan.md)'s status line says the parser is what is under way.
`src/lib.rs` already said so.

Anything found stale from here goes in this section rather than being fixed
silently, because a doc that drifts is what this file exists to prevent.
