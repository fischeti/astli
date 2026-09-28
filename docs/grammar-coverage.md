# Grammar coverage

By feature area rather than by production ([D11](plan.md#4-decisions)).

`[x]` done · `[~]` partial · `[ ]` not started · `[v]` left to the verbatim
fallback on purpose ([limitation](limitations.md#specify-is-left-to-the-fallback-on-purpose))

## Lexical

- [x] Identifiers (simple, escaped, system), all literal forms including
      `8 'h FF` and `1step`, comments, the full operator set, attributes, CRLF
- [~] Keywords: the 1800-2023 set only
- [~] Strings: no triple-quoted form
- [~] Lexer modes: `` `define `` bodies yes; an encrypted
      `` `pragma protect `` envelope in expansion only; UDP tables no

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
- [~] `` `include ``: expanded mode only; a macro name is read as one token
- [~] `` `__FILE__ `` / `` `__LINE__ ``; `` `line `` recognised, no effect
- [~] `` `timescale ``, `` `default_nettype ``, `` `pragma `` and the rest:
      recognised, operands kept as tokens

## Source text (A.1)

- [x] `module`, ANSI and non-ANSI headers, parameter port lists, ports
- [x] `interface`, `modport`, `package`, `import`/`export`, `program`
- [x] `timeunit`, `timeprecision`, anywhere an item may stand
- [ ] `checker`, `config`, `extern module`
- [x] `bind`, into a module, the instances listed after `:`, or a path

## Declarations (A.2)

- [x] Nets, variables, all data types, `enum`/`struct`/`union`, `typedef`
      and its forward forms, dimensions, queues, associative arrays
- [x] `parameter`/`localparam`, including `parameter type`
- [x] Type-vs-expression by shape
      ([limitation](limitations.md#a-type-is-decided-by-shape-never-resolved))
- [x] `class`: `extends`, `implements`, `virtual`, `interface class`,
      parameterised
- [x] Class members; `constraint`, its prototypes and `class::name`
      definitions, and `randomize() with`
- [x] `function`/`task`, prototypes, out-of-class definitions, DPI
- [x] `covergroup`: cover points, crosses, bins of every kind, `binsof`
- [ ] `let`, `nettype`

## Instances (A.3–A.5)

- [x] Module and interface instantiation, named, positional and `.*`
      connections with attributes, `#(...)` overrides of values and types,
      instance arrays
- [x] `generate` `for`/`if`/`case`, with no node kinds of its own
- [ ] `defparam`, gate primitives, UDPs

## Behavioural (A.6)

- [x] `always*`, `initial`, `final` as one `PROCEDURAL_BLOCK`
- [x] Assignments, all compound forms, `assign`; left-hand sides are lvalues
      so `<=` is never read as a comparison
- [x] Blocks, `fork`/`join*`, `if`, `case`/`casex`/`casez`, `inside`,
      `matches`, loops, jumps
- [x] Event and timing controls, `wait`, `->`
- [x] Immediate and deferred assertions, labelled or not, among items too
- [x] `force`/`release`, procedural `assign`/`deassign`
- [x] `randcase`
- [ ] `wait_order`, `randsequence`
- [x] Concurrent assertions, `property`/`sequence` declarations and every
      operator of theirs, `default disable iff`
- [x] `clocking`, `default` and `global`, and in a modport
- [v] `specify`

## Expressions (A.8)

- [x] Precedence climbing over Table 11-2, `?:`, concatenation, replication,
      streaming, assignment patterns, casts, `inside`, `dist`, ranges,
      hierarchical and scoped references, calls, `with`, `tagged`
- [~] No `( operator_assignment )`

## Metrics

`cargo run --release --example metrics` prints per-repo figures, and
`--example verbatim-report` ranks what the fallback still takes.

Over the [pinned corpus](plan.md#6-corpus-and-testing), deduplicated (4475
files, 6.18M tokens):

| After | Verbatim |
| --- | ---: |
| M3 (96.4% of regions live, 9.2M tokens/s single-threaded) | 4.03% |
| Constraint bodies, immediate assertions | 2.78% |
| RTL misparses | 2.36% |
| `bind` | 1.98% |
| Covergroups | 0.67% |
| Concurrent assertions | 0.36% |
| `randcase` | 0.24% |
| `clocking` | 0.20% |
| `timeunit`, `timeprecision` | 0.19% |

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
  are what `astli.ungram` names, bar the few `SHAPE_RATCHET` counts.

## Shape defects

The nodes `SHAPE_RATCHET` counts, each a parser inconsistency. Fix one ahead
of the formatter rule that meets it, and lower the ratchet.

- `typedef name;` builds a `TYPE_REF` where `typedef class C;` builds a
  `DECLARATOR`.
- The `?` digit of a casez pattern written in pieces (`2'b 1?`) is read as a
  conditional.
- A macro standing for an `inside` list is left a bare token.
- A struct member comes out incomplete.
