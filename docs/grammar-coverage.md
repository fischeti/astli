# Grammar coverage

Where the parser actually stands, tracked by feature area rather than by
individual production. IEEE 1800-2023 Annex A is roughly 600 productions;
enumerating them by hand here would rot immediately.

> **This file should eventually be generated.** Once Annex A is transcribed
> into a machine-readable grammar (see [plan.md](plan.md), M3), the source of
> truth becomes that file plus a per-production `implemented` flag, and this
> document becomes its rendering. Until then it is maintained by hand and is
> approximate.

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
- [ ] Lexer modes: macro body text, UDP `table`/`endtable`,
      `` `pragma protect `` envelope ([limitation](limitations.md))
- [x] CRLF handling

## Preprocessor (see [preprocessor.md](preprocessor.md))

- [ ] `` `define `` / `` `undef `` / `` `undefineall ``, incl. parameters and
      default arguments
- [ ] Macro invocation as a grammar atom (item/member/statement/expression/
      port/type position)
- [ ] Stringification `` `" ``, escaping `` `\`" ``, token pasting ``` `` ```
- [ ] `` `ifdef `` / `` `ifndef `` / `` `elsif `` / `` `else `` / `` `endif ``
      as structured regions, with self-delimiting classification
- [ ] `` `include ``, and include-path resolution (compiler mode only)
- [ ] `` `line ``, `` `__FILE__ ``, `` `__LINE__ ``
- [ ] `` `timescale ``, `` `default_nettype ``, `` `resetall ``,
      `` `celldefine `` / `` `endcelldefine ``, `` `unconnected_drive ``
- [ ] `` `pragma ``, incl. `protect` envelopes
- [ ] Recursion detection

## A.1 Source text

- [ ] `module` declarations: ANSI and non-ANSI headers, `extern`
- [ ] Parameter port lists, `type` parameters
- [ ] Port declarations: directions, nettypes, interface ports, `.*`
- [ ] `interface`, `modport`
- [ ] `package`, `import`/`export`, scope resolution
- [ ] `program`, `checker`
- [ ] `config` / `endconfig`
- [ ] `$unit` / compilation-unit scope
- [ ] `bind`

## A.2 Declarations

- [ ] Net and variable declarations, all nettypes, `var`
- [ ] Data types: integer vector/atom, `real`, `string`, `chandle`, `event`
- [ ] `enum`, `struct`, `union` (packed / unpacked / tagged)
- [ ] `typedef`, forward typedefs
- [ ] Packed and unpacked dimensions, dynamic arrays, queues, associative arrays
- [ ] `class`: extends/implements, `virtual`, parameterised classes,
      constructors, `super`, `this`
- [ ] Class members, `rand`/`randc`, constraints, `local`/`protected`/`static`
- [ ] `covergroup`, `coverpoint`, `cross`, bins
- [ ] `function` / `task`, all argument forms, `ref`, default arguments
- [ ] DPI `import`/`export`
- [ ] `let`, `nettype`, user-defined nettypes

## A.3–A.5 Instances

- [ ] Module and interface instantiation, named and positional connections
- [ ] Parameter overrides `#(...)`, `defparam`
- [ ] `.*` implicit connections
- [ ] Gate primitives, strengths, delays
- [ ] UDP declaration, `table`/`endtable` (lexer mode)
- [ ] `generate`: `for`, `if`, `case`; labels

## A.6 Behavioral statements

- [ ] `always`, `always_comb`, `always_ff`, `always_latch`, `initial`, `final`
- [ ] Blocking / nonblocking assignment, `assign` / `deassign`, `force` /
      `release`
- [ ] `begin`/`end`, labels, `fork`/`join`/`join_any`/`join_none`
- [ ] `if`/`else`, `unique`/`unique0`/`priority` prefixes
- [ ] `case`/`casex`/`casez`, `inside`, `matches`
- [ ] Loops: `for`, `foreach`, `while`, `do…while`, `repeat`, `forever`
- [ ] `break`, `continue`, `return`, `disable`
- [ ] Event control, `@`, `@*`, `wait`, `wait_order`, `->` / `->>`
- [ ] Timing controls and delays
- [ ] `randsequence`, `randcase`
- [ ] Immediate and deferred assertions
- [ ] Concurrent assertions: `property`, `sequence`, `assert`/`assume`/`cover`
      — **large, and a good early candidate for `[v]`**

## A.7 Specify section

- [v] `specify` / `endspecify`, path declarations, timing checks
      — rare in the target corpus; verbatim until proven otherwise

## A.8 Expressions

- [ ] Full operator precedence table
- [ ] Conditional `?:`, and chained ternaries (a formatting problem too)
- [ ] Concatenation, replication, streaming `{<<{ }}` / `{>>{ }}`
- [ ] Assignment patterns `'{...}`
- [ ] Casts: type, size, sign, `$cast`
- [ ] `inside`, `dist`
- [ ] Ranges: `[a:b]`, `[a+:b]`, `[a-:b]`
- [ ] Hierarchical and class-scoped references
- [ ] Method calls, `with` clauses, array manipulation methods
- [ ] System tasks and functions
- [ ] **Type-vs-expression ambiguity resolution** — the load-bearing one; see
      [plan.md decision D2](plan.md#4-decisions)

## A.9 General

- [ ] Attribute instances in every legal position
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
