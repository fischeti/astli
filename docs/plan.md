# Project plan

> **Status:** exploratory, with the lexer done and the preprocessor under way.
> Nothing here is a commitment; it is a record of what was decided and *why*,
> so that picking the project up after a three-month gap costs an afternoon
> instead of a week.

The project is `svirig`; see [Naming](#2-naming). Every crate carries that
prefix.

---

## 1. What this is

A suite of SystemVerilog language tooling in Rust, built around one lossless
syntax tree. The first product is a formatter, because that is the smallest
thing that is both genuinely useful and forces the hard parts (a real lexer, a
real preprocessor story, a real grammar) to be solved properly.

The motivation is that SystemVerilog developer experience is poor and the
existing options are unsatisfying: `verible-verilog-format` produces output I
don't like, `slang` is excellent but C++ and not aimed at formatting, and the
Rust attempts (`sv-parser`, `moore`, `svls`) are either slow, abandoned, or
not lossless.

### Objectives, in priority order

1. **A SystemVerilog formatter I actually want to use.** Correct on real
   `` `ifdef ``-heavy, UVM-macro-heavy code, not just on textbook RTL.
2. **A reusable lossless syntax layer** that other tools can be built on —
   linter, LSP, refactoring, doc extraction.
3. **A standalone, correct, fast preprocessor crate.** Independently valuable
   and independently shippable; the Rust ecosystem has nothing good here.
4. *(Speculative, years out)* Semantic analysis — name resolution,
   elaboration, type checking.

### Non-goals

- Simulation, synthesis, or netlist anything.
- Beating `slang` at compilation. `slang` is 200k+ lines and excellent; the
  point is not to replace it.
- Full IEEE 1800-2023 grammar coverage as a precondition for shipping
  anything. See the [verbatim fallback](#the-verbatim-fallback) decision.
- Supporting Verilog-95/2001-only flows as a distinct mode. SystemVerilog
  subsumes them; legacy quirks get handled or they don't.

### Relationship to `slang`

`slang` is the reference implementation and the right thing to consult when a
design question has a non-obvious answer. It is **not** to be transliterated.
In particular: do not copy its type names (`SourceManager`, `Trivia`,
`ParserBase`, …). Pick names that fit Rust and fit this codebase. Understanding
*why* it made a choice is the useful part; the naming is not.

Worth knowing: `slang` is `clang` with an S, and its `SourceManager` /
`Diagnostics` naming comes from there — but its *tree* design (green/red nodes,
trivia attached to tokens) is Roslyn's. `rowan` is that same Roslyn design
ported to Rust via rust-analyzer. So the core data model is already shared;
that convergence is inheritance from a common ancestor, not imitation.

---

## 2. Naming

**`svirig`** — `sv` plus Swiss German *schwirig*, "difficult". Free on
crates.io and on GitHub as of 2026-08-17.

- **Every crate is prefixed `svirig-`** — `svirig-lexer`, `svirig-syntax`,
  `svirig-preproc`, `svirig-fmt`, `svirig-text`. Unprefixed `sv-*` names were
  considered and rejected: the prefix makes provenance obvious and puts the
  whole namespace out of reach of a future collision. (`sv-parser` is already
  taken by dalance, which is the argument in miniature.)
- The driver binary is `svirig`.
- `svfmt` is **taken on crates.io** by an existing SystemVerilog formatter.
  Don't plan on it as a package name.

Also considered and free, should this ever need reopening: `sven`, `cant`
(thieves' *cant*, the direct riff on `slang`), `svengali`, `verilust`,
`metastable`. Most of the Rust/iron-chemistry space (`ferrite`, `patina`,
`oxide`, `ferrous`, `hematite`, `magnetite`, `anneal`, `slag`) is taken.

---

## 3. Architecture

### Data flow

```
                                          ┌──────────────────────┐
   source bytes                           │  raw mode            │  ← formatter, LSP
        │                                 │  directives kept as  │
        ▼                                 │  structure, no       │
   ┌──────────────┐  ┌────────────────┐   │  expansion           │
   │ svirig-lexer │─▶│ svirig-preproc │──▶┤                      │
   └──────────────┘  └────────────────┘   │  expanded mode       │  ← compiler
        │                   │             │  macros expanded,    │
   logos, modes,       macro table,       │  includes followed,  │
   no preprocessing    conditionals       │  + source map        │
                                          └──────────┬───────────┘
                                                     │  token stream
                                                     ▼
                                            ┌─────────────────┐
                                            │  svirig-syntax  │  event-based parser
                                            │                 │  → rowan green tree
                                            └────────┬────────┘
                                                     │  CST
                                  ┌──────────────────┼──────────────────┐
                                  ▼                  ▼                  ▼
                             svirig-fmt         svirig-lint         svirig-hir
```

The parser is **parameterised over its token source** and does not know which
mode it is in. That is what lets one grammar serve both the formatter and any
future compiler.

### Crates

| Crate | Contents | When |
| --- | --- | --- |
| `svirig-text` | File ids, spans, the origin map for expanded tokens, diagnostic rendering | M2 — *exists*, built before expansion rather than after |
| `svirig-lexer` | `logos` lexer with modes; tokens including directive tokens; no preprocessing | M1 |
| `svirig-preproc` | Macro table, expansion, `` `include `` resolution, conditional evaluation. Two output modes. | M2 |
| `svirig-syntax` | `SyntaxKind`, `rowan` `Language` impl, event-based parser, generated AST accessors | M3 |
| `svirig-fmt` | Formatting IR, layout rules, alignment pass | M4 |
| `svirig-hir` | Name resolution, elaboration, types | someday / never |
| `svirig` | Driver binary: CLI, file discovery, filelist/`bender` integration | M4 |

**Do not create all of these up front.** The workspace holds `svirig-syntax`,
which accumulates the lexer, then the preprocessor, then the parser, and
`svirig-text`, which had to come early for the reason M2 gives; split further
outward when a boundary starts hurting. Until there is a
formatter there is nothing for a binary to drive, so the `dump-tokens` and
`dump-directives` examples in `svirig-syntax` — mirroring `rdlfmt`'s `dump-cst`
— cover M1 and M2. The
module layout inside `svirig-syntax` should be drawn as if the splits already
existed.

The one split worth having on day one is **`svirig-fmt` out of `svirig-syntax`**,
because everything downstream shares the syntax crate and formatting concerns
must not leak into the tree shape.

### Borrowed from `rdlfmt`

`rdlfmt` is one worked example, not a standard — it was a first attempt at this
kind of thing, and SystemRDL is a far smaller language. Where it disagrees with
what SystemVerilog needs, SystemVerilog wins. These four ideas look like they
survive the size difference and are worth starting from:

- **Trivia is placed, never skipped.** Leading trivia belongs to the item that
  follows; a same-line trailing comment stays with the token it annotates.
- **Separation is requested, not written.** No rule writes a space or newline;
  it requests a minimum separation, resolved lazily when the next thing is
  written. Gives no-trailing-whitespace and call-site-free indentation for
  free.
- **Whitespace is discarded, its signal is not.** Blank lines are lifted out as
  a gap *width*, not as a separation in their own right.
- **Verify by re-lexing.** The output's token stream must equal the input's.

### Where it does not carry over

- **Event-based parser, not direct `GreenNodeBuilder` calls.** `rdlfmt` drives
  the builder inline with `Checkpoint`. That works because SystemRDL is nearly
  LL(1). SystemVerilog is not — see [decision D2](#4-decisions) — and checkpoints
  allow retroactive *wrapping* but not *undo*. Emit a flat `Vec<Event>` and
  build the green tree at the end; snapshot is `(events.len(), token_pos)` and
  rollback is a truncate. **Retrofitting this later is a parser rewrite.**
- **Directives cannot all be opaque single-line trivia.** See
  [`preprocessor.md`](preprocessor.md).
- **Alignment is a first-class requirement**, and the `Gap` model cannot
  express it. Needs a post-pass. See [decision D4](#4-decisions).

---

## 4. Decisions

| # | Decision | Rationale |
| --- | --- | --- |
| D1 | `logos` for lexing, `rowan` for the tree | Proven in `rdlfmt`. `rowan` is the Roslyn model, which is the right model for lossless trees. Lexer **modes** (`Lexer::morph`) were expected for macro body text, UDP `table`/`endtable`, directive arguments and `` `pragma protect `` envelopes. Bodies then turned out to need one differing rule and their extent, not a second token enum — so budget for modes, but make each one earn itself. See [`limitations.md`](limitations.md). |
| D2 | Hand-written recursive descent, **event-based**, with speculative parse + rollback | SV is not LL(k). The killer is type/expression ambiguity: `foo bar;` is a declaration only if `foo` names a type; `(a)(b)` is a cast or a call. `slang` resolves this with lookahead heuristics and rollback. No generated-parser framework handles this cleanly. |
| D3 | **Verbatim fallback node from day one** | See below. Highest-leverage single decision in this document. |
| D4 | Column alignment is a separate post-pass over emitted lines | SV culture expects aligned `.port_i (sig)` connections and `assign` RHS (lowRISC, PULP styles mandate it). This fits neither a Wadler IR nor the `Gap` model. Align within maximal runs of same-shaped siblings, broken by blank lines and comments. |
| D5 | Formatter is **preprocessor-transparent**, enforced as an assertion | See [`preprocessor.md`](preprocessor.md#the-transparency-invariant). |
| D6 | Formatter never follows `` `include `` | Each file is formatted independently. A compiler must follow includes; a formatter must not. |
| D7 | Few knobs: indent width, line width, alignment on/off | Resist a style-option matrix. `gofmt`-style opinionation is cheaper to maintain and the thing people actually want. Default line width 100. |
| D8 | Dual MIT / Apache-2.0, matching `rdlfmt` | Rust ecosystem norm. |
| D9 | **Provenance is recorded per token, not per byte** | See below. |
| D10 | **The line table is built eagerly**, when a buffer is added | One cache-hot, vectorisable pass and 4 bytes per line, against a lexing pass that costs far more. Lazy would want a `OnceLock`, and the query pattern that settles the design — a diagnostics layer, or an editor — does not exist yet. |

### Per-token provenance

The state of the art tracks source locations as a byte offset into a flat
space, where each file — and each macro expansion — owns a contiguous chunk of
it. A location is one integer and the manager decodes it. `slang` does this,
inheriting it from `clang`; `rustc` does the same.

It is the right model for a preprocessor that re-emits *text*, and it has one
awkward consequence. Expanding `` `define M(x) f(x) `` at `` `M(a+b) ``
produces tokens with two different homes: `f` is written in the body, `a` is
written in the argument at the call site. A byte-oriented map cannot say that
directly, because the expansion is one chunk with one spelling. `slang`
resolves it by *swapping the roles* of "written here" and "expanded there" for
argument tokens, and carries a flag saying which way round a given chunk is.

We emit tokens, not text, so we can record the spelling on each token and skip
the whole problem: the two tokens simply carry different spans and the same
expansion. Cost is a few bytes per token on the expanded path only; the raw
path keeps the plain `Token` it already has.

**This is only available because the expanded output is a token stream.** If
that ever changes — a `-E` mode that prints preprocessed text is the obvious
candidate — that mode gets its own path rather than dragging the map back to
bytes.

### The verbatim fallback

Annex A of IEEE 1800-2023 is roughly 600 productions. Full coverage is a
multi-year slog. **If an unrecognised construct degrades to "leave this
region's bytes alone" instead of failing the file, a genuinely useful formatter
can ship at 40% grammar coverage.** Every existing tool fails hard on
constructs it doesn't know; that is the gap worth exploiting.

Concretely: a `VERBATIM` node kind that holds a balanced, byte-exact token
span, produced whenever the parser cannot make progress at an item, member, or
statement boundary. The formatter emits it untouched and resyncs after it.

---

## 5. Milestones

Each rung is independently completable and independently valuable. Resist
starting the next before the current one is *done*, because the temptation to
skip ahead to the formatter is what kills projects like this.

**M0 — Scaffolding.** *Done.* Workspace, git hooks (`cargo fmt`, `actionlint`
and `typos` at commit; `clippy` and `cargo doc` at push),
`scripts/fetch-corpus.sh`, these documents.

The split is about what each check needs. Formatting and spelling hold for a
commit that does not compile, and a checkpoint mid-thought is a reasonable
thing to record; `clippy` and `cargo doc` demand a workspace that builds, so
they run before the work leaves the machine rather than before it is written
down. Both cost about half a second on this codebase, which is why neither is
worth deferring further.

There is **deliberately no CI**, because Actions minutes are billed on private
repositories and free on public ones. What it would add over the hooks is
`test`, which wants the corpus and is far too slow to hang off a push; add it
on the day this goes public, or sooner if something ever regresses past the
hooks.

**M1 — Lexer complete.** *Done.* Token inventory, keyword table, and a gapless
token stream; round-trip holds over the whole corpus with zero unlexable spans.

The gate was originally "byte-exact round-trip over the corpus", and that
turned out to be **too weak to be worth much**: it was met by the raw `logos`
rules before a `Lexer` type existed, because it proves only that every byte
lands in exactly one token, never that the token was labelled correctly. A
miss-kinded token passes it.

The gate is therefore round-trip *plus* the kind audits in
`crates/svirig-syntax/tests/lexer.rs` — realistic snippets with their full kind
sequences written out, covering the places a wrong label is plausible. That is
weaker than a differential test against another implementation's lexer, which
was considered and deferred; see [`limitations.md`](limitations.md). The
parser is the real oracle and it arrives at M3.

Lexer modes were the one piece left, and they are deferred rather than
outstanding. `` `define `` bodies are handled, as the first piece of M2. UDP
tables and `` `pragma protect `` envelopes occur zero and one times in the
corpus and are not. See [`limitations.md`](limitations.md).

**M2 — Preprocessor complete.** *Done.* Expansion, `` `include ``,
conditionals, both output modes, origin map. Publishable as `svirig-preproc`
on its own.

A workable order, cheapest and most-constrained first: `` `define `` body
lexing (the one lexer mode that is not speculative) → directive parsing with
`\` continuations → the macro table and expansion → `` `include `` resolution
→ conditional evaluation → the origin map. Only the last has a hard
constraint: **the origin map has to exist before the expanded mode has any
users**, because every diagnostic on the compiler path needs it and
retrofitting it means touching everything that already consumes tokens.

That order held except for the origin map, which moved to the front —
deliberately, because its constraint reaches back into expansion itself:
expansion is what *produces* the expanded stream, so it had to decide first
what an expanded token is.

**The gate is met.** `slang -E --comments` over the whole corpus, token
sequences compared: 1843 files agree outright and 153 agree except where the
reference is wrong, both of those defects confirmed against a third
preprocessor. Nothing is filtered out; the 3630 the reference declines are the
ones it wants an include path or a `+define+` for, and reaching those means
handing *both* sides what a build passes — which is the driver's job, at M4.

Widening the comparison is what found the last two real bugs, both in code that
had passed every targeted test. That is the argument for an oracle over a test
suite, and for [running it against everything](preprocessor.md#the-oracle)
rather than against what is convenient.

**M3 — Parser skeleton + RTL subset.** Event infrastructure, rollback, the
`VERBATIM` fallback, `SyntaxKind` generated from a transcribed Annex A. Cover
module/interface/package/class declarations, `always` blocks, expressions.
**Gate: parses the corpus with a measured, decreasing verbatim-fallback rate.**
*Months.*

**M4 — Formatter v0.** Declarations, port lists, `always` blocks, expressions.
**Gate: idempotency + preprocessor-transparency assertions hold over the whole
corpus; `slang --parse-only` succeeds identically before and after.** First
release. *Months.*

**M5 — Alignment pass, config, real-world polish.**

**M6 — Open question:** LSP, linter, or semantic analysis. Decide with data
about what is actually missing, not now.

---

## 6. Corpus and testing

Fetched by `scripts/fetch-corpus.sh` into a gitignored `corpus/`, never
vendored. Start small — three repos is enough to keep the lexer honest, and a
corpus that takes ten minutes to fetch is one that stops being run:

| Repo | Why |
| --- | --- |
| `pulp-platform/common_cells` | Small, the house style this tool has to be good at |
| `openhwgroup/cva6` | Real, substantial RTL |
| `lowRISC/ibex` | Real, well-written, different house style |
| `lowRISC/opentitan` | Added to answer the Level C question: large, and heavy on `` `ifdef `` across FPGA/ASIC/DV variants |
| `pulp-platform/axi` | The macro-dense end of the house style — a dozen invocations per file, and conditionals gating four simulators |
| `pulp-platform/cheshire` | SoC integration level, where FPGA-versus-ASIC conditionals live |
| `pulp-platform/FlooNoC` | Modern and generate-heavy |
| `pulp-platform/iDMA` | Template-generated RTL |
| `pulp-platform/snitch_cluster` | Large, and a different subsystem style |

Add repos when there is a question they would answer, not before:
`accellera/uvm-core` once macro invocations are handled (**the** macro torture
test — if UVM parses, the model works), `black-parrot` and `chipsalliance/*`
for breadth after that.

Note that the repos overlap: `cva6` vendors `common_cells`, and both `cva6`
and `ibex` vendor `lowrisc_ip` and `google_riscv-dv`. Any number pooled across
the corpus has to deduplicate by file content or it double-counts — 1149 of
5305 files are copies.

`scripts/fetch-corpus.sh` writes `corpus/MANIFEST` with the resolved commit of
each repo. `corpus/` is gitignored and the file is overwritten by the next
fetch, so **a number quoted in these documents must write its own commits out
beside it**; pointing at `MANIFEST` traces to whatever happens to be on disk.

Three oracles, all cheap, and between them they catch nearly everything:

1. **Round-trip / token-stream equality** — output re-lexes to the input's
   token stream modulo whitespace.
2. **Idempotency** — `format(format(x)) == format(x)`.
3. **`slang` differential** — `slang --parse-only` succeeds on input and
   output identically; `slang -E` agrees with `svirig-preproc`.

Plus snapshot tests for formatting decisions, and a fuzzer once M3 lands.

**A test that reads the corpus is named `corpus_*`.** Four of them exist and
they are the whole cost of the suite: 43s of a 43.2s run, against 0.15s for
the other 120. The name is what lets them be left out --
`cargo nextest run -P quick`, or `cargo test -- --skip corpus_` without
nextest -- so the tight loop stays instant while a plain `cargo nextest run`
still runs everything. Excluding them by *default* was considered and
rejected: a green run that quietly skipped the corpus is worse than a slow one.

---

## 7. How to resume this project

If you're reading this after a long gap:

1. Read this file, then [`preprocessor.md`](preprocessor.md).
2. If [`next.md`](next.md) exists, it is the queue for the milestone in
   progress, and says where the last session stopped.
3. `git log --oneline docs/` — the *history* of these documents is usually more
   informative than their current state, because it shows what was reconsidered.
4. Check [`grammar-coverage.md`](grammar-coverage.md) for where the parser
   actually stands, and [`limitations.md`](limitations.md) for what was
   deliberately left undone and why.
5. Re-read the module docs in `reference/rdlfmt/src/syntax/parser/mod.rs` and
   `formatter.rs`. They are the design brief for half of this project.

---

## 8. Open questions

- ~~Does the "self-delimiting branch" classification cover the common cases?~~
  **Answered: yes, 96.4% of 1366 regions**, and the figure is exact rather than
  a lower bound — re-measured with the macros expanded, not one region reads
  differently. See [`preprocessor.md`](preprocessor.md#measured). One caveat
  stands: the corpus is all well-kept code, so the number says clean
  SystemVerilog is clean, not that hostile SystemVerilog is rare.
- Is `rowan` the right tree for a file the size of a preprocessed UVM
  testbench? Probably, but measure at M3.
- How much of Annex A can be transcribed mechanically from the PDF versus by
  hand? Affects M3 substantially.
- `bender` integration for filelists/defines/incdirs: at M4 or later?
