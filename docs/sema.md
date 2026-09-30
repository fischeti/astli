# Semantics

> **Status:** tree rules done (M8); names in a definition open (M9): the
> HIR and name resolution are done.
> `astli lint` and `astli check` are its readers; milestones M8–M10 in
> [`plan.md`](plan.md#5-milestones).

Semantic analysis answers what the tree means: which declaration a name
refers to, what each expression's type and width are, what each parameter
evaluates to, and what the design is once instantiated from its tops. The
layers depend on each other: a body cannot be typed without its parameters,
and a generate block decides which declarations exist.

## Layers

| Layer | Answers | Needs |
| --- | --- | --- |
| HIR | A definition's ports, parameters, declarations, processes, statements and expressions, in arenas, each id pointing back at its syntax | A tree |
| Scopes | What a name refers to: local, enclosing, explicit imports, wildcard imports (lazy: only a name used and not declared locally), `$unit`, the definitions namespace | The HIR of the definition and of what it imports |
| Constants | Parameter values, `$bits`, `$clog2`, enum values, generate conditions, constant functions | Arbitrary-width 4-state values, and an interpreter over a subset of statements |
| Types | Typedefs, dimensions, structs, unions, enums, type parameters; each expression's type, width and signedness, self- or context-determined | Constants |
| Elaboration | The instance tree from the tops: parameter overrides, generate blocks unrolled, one body per definition and parameter values, `bind`, hierarchical names | All of the above |

A lint rule declares the layer it needs: **tree** (syntax alone),
**definition** (names resolved inside one definition, parameters symbolic) or
**elaborated** (parameters known). The driver builds no more than the enabled
rules ask for.

## Decisions

| # | Decision | Why |
| --- | --- | --- |
| S1 | A tree rule reads the raw tree of one file, as `fmt` does; everything above reads expanded trees over a filelist | A style rule is about what was written and must agree between editor and CI ([D15](plan.md#4-decisions)). What a file declares depends on its macros and conditionals. |
| S2 | Lower to a HIR with ids, rather than analyse the rowan tree | It absorbs syntax variants: ANSI and non-ANSI ports are one port list. Rowan nodes are `!Send` and heavy, and every analysis walking all node shapes is the formatter's exhaustiveness without its reason. |
| S3 | Unknown is silent: a `VERBATIM`, or a construct sema does not model, becomes an error type or value that absorbs what is derived from it, and a scope holding a `VERBATIM` reports no undeclared name | A false error costs more trust than a missed one, and 4% of tokens are verbatim. |
| S4 | Queries on demand, memoised, with an in-progress mark for cycles; no `salsa` | A parameter can call a package function that takes `$bits` of a type, so no fixed pass order works. `salsa` pays off with an editor's edits; keeping each query a function of ids leaves room for it. |
| S5 | `astli check` reports a subset of errors and no false one | Replacing `slang` is a non-goal; a checker that fails on correct UVM is worse than none. |
| S6 | One crate, `astli-sema`, for all layers; `astli-lint` depends on it | Split when a reader needs part of it alone ([D12](plan.md#4-decisions)). M8's tree rules need none of it, so the dependency arrives with M9. |
| S7 | A lint waiver is an attribute, `(* astli_allow = "rule" *)`, covering the node it stands on; never a comment. What no attribute reaches, such as a `` `define `` or vendored code, gets levels by path in `astli.toml` | An attribute is parsed, so an unknown rule is reported, and moves with its node when the formatter moves lines; tools ignore one they do not know. One waiver syntax, and no other tool's comments read. `` `pragma `` would do, but `slang` warns on each one. |
| S8 | `astli lint` takes a build; tree rules read the raw tree, definition rules the expanded one | One command and one set of waivers. Attributes survive expansion. |
| S9 | A class is a declared type whose body is not lowered | UVM is classes; a checker that fails on it is worse than none ([S5](#decisions)). Revisit when a rule needs members. |
| S10 | A declaration a macro wrote gets no name lint; an error is placed at the outermost call | Its user can fix it only in the macro, often a library's. |
| S11 | A package two files declare lends its members to nothing: a name the kept one lacks is unknown | Which one a compiler keeps depends on order and tool; OpenTitan has one `top_racl_pkg` per top. |
| S12 | An `` `include `` expansion did not find is kept as trivia, and makes the scope it stands in opaque | The header may declare anything; a missing generated header otherwise turns into dozens of undeclared names. |

## Oracles

- **Lint:** OpenTitan runs verible's lint with lowRISC rules in CI, so a rule
  both tools have should fire on OpenTitan RTL only where a
  `verilog_lint: waive` comment stands.
- **Check:** the corpus elaborates clean in `slang` (a pickled `cheshire`
  does, since M6), so `astli check` reports nothing on it; each error it
  reports on sv-tests must be one `slang` reports too. The tests
  `scripts/sv-tests.py` skips as elaboration-only become `check`'s score.

## Prior art

- `slang`: a compilation whose scopes create their members lazily, constant
  evaluation on demand, one body per instance cache key, and a visitor that
  forces everything for diagnostics.
- rust-analyzer: a per-file item tree that body edits leave alone, the HIR
  below it, and `salsa` for queries.
- verible: tree rules only, configured per rule, waived by comment.

## Open questions

- **One compilation unit** ([D17](plan.md#4-decisions)): `$unit` shared
  across files changes what a name resolves to.
- **`defparam`** needs a fixpoint over the instance tree; a limitation until
  the corpus shows one.
