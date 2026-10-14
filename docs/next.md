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
- [x] **The instance tree:** from the tops named, each instance's
  overrides evaluated where it stands, one body per definition and
  parameter values ([S22](sema.md#decisions)), generate `if` and `case`
  decided and loops unrolled ([S23](sema.md#decisions)), what cannot be
  decided marked so ([S24](sema.md#decisions)), instance arrays. `astli
  elab --top` prints it. Against slang, through `scripts/tree-oracle.py`,
  228k paths agree and none differ but for `bind` and a degenerate default
  ([oracles](sema.md#oracles)); every definition a top's tree reaches is in
  a file `astli files --top` keeps.
- [x] **`astli check --top`:** what it elaborates, reported as the errors
  they are there ([S26](sema.md#decisions)): a definition missing in a
  branch that is built, where [S13](sema.md#decisions) warns or says
  nothing; a connection or override the definition does not take;
  `multiple-drivers` in what is built, each loop iteration a place of its
  own ([S27](sema.md#decisions)). Nothing is sure below a definition two
  files declare ([S28](sema.md#decisions)). Every `multiple-drivers` it
  finds in the corpus slang reports too; on sv-tests it rejects 4 of the 54
  invalid tests only elaboration can fail, from 2, and no valid one.
- [x] **Width lints:** `width-expand` and `width-trunc`, Verilator's
  `WIDTHEXPAND` and `WIDTHTRUNC` (verible has neither), with `astli lint
  --top`: an assignment, connection or comparison whose widths differ
  where the standard extends or truncates without a word, once however
  many bodies build it. A constant counts the bits of its value, a sum its
  carry, a loop variable each iteration's value ([S29](sema.md#decisions)).
  Over the corpus, 160 and 68 hits, 26 under a Verilator waiver; each
  unwaived one checked in OpenTitan's IPs is real ([oracles](sema.md#oracles)).
- [ ] **What elaboration resolves:** hierarchical names through the
  instance tree, members through types, selects compared for
  `multiple-drivers` and `undriven-signal`. Each closes an entry in
  [`limitations.md`](limitations.md#semantics) that waits on M11.
