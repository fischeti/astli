# Grammar coverage

By feature area rather than by production ([D11](plan.md#4-decisions)).
`grammar/productions.txt` lists all 747 Annex A names as a checklist.

`[x]` done · `[~]` partial · `[ ]` not started · `[v]` left to the verbatim
fallback on purpose ([limitation](limitations.md#six-constructs-are-left-to-the-fallback-on-purpose))

## Lexical

- [x] Identifiers (simple, escaped, system), all literal forms including
      `8 'h FF` and `1step`, comments, the full operator set, attributes, CRLF
- [~] Keywords: the 1800-2023 set only
- [~] Strings: no triple-quoted form
- [~] Lexer modes: `` `define `` bodies yes; UDP tables, `` `pragma protect ``
      no

## Preprocessor

All 22 directives are recognised, and the expanded path evaluates them. In the
tree, `` ` `` builds a `MACRO_CALL`, `DIRECTIVE` or `CONDITIONAL_REGION`
wherever it stands. The `[~]`s are entries in [`limitations.md`](limitations.md).

- [x] `` `define `` / `` `undef `` / `` `undefineall ``, formals with defaults
- [x] Macro calls as grammar atoms; arity guessed when unknown
- [x] `` `" ``, `` `\`" ``, ` `` `
- [x] Conditionals as regions, live or ragged
- [x] Recursion detection
- [~] `` `include ``: expanded mode only; a macro name is read as one token
- [~] `` `__FILE__ `` / `` `__LINE__ ``; `` `line `` recognised, no effect
- [~] `` `timescale ``, `` `default_nettype ``, `` `pragma `` and the rest:
      recognised, operands kept as tokens

## Source text (A.1)

- [x] `module`, ANSI and non-ANSI headers, parameter port lists, ports
- [x] `interface`, `modport`, `package`, `import`/`export`, `program`
- [ ] `checker`, `config`, `extern module`
- [v] `bind`

## Declarations (A.2)

- [x] Nets, variables, all data types, `enum`/`struct`/`union`, `typedef`
      and its forward forms, dimensions, queues, associative arrays
- [x] `parameter`/`localparam`, including `parameter type`
- [x] Type-vs-expression by shape
      ([limitation](limitations.md#a-type-is-decided-by-shape-never-resolved))
- [x] `class`: `extends`, `implements`, `virtual`, `interface class`,
      parameterised
- [~] Class members; `constraint` gets a shell and a verbatim body
- [x] `function`/`task`, prototypes, out-of-class definitions, DPI
- [v] `covergroup`
- [ ] `let`, `nettype`

## Instances (A.3–A.5)

- [x] Module and interface instantiation, named, positional and `.*`
      connections, `#(...)` overrides, instance arrays
- [x] `generate` `for`/`if`/`case`, with no node kinds of its own
- [ ] `defparam`, gate primitives, UDPs

## Behavioural (A.6)

- [x] `always*`, `initial`, `final` as one `PROCEDURAL_BLOCK`
- [x] Assignments, all compound forms, `assign`; left-hand sides are lvalues
      so `<=` is never read as a comparison
- [x] Blocks, `fork`/`join*`, `if`, `case`/`casex`/`casez`, `inside`,
      `matches`, loops, jumps
- [x] Event and timing controls, `wait`, `->`
- [ ] `force`/`release`, `deassign`, `wait_order`, `randcase`,
      `randsequence`, immediate assertions
- [v] Concurrent assertions, `property`/`sequence`, `clocking`, `specify`

## Expressions (A.8)

- [x] Precedence climbing over Table 11-2, `?:`, concatenation, replication,
      streaming, assignment patterns, casts, `inside`, `dist`, ranges,
      hierarchical and scoped references, calls, `with`
- [~] No `( operator_assignment )`

## Metrics

`cargo run --release --example metrics` prints per-repo figures, and
`--example verbatim-report` ranks what the fallback still takes.

At the close of M3, over the [pinned corpus](plan.md#6-corpus-and-testing),
deduplicated: **4475 files, 6.18M tokens, 4.03% verbatim**, 96.4% of
regions live, 9.2M tokens/s single-threaded. About three quarters of the
verbatim tokens are the `[v]` constructs. The largest remaining causes are
covergroups (1.4%), constraint bodies (1.0%), `bind` (0.4%) and immediate
assertions (0.3%).

Corpus tests (the M3 gate):

- `corpus_round_trips_through_the_tree`: byte-exact.
- `corpus_verbatim_rate_does_not_rise`: a ratchet (`RATCHET` in
  `svirig-parse/tests/gates.rs`), counted without deduplication.
- `corpus_shells_close_what_they_open`: every shell node starts with its
  keyword and ends with its matching `end…`, so the rate cannot fall by
  being wrong.
- `corpus_spliced_files_round_trip`: files cut at random points.
- `corpus_references_stay_inside_their_file`: raw mode follows no include.
- `corpus_trees_have_the_shapes_the_grammar_names`: every node's children
  are what `svirig.ungram` names, bar the few `SHAPE_RATCHET` counts.
