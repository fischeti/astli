# M8 queue

`astli lint`: tree rules over one raw tree per file, no filelist, as `fmt`
([S1](sema.md#decisions)). Rules that need names wait for M9. Each rule gets
`tests/data/<rule>/*.sv` cases with a `.lint` snapshot, and a count over the
corpus before it lands: OpenTitan RTL should hit only where a
`verilog_lint: waive` stands ([oracles](sema.md#oracles)).

- [ ] **Decide first:** rule names (verible's where the rule is the same, so
  existing waivers and habits carry over, or our own); whether to honour
  `verilog_lint: waive`; which groups are on by default.
- [ ] **`astli-lint`:** a static slice of `Rule { name, group, check }`, each
  `check` a function over the tree; `lint(&tree, &Config) -> Vec<Diagnostic>`.
  The first reader of the `ast` views ([D16](plan.md#4-decisions)); fix a
  view where it gets in the way.
- [ ] **`astli lint`** in the CLI: files or `-f`, parallel per file,
  `-A`/`-W`/`-D <rule|group>`, exit 1 on a denied rule, `--list`. Its page in
  `site/`.
- [ ] **Waivers:** `// astli: allow(rule)` on the line or the one before.
- [ ] **Corpus report:** an example counting hits per rule, as `unformatted`
  does for the formatter.
- [ ] **Correctness rules:** blocking assignment in `always_ff` (a variable
  declared in the block excepted); non-blocking in `always_comb`; `case`
  without `default`; plain `always`; a duplicated constant `case` item.
- [ ] **lowRISC rules:** naming (`lower_snake_case`, `UpperCamelCase`
  parameters, `CamelCase` or `ALL_CAPS` localparams, `ALL_CAPS` macros, `_e`
  and `_t` type suffixes, `_i`/`_o`/`_io` ports, `clk`/`rst_n` prefixes);
  `logic` over `reg` and `wire`; `.*` or positional connections; a parameter
  without a type; a floating `begin`/`end`; an unsized literal where a width
  is known from the syntax.
- [ ] **Raw-tree traps:** a macro call can hide what a rule looks for (a
  `default` item written by a macro), so a rule says nothing about a node
  that holds one; every conditional branch is in the tree, so a rule that
  counts must not count across exclusive branches.
