# Building a testbench

`astli build` compiles a top of your package into a simulation with Verilator, from the files the top needs, in the order they compile. You write no filelist and no build script: the package, its dependencies and its `[top]` tables already say everything Verilator needs.

```sh
# Build the package's default top under the dev profile
astli build

# Build a specific top or [top.NAME] table
astli build --top tb_xbar

# Build for speed of simulation rather than of compilation
astli build --release
```

`astli build` needs [Verilator](https://verilator.org) 5 or later on your `PATH`. The testbench is written in SystemVerilog: Verilator builds it with `--binary --timing`, so it needs no C++ harness.

---

## What gets built

The top is chosen as for `astli check --top`: the `--top` you name, else the `default-top` in `[package]`, else the only `[top.NAME]` table. A `[top.NAME]` table's `defines` are passed to Verilator, as are the include directories the design reads its headers from.

```toml
[package]
default-top = "tb_xbar"

[top.tb_xbar]
defines = ["NUM_MASTERS=4"]
```

Before Verilator runs, `astli build` checks the design from the top exactly as `astli check --top` does, and stops at the first error. Verilator's own lint and style warnings are turned off, since `astli` has judged the design already; what Verilator still reports, such as a construct it does not support, is shown as it writes it.

The build goes to `.astli/build/verilator/<profile>/<top>/` in your package, beside the file list `astli` handed Verilator (`files.f`). The executable is `V<module>` there. `.astli/` keeps itself out of git with a `.gitignore` of its own.

!!! note "One compilation unit"
    Verilator reads every file of a build as one compilation unit: a macro defined in one file is still defined in the next, and a class declared at the top level of two files is declared twice. `astli check` treats each file as its own unit, so Verilator can refuse what the check accepted.

---

## Profiles

A build trades compile time for simulation time. The `dev` profile, the default, compiles quickly and records waveforms; `--release` selects the `release` profile, which optimises the simulation instead.

| Knob | `dev` | `release` | Verilator flag |
| :--- | :--- | :--- | :--- |
| `opt` | `"default"` | `"high"` | `-O3` for `"high"` |
| `trace` | `true` | `false` | `--trace-fst` |
| `assert` | `true` | `true` | `--no-assert` when `false` |
| `threads` | `1` | `1` | `--threads N` |

Set any of them in `astli.toml`; a profile keeps the defaults it does not write. Flags no knob covers go in the simulator's own table, after every flag `astli` passes:

```toml
[profile.dev]
opt = "high"            # for a testbench that simulates long

[profile.release]
threads = 4

[profile.release.verilator]
args = ["--x-initial", "fast"]
```

Each profile builds into a directory of its own, so switching between them does not rebuild the other.
