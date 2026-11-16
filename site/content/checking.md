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
- In a package with [`[top.NAME]`](reference/manifest.md#topname) tables, `check` without `--top` checks from every table, each under its own `defines`, and reports a finding several tables meet once. `-G NAME=VALUE` then sets the parameter in every table's top that declares it, and fails only if none does.
- `check --tests` also checks every point of every [test](testing.md) the package marks, each under its test's table, as `astli test` checks them before building; CI is the place for it, since `check` after every edit need not elaborate every bench. `--top 'tb_fifo[Depth=16]'` checks one point, and `--tests --top soc_top` checks the top named and every point. `--tests` takes no files named, which are no package's. `lint --tests` does the same for the lint rules that read the design.
- A class that extends one `astli` can't see, such as a UVM base class, may have members declared there, so an unknown member of such a class isn't reported.
- After a missing include or a construct that doesn't parse, a name the missing code might have declared isn't reported.
- A hierarchical name such as `u_sub.reg_field` is checked as far as `check` can follow it.

## Hard macros and cell libraries

A block that has no source, such as an SRAM, a PLL, a standard cell or analog IP, can be declared with `extern module`:

```systemverilog
extern module sram_256x32 #(parameter int unsigned NumWords = 256)
  (input logic CLK, input logic CEN, input logic WEN,
   input logic [$clog2(NumWords)-1:0] A, input logic [31:0] D, output logic [31:0] Q);
```

`check` holds every instance to the declared ports and parameters, and counts the outputs as drivers. `astli elab` shows the block as an `extern` leaf. If a file also has the module's body, such as a behavioural model, the body replaces the declaration, and the two don't count as declaring the name twice.

Without a declaration, an instance of such a block is an `unknown-definition` error: `check` can't judge an instance of nothing. To check a design against a library you don't have the source of, put a declaration for each block it uses in a file of its own, a stub:

```systemverilog
// cells.sv
extern module AND2_X1 (input logic A, input logic B, output logic Y);
extern module DFF_X1 (input logic CK, input logic D, output logic Q);
```

Name the stub with the design's files, or keep it in the package, where `check` finds it like any other file. `check` then reports a wrong connection as `unknown-port` and the rest. A file holding nothing but `extern` declarations stays out of the list `astli files` writes and the one `astli build` compiles, since the tool given the list gets the blocks from the library itself.

## The instance tree: `astli elab`

`astli elab` prints the tree a top builds:

```sh
astli elab -f src.f --top soc_top
```

Inside a package, it reads the package and its dependencies as `astli check` does, so naming the top is enough. A [`[top.NAME]`](reference/manifest.md#topname) table applies here too, and when the manifest sets `default-top` or has a single table, `astli elab` alone builds that top.

For each instance, it shows:

- The instance's path in the tree.
- The value of each parameter in its definition's parameter port list, a `localparam` there too, or of each `parameter` of a definition that has no list.
- The generate blocks it builds, the branches it chose, and the loops it unrolled.
- An instance of a definition that is missing, or whose parameters `astli` couldn't evaluate.

```text
soc_top #(NumCores = 32'sd2)
  u_core[1:0]: core #(Width = 32'sd32, cfg_t = core_cfg_t)
  g_cache
    u_cache: cache #(Ways = 32'sd4)
```

### The design as JSON

`astli elab --json` writes the elaborated design as one JSON document for a program to read. Each body appears once: a definition with its parameters decided, which every instance that decides them alike shares. A body lists:

- Every parameter its definition declares, body `localparam`s too, with its type, its value, and whether that is the definition's default.
- Every port, with its direction, its type, and its width.
- What it holds: each instance with the body it gets and what each of its ports is connected to, each generate block with what that holds, and each gate, missing definition, and construct `astli` couldn't decide.

```json
{
  "tops": [{"name": "soc_top", "definition": "soc_top", "body": 0}],
  "bodies": [
    {
      "id": 0, "definition": "soc_top", "keyword": "module", "path": "rtl/soc_top.sv", "line": 3, "extern": false,
      "parameters": [{"name": "NumCores", "local": false, "kind": "value", "type": "bit signed [31:0]", "value": "32'sd2", "integer": 2, "default": true}],
      "ports": [{"name": "clk_i", "interface": false, "direction": "input", "type": "logic", "width": 1}],
      "children": [
        {"kind": "instance", "name": "u_core", "definition": "core", "body": 1, "dims": [[1, 0]], "path": "rtl/soc_top.sv", "line": 12,
         "connections": [{"port": "clk_i", "text": "clk_i"}]}
      ]
    }
  ]
}
```

A value is written as a SystemVerilog literal, with `integer` beside it where it fits in 64 bits. A value or a body `astli` couldn't work out is `null`.

!!! warning "A preview"
    `astli elab` is a preview: what it writes, the tree and the JSON alike, may change in any release. Each run says so in a note, which `-q` leaves out.
