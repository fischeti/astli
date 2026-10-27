# Semantics

> **Status:** tree rules (M8) and names in a definition (M9) done; elaboration
> (M11) in progress: constants, types, the instance tree and `check --top`
> done; width lints next.
> `astli lint` and `astli check` are its readers; milestones M8, M9 and M11 in
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

Constants are evaluated in a **body**: a package, a definition with its
parameters decided, or a file's top level. Until the instance tree exists,
a definition's body has its parameters at their defaults.

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
| S14 | A constant is evaluated in a body; a name another package or another file's top level declares, in that one's | A definition's parameters take a value per instance, a package's one for the whole design. |
| S15 | What sema cannot evaluate is `Value::Unknown`, which is not `x` | `x` is a value the standard computes, and a design may mean it, as a `'x` default; unknown is sema not seeing, which absorbs what is computed from it and is never reported ([S3](#decisions)). |
| S16 | Expressions are sized as the standard sizes them, and where it leaves room, as `slang` does: an unsized decimal is an `int`, truncated; an unsized based literal is as wide as its value needs; a parameter with neither type nor range set to `'1` is 32 bits | `slang` is what the values are held to ([oracles](#oracles)). |
| S17 | A constant function runs statement by statement in a frame of its own, under a budget of a million statements and 64 nested calls. One that writes outside itself, has an output argument, waits, forks, or reaches what is opaque is unknown | A loop that never ends is unknown rather than a hang. |
| S18 | Outside a package two files declare, its members have no value | As [S11](#decisions): which one a compiler keeps is a guess, and OpenTitan's per-top packages give one name different constants. |
| S19 | A type is its structure: a packed type its bits and how they are organised, an unpacked array its bounds and element, a struct its members. A typedef names the struct or enum it declares, and one renaming it names the same type; a type parameter is the type it names. A struct or enum keeps where it and its members are declared, so two of one shape declared apart differ | Selects, members, `$bits` and patterns read the structure; a name is what a message or a lint about a type should say, and a declaration what an editor goes to. The standard's struct and enum types match only if declared once. |
| S20 | An aggregate keeps the members and elements sema knows when it cannot know them all | A struct parameter with one member from an unknown package still gives every other member to what reads it. |
| S21 | A dynamic array, a queue, an associative array and an unpacked union have no constant value | None holds one in a design's constants; each is a test bench's. |
| S22 | An instance's body is its definition's with its overrides, evaluated where it stands; instances of one definition whose parameters all have the same values share one body, and one with no override shares the definition's defaults | What a body holds depends on its parameters alone, so it is built once however often it is instantiated. A definition sees nothing of where it is instantiated, so its body has no parent. |
| S23 | A generate block's body is built inside the one around it, and a loop's genvar has its value in the body of each iteration | A name in the block is found in the innermost body around it whose scope declares it, as the standard scopes it. |
| S24 | A generate construct whose condition, selector or bounds sema cannot evaluate is undecided: any arm may be built, or none. A condition with an `x` or `z` does not hold | As [S3](#decisions). A reader of the tree treats an undecided construct as it treats an opaque region. |
| S25 | An unnamed generate block is `genblk` and its construct's place among its scope's, counted from 1 | The standard's name, which a hierarchical path through it spells. |
| S26 | With tops, an instance in a generate block is judged by the tree: in a block some body builds, its errors are errors; in a definition the tops build but no body's block, it is not checked; under a construct sema cannot decide, or where no top reaches, it is as [S13](#decisions) has it | An error only where it is built, and silence only where it surely is not. Lifts S13 for what the tops build. |
| S27 | With tops, a variable two things drive in a body built is an error, `multiple-drivers`; each generate block built is a place of its own, so a loop writing all of a variable drives it once per iteration, and an arm not built drives nothing. In a definition no top reaches, or without tops, its drivers are compared at its defaults with every generate arm, only those that exist together clashing, and a clash is a warning | What is built is what the standard makes an error. A definition not elaborated may never be built, or with other parameters, as [S13](#decisions) has it for instances. |
| S28 | Below an instance of a module, interface or program two files declare, nothing is sure to be built: its blocks are undecided and its drivers not compared | As [S11](#decisions): which one a compiler keeps is a guess, and OpenTitan keeps a generic, a Xilinx and an UltraScale `prim_clock_buf`, one of which instantiates vendor primitives. |
| S29 | A width lint compares the widths a value may take without a word: a constant needs only the bits of its value, and takes any wider width but for a literal written with a size; `a + b` may take its carry and `a * b` its product's, all 64 bits where one is a `time` parameter; a shift left fills what it is taken at; a loop's variable is a constant in each iteration. What sema cannot size, or a constant it cannot evaluate, is silent | An unsized constant's 32 bits are the standard's, not the author's, and a target one bit wider than a sum is there for the carry; a `time` counts units, and `Period * n` is a time too; the rest is Verilator's `WIDTHEXPAND` and `WIDTHTRUNC`, which the corpus already waives against, and which unrolls a loop before it compares. |
| S30 | A member of an instance, a generate block, a definition named from the top, or an interface port or virtual interface resolves in that definition's or block's scope, and is unknown where it is not declared there. A member of a class handle, or of a name sema cannot see, is unknown; a struct's member is no symbol | A hierarchical name is then a use of the signal it names, from any file, and a struct's member stops keeping every signal so named in use. A generate block the HIR does not name may hold a member, so a missing one is not an error. |
| S31 | An access to a part of a variable reaches the bits its select evaluates to, in what `--top` builds with each body's parameters and elsewhere at the definition's defaults. Two writes clash where their bits overlap; bits read, or all of an output, that no write reaches are undriven. A select sema cannot evaluate may reach any bit: it clashes only with a write to all of the variable, and its variable's bits are not compared | `multiple-drivers` sees two blocks writing one bit, and `undriven-signal` a vector written in part; a genvar's select is evaluated per iteration where it is built, and silent at the defaults, as [S3](#decisions) has it. |
| S32 | `multiple-drivers` is `check`'s alone; `astli lint` has no such rule. A lint rule reads `Elaboration`'s public queries, so what a width rule tolerates ([S29](#decisions)) lives in `astli-lint` | A clash is an error the standard defines, and reported by two commands it was reported twice, waivable in one and not the other. A rule's tolerance is policy, not meaning. |
| S33 | An elaboration owns its design, and threads share it by taking turns at its queries, behind one lock | A query in progress is a cycle only to the thread running it; queries run in parallel would need cycles across threads detected, and a design elaborates in a fraction of a second. |
| S34 | Sema may leave unknown only what the files it reads do not decide: a header not found ([S12](#decisions)), a package or definition two files declare ([S11](#decisions), [S18](#decisions), [S28](#decisions)), an encrypted envelope ([D21](plan.md#4-decisions)), a region the parser rejected ([D22](plan.md#4-decisions)), what depends on where a definition is instantiated in one no top builds, and what runs past a budget that keeps sema from hanging ([S17](#decisions), a generate loop's). Anything else unknown is a construct sema does not model yet | Each of these is reported where it stands, left to a compiler's choice, or decided only by the tops, and [S3](#decisions) keeps one cause from becoming many errors. Anywhere else a compiler reading the same files knows the value, so an unknown hides errors it reports. |
| S35 | A `bind` at a file's top level adds its instances to every instance of its target; one a definition holds, only once the tops reach that definition, by instantiating it in any generate arm or by a `bind` that applies. Without tops every `bind` applies. Its overrides and connections are names in the target, and one the target does not declare is unknown. One into chosen instances, by a list or a path, is not in the tree | As a compiler elaborates it: OpenTitan holds its assertions' `bind`s in a module it names as a top of its own. One instance's body is its siblings' too, so it cannot have what they lack. A name the target lacks may be a net it declares by using it. |
| S36 | A gate or a switch is an instance of its keyword, a leaf of the tree. An instance of one or of a UDP may be unnamed: no name finds it and the tree has no place for it, but what it connects is read and driven | As `slang`'s tree has them, a path for each named one. |
| S37 | A concurrent or deferred assertion among items is a process of its keyword, its label a name for it; in a procedural block, a statement. A property or a sequence declares its formals, inputs unless written otherwise, and its local variables in a scope of its own, and an instance of it is a call. Its operators build expressions with no value: the names in them resolve, but nothing in them is typed, sized or evaluated | An assertion in a module runs as an `always` block does. Its names are uses: a signal only an assertion reads is read, a name it reads that nothing declares is undeclared, and a module holding one is no longer opaque ([S3](#decisions)). What a sequence matches decides no constant, width or driver. |
| S38 | A modport lists what its users read and drive, each a name or an expression in the interface; a clocking block, the signals it samples and drives on its event. A clocking block reads its inputs; a modport's entries and a clocking block's outputs are accesses by whoever uses them, not certain. `cb.name` is what the block names it for, where the block stands before it in the same file, and unknown elsewhere | A clocking block samples its inputs on every event, but drives an output only when a procedure drives it through the block, and nothing uses a modport but a port that names it. A drive through `cb` is then a write of the signal, as a compiler has it. |
| S39 | A covergroup declares its formals and `sample`'s arguments in a scope of its own, and its cover points and crosses in one within that: each by its label, or a cover point of a variable alone by the variable. What a cover point samples is a name outside the labels; what a cross crosses, and what a bin takes, a name among them. A cover point is a member of its covergroup's instances; a bin is no name. `with (…)` declares its iterator in a scope of its own: `item`, or the one name an array method is passed. Nothing in a covergroup is typed, sized or evaluated | `a: coverpoint a` is common and samples the variable, and `cross a, b` crosses the cover points. Its names are uses, as an assertion's are ([S37](#decisions)): a signal only a covergroup samples is read, and an interface of covergroups is no longer opaque ([S3](#decisions)). |
| S40 | The constraints of `randomize() with {…}` are a block of statements where the call stands: each constraint an expression it reads, an implication an `if` with no `else`, `if` and `foreach` as in a procedure. An object's, `c.randomize() with {…}`, is opaque. A `let` declares its formals, inputs unless written otherwise, in a scope of its own, and a use of it is a call | A constraint's names are uses, as an assertion's are ([S37](#decisions)). An object's constraints name its members first, which a class body does not declare ([S9](#decisions)), so any of them may be one. |
| S41 | A time literal is a `realtime` in the time unit of the design element it is written in, rounded to its precision. An element's time unit and precision are what it declares with `timeunit` and `timeprecision`, else its enclosing element's, else those of the last `` `timescale `` before it in its file, else those its file's top level declares, else 1ns/1ns; the top level's, what it declares, else 1ns/1ns | As the standard has it within one compilation unit, and each file is one ([`$unit` is the file's own](limitations.md#unit-is-the-files-own)): a `` `timescale `` carried across files would make a value depend on the order astli finds them in, which is not a compiler's. 1ns/1ns is `slang`'s default, which the constants are held to ([oracles](#oracles)). |
| S42 | A rule's options are its own, declared beside it in `RULES` with their defaults, lowRISC's; a naming rule takes the cases a name may be in and patterns around it, each with one `*` for the rest of the name, which is never empty, and the case holds of what `*` stands for. `astli.toml` sets them in `[lint.rules.<rule>]`, for the whole project, not per path; an option set replaces its default whole | Teams name things differently, `i_` against `_i`, and a rule they can configure is one they keep rather than allow; a rule per variant multiplies the table and still misses the third way. A pattern and a case say every style in the corpus without a regex engine, and a regex waits for one they cannot. A vendored file wants `allow`, not another style. |

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
  Resolving hierarchical names ([S30](#decisions)) adds five there, each a
  signal a register struct's member of the same name hid, as `i3c`'s
  `read_stalled`; over the corpus `unused-signal` falls from 972 to 869
  hits, as a DPI import's arguments are no longer reported. Comparing
  selects ([S31](#decisions)) adds four bits undriven, among them a
  commented-out `led[0]` and a trace struct's `tag`, and no driver clash.
  Lowering concurrent assertions ([S37](#decisions)) adds 49
  `unused-signal`, 21 `undriven-signal` and 8 `unused-parameter` hits, each
  one checked hidden only because an assertion spelled its name: a
  property's local variable assigned and never read, an output no
  assignment drives, and the free inputs of formal testbenches, which they
  leave undriven on purpose. Lowering modports and clocking blocks
  ([S38](#decisions)) adds 8 `undriven-signal` and 5 `unused-signal` hits,
  among them interface signals a clocking block samples and no testbench
  drives, as ibex's `rvfi_if.halt`. Lowering covergroups and `with`
  ([S39](#decisions)) adds 2 `unused-signal`, 2 `undriven-signal` and 2
  `unused-import` hits, each one checked hidden only because a covergroup
  spelled its name: inputs nothing reads, a task's output it never drives,
  and imports whose members are only ever named as `pkg::name`. Lowering
  constraints, `let` and deferred assertions ([S40](#decisions)) adds no
  hit.
- **Resolution:** each corpus repository resolves as one design with 419
  names undeclared and 23,623 unknown, a ratchet in
  `astli-sema/tests/gates.rs`; the undeclared are stale testbenches, code a
  missing define makes whole, and uses of repositories the corpus lacks.
  Outside class bodies, 5.4% of names land in an opaque region.
- **Check:** `scripts/check-oracle.py` builds `axi`, `cheshire`,
  `common_cells`, `FlooNoC` and `snitch_cluster` as their own simulation
  flows have bender list them, and holds each error `astli check` reports
  without a top, and each `not-parsed`, to what `slang` reports on its
  line. Of 93, none `not-parsed` and none false, 42 are slang's errors too
  and 51 its warnings, each an override of a parameter the definition
  lacks, which the standard does not allow. With `--top` from every
  top `astli files` finds in each corpus repository, the errors it adds are
  definitions missing in blocks that are built, dependencies the corpus
  lacks, and 14 `multiple-drivers`, each of which `slang` reports where it
  elaborates the top: two instances driving one testbench output, an
  `initial` beside an `always_ff`. On sv-tests, `scripts/sv-tests.py`
  runs the tests only elaboration can fail through `check --top`:
  none of the 4 valid is rejected, and 4 of the 54 invalid are, all by
  `slang` too. The corpus gates skip what `bender checkout` puts in a
  corpus repository, `.bender` and the links into it, so the checkouts the
  oracle needs can stay.

- **Widths:** the corpus report builds each repository from every module
  nothing instantiates, at its defaults, and counts `width-expand` and
  `width-trunc` with Verilator's waivers: 160 and 68 hits, of which 25 and
  1 stand under a `WIDTH` waiver. In OpenTitan's IP RTL, which Verilator
  lints, 31 are not waived; each checked is a width that differs, most in
  `i3c`, which is newer than its lint runs, and in code under a default no
  top builds, as `sram_ctrl`'s single RAM instance against the 32
  `prim_ram_1p_scr` computes from its depth. Before
  [S29](#decisions) counted a constant by its value, 834 hits, most of them
  `count + 1` and `Width - 1`.

- **Constants:** `scripts/consts-oracle.py` compares the value of each
  parameter and enum member with `slang`'s, through `pyslang`, each
  repository one design at its defaults. Of the 28,413 values `slang`
  computes over the corpus, 28,015 agree, none differ, and astli leaves 398
  unknown: members of packages two files declare, macros only a build
  defines, and what is computed from them. Time literals ([S41](#decisions))
  were 217 more. An aggregate astli knows only in part counts as unknown.
  Of all 50,972 constants the corpus declares, 47,343 evaluate. Measured at
  `axi` `70b8e54fd4`, `cheshire` `6c6dd5e834`, `common_cells`
  `e73baaec2c`, `cva6` `742dc82a0c`, `FlooNoC` `c48b645607`, `iDMA`
  `a6b1d6657c`, `ibex` `a896ef2277`, `opentitan` `478afb033b`,
  `snitch_cluster` `8142c0f2da`.

- **Tree:** `scripts/tree-oracle.py` compares the instance tree `astli
  elab` prints from a top with `slang`'s, path by path: each instance with
  its definition, a gate's with its keyword, each generate block built.
  From every top `astli files` finds in `axi`, `iDMA`, `FlooNoC`,
  `cheshire` and `snitch_cluster`, and from nine of OpenTitan's, every path
  both build agrees, 228k of them, and neither has one the other lacks but
  where astli leaves a construct undecided. So do OpenTitan's `bind`s, with
  and without the module holding them named as a top. What else differs
  elsewhere is a generate block numbered after a region left as written,
  and `cva6`'s `find_first_one` at its default `WIDTH = -1`, whose loops
  slang gives up on. The corpus gate walks every top's tree, and each
  definition it reaches is in a file `files --top` keeps.

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
