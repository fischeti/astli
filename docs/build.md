# Build and run

> **Status:** `build`, `run` and `clean` with Verilator are built (B1,
> B4–B9); a simulator that compiles packages into libraries is designed (B2,
> B3). Milestone M17 in [`plan.md`](plan.md#5-milestones).

`astli build` compiles a top of the design with a simulator, and `astli run`
builds it and runs the simulation, as `cargo build` and `cargo run` drive
`rustc`. astli simulates nothing itself: it hands the simulator the files,
include directories and defines it already selects for `files`
([`packages.md`](packages.md)), with flags from a profile, and skips what an
earlier build left current. Verilator comes first; the steps below are
shaped so that Questa, VCS, Xcelium and xsim fit them too.

```toml
[build]
simulator = "verilator"          # the default; `--sim` overrides it

[top.tb_xbar]                    # as for `check` and `files` (P32)
defines = { NUM_MASTERS = "4" }

[profile.dev]                    # what a run builds without `--release`
trace = true

[profile.release]
threads = 8

[profile.release.verilator]
args = ["--x-initial", "fast"]   # after every flag astli passes
```

```text
astli build                      # the default top, dev profile
astli run --top tb_xbar --release -- +SEED=3
astli clean                      # deletes .astli/build
```

## The steps

| Step | Per | Verilator | Questa, VCS, Xcelium, xsim |
| --- | --- | --- | --- |
| Analyse | package | nothing | each package's units compiled into a library of its own |
| Elaborate | top and profile | the whole design verilated and compiled | the top elaborated against the libraries |
| Run | invocation | the executable | the simulator on the elaborated design |

## Decisions

| # | Decision | Why |
| --- | --- | --- |
| B1 | `build` and `run` drive a simulator astli does not ship; each is a backend behind the steps above. The root's `[build] simulator` chooses one, `verilator` by default, and `--sim` overrides it. Verilator is the first backend, and another comes once it can be run where it is developed | A file list was the one input every tool takes ([P29](packages.md#decisions)), but what a flow passes besides it is mostly the same few flags, and only a build astli drives can tell what is current and keep what it compiled. Verilator is the open simulator, so CI can test it; a commercial one needs a licence to test against. |
| B2 | A build has three steps: analysing a package, elaborating a top under a profile, and running it. A backend without separate compilation does all its work in the second | Questa, VCS, Xcelium and xsim compile source into libraries and resolve parameters only when they elaborate, so a library compiled once serves every top that uses it. Verilator specialises each instance's parameters as it verilates, as C++ instantiates a template, so no part of a dependency can be compiled ahead of the design. |
| B3 | A package fetched into the store (P20) is analysed into the store, beside its checkout, keyed by its commit, the files the design selects of it and their contents, the defines, the simulator and its version, the profile's flags, and the keys of the packages it imports. The root and a `path` dependency are analysed into `.astli/build` | Cargo's metadata hash: one compiled `common_cells` serves every design on the machine that selects the same files of it with the same defines. A package exports no defines (P12) and few tops define what a dependency reads, so most designs share. An override drops a unit (P10), which changes the files and so the key. A library records the packages it imports, and goes stale when one is compiled again. A store checkout is sealed and its path fixed, so a library recording source paths stays valid; the root and a `path` dependency are edited in place. |
| B4 | A top is elaborated into `.astli/build/<simulator>/<profile>/<top>/`: the file list astli wrote, the simulator's output, a fingerprint of everything that went in, and the executable. The fingerprint hashes astli's and the simulator's versions, the simulator's arguments but for how many jobs build, the file list, and the bytes of every file and header in it, and is written once the build succeeds. A build whose fingerprint is unchanged, with its executable there, runs nothing, and says it is `Fresh`; otherwise the simulator runs, and for Verilator its generated Makefile, with `ccache` as `OBJCACHE` when it is installed and nothing else is set, recompiles only the C++ that changed. `astli clean` deletes `.astli/build`, and leaves the patches beside it | cheshire, iDMA and snitch_cluster commit a `target/` holding their testbenches, so Cargo's directory would mix build output with sources. `.astli/` already holds patches behind a `.gitignore` of `*` (P28), so the walk skips the build (P3) and nothing is committed by mistake. A directory per profile keeps switching from rebuilding the other. The fingerprint covers what the simulator's own check does not, such as a flag or the simulator's version; Verilator's C++ is the compiler cache's to reuse, as no unit of it is a package's. The check still runs before every build, since which files the top needs is what it finds. A fingerprint written last is never that of a build stopped halfway, and the number of jobs changes how fast a build is, not what it builds. A patch is the person's (P28), not a build's. |
| B5 | What is built is a top, chosen as P32 chooses one: `--top`, `[package] default-top`, or the only `[top]` table; a root with none and no `--top` is an error. The table's defines are passed as defines, the include directories as include directories, and the top as the top module | `[top]` already says what a top needs, as a `[[bin]]` does for Cargo. Building every file at its defaults has no top a simulator can start from. |
| B6 | `build` runs `check` on its tops first, and stops at an error. Verilator's lint and style warnings are off, `-Wno-lint -Wno-style`, its other warnings are not fatal, `-Wno-fatal`, but for an elaboration-time `$error` or `$fatal`, `-Werror-USERERROR -Werror-USERFATAL`, and what it writes is shown as it writes it, on stderr | Real designs rarely pass Verilator's lint without waivers, and astli's own checks are the design's: what is left is what this simulator refuses, such as a construct it does not support. An elaboration-time `$error` is how a design refuses parameters it was not written for, and Verilator, which demotes it to a warning, would build it anyway. Verilator reads its file list as one compilation unit, so a macro defined in one file is defined in the next, and a class two files declare is declared twice, which `check` accepts ([P7](packages.md#decisions)); that is the simulator's error, and shown as one. |
| B7 | Verilator builds a testbench written in SystemVerilog, `--binary --timing`, which needs Verilator 5; an older one is refused, naming the version found. A C++ harness and a dependency's DPI sources wait for the manifest to name C and C++ sources | Most PULP testbenches are SystemVerilog alone, and astli knows no other kind of file. A dependency's C++ is the one part of a design that compiles once as Cargo's crates do, and keyed as B3 keys a library it would go to the store too. |
| B8 | A profile is `dev` or `release`, and `dev` is the default; `--release` chooses the other. `[profile.NAME]` sets three knobs, `trace`, `assert` and `threads`, which each backend turns into its own flags, and `[profile.NAME.<simulator>] args` adds flags of its own after astli's. `release` differs from `dev` in tracing nothing and running on four threads. Only the root's profiles count | Most runs are the short loop of editing a testbench and running it again, on a model small enough that more threads slow it; a regression or a long simulation of a large design asks for `release`, where threads pay most. No knob asks for optimisation, since none measured changed compile or simulation time (below). Three knobs mean the same in every simulator; what they do not cover is a tool's own and changes with its versions, so it is left to `args`, as D7 keeps the formatter's knobs few. A dependency's profile would change the build of a design that never chose it, as Cargo ignores one too. |
| B9 | `run` builds as `build` does, then runs the result from the current directory with the arguments after `--` as they are, and exits with the simulation's status, or where a signal ends it, with 128 and the signal, as a shell does, and a note. `build` and `run` take `-D` and `-I`, but not a filelist's `+define+` and `+incdir+` | A testbench reads its memory images and stimuli by paths relative to where it is run, and passes its options as plusargs. A `$fatal` or a failed assertion then fails a CI job without a script to read the log. Verilator before 5.040 ends a `$fatal` by aborting, having said why, so a signal is no error of astli's. The command line has no `--` the parser keeps a `+define+` from, so it would take a simulation's plusarg for a define; a build reads no filelist, where that spelling comes from. |

## Profiles

| Knob | `dev` | `release` | Verilator |
| --- | --- | --- | --- |
| `trace` | `true` | `false` | `--trace-fst` |
| `assert` | `true` | `true` | `--no-assert` when `false` |
| `threads` | `1` | `4` | `--threads N` above 1 |

Every Verilator build also gets `--binary --timing -Wno-fatal -Wno-lint
-Wno-style -Werror-USERERROR -Werror-USERFATAL`, `-j` as astli's own, `--top-module`, `--Mdir` and `-f` with the
file list, and from 5.024, which first takes it, `--quiet`. Assertions stay
on in `release`, since they are what a regression checks.

The defaults were measured with Verilator 5.054 on an Apple M1 Pro
of ten cores, on three testbenches of `eda-benchmarks` 0.2.0's simulation
archive, each built and run alone, in seconds:

| Flags over Verilator's own | cv32e40p `hello_world` | ita `S128…Gelu` | pulp_c910 `coremark` |
| --- | --- | --- | --- |
| none | 3.6 build, 0.1 run | 4.7, 3.3 | 31.0, 31.1 |
| `-O3` | 3.3, 0.1 | 4.6, 3.4 | 30.9, 31.3 |
| C++ `-O2` for `OPT_FAST` and `OPT_GLOBAL` | 3.8, 0.2 | 5.0, 3.3 | 30.1, 30.5 |
| `-O3`, C++ `-O3` | 3.8, 0.2 | 4.4, 3.4 | 30.5, 29.2 |
| `-O3`, C++ `-O2`, `--threads 2` | | 4.5, 3.5 | 34.4, 19.5 |
| `-O3`, C++ `-O2`, `--threads 4` | | 4.0, 3.6 | 34.0, 11.5 |

Optimisation moved nothing beyond a few percent, while four threads ran the
large model 2.7 times as fast for a tenth more compile time and slowed the
small one by a tenth.

## Prior art

Cargo gives the commands, the profiles and the fingerprinted build directory,
and its metadata hash is B3's key. Questa's `vlib`/`vlog -work`, VCS's
`vlogan` with its setup file's library map, Xcelium's `xrun -makelib` and xsim's
`xvlog -work` are the separate compilation B2 relies on. FuseSoC drives
simulators through Edalize, a backend per tool writing that tool's script
into `build/`; bender's `script` writes a script per tool and leaves running
it to the flow, which is what [P29](packages.md#decisions) chose before
`build`.
