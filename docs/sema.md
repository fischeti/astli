# Semantics

> **Status:** tree rules (M8) and names in a definition (M9) done; next,
> elaboration (M10).
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
| S8 | `astli lint` takes a build; tree rules read the raw tree, definition rules the expanded one; both are waived by the raw tree's attributes, matched by where a finding is reported | One command and one set of waivers. What is written in the file is in the raw tree, which `lint` reads anyway. |
| S9 | A class is a declared type whose body is not lowered | UVM is classes; a checker that fails on it is worse than none ([S5](#decisions)). Revisit when a rule needs members. |
| S10 | A declaration a macro wrote gets no name lint; an error is placed at the outermost call | Its user can fix it only in the macro, often a library's. |
| S11 | A package two files declare lends its members to nothing: a name the kept one lacks is unknown | Which one a compiler keeps depends on order and tool; OpenTitan has one `top_racl_pkg` per top. |
| S12 | An `` `include `` expansion did not find is kept as trivia, and makes the scope it stands in opaque | The header may declare anything; a missing generated header otherwise turns into dozens of undeclared names. |
| S13 | An instance in a generate construct needs no definition, and what its connections get wrong is a warning | Nothing is elaborated, so no branch is known to be built; configurable designs leave untaken branches' modules out of the filelist, and slang, elaborating, reports neither. |

## Oracles

- **Lint:** OpenTitan runs verible's lint with lowRISC rules in CI, so a rule
  both tools have should fire on OpenTitan RTL only where a
  `verilog_lint: waive` comment stands; and Verilator's, so a design rule
  should fire only where its Verilator warning is waived. The corpus report
  (`astli-lint/examples/lint-report.rs`) counts both. In OpenTitan's IP RTL,
  16 of 17 `unused-parameter` hits carry a Verilator waiver. Of the other
  57, every one checked is real: unused and undriven signals in IPs
  Verilator does not lint yet, unused imports, which Verilator has no
  warning for, and two driver clashes in code no configuration builds.
- **Resolution:** each corpus repository resolves as one design with 419
  names undeclared and 21,631 unknown, a ratchet in
  `astli-sema/tests/gates.rs`; the undeclared are stale testbenches, code a
  missing define makes whole, and uses of repositories the corpus lacks.
  Outside class bodies, 17% of names land in an opaque region.
- **Check:** over the `bender` filelists of `cheshire`, `FlooNoC`,
  `snitch_cluster`, `axi` and `common_cells`, each error `astli check`
  reports is one `slang` reports too, without a top: 57 of 63. The rest of
  slang's lie in generate branches, or need types. On sv-tests,
  `scripts/sv-tests.py --check` runs the tests only elaboration can fail:
  none of the 4 valid is rejected, and 2 of the 54 invalid are, both by `slang`
  too. `bender checkout` links dependencies into a corpus repository, where
  the corpus gate reads them; remove them afterwards.

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
