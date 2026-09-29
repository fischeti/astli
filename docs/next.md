# M8 queue

`astli lint`: tree rules over one raw tree per file, no filelist, as `fmt`
([S1](sema.md#decisions)). Rules that need names wait for M9. Each rule gets
`tests/data/<rule>/*.sv` cases with a `.lint` snapshot, and a count over the
corpus before it lands: OpenTitan RTL should hit only where a
`verilog_lint: waive` stands ([oracles](sema.md#oracles)).

- [x] **Rule names are verible's** where the rule is the same, so habits and
  existing waivers carry over.
- [x] **Groups:** `correctness` and `suspicious` on; `lowrisc` (the style
  guide, as OpenTitan configures verible) likely on; `restriction` off;
  verible's `line-length`, `no-tabs`, `no-trailing-spaces` and `posix-eof`
  dropped, since `astli fmt --check` owns them.
- [ ] **Decide from the corpus report:** `lowrisc` on by default, and moving
  to `restriction` any `lowrisc` rule OpenTitan and PULP code break often.
- [x] **`astli-lint`:** a static slice of `Rule { name, group, check }`, each
  `check` a function over the tree; `lint(&tree, &Config) -> Vec<Diagnostic>`.
- [x] **`astli lint`** in the CLI: files or `-f`, parallel per file,
  `-A`/`-W`/`-D <rule|group>`, exit 1 on a denied rule, `--list`.
- [ ] **User docs**, a `site/` page and a README section, once waivers exist.
- [x] **Waivers are attributes:** `(* astli_allow = "rule, group" *)` on an
  item or statement covers that node; an unknown name or a value that is not
  a string is an `invalid-waiver` warning.
- [ ] **A comment waiver** where an attribute cannot stand, which in the
  corpus is a `` `define ``'s name; with the first rule that reads one.
- [ ] **Per-path levels in a config file**, for what no file can say about
  itself: a header of lowercase macros, or vendored code nobody edits.
  `ruff`'s `per-file-ignores` is the model; the same file holds the groups,
  so `-W lowrisc` need not be repeated. After the rules, since the flags
  cover until then.
- [x] **Corpus report:** `cargo run --release -p astli-lint --example
  lint-report [-- <rule>]`, hits per rule with every rule on, and how many a
  verible waiver for the same rule covers, in a comment or a `.vbl` file.
- [x] **`always-ff-non-blocking`** and **`always-comb-blocking`.** Unlike
  verible's defaults, a write to a local passes and an increment does not:
  `axi` needs 9 waivers for locals, and OpenTitan's SVA counters `x++` race.
  75 hits in deduplicated files, 20 under a verible waiver; the rest are
  testbenches, SVA counters, and FPGA RAM models written with `=`.
- [x] **`duplicate-case-item`** (correctness), **`case-missing-default`** and
  **`always-comb`** (suspicious, verible's: `unique` and `unique0` need no
  `default`, and only `always @*` is flagged). Labels compare as written,
  spacing aside. No hit in OpenTitan's design code that verible does not
  waive; riscv-dv lists `MULH` to `REMU` twice.
- [ ] **A bare macro call among `case` items** makes the parser keep the
  whole module as written, so no rule sees it.
- [x] **Names, as verible checks them:** `parameter-name-style` (both kinds
  `CamelCase` or `ALL_CAPS`, as OpenTitan configures it), `macro-name-style`,
  `enum-name-style`, `struct-union-name-style`, `interface-name-style`,
  `constraint-name-style`. Styles are written by hand, with no regex engine.
  No unwaived hit in OpenTitan's design code but vendored PULP debug.
- [ ] **lowRISC rules:** naming verible leaves off (signals `lower_snake_case`,
  `_i`/`_o`/`_io` ports, `clk`/`rst_n` prefixes, `parameter type` `_t`);
  `logic` over `reg` and `wire`; `.*` or positional connections; a parameter
  without a type; a floating `begin`/`end`; an unsized literal where a width
  is known from the syntax.
- [ ] **Raw-tree traps:** a macro call can hide what a rule looks for (a
  `default` item written by a macro), so a rule says nothing about a node
  that holds one; every conditional branch is in the tree, so a rule that
  counts must not count across exclusive branches.
