# Checking

```sh
astli check -f src.f                  # every file the design is made of
astli check +incdir+include rtl/*.sv  # an include path, and the files
astli check -f src.f --top soc_top    # and what soc_top builds, checked as built
astli check                           # the package here, with its dependencies
bender script flist-plus -t rtl > design.f && astli check -f design.f
```

`astli check` reads a design the way a compiler does: every file expanded,
with the include paths and `+define+`s the filelist gives, and the names each
file declares visible to the others. It reports errors the standard defines,
and exits 1 if there is one:

- **`undeclared-name`**: a name declared nowhere it could be.
- **`unknown-definition`**, **`unknown-package`**: a module, interface,
  program or package no file declares.
- **`not-in-package`**: `pkg::name`, or an import of it, where the package has
  no `name`. A package's own imports are not its members unless it exports
  them.
- **`unknown-port`**, **`unknown-parameter`**: a connection or override by
  name that the definition does not take.
- **`too-many-ports`**, **`too-many-parameters`**: more by position than the
  definition has.
- **`connected-twice`**: a port connected, or a parameter overridden, twice.
- **`local-parameter`**: an override of a `localparam`.
- **`multiple-drivers`**: a variable two things drive, where one of them is
  an `always_comb`, `always_ff` or `always_latch`, a continuous assignment
  or an instance's output. Writes to parts of it clash where their bits
  overlap. With `--top`, in what the tops build, with each instance's
  parameters and each loop iteration's genvar.

What the expansion and the parser report is printed as well, and an error
there fails the run too.

## Only what is certain

`astli check` reports a subset of the errors a compiler would, and aims for
no error a compiler would not report. Where it cannot see, it stays silent:

- Without `--top`, nothing is elaborated, so no generate branch is known to
  be taken. An instance in a generate construct may never be built, so its
  definition need not exist, and what its connections get wrong is a
  warning. With `--top`, one in a branch that is built is checked, one in a
  branch that is not is skipped, and one under a condition astli cannot
  evaluate is a warning still.
- A module no top builds, or every module without `--top`, has its drivers
  compared at its parameter defaults, every generate branch included but
  two that cannot both be built. What clashes there is a warning.
- What the parser could not read, or an `` `include `` that was not found, may
  declare any name, so nothing around it is undeclared.
- A class's members, a struct's fields and the rest of a hierarchical name
  `a.b.c` after its head are not resolved.
- Each file is its own compilation unit: what one declares outside a module
  or package is not seen from another.

## Seeing what a top builds

```sh
astli elab -f src.f --top soc_top
```

`astli elab` elaborates the design from a top and prints its instance
tree: each instance with its definition and the parameters its overrides
change, each generate block built, an instance of a definition no file
declares, and a generate construct whose condition cannot be evaluated,
which it calls undecided. `astli check --top` checks what it builds.

A design gives the check the files it needs. A module in the filelist that
no top instantiates is checked all the same, and one that uses a module
missing from the list is reported.
