# Building and running a testbench

`astli build` compiles a top of your package into a simulation with Verilator, from the files the top needs, in the order they compile, and `astli run` builds it and runs it. You write no filelist and no build script: the package, its dependencies and its `[top]` tables already say everything Verilator needs.

```sh
# Build the package's default top under the dev profile
astli build

# Build a specific top or [top.NAME] table
astli build --top tb_xbar

# Build for speed of simulation rather than of compilation
astli build --release

# Build, then run the simulation with plusargs
astli run --top tb_xbar -- +SEED=3 +verbose
```

`astli build` needs [Verilator](https://verilator.org) 5 or later on your `PATH`. The testbench is written in SystemVerilog: Verilator builds it with `--binary --timing`, so it needs no C++ harness.

## What gets built

The top is chosen as for `astli check --top`: the `--top` you name, else the `default-top` in `[package]`, else the only `[top.NAME]` table. A `[top.NAME]` table's `defines` are passed to Verilator, as are the include directories the design reads its headers from, and its `params` are passed as `-G`, after them any you give `astli build -G NAME=VALUE`.

```toml
[package]
default-top = "tb_xbar"

[top.tb_xbar]
defines = { NUM_MASTERS = 4 }
params = { NumSlaves = 2 }
```

Before Verilator runs, `astli build` checks the design from the top exactly as `astli check --top` does, and stops at the first error. Verilator's own lint and style warnings are turned off, since `astli` has judged the design already; what Verilator still reports, such as a construct it does not support, is shown as it writes it.

The build goes to `.astli/build/verilator/<profile>/<top>/` in your package, beside the file list `astli` handed Verilator (`files.f`). The executable is `V<module>` there. `.astli/` keeps itself out of git with a `.gitignore` of its own.

A build that would be made from exactly what the last one was made from is skipped, and reported `Fresh`: the same files and headers, byte for byte, the same flags and defines, and the same versions of `astli` and Verilator. Otherwise Verilator recompiles only the C++ that changed, and with [`ccache`](https://ccache.dev) on your `PATH`, `astli` hands it to Verilator so that what was compiled before, in any build, is reused.

```sh
# Delete every build of the package, every profile's
astli clean
```

`astli clean` leaves patches in `.astli/` where they are.

## Running

`astli run` takes every flag `astli build` does, builds the top unless it is fresh, and runs the simulation. Everything after `--` goes to the simulation as it is written, so a `+define+` there is a plusarg, not a define; give `astli` its defines with `-D` before the `--`.

The simulation runs in your current directory, where a testbench finds the memory images and stimuli it opens by relative path. Its output is `astli run`'s standard output, and `astli run` exits with the simulation's status: a `$fatal`, or a failed immediate assertion, fails the command and so a CI job.

```yaml
- run: astli run --release --top tb_xbar -- +SEED=${{ github.run_number }}
```

!!! note "One compilation unit"
    Verilator reads every file of a build as one compilation unit: a macro defined in one file is still defined in the next, and a class declared at the top level of two files is declared twice. `astli check` treats each file as its own unit, so Verilator can refuse what the check accepted.

## Profiles

The `dev` profile, the default, records waveforms and simulates on one thread, which is fastest for the small models of an edit-and-rerun loop. `--release` selects the `release` profile, which records nothing and simulates on four threads: a large design runs over twice as fast, for about a tenth more compile time.

| Knob | `dev` | `release` | Verilator flag |
| :--- | :--- | :--- | :--- |
| `trace` | `true` | `false` | `--trace-fst` |
| `assert` | `true` | `true` | `--no-assert` when `false` |
| `threads` | `1` | `4` | `--threads N` |

Set any of them in `astli.toml`; a profile keeps the defaults it does not write. Flags no knob covers go in the simulator's own table, after every flag `astli` passes:

```toml
[profile.dev]
trace = false           # for a testbench whose waveforms you never open

[profile.release]
threads = 8             # for a design large enough to use them

[profile.release.verilator]
args = ["--x-initial", "fast"]
```

Each profile builds into a directory of its own, so switching between them does not rebuild the other.
