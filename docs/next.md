# M11 queue

Elaboration: what a design is once its parameters are known. Constants,
then types, then the instance tree from `--top`, each the layer the next
evaluates against ([`sema.md`](sema.md#layers)). Readers are `astli check
--top` and the width lints. Taken before discovery (M10), which it does
not need: `check --top` reads a filelist as `check` does. What sema cannot
evaluate is unknown and silent ([S3](sema.md#decisions)), as for names.

- [x] **Values:** `astli_sema::Value`, an integral value of any width with
  each bit 0, 1, `x` or `z`, a real or a string, and an unknown that
  absorbs what is computed from it ([S15](sema.md#decisions)). The
  operators of the standard's chapter 11 on them, `x` and `z` included;
  literals as written, sized, based, unbased and unsized.
- [x] **Constant evaluation:** `Elaboration`, a query per parameter,
  memoised, with a mark for cycles ([S4](sema.md#decisions)), over a body:
  a package, a definition with its parameters' defaults, a file's top
  level ([S14](sema.md#decisions)). Expressions sized as the standard
  sizes them ([S16](sema.md#decisions)). Parameters, `localparam`s, enum
  members (a name with a range now declares one per index), package
  members, `$clog2`, `$bits`, `$signed`, `$size` and its kind, the real
  math functions, casts to a width, a signing or a type, and constant
  functions, interpreted under a budget ([S17](sema.md#decisions)).
  Integral types are those written with keywords, packed dimensions,
  typedefs of them, enums and packed structs. Against `slang`, through
  `scripts/consts-oracle.py`: 27,340 of the 28,140 values it computes over
  the corpus agree and none differ ([oracles](sema.md#oracles)).
- [x] **Types:** a type for each declaration and expression
  ([S19](sema.md#decisions)): packed vectors, arrays, structs and unions by
  member, enums by name, unpacked arrays and structs, type parameters;
  `$bits` of any of them and `$left` and its kind by dimension; selects
  through every dimension and members through structs; assignment
  patterns by position, member, index, type and default, replicated, and
  a concatenation assigned to an unpacked array. `Elaboration::expr_type`
  gives an expression's type by itself. Against `slang`: 27,759 of the
  28,377 values it computes agree and none differ; 46,835 of the corpus's
  50,781 constants evaluate, up from 45,964.
- [ ] **The instance tree:** from the tops named, each instance's
  overrides evaluated where it stands, one body per definition and
  parameter values, generate `if` and `case` decided and loops unrolled,
  instance arrays. A dump to see it, and `astli files --top` agreeing with
  it on which definitions are needed.
- [ ] **`astli check --top`:** what it elaborates, reported as the errors
  they are there: a definition missing in a branch that is built, where
  [S13](sema.md#decisions) warns or says nothing; a connection or override
  the definition does not take; `multiple-drivers` in what is built.
- [ ] **Width lints:** an assignment, connection or comparison whose
  operands differ in width where the standard extends or truncates
  silently; verible's and Verilator's names where they have the rule.
- [ ] **What elaboration resolves:** hierarchical names through the
  instance tree, members through types, selects compared for
  `multiple-drivers` and `undriven-signal`. Each closes an entry in
  [`limitations.md`](limitations.md#semantics) that waits on M11.
