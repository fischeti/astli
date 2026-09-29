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
- [x] **`lowrisc` is on by default,** the guide's rules all in it.
  OpenTitan's design code is all but clean under verible's rules of the same
  names; `port-name-suffix` and `signal-name-style`, which verible has off,
  it breaks 606 and 316 times (`reg2hw`, pads named for their pins), which a
  project waives by path.
- [x] **`astli-lint`:** a static slice of `Rule { name, group, check }`, each
  `check` a function over the tree; `lint(&tree, &Config) -> Vec<Diagnostic>`.
- [x] **`astli lint`** in the CLI: files or `-f`, parallel per file,
  `-A`/`-W`/`-D <rule|group>`, exit 1 on a denied rule, `--list`.
- [x] **User docs:** `site/content/linting.md`, and a README section.
- [x] **Waivers are attributes:** `(* astli_allow = "rule, group" *)` on an
  item or statement covers that node; an unknown name or a value that is not
  a string is an `invalid-waiver` warning.
- [x] **`astli.toml`**, for what no attribute reaches: a header of lowercase
  macros, or vendored code nobody edits. `[lint]` takes `allow`, `warn` and
  `deny`; `[lint.paths]` the same per glob, relative to the file. The file,
  then the flags, then the paths. One per run, found from the current
  directory up, or `--config`. No comment waivers.
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
- [x] **`restriction`,** verible's rules that take no options: `explicit-begin`,
  `endif-comment`, `legacy-generate-region`, `legacy-genvar-declaration`,
  `one-module-per-file`, `proper-parameter-declaration` (no `parameter` in a
  package, as verible's default), `forbid-negative-array-dim`,
  `invalid-system-task-function`, `uvm-macro-semicolon`; and
  `forbid-defparam` in `suspicious`, found by its keyword in unparsed code.
- [ ] **Left out of `restriction`:** `forbidden-macro` and
  `banned-declared-name-patterns` need a list per project, which `astli.toml`
  has no place for yet; `disable-statement` needs the label resolved;
  `macro-string-concatenation` reads macro bodies. `mismatched-labels` is a
  compile error, so `correctness` if anywhere.
- [x] **A bare macro call among `case` items** made the parser keep the
  whole module as written. One followed by no `:`, `,` or operator now
  stands for whole items; three corpus files format better for it.
- [x] **A generate block labelled before its `begin`**, `if (P) gen_a :
  begin`, is parsed, and the label rules read either place. No corpus use.
- [x] **Names, as verible checks them:** `parameter-name-style` (both kinds
  `CamelCase` or `ALL_CAPS`, as OpenTitan configures it), `macro-name-style`,
  `enum-name-style`, `struct-union-name-style`, `interface-name-style`,
  `constraint-name-style`. Styles are written by hand, with no regex engine.
  No unwaived hit in OpenTitan's design code but vendored PULP debug.
- [x] **Instances and generate blocks, as verible checks them:** `module-port`,
  `module-parameter`, `generate-label`, `generate-label-prefix`,
  `v2001-generate-begin`, `module-begin-block`. OpenTitan's hits are all in
  generated testbench code.
- [x] **verible's other default rules for lowRISC:** `explicit-function-lifetime`,
  `explicit-task-lifetime`, `explicit-function-task-parameter-type`,
  `explicit-parameter-storage-type` (a `string` exempt, as OpenTitan sets it),
  `typedef-enums`, `packed-dimensions-range-ordering`,
  `unpacked-dimensions-range-ordering`, `positive-meaning-parameter-name`,
  `module-filename`, `package-filename`. One hit in OpenTitan's design
  code, in a file no build lists.
- [x] **lowRISC's own:** `forbid-reg`, `forbid-wildcard-connection` (`.*`),
  `instance-name-style`; and verible's `port-name-suffix` and
  `signal-name-style`. The guide's ban on a signal ending
  in `_` and a number is not in `signal-name-style`, which is verible's.
- [x] **Raw-tree traps:** a macro call can hide what a rule looks for (a
  `default` item written by a macro), so a rule says nothing about a node
  that holds one; every conditional branch is in the tree, so a rule that
  counts must not count across exclusive branches.
