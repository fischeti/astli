# Files and designs

`astli` reads code in one of two ways, depending on the command.

## Analysis modes

### As written

Used by `astli fmt` and by the lint rules that read only syntax. Each file is read on its own, as it is on disk:

- An `` `include `` is not followed.
- A macro is not expanded, and no `+define+` applies.
- Every branch of an `` `ifdef `` is read.

So what `fmt` and these rules do with a file depends on that file alone, and a file can be formatted and linted even when it does not compile.

### As a design

Used by `astli check`, `astli elab`, `astli files`, `astli pickle`, `astli build`, and the lint rules that read the design, such as `unused-signal`. Files are read as a compiler reads them:

- Includes are followed through the include directories, and macros are expanded with the defines in effect.
- The modules, interfaces, programs and packages every file declares are known to every other.
- An instance or a connection a macro writes counts like any other.

!!! note "Separate compilation units"
    Each file is its own compilation unit. A declaration outside any `module`, `interface` or `package` belongs to its file, and other files cannot see it.

## Defining a design

Named no files, `astli` reads the design from the current [package](packages.md) and its [dependencies](dependencies.md). Otherwise it reads the files and filelists you name:

```sh
astli check -f design.f                    # Load files from a filelist
astli check +incdir+include rtl/*.sv       # Pass files and an include directory
astli check -f design.f -I include -D SIM  # Combine a filelist with additional flags
```

### Filelists and flags

`astli` reads the usual filelist options:

- `-f <path>` reads a filelist whose relative paths are relative to the current directory.
- `-F <path>` reads one whose relative paths are relative to the filelist itself.
- `+incdir+<dir>` or `-I <dir>` adds an include directory.
- `+define+<NAME>=<VAL>` or `-D <NAME>=<VAL>` defines a macro.

A filelist may name environment variables, as `$ROOT` or `${PROJ_DIR}`. One that is not set is an error.

## Top-level modules and elaboration

A **top** is the module elaboration starts from, usually the chip or the testbench, which nothing else instantiates.

Without `--top`, `astli check` checks every module on its own, at its default parameters, and nothing says which generate branches are built.

With `--top`, it builds the instance tree from the top, as a simulator would. Every instance gets the parameters it is given, and every generate condition and loop is decided. What the tree does not build is not checked, so a testbench no top reaches cannot fail the run.

```sh
# Discover modules that are not instantiated by anything else in the design
astli files -f design.f --emit tops

# Inspect the elaborated instance tree for a top module
astli elab -f design.f --top soc_top

# Check the design with full hierarchical elaboration
astli check -f design.f --top soc_top
```

!!! tip "Configuring tops in `astli.toml`"
    A top that needs defines, such as `SIMULATION`, or parameters set, can have a [`[top.NAME]`](reference/manifest.md#topname) table. `--top NAME` then applies them, and `default-top` makes it the top when no `--top` is given.

!!! tip "Setting a top's parameters"
    `-G NAME=VALUE` sets a parameter of each top, as Verilator's `-G` does, so `check` and `elab` see the configuration a simulation builds: `astli check --top tb_fifo -G Depth=8`. A value is a number, sized or not, or a string in quotes: `-G 'Mode="fast"'`.

!!! tip "Multiple tops"
    `--top` may be given more than once, as `--top soc_top --top test_harness`, to check several trees in one run.
