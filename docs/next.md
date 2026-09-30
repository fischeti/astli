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
- [ ] **`astli-sema`, the HIR:** lowered per file from an expanded `Parsed`,
  `Send`, each id carrying its `Span`, so files lower in parallel
  ([D13](plan.md#4-decisions)). Per definition: ports (ANSI and not, one
  list), parameters, nets and variables, typedefs and enum members,
  functions and tasks, generate blocks as scopes (the arms of one `if` or
  `case` marked exclusive), processes, statements, and expressions down to
  the names they read and write. A region not lowered (a `VERBATIM`, a class
  body, a construct not modelled yet) keeps the identifiers it spells, and
  each counts as a use. The first large reader of the `ast` views; fix a view
  where it gets in the way.
- [ ] **Scopes:** local, enclosing, explicit imports, wildcard imports
  (lazily), `$unit`, then definitions; packages from every file named. A
  wildcard import of a package no file declares, or a scope holding an
  unlowered region, makes an unresolved name unknown rather than undeclared.
  Implicit nets as `` `default_nettype `` allows, read from the file's own
  directives.
- [ ] **Corpus gate:** every corpus file lowers without a panic, and
  resolution finds no undeclared name, since the corpus compiles; a ratchet
  on how many names end unknown.
- [ ] **`astli check`:** a filelist and a build, as `files` takes. Errors: an
  unknown module, interface, program or package; an undeclared name; a named
  port or parameter the definition lacks, more positional ones than it has,
  one connected twice. Its page in `site/`. Oracle: nothing on the corpus,
  whose filelists come from `bender script flist` for the PULP repos; each
  error on sv-tests one `slang` reports too, and the elaboration-only tests
  `sv-tests.py` skips become its score.
- [ ] **Definition rules in `astli-lint`:** a rule declares its layer, and
  the driver lowers nothing unless an enabled rule needs it. Waivers read
  the expanded tree, where attributes survive.
- [ ] **`unused-signal`, `unused-parameter`, `unused-import`:** declared in a
  module, interface or program and never read. A package's members are its
  API and exempt, as is a name containing `unused`, Verilator's convention
  (568 `unused_` signals in OpenTitan).
- [ ] **`undriven-signal`:** read and never written. Not an input, not
  initialised. A connection to an instance's output, or to a port or task
  argument whose direction is unknown, drives it; so does a hierarchical
  write anywhere.
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
  compared.
