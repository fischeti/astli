# Grammar coverage

Where the parser actually stands, tracked by feature area rather than by
individual production. IEEE 1800-2023 Annex A is 747 productions; enumerating
them by hand here would rot immediately.

> **This file is maintained by hand, and stays that way.**
> [D11](plan.md#node-kinds-are-not-annex-as-productions) settled the question:
> the tree's node kinds are not Annex A's productions, so a per-production
> `implemented` flag would not describe this parser. `grammar/productions.txt`
> holds the 747 names as an honest checklist to read; what is written below is
> a feature area, which is what a reader actually wants to know.

Legend: `[ ]` not started · `[~]` partial · `[x]` done · `[v]` deliberately
left to the [verbatim fallback](plan.md#the-verbatim-fallback)

---

## Lexical

- [x] Identifiers: simple, escaped (`\foo.bar ` — **terminated by whitespace**,
      and the terminator is kept inside the token)
- [x] System identifiers, including `$root` / `$unit`
- [~] Keywords — the 1800-2023 set only; `` `begin_keywords `` is ignored
      ([limitation](limitations.md))
- [x] Integer literals: sized, unsized, `'0` `'1` `'x` `'z`, all bases,
      underscores, **embedded whitespace** (`8 'h FF`, lexed in pieces and
      rejoined by the parser)
- [x] Real literals, including exponent forms
- [x] Time literals, `1step`
- [~] String literals: escapes and `\`-continuations done; triple-quoted
      (1800-2023) not ([limitation](limitations.md))
- [x] Comments: line, block
- [x] Operators — the full set, including the `<=` overload, `->`, `->>`, `##`,
      `|->`, `|=>`, `'{`, `+:`, `+/-`
- [x] Attributes `(* ... *)` — nothing lexically special; the parser
      distinguishes them from `(*x)` by lookahead
- [~] Lexer modes — `` `define `` bodies are handled, and needed one differing
      rule rather than a mode; UDP `table`/`endtable` and `` `pragma protect ``
      envelopes are not ([limitation](limitations.md))
- [x] CRLF handling

## Preprocessor (see [preprocessor.md](preprocessor.md))

M2 is closed: all 22 directives of 1800-2023 22.1 are recognised, and the
expanded path expands, resolves and evaluates them. What `[~]` marks below is
therefore no longer "not implemented" but a named gap with an entry in
[`limitations.md`](limitations.md).

The **parser's** view of the preprocessor is a separate axis, and M3 closed it
too: a `` ` `` builds a `MACRO_CALL`, a `DIRECTIVE` or a
`CONDITIONAL_REGION` wherever it stands, inside a `VERBATIM` run as readily as
at the top level — and a region whose branches all balance has each branch
parsed in the enclosing context.

- [x] `` `define `` / `` `undef `` / `` `undefineall ``, incl. parameters and
      default arguments
- [x] Macro invocation as a grammar atom (item/member/statement/expression/
      port/type position) — `MACRO_CALL` over the introducer and a
      `MACRO_ARG_LIST` of `MACRO_ARG`s, split on commas at depth zero and
      never parsed as expressions. Where the table has no definition the
      argument list is [guessed](limitations.md)
- [x] Stringification `` `" ``, escaping `` `\`" ``, token pasting ``` `` ``` —
      a pasted or stringified token is spelled in no file, and the origin map
      says so
- [x] `` `ifdef `` / `` `ifndef `` / `` `elsif `` / `` `else `` / `` `endif ``
      as structured regions — nested by `regions()`, evaluated on the expanded
      path, and in the tree as `CONDITIONAL_REGION` with every branch present.
      A region [does not cross a file boundary](limitations.md)
- [x] Classifying a region self-delimiting or ragged — `RegionShape::live`,
      computed over raw tokens when the stream is built, and the parser acts
      on it: a live region's branches are parsed in the enclosing context and
      a ragged one's hold only what is self-contained. 1,571 of 1,660 corpus
      regions are live. Counted over eight delimiter pairs rather than
      thirteen ([limitation](limitations.md))
- [~] `` `include ``, and include-path resolution — resolved on the expanded
      path, with cycle and depth limits; never followed in raw mode
      ([D6](plan.md#4-decisions)). A name that expands is
      [read as one token](limitations.md)
- [~] `` `__FILE__ `` / `` `__LINE__ `` expand; `` `line `` is recognised but
      [does not move the numbers we report](limitations.md)
- [~] `` `timescale ``, `` `default_nettype ``, `` `resetall ``,
      `` `celldefine `` / `` `endcelldefine ``, `` `unconnected_drive `` —
      recognised; operands kept as tokens ([limitation](limitations.md))
- [~] `` `pragma `` — recognised; `protect` envelopes not
- [x] Recursion detection — a macro that reaches itself stands as written
- [~] Predefined names — the table starts empty, and nothing can seed it until
      the driver knows a filelist ([limitation](limitations.md))

## A.1 Source text

- [x] `module` declarations: ANSI and non-ANSI headers, lifetimes, labels
- [x] Parameter port lists, `type` parameters, and the elements that leave the
      keyword out
- [x] Port declarations: directions, nettypes, interface ports, `.name(expr)`,
      `.*`, and the `input a;` form a non-ANSI header writes as an item
- [x] `interface`, `modport`
- [x] `package`, `import`/`export`, including the import list a header may
      write before its parameters
- [x] `program`
- [ ] `checker`, `config` / `endconfig`
- [ ] `extern module`
- [ ] `$unit` / compilation-unit scope
- [v] `bind` — verbatim ([limitation](limitations.md))

## A.2 Declarations

- [x] Net and variable declarations, all nettypes, `var`, `const`, net delays
- [x] Data types: integer vector/atom, `real`, `string`, `chandle`, `event`,
      `virtual interface`, `type(expr)`
- [x] `enum`, `struct`, `union` (packed / tagged), including a macro that
      writes its own variants and their separators
- [x] `typedef`, and the forward forms, told apart by what follows the keyword
- [x] Packed and unpacked dimensions, dynamic arrays, queues, associative
      arrays
- [x] `parameter` / `localparam`, including `parameter type T = …`
- [~] **Type-vs-expression ambiguity resolution** — a set of the names this
      file typedefs, plus three questions about *shape* that need no names at
      all: a qualifier, a scope, and whether a name follows the brackets.
      A type from a package is still invisible
      ([limitation](limitations.md))
- [x] `class`: `extends` with constructor arguments, `implements`, `virtual`
      and `interface` classes, parameterised classes
- [~] Class members — declarations carry `local`, `protected` and `static` the
      way they carry `const`; a `constraint` gets a shell and a verbatim body
      ([limitation](limitations.md))
- [v] `covergroup`, `coverpoint`, `cross`, bins — verbatim
      ([limitation](limitations.md))
- [x] `function` / `task`, ANSI and non-ANSI argument lists, return types told
      from names by what follows the `::` chain, out-of-class definitions,
      `extern` and `pure virtual` prototypes
- [x] DPI `import`/`export`, including the `c_name =` form
- [ ] `let`, `nettype`, user-defined nettypes

## A.3–A.5 Instances

- [x] Module and interface instantiation, named and positional connections,
      several instances per statement, instance arrays — told from a
      declaration by the `(` after the second name
- [x] Parameter overrides `#(...)`
- [x] `.*` implicit connections
- [ ] `defparam`
- [ ] Gate primitives, strengths, delays
- [ ] UDP declaration, `table`/`endtable` (lexer mode)
- [x] `generate`: `for`, `if`, `case`, labels — and no node kinds of their own,
      because a generate `for` is a `FOR_STMT` over an item body

## A.6 Behavioral statements

- [x] `always`, `always_comb`, `always_ff`, `always_latch`, `initial`, `final`
      — one `PROCEDURAL_BLOCK`, because the six are one shape
- [x] Blocking and nonblocking assignment and the compound forms, `assign` as
      an item, and an assignment delayed by its own operator. The left-hand
      side is an *lvalue* rather than an expression, which is what keeps `<=`
      from reading as the comparison it also is
- [ ] `deassign`, `force` / `release`
- [x] `begin`/`end`, labels at both ends, `fork`/`join`/`join_any`/`join_none`
- [x] `if`/`else`, `unique`/`unique0`/`priority` prefixes
- [x] `case`/`casex`/`casez`, `inside`, `matches`, multi-value arms, `default`
- [x] Loops: `for`, `foreach`, `while`, `do…while`, `repeat`, `forever`
- [x] `break`, `continue`, `return`, `disable`
- [x] Event control `@( … )`, `@*`, `@name`, sensitivity lists with `or` and
      `iff`, `wait ( … )`, `wait fork`, `->` / `->>`
- [x] Timing controls and delays, `#`, `##`
- [ ] `wait_order`
- [ ] `randsequence`, `randcase`
- [ ] Immediate and deferred assertions
- [v] Concurrent assertions: `property`, `sequence`, `assert`/`assume`/`cover`
      — verbatim, deliberately ([limitation](limitations.md))
- [v] `clocking` blocks — verbatim ([limitation](limitations.md))

## A.7 Specify section

- [v] `specify` / `endspecify`, path declarations, timing checks
      — rare in the target corpus; verbatim until proven otherwise

## A.8 Expressions

Precedence climbing over Table 11-2, in `src/parser/expr.rs`. The node kinds
are not Annex A's names and could not be: see
[D11](plan.md#node-kinds-are-not-annex-as-productions).

- [x] Full operator precedence table, including the unary operators binding
      tighter than `**`
- [x] Conditional `?:`, right-associative
- [x] Concatenation, replication, streaming `{<<{ }}` / `{>>{ }}`
- [x] Assignment patterns `'{...}`, with keys, with replication, and with the
      type named (`T'{...}`)
- [x] Casts: type, size and sign, all as one postfix
- [x] `inside`, `dist`
- [x] Ranges: `[a:b]`, `[a+:b]`, `[a-:b]`
- [x] Hierarchical and class-scoped references, as a postfix chain
- [x] Method calls, `with` clauses, array manipulation methods
- [x] System tasks and functions
- [x] An integer literal **lexed in pieces** — `8 'h FF`, and digits that come
      out as several tokens — rejoined into one `LITERAL_EXPR`
- [~] `( operator_assignment )` as an expression — not parsed, deliberately
      ([limitation](limitations.md)); a `for` initialiser is the statement
      rule's job
- [~] **Type-vs-expression ambiguity resolution** — see
      [A.2](#a2-declarations) and [plan.md decision D2](plan.md#4-decisions)

## A.9 General

- [x] Attribute instances — in expression, item, port and statement position;
      told from a parenthesised expression by lookahead
- [ ] Identifier kinds and their scoping rules

---

## Metrics, at the close of M3

`cargo run --release --example metrics` walks the corpus once and prints the
table below. It writes the commits out beside the numbers because `corpus/` is
gitignored and the next fetch overwrites it, which is what
[§6](plan.md#6-corpus-and-testing) requires of any figure quoted here.

| Repo | Commit | Files | Tokens | Verbatim | Regions | Live | MB/s |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `FlooNoC` | `2fa02eb23c` | 66 | 96844 | 0.42% | 3 | 100.0% | 37.5 |
| `axi` | `4da1597974` | 93 | 182174 | 8.49% | 124 | 100.0% | 60.6 |
| `cheshire` | `6234e9e989` | 19 | 48518 | 1.13% | 57 | 100.0% | 59.5 |
| `common_cells` | `db42769334` | 214 | 93208 | 11.14% | 71 | 100.0% | 68.8 |
| `cva6` | `6cb200105f` | 466 | 606087 | 9.21% | 298 | 95.6% | 68.8 |
| `iDMA` | `2e0b0fe53b` | 78 | 94821 | 2.25% | 14 | 100.0% | 60.1 |
| `ibex` | `8b8ee086ae` | 650 | 513337 | 4.99% | 295 | 87.5% | 65.1 |
| `opentitan` | `34ceb5eb56` | 3966 | 5227695 | 3.56% | 772 | 94.9% | 71.3 |
| `snitch_cluster` | `f78a978343` | 74 | 146490 | 2.18% | 26 | 100.0% | 62.0 |
| **all, deduplicated** | — | **4475** | **6184959** | **4.03%** | **1362** | **96.4%** | **69.8** |

**Deduplicated, and that changes the number.** `cva6` vendors `common_cells`,
and both `cva6` and `ibex` vendor `lowrisc_ip` — 1151 of 5626 files are copies.
Pooled without hashing file contents the verbatim rate is 4.28%, which is what
`corpus_verbatim_rate_does_not_rise` records and asserts. That test is a
*ratchet* rather than an estimate, so double-counting costs it nothing as long
as it is consistent; 4.03% is the honest figure.

M3 closed at 5.18% undeduplicated, and the gap between that and the number
above is one rule: a declaration is now [recognised by its
shape](limitations.md#a-type-is-decided-by-shape-and-never-resolved) rather
than by whether the file typedef'd the name, which is what took `FlooNoC` from
3.70% to 0.42%.

What each column is, and why it is worth keeping:

| Metric | Why |
| --- | --- |
| Files | Every one of them parses and round-trips; the parser has no failure mode but the fallback |
| **Verbatim rate** (tokens inside `VERBATIM` / all tokens) | The real coverage number, and the one that should trend to zero. Nearly three quarters of what is left is the six constructs [left to the fallback on purpose](limitations.md) |
| Regions, Live | Validates the [Level C assumption](preprocessor.md#level-c--conditionals-as-structured-regions): a live region's branches are parsed in the enclosing context |
| MB/s | Catches accidental quadratics early. 6.2M tokens in 0.67s, 9.2M tokens/s single-threaded |

**The Live column agrees with an independent measurement, which is the point
of having two.** `examples/conditionals.rs` classifies regions from raw tokens
with no parser at all and reports **96.4% of 1366** in
[`preprocessor.md`](preprocessor.md#measured); the library's own
`RegionShape::live`, written separately and consumed by the parser rather than
merely reported, gives 96.4% of 1362 over the parser's own file set. The four
regions between them are the `.v` and `.vh` files `conditionals.rs` also
walks.

## What the rate is made of

`cargo run --release --example verbatim-report` walks the same files and
groups every `VERBATIM` run by a guess at what put it there, so that the rate
above can be spent rather than only watched. A cause is inferred from the
tokens in the run -- nothing records which rule gave up -- so a row is a
ranking and the per-run lines above the table are the evidence. `other` is the
honest bucket.

| Tokens | Runs | Share | Cause |
| ---: | ---: | ---: | --- |
| 98146 | 1899 | 1.40% | covergroup / coverpoint / bins |
| 70420 | 2157 | 1.00% | constraint block |
| 27855 | 711 | 0.40% | `bind` |
| 20110 | 1284 | 0.29% | immediate / deferred assertion |
| 17292 | 913 | 0.25% | other |
| 16520 | 544 | 0.24% | concurrent assertion |
| 8074 | 323 | 0.12% | an arm of a `randcase` above it |
| 6381 | 64 | 0.09% | type-valued parameter override |
| 5852 | 21 | 0.08% | `(* … *)` attribute before a `module` |
| 5468 | 138 | 0.08% | `randcase` / `randsequence` |
| 5343 | 154 | 0.08% | port connections left by a broken instantiation |
| 3560 | 331 | 0.05% | `force` / `release` |
| 3426 | 130 | 0.05% | clocking block |
| 3227 | 348 | 0.05% | declaration of an unknown type name |
| 2989 | 108 | 0.04% | `property` / `sequence` declaration |
| 2294 | 55 | 0.03% | case arm written as a range |
| 1987 | 81 | 0.03% | `randomize() with { … }` |
| 572 | 74 | 0.01% | system task in item position |
| 351 | 351 | 0.01% | stray `;` after a macro item |
| 72 | 8 | 0.00% | `void'( … )` call |

Over the whole corpus, undeduplicated. The six rows that carry a
[deliberate gap](limitations.md) -- covergroups, constraints, `bind`,
concurrent assertions, clocking blocks and `property`/`sequence` -- come to
73% of what is left, which is what "nearly three quarters" above means. The
rest is the queue: five of these rows name a shape that has no entry anywhere
yet.

`declaration of an unknown type name` was 52407 tokens over 9669 runs and is
now 3227 over 348, and what is left under that heading is mostly mislabelled:
an assignment at file scope and a `'{…}` initialiser the declarator rule gives
up on, counted here because the run starts on an identifier.

## What the corpus tests assert

Five, and between them they are the M3 gate:

| Test | Claim |
| --- | --- |
| `corpus_round_trips_through_the_tree` | The tree's text is the file's, byte for byte. Live since before there was a grammar, and never allowed to regress |
| `corpus_verbatim_rate_does_not_rise` | The ratchet. Down is a new number to record; up is a bug |
| `corpus_shells_close_what_they_open` | Every `MODULE_DECL`, `CLASS_DECL`, `CASE_STMT` and the rest begins with the keyword it claims and ends with the `end…` that matches. This is what stops the rate falling *by being wrong* — a node closed over text no rule read would improve the number and corrupt the tree |
| `corpus_spliced_files_round_trip` | Real files cut where nobody would cut them, reassembled, and held to the same two properties |
| `corpus_references_stay_inside_their_file` | Raw mode never follows an `` `include `` ([D6](plan.md#4-decisions)) |

Plus `tests/differential.rs`, the M2 gate, against the reference preprocessor.

## The fuzzer

`tests/fuzz.rs`, added at the close of M3. It holds random input to the two
properties that must hold for *all* input — **the tree's text is the input,
and nothing panics** — because everything else the parser does is a judgement
about what the text means, and a judgement can be wrong without the tool being
broken. That is what the fallback is for. These two are not judgements.

Three generators: random bytes, for the lexer's edges; random sequences of
**real tokens**, drawn from a vocabulary chosen for what a token makes a rule
do rather than for how often it is written — every keyword that opens a body
is there with the one that closes it, and several that close a body nothing
opened; and splices of corpus files, which is the only one that produces input that is *nearly*
valid — and nearly valid is where a rule that reads one token too far shows up.
The second reaches 68 of the tree's node kinds on its own, so this is a test of
the grammar rather than of the lexer.

It is seeded from a counter rather than from the clock, so a failure names an
input and repeats. A `cargo-fuzz` target would find more given a week; it is
worth adding when there is CI to run it in, and it would use these same
generators. Meanwhile a twentieth of the cases run in debug, so
`cargo nextest run -P quick` stays the tight loop, and all 45,600 of them run
in 0.7s in release.
