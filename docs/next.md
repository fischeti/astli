# M7 queue

`cargo build --release && scripts/sv-tests.py --all` lists what fails; 923 of
965 pass. A test passes when astli reports an error or warning exactly when
it should fail, so each fix also checks the `_inv` tests beside it stay
rejected. After `UPDATE_EXPECT=1`, read every changed snapshot, not only the
new ones: `matches` once swallowed a cross bin's count unnoticed.

- [x] **Silent skips in a header.** What a header's or a dimension's rule
  leaves before its `)` or `]` is now `VERBATIM`, reported and counted: 146
  corpus tokens, and three tests that passed without parsing.
- [x] **A `for` declaring several variables**, each with its own type:
  `for (int i = 0, state_e s = s.first(); …)`, `var` allowed.
- [x] **A long `for` header breaks at its `;`s**, not inside a clause.
- [ ] **`#(min:typ:max)` as a delay**, `#(100:200:300) stmt` (§11.11).
- [ ] **`randsequence`** (§18.17), 12 tests.
- [ ] **Preprocessor checks**, 9 invalid tests accepted: `` `line `` operands
  (5), `` `pragma `` without a name, `` `resetall `` inside a design element,
  redefining a directive, a string split across macro text (§22).
- [ ] **Intra-assignment `repeat`**: `a = repeat(3) @(posedge clk) b;`, 4
  tests (§9.4.5).
- [ ] **`interface class` extending two or more**, 2 tests (§8.26.6).
- [ ] **`nettype`** (2), **`interconnect`**, **`specparam`** outside
  `specify` (§6).
- [ ] **One each:** `let` (§11.12); `with [...]` inside a stream
  (§11.4.14.4); a type as an assignment pattern key, `'{int: 1}` (§5.10);
  `type(...)` as a parameter's default (§6.23); `new obj` shallow copy
  (§8.12); `-'d8` literal syntax (§5.7.1, accepted though invalid).
- [ ] **Look before fixing:** `generic/member/class_member_test_14.sv` puts
  `input a;` in a class.
- [ ] **Known limitations, not failures to chase:** `` `begin_keywords ``
  (only the 2023 keyword set), an `` `include `` name built by a macro, and
  `` `SV_COV_CHECK ``, which §20.14 has tools predefine.
