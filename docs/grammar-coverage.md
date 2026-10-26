# Grammar coverage

By feature area rather than by production ([D11](plan.md#4-decisions)).

`[x]` done · `[~]` partial · `[ ]` not started

## Lexical

- [x] Identifiers (simple, escaped, system), all literal forms including
      `8 'h FF` and `1step`, comments, the full operator set, attributes, CRLF
- [x] Keywords: each version's set, as `` `begin_keywords `` names it
- [x] Strings, triple-quoted ones too
- [~] Lexer modes: `` `define `` bodies and UDP tables yes; an encrypted
      `` `pragma protect `` envelope in expansion only

## Preprocessor

All 22 directives are recognised, and the expanded path evaluates them. In the
tree, `` ` `` builds a `MACRO_CALL`, `DIRECTIVE` or `CONDITIONAL_REGION`
wherever it stands. The `[~]`s are entries in [`limitations.md`](limitations.md).

- [x] `` `define `` / `` `undef `` / `` `undefineall ``, formals with defaults
- [x] Macro calls as grammar atoms; arity guessed when unknown
- [x] `` `" ``, `` `\`" ``, ` `` `
- [x] Conditionals as regions, live or ragged; a live one among a list's
      entries or a `case`'s arms holds those
- [x] Recursion detection
- [~] `` `include ``: expanded mode only
- [~] `` `__FILE__ `` / `` `__LINE__ ``, renumbered by `` `line ``; diagnostics
      stay physical
- [~] `` `timescale ``, `` `default_nettype ``, `` `pragma `` and the rest:
      recognised, operands kept as tokens

## Source text (A.1)

- [x] `module`, ANSI and non-ANSI headers, parameter port lists, ports
- [x] `interface`, `modport`, `package`, `import`/`export`, `program`; an
      `import` in a block or a subroutine too
- [x] A modport's subroutines, imported or exported by name or as a
      prototype
- [x] `timeunit`, `timeprecision`, anywhere an item may stand
- [x] `checker`, instantiated among items, in a procedure, or from a package
- [x] `config`, its `design`, and its `liblist` and `use` rules
- [x] `extern` modules, interfaces and programs, and `(.*)` for their ports;
      `extern forkjoin task`
- [x] `bind`, into a module, the instances listed after `:`, or a path

## Declarations (A.2)

- [x] Nets, their drive and charge strengths, `interconnect`, variables, all
      data types, `enum`/`struct`/`union`, `typedef` and its forward forms,
      `nettype`, `let`, dimensions, queues, associative arrays
- [x] `parameter`/`localparam`, including `parameter type`; a value or an
      override may be `min:typ:max`, which sema reads as its typical value
- [x] Type-vs-expression by shape
      ([limitation](limitations.md#a-type-is-decided-by-shape-never-resolved))
- [x] `class`: `extends`, `implements`, `virtual`, `interface class`,
      parameterised
- [x] Class members; `constraint`, its prototypes and `class::name`
      definitions, and `randomize() with`
- [x] `function`/`task`, prototypes, out-of-class definitions, DPI, ports
      declared in the body
- [x] `covergroup`: cover points, crosses, bins of every kind, `binsof`

## Instances (A.3–A.5)

- [x] Module and interface instantiation, named, positional and `.*`
      connections with attributes, `#(...)` overrides of values and types,
      instance arrays
- [x] `generate` `for`/`if`/`case`, with no node kinds of its own
- [x] `defparam`; gates and switches, with strengths, delays and unnamed
      instances
- [x] UDPs, combinational and sequential, a symbol a row's place does not
      take reported; their instances with a strength and a delay

## Behavioural (A.6)

- [x] `always*`, `initial`, `final` as one `PROCEDURAL_BLOCK`
- [x] Assignments, all compound forms, `assign` and its drive strength;
      left-hand sides are lvalues
      so `<=` is never read as a comparison
- [x] Blocks, `fork`/`join*`, `if`, `case`/`casex`/`casez`, `inside`,
      `matches` with its patterns and `&&&`, loops, jumps
- [x] Event and timing controls, `wait`, `->`
- [x] Immediate and deferred assertions, labelled or not, among items too
- [x] `force`/`release`, procedural `assign`/`deassign`
- [x] `randcase`
- [x] `randsequence`, which the formatter keeps as written
- [x] `wait_order`
- [x] Concurrent assertions, `property`/`sequence` declarations and every
      operator of theirs, `default disable iff`
- [x] `clocking`, `default` and `global`, and in a modport
- [x] `specify`: paths, timing checks, pulse styles, `PATHPULSE$` limits

## Expressions (A.8)

- [x] Precedence climbing over Table 11-2, `?:`, concatenation, replication,
      streaming, assignment patterns, casts, `inside`, `dist`, ranges,
      hierarchical and scoped references, calls, `with`, `tagged`
- [x] `( operator_assignment )`, an assignment as a value in parentheses

## Metrics

`cargo run --release --example metrics` prints per-repo figures, and
`--example verbatim-report` ranks what the fallback still takes.

Over the [pinned corpus](plan.md#6-corpus-and-testing), deduplicated, 0.17%
of tokens are verbatim.

Corpus tests (the M3 gate):

- `corpus_round_trips_through_the_tree`: byte-exact.
- `corpus_verbatim_rate_does_not_rise`: a ratchet (`RATCHET` in
  `astli-parse/tests/gates.rs`), counted without deduplication.
- `corpus_shells_close_what_they_open`: every shell node starts with its
  keyword and ends with its matching `end…`, so the rate cannot fall by
  being wrong.
- `corpus_spliced_files_round_trip`: files cut at random points.
- `corpus_references_stay_inside_their_file`: raw mode follows no include.
- `corpus_trees_have_the_shapes_the_grammar_names`: every node's children
  are what `astli.ungram` names.
