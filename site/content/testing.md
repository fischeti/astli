# Testing

`astli test` finds the testbenches your package marks, builds each with Verilator as `astli build` builds a top, once for each combination of the parameter values it sweeps, runs them in parallel, and reports each one on its own. You keep no list of testbenches or configurations in a script: a testbench says it is a test, and which values to try.

```systemverilog
(* astli_test *)
module tb_fifo;
  // clock, reset, DUT, stimulus, checks
  initial begin
    // ...
    $finish;
  end
endmodule
```

`astli_test` is an attribute, standard SystemVerilog that other tools ignore. It stands on a module and takes no value; on anything else, or with a value, it is an error.

```sh
astli test                      # build and run every test of the package
astli test fifo                 # only those whose name contains `fifo`
astli test --exact tb_fifo      # only the one called `tb_fifo`
astli test --list               # the tests, one per line, without building
astli test --no-run             # build every test, and run none
astli test --release --trace    # as astli build takes them
astli test tb_fifo --seed 1234  # with a seed of your own
astli test --seeds 4            # each point under 4 random seeds
astli test --junit report.xml   # also a JUnit report, for CI test views
astli test --nocapture          # show every run's output, not only a failure's
```

A filter that matches no test is an error, so a misspelt name fails CI rather than running nothing. Quote a name that holds `[`, which zsh takes for a pattern: `astli test --exact 'tb_fifo[Depth=16]'`.

## Sweeping parameters

A test covers several configurations by sweeping its parameters: `astli_sweep` on a `parameter` declared in the module's body lists the values to build it at.

```systemverilog
(* astli_test *)
module tb_fifo;
  (* astli_sweep = "1, 2, 4, 16" *) parameter int unsigned Depth = 4;
  (* astli_sweep *)                 parameter fifo_pkg::mode_e Mode = fifo_pkg::Registered;
  // clock, reset, DUT, stimulus, checks
endmodule
```

- The values are a list separated by commas, each a number in decimal or an enum member's name. Nothing is evaluated: write `8`, not `2**3`.
- `astli_sweep` without a value takes every value of an enum, in the order its members are declared, of a `bit`, or of a 1-bit `logic`.
- The test runs once per point, the product of its swept parameters, so the bench above has eight. A parameter not swept keeps its default, or what the test's table sets. Parameters that must move together are one swept parameter the bench derives the others from.
- A point is named for its values in declaration order, `tb_fifo[Depth=16,Mode=Bypass]`, which is how `--list` shows it, how a filter selects it, and how a failure is reported: `astli test 'Depth=16'` runs the four points of that depth.
- A test that sweeps declares its parameters in its body, with no `#( ... )` list: once a module has a list, a `parameter` in its body is local and nothing outside can set it, and a parameter in the list takes no attribute. `-G` and an instance's `#(.Depth(4))` set a body parameter as they set one in a list.
- A value the parameter cannot hold, a type with no short spelling (a `real`, a `string`, a struct, an unpacked type or one wider than 64 bits), a bare `astli_sweep` on a type with more values than an enum or a bit, an `astli_sweep` on a `localparam` or on a module that is no test, and a `[top.NAME] params` entry for a swept parameter are errors at the attribute.

`-G NAME=VALUE` sets a parameter in every selected test that declares it, over what its table sets, and is an error if no selected test declares it. It cannot set a swept parameter: the filter `'Depth=16'` selects that value.

A point is a top like any other: `astli check --top 'tb_fifo[Depth=16,Mode=Bypass]'` checks it, `astli files` lists what it needs, and `astli build` and `astli run` build it where `astli test` does. [`astli check --tests`](checking.md) checks every point of every test, without building any: a CI step for it fails fast, before the builds.

## Plusargs and a timeout of its own

What a test's runs take, it says where it is written, on its module, in the attribute instance that marks it or in one of their own:

```systemverilog
(* astli_test, astli_timeout = 600, astli_plusargs = "+TEST=burst, +TEST=wrap +LEN=4" *)
module tb_fifo;
```

- `astli_plusargs` lists the plusargs each point runs under, separated by commas: a run per entry, all from the point's one build, as UVM's `+UVM_TESTNAME` picks a test. An entry may hold several plusargs separated by spaces, for one run that needs them together. A run is named after its point and its entry, `tb_fifo[Depth=16] +TEST=burst`, which is how `--list` shows it and a filter selects it: `astli test wrap` runs every point's `+TEST=wrap +LEN=4`. Each entry's runs have a directory of their own, `run/<n>-<entry>/<seed>/`, and runs multiply with seeds, not builds.
- `astli_timeout` bounds each run of the test, in whole seconds, over `[test] timeout`.
- An `astli_plusargs` that is no string, an empty entry, one that does not start with `+`, or one listed twice, an `astli_timeout` that is no whole number above 0, and either on a module that is no test, are errors at the attribute.

## Which tests

The tests are the marked modules in any file of your package, its `dev` sources included. A dependency's tests are its own, and so are those of a [workspace](workspaces.md) member your package requires by `path`: run them in that member. Each file is read as written, every branch of an `` `ifdef `` included, so a test that only a `[top.NAME]` table's defines declare is still found.

## How a test is built

A test needs nothing in `astli.toml`: a package whose manifest holds only its name runs its tests. Each point is built as a top, as `astli build --top 'tb_fifo[Depth=16]'` would build it, its swept values set as parameters and nothing defined. The tests are checked together before any is built, exactly as [`astli check`](checking.md) checks them, and an error stops every build. Each point is then built into `.astli/build/verilator/<profile>/<package>/<test>/<hash>/`, the hash of its values, or a test that sweeps nothing into `.astli/build/verilator/<profile>/<package>/<test>/`. A point is `Fresh` when nothing it reads has changed. Only the points selected are checked and built.

A `[test]` table holds what applies to every test of the package:

```toml
[test]
timeout = 120           # seconds, for every run
seeds = 4               # random seeds each point runs under
```

A `[top.NAME]` table of the test's name is for a test that needs defines. Defines are applied before a file is parsed and hold for the whole build, so no attribute in the test's source can carry them. The test is then built under the table: its `defines`, and its `params` for a parameter no sweep names. Tests whose tables define the same are checked together, and the others apart.

```toml
[top.tb_fifo]
defines = { FIFO_DEPTH = 16 }
```

Builds and runs are parallel under `-j`, which the Verilator builds share.

## Passing and failing

A test passes when its simulation exits with status 0: it reaches `$finish` with no `$error` or `$fatal`. No `PASS` line is needed. It fails on any other status, if a signal ends it, or if it runs past its `astli_timeout` or the package's `[test] timeout`; without one it may run as long as it likes, and a run past 60 seconds is reported as still running.

Each run starts in a directory of its own, `run/<seed>/` in the point's build directory, so what one writes, waveforms included, never overwrites another's. It is given `+astli_root=<path>`, the root of the test's package, to open its memory images and stimuli by, and a seed, `+verilator+seed+N`, which comes from the point's name unless `--seed` gives one, so a rerun is the same run. Its output is always kept in `output.log` there, and `astli clean` deletes it with the builds.

A failure shows its output and where its log is, and the command that repeats it, with its seed and `--trace`:

```text
      Passed tb_ok in 0.2s
error: tb_fifo failed: the simulation exited with status 1, after 0.3s
  = note: its output is in .astli/build/verilator/dev/fifo/tb_fifo/run/341838526/output.log
  = help: astli test --exact tb_fifo --seed 341838526 --trace repeats it
error: 1 of 2 tests failed: tb_fifo
```

Every test is built and run, whatever failed before it, and `astli test` fails if any test did.

## Several seeds

`astli test --seeds N`, or `seeds = N` in `[test]`, runs each point under N random seeds in place of the one its name gives, all from one build, each in its own `run/<seed>/` in the point's build directory. Each run is shown with its seed, as `tb_fifo[Depth=16] seed 1234`, and a failure's repeat command carries it as `--seed`, which takes the place of `seeds` in the manifest. `--seeds` with `--seed` is refused.

## Reports for CI

`--junit PATH` writes a JUnit report as well as the summary, for the test views of GitHub, GitLab and Jenkins: a test suite for each package, and a test case for each run, named by its point and its entry, as `tb_fifo[Depth=16] +TEST=burst`, and with several seeds by its seed too. A failure holds why it failed and the run's output. A point that did not build is a failure too, and one whose package failed its check an error; under `--no-run` every point built is skipped, so the report names every point selected however it ended.

## In a workspace

`astli test -p NAME` runs the tests of one package of the workspace, and `astli test --workspace`, or `astli test` at a virtual root, those of every package, under one `-j` and one summary. Two members may each have a `tb_top`, so each test is shown with its package's name before it, and the command repeating a failure names the package with `-p`. The package's name is no part of what a filter matches. `[test]` is each package's own; a virtual root's is not read.
