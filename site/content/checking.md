# Checking

`astli check` reads the whole design, resolves names across its files, and reports what a SystemVerilog compiler would reject.

```sh
astli check -f src.f                  # Check the design a filelist names
astli check +incdir+include rtl/*.sv  # Check files, with an include directory
astli check -f src.f --top soc_top    # Elaborate from a top, and check what it builds
astli check                           # Check the current package and its dependencies
```

It reports, among others:

- A name nothing declares: a signal, type, package, member, or module. Also a name used before its declaration, or declared twice in one scope.
- A port or parameter that the instantiated definition doesn't have, too many positional connections, or a connection made twice.
- A net, such as a `wire`, an `input` or an untyped port, assigned in a procedural block.
- A variable driven from more than one place, such as two `always_ff` blocks, an `assign`, or an output port. Drivers of separate bits don't clash.

The [check diagnostics reference](reference/check.md) lists every diagnostic.

## Only what is certain

`check` reports a problem only when it is sure of it. Where it can't know, it says nothing:

- Without `--top`, nothing says which generate branches are built, so a definition missing inside a generate block is a warning, not an error.
- With `--top`, `check` decides every generate condition and loop. It reports on what the tops build, and nothing that no top reaches, such as another testbench, can fail the run.
- A class that extends one `astli` can't see, such as a UVM base class, may have members declared there, so an unknown member of such a class isn't reported.
- After a missing include or a construct that doesn't parse, a name the missing code might have declared isn't reported.
- A hierarchical name such as `u_sub.reg_field` is checked as far as `check` can follow it.

## Hard macros

A block that has no source, such as an SRAM or a PLL, can be declared with `extern module`:

```systemverilog
extern module sram_256x32 #(parameter int unsigned NumWords = 256)
  (input logic CLK, input logic CEN, input logic WEN,
   input logic [$clog2(NumWords)-1:0] A, input logic [31:0] D, output logic [31:0] Q);
```

`check` holds every instance to the declared ports and parameters, and counts the outputs as drivers. `astli elab` shows the block as an `extern` leaf. If a file also has the module's body, such as a behavioural model, the body replaces the declaration, and the two don't count as declaring the name twice.

## The instance tree: `astli elab`

`astli elab` prints the tree a top builds:

```sh
astli elab -f src.f --top soc_top
```

Inside a package, it reads the package and its dependencies as `astli check` does, so naming the top is enough. A [`[top.NAME]`](reference/manifest.md#topname) table applies here too, and when the manifest sets `default-top` or has a single table, `astli elab` alone builds that top.

For each instance, it shows:

- The instance's path in the tree.
- The parameters it overrides.
- The generate blocks it builds, the branches it chose, and the loops it unrolled.
- An instance of a definition that is missing, or whose parameters `astli` couldn't evaluate.
