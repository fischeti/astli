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

The **parser's** view of the preprocessor is a separate axis, and steps 6 and 9
of M3 closed it: a `` ` `` builds a `MACRO_CALL`, a `DIRECTIVE` or a
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

## Metrics to track from M3 onward

Recorded per corpus repo, per commit, so the trend is visible:

| Metric | Why |
| --- | --- |
| Files parsed without error | Basic health |
| **Verbatim-fallback rate** (tokens inside `VERBATIM` / total tokens) | The real coverage number, and the one that should trend to zero |
| Conditional regions classified self-delimiting vs. ragged | Validates the [Level C assumption](preprocessor.md#level-c--conditionals-as-structured-regions) |
| Parse wall-clock, tokens/sec | Catches accidental quadratics early |

The third of those did not wait for M3: it needs only the lexer, and
`cargo run --release --example conditionals` already reports it. Its value is
in [`preprocessor.md`](preprocessor.md#measured), **96.4% of 1366 regions**,
and it has been re-run with the macros expanded — a macro standing in for a
delimiter is invisible to a token-level pass, so the figure was a lower bound
until then, and it turned out to be exact.

The second is the M3 gate, and it is live from step 5 of
[`next.md`](next.md): `corpus_verbatim_rate_does_not_rise` records the rate and
allows it only to fall. It stands at **5.47% of 7,009,174 tokens** after step 9,
and over half of what is left is the six constructs
[left to the fallback on purpose](limitations.md). Step 10 is where this table
gets filled in per repo and per commit.

A third number arrived with step 9 and belongs beside them.
`corpus_shells_close_what_they_open`, in `tests/item.rs`, checks that every
`MODULE_DECL`, `CLASS_DECL`, `CASE_STMT` and the rest begins with the keyword
it claims and ends with the `end…` that matches. It is what stops the rate
falling *by being wrong* — a node closed over text no rule read would improve
the number and corrupt the tree.
