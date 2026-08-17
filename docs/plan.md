# Project plan

> **Status:** exploratory. No code yet beyond scaffolding. Nothing here is a
> commitment; it is a record of what was decided and *why*, so that picking the
> project up after a three-month gap costs an afternoon instead of a week.

The project is `svirig`; see [Naming](#naming). Every crate carries that
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
| `svirig-text` | File ids, spans, the origin map for expanded tokens, diagnostic rendering | M2 (needed once macros expand) |
| `svirig-lexer` | `logos` lexer with modes; tokens including directive tokens; no preprocessing | M1 |
| `svirig-preproc` | Macro table, expansion, `` `include `` resolution, conditional evaluation. Two output modes. | M2 |
| `svirig-syntax` | `SyntaxKind`, `rowan` `Language` impl, event-based parser, generated AST accessors | M3 |
| `svirig-fmt` | Formatting IR, layout rules, alignment pass | M4 |
| `svirig-hir` | Name resolution, elaboration, types | someday / never |
| `svirig` | Driver binary: CLI, file discovery, filelist/`bender` integration | M4 |

**Do not create all of these up front.** The workspace currently holds only
`svirig-syntax`, which will accumulate the lexer, then the preprocessor, then the
parser; split outward when a boundary starts hurting. Until there is a
formatter there is nothing for a binary to drive, so a `dump-tokens` example in
`svirig-syntax` — mirroring `rdlfmt`'s `dump-cst` — covers M1 and M2. The module
layout inside `svirig-syntax` should be drawn as if the splits already existed.

The one split worth having on day one is **`svirig-fmt` out of `svirig-syntax`**,
because everything downstream shares the syntax crate and formatting concerns
must not leak into the tree shape.

### Reused from `rdlfmt`

The `rdlfmt` design carries over almost wholesale and should be re-read before
starting:

- **Trivia is placed, never skipped.** Leading trivia belongs to the item that
  follows; a same-line trailing comment stays with the token it annotates.
- **Separation is requested, not written.** No rule writes a space or newline;
  it requests a minimum separation, resolved lazily when the next thing is
  written. Gives no-trailing-whitespace and call-site-free indentation for
  free.
- **Whitespace is discarded, its signal is not.** Blank lines are lifted out as
  a gap *width*, not as a separation in their own right.
- **Verify by re-lexing.** The output's token stream must equal the input's.

### What must change from `rdlfmt`

- **Event-based parser, not direct `GreenNodeBuilder` calls.** `rdlfmt` drives
  the builder inline with `Checkpoint`. That works because SystemRDL is nearly
  LL(1). SystemVerilog is not — see [decision D2](#decisions) — and checkpoints
  allow retroactive *wrapping* but not *undo*. Emit a flat `Vec<Event>` and
  build the green tree at the end; snapshot is `(events.len(), token_pos)` and
  rollback is a truncate. **Retrofitting this later is a parser rewrite.**
- **Directives cannot all be opaque single-line trivia.** See
  [`preprocessor.md`](preprocessor.md).
- **Alignment is a first-class requirement**, and the `Gap` model cannot
  express it. Needs a post-pass. See [decision D4](#decisions).

---

## 4. Decisions

| # | Decision | Rationale |
| --- | --- | --- |
| D1 | `logos` for lexing, `rowan` for the tree | Proven in `rdlfmt`. `rowan` is the Roslyn model, which is the right model for lossless trees. Plan for lexer **modes** (`Lexer::morph`): macro body text, UDP `table`/`endtable`, directive arguments, `` `pragma protect `` envelopes. |
| D2 | Hand-written recursive descent, **event-based**, with speculative parse + rollback | SV is not LL(k). The killer is type/expression ambiguity: `foo bar;` is a declaration only if `foo` names a type; `(a)(b)` is a cast or a call. `slang` resolves this with lookahead heuristics and rollback. No generated-parser framework handles this cleanly. |
| D3 | **Verbatim fallback node from day one** | See below. Highest-leverage single decision in this document. |
| D4 | Column alignment is a separate post-pass over emitted lines | SV culture expects aligned `.port_i (sig)` connections and `assign` RHS (lowRISC, PULP styles mandate it). This fits neither a Wadler IR nor the `Gap` model. Align within maximal runs of same-shaped siblings, broken by blank lines and comments. |
| D5 | Formatter is **preprocessor-transparent**, enforced as an assertion | See [`preprocessor.md`](preprocessor.md#the-transparency-invariant). |
| D6 | Formatter never follows `` `include `` | Each file is formatted independently. A compiler must follow includes; a formatter must not. |
| D7 | Few knobs: indent width, line width, alignment on/off | Resist a style-option matrix. `gofmt`-style opinionation is cheaper to maintain and the thing people actually want. Default line width 100. |
| D8 | Dual MIT / Apache-2.0, matching `rdlfmt` | Rust ecosystem norm. |

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

**M0 — Scaffolding.** *Done.* Workspace, CI (`cargo fmt --check`, `clippy`,
`test`), `scripts/fetch-corpus.sh`, these documents.

**M1 — Lexer complete.** Every token kind in the standard, all lexer modes,
`dump-tokens` example mirroring `rdlfmt`'s `dump-cst`. **Gate: byte-exact
round-trip over the whole corpus** — concatenating every token's text
reproduces the input file exactly. This is a hard, verifiable, finishable
gate. *Weeks.*

**M2 — Preprocessor complete.** Expansion, `` `include ``, conditionals, both
output modes, origin map. **Gate: differential test against `slang -E`** over
the corpus. Publishable as `svirig-preproc` on its own. *Weeks to months.*

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

Add repos when there is a question they would answer, not before:
`accellera/uvm-core` once macro invocations are handled (**the** macro torture
test — if UVM parses, the model works), `lowRISC/opentitan` once conditionals
are, `black-parrot` and `chipsalliance/*` for breadth after that.

`scripts/fetch-corpus.sh` writes `corpus/MANIFEST` with the resolved commit of
each repo, so a coverage number can always be traced to the input that
produced it.

Three oracles, all cheap, and between them they catch nearly everything:

1. **Round-trip / token-stream equality** — output re-lexes to the input's
   token stream modulo whitespace.
2. **Idempotency** — `format(format(x)) == format(x)`.
3. **`slang` differential** — `slang --parse-only` succeeds on input and
   output identically; `slang -E` agrees with `svirig-preproc`.

Plus snapshot tests for formatting decisions, and a fuzzer once M3 lands.

---

## 7. How to resume this project

If you're reading this after a long gap:

1. Read this file, then [`preprocessor.md`](preprocessor.md).
2. `git log --oneline docs/` — the *history* of these documents is usually more
   informative than their current state, because it shows what was reconsidered.
3. Check [`grammar-coverage.md`](grammar-coverage.md) for where the parser
   actually stands.
4. Re-read the module docs in `reference/rdlfmt/src/syntax/parser/mod.rs` and
   `formatter.rs`. They are the design brief for half of this project.

---

## 8. Open questions

- Does the "self-delimiting branch" classification in
  [`preprocessor.md`](preprocessor.md) actually cover the common cases? Measure
  it on the corpus during M2 — this is the assumption the whole formatter rests
  on, and it is cheap to falsify early.
- Is `rowan` the right tree for a file the size of a preprocessed UVM
  testbench? Probably, but measure at M3.
- How much of Annex A can be transcribed mechanically from the PDF versus by
  hand? Affects M3 substantially.
- `bender` integration for filelists/defines/incdirs: at M4 or later?
