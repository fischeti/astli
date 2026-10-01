# M9 queue

Names inside a definition: a HIR lowered from each file's expanded tree,
scopes over it, the lints that need names, and `astli check` for what the
standard makes an error. Parameters stay symbolic, and nothing is elaborated
(M10). What sema cannot see is unknown and silent ([S3](sema.md#decisions)).

- [x] **`astli lint` takes a build** (`-f`, `+incdir+`, `+define+`). Tree
  rules still read each file's raw tree; definition rules read its expanded
  tree, with packages from the other files named.
- [x] **Classes are opaque.** A class is a declared type and its body is not
  lowered.
- [x] **A declaration a macro wrote gets no name lint**: its user can fix it
  only in the macro. An error `check` reports is placed at the outermost call,
  once per span.
- [x] **Rule names per kind:** `unused-signal`, `unused-parameter`,
  `unused-import`, `undriven-signal`, `multiple-drivers`, and verible's
  `disable-statement`.
- [x] **`astli-sema`, the HIR:** lowered per file from an expanded `Parsed`,
  `Send`, each id carrying its `Span`, so files lower in parallel
  ([D13](plan.md#4-decisions)). Per definition: ports (ANSI and not, one
  list), parameters, nets and variables, typedefs and enum members,
  functions and tasks, generate blocks as scopes (the arms of one `if` or
  `case` marked exclusive), processes, statements, and expressions down to
  the names they read and write. A region not lowered (a `VERBATIM`, a class
  body, a construct not modelled yet) keeps the identifiers it spells, and
  each counts as a use. Over the corpus, 17% of the names outside class
  bodies land in an opaque region: two thirds in what the parser kept as
  written, mostly classes, and the rest in modports, properties,
  covergroups and the like.
- [x] **Scopes:** local, enclosing, imports by name, then by wildcard,
  then definitions; `pkg::name` in the package and what it exports.
  Unknown rather than undeclared: what a wildcard import of a package no
  file declares, or two do, may supply; what a scope holding an opaque
  region or an `` `include `` not followed may declare; a name up the
  instance tree (the head of `a.b`, a task called by name); a pattern key.
  Implicit nets where `` `default_nettype `` allows them. Only the head of a
  dotted name resolves.
- [x] **Corpus gate:** each repository one design, a file another includes
  not a unit. A repository is not one design that compiles, so 419 names
  are undeclared: stale testbenches, code a missing define makes whole, uses
  of repositories the corpus lacks. They and the 21,631 unknown are a
  ratchet.
- [x] **`astli check`:** a filelist and a build, as `files` takes. Errors:
  an undeclared name; an unknown module, interface, program or package; a
  name a package lacks; a named port or parameter the definition lacks,
  more positional ones than it has, one given twice, a `localparam`
  overridden. Nothing it reports on five `bender` designs is an error
  `slang` lacks, and on sv-tests it rejects no valid test
  ([oracles](sema.md#oracles)). Its page in `site/`.
- [x] **`paste-without-operand` is a warning**, the standard defining the
  operator only between two tokens. One that opens or closes a macro
  argument is dropped silently: substituted, it stands beside a delimiter,
  white space or another paste. FlooNoC's macros now expand clean.
- [x] **Definition rules in `astli-lint`:** a rule reads a tree or a
  design, and `lint` lowers nothing unless a design rule is on. Both are
  waived by the raw tree's attributes ([S8](sema.md#decisions)).
  `astli_sema::accesses` says what each name use reads and writes, and
  what drives each write.
- [x] **`unused-signal`, `unused-parameter`, `unused-import`:** declared in a
  module, interface or program and never read. A package's members are its
  API and exempt, as is a name containing `unused`, Verilator's convention
  (568 `unused_` signals in OpenTitan), and one another file spells where
  sema cannot see, or any file names as a member. On `cheshire`, 384, 103
  and 33; those sampled are right, and slang, warning only on what it
  elaborates, has 121, 62 and 3 of them.
- [x] **`undriven-signal`:** a net or variable read and never written, or
  an output never driven. A `supply` or `tri0`/`tri1` net drives itself; a
  `foreach` writes its variables. A connection to a port without a
  direction, an unknown callee's argument and a name sema cannot see into
  count as writes, as does any name a file reaches as a member. On
  `cheshire`, 14: among them iDMA's 32-bit descriptor reader, whose two
  registers no `` `FF `` updates, which slang misses for elaborating only the
  64-bit branch; slang's 71 others are writes in loops and macros it does
  not credit with its default parameters.
- [ ] **`multiple-drivers`:** a variable written by two processes, or by a
  continuous assignment and anything else. Only writes to the whole name, or
  to one literal index, collide until M10 evaluates selects; exclusive
  generate arms never do. The standard makes it an error; decide whether
  `check` reports it rather than the lint.
- [ ] **`disable-statement`:** verible's rule, reading what the label
  resolves to.
- [ ] **Corpus report** extended to definition rules. Oracle: each hit on
  OpenTitan is one Verilator's `UNUSED`, `UNDRIVEN` or `MULTIDRIVEN` also
  reports.
- [ ] **Limitations:** opaque classes, silence in macros, selects not
  compared, members and hierarchical names unresolved, declaration order
  not checked.
