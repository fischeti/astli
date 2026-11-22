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
astli test -- +verilator+quiet  # pass what follows -- to every run
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
- The test runs once per point, the product of its swept parameters, so the bench above has eight. A parameter not swept keeps its default, or what the test's table sets. Parameters that must move together are one swept parameter the bench derives the others from, as [below](#configurations-that-move-together).
- A point is named for its values in declaration order, `tb_fifo[Depth=16,Mode=Bypass]`, which is how `--list` shows it, how a filter selects it, and how a failure is reported: `astli test 'Depth=16'` runs the four points of that depth.
- A test that sweeps declares its parameters in its body, with no `#( ... )` list: once a module has a list, a `parameter` in its body is local and nothing outside can set it, and a parameter in the list takes no attribute. `-G` and an instance's `#(.Depth(4))` set a body parameter as they set one in a list.
- A value the parameter cannot hold, a type with no short spelling (a `real`, a `string`, a struct, an unpacked type or one wider than 64 bits), a bare `astli_sweep` on a type with more values than an enum or a bit, an `astli_sweep` on a `localparam` or on a module that is no test, and a `[top.NAME] params` entry for a swept parameter are errors at the attribute.

### Configurations that move together

A sweep is always a product, so a set of configurations whose parameters change together, such as a round-robin arbiter at five widths and a fixed-priority one at two, is not a list of values. Name each configuration in an enum of the bench's own, sweep it with a bare `astli_sweep`, and look its values up in a table with a row per configuration:

```systemverilog
(* astli_test *)
module tb_arbiter;
  typedef enum int unsigned {RoundRobin, Fixed} policy_e;
  typedef enum int unsigned {Rr1, Rr2, Rr3, Rr5, Rr8, Fixed4, Fixed7} config_e;
  (* astli_sweep *) parameter config_e Config = Rr1;

  typedef struct packed {
    int unsigned num_req;
    policy_e     policy;
  } cfg_t;
  localparam cfg_t Cfgs[Config.num()] = '{
      '{1, RoundRobin}, '{2, RoundRobin}, '{3, RoundRobin}, '{5, RoundRobin},
      '{8, RoundRobin}, '{4, Fixed},      '{7, Fixed}
  };
  localparam cfg_t Cfg = Cfgs[Config];
  // the DUT at Cfg.num_req and Cfg.policy, stimulus, checks
endmodule
```

The points are named by their labels, `tb_arbiter[Config=Fixed4]`, and a filter selects by them: `astli test Fixed` runs the two fixed-priority ones. A bare sweep follows the enum, so the list of points cannot fall out of step with it, and a label says what a configuration is for where an index would say nothing. The table is sized by `Config.num()`, the enum's count, so a member added without a row is an error from `astli check` rather than a point that reads past the end of the table.

`-G NAME=VALUE` sets a parameter in every selected test that declares it, over what its table sets, and is an error if no selected test declares it. It cannot set a swept parameter: the filter `'Depth=16'` selects that value.

A point is a top like any other: `astli check --top 'tb_fifo[Depth=16,Mode=Bypass]'` checks it, `astli files` lists what it needs, and `astli build` and `astli run` build it where `astli test` does. [`astli check --tests`](checking.md) checks every point of every test, without building any: a CI step for it fails fast, before the builds. With `--top`, it checks the tops named too, such as a synthesis top's overrides, in the same run.

## Which tests

The tests are the marked modules in any file of your package, its `dev` sources included. A dependency's tests are its own, and so are those of a [workspace](workspaces.md) member your package requires by `path`: run them in that member. Each file is read as written, every branch of an `` `ifdef `` included, so a test that only a `[top.NAME]` table's defines declare is still found.

## How a test is built

A test needs nothing in `astli.toml`: a package whose manifest holds only its name runs its tests. Each point is built as a top, as `astli build --top 'tb_fifo[Depth=16]'` would build it, its swept values set as parameters and nothing defined. The tests are checked together before any is built, exactly as [`astli check`](checking.md) checks them, and an error stops every build. Each point is then built into `.astli/build/verilator/<profile>/<package>/<test>/<hash>/`, the hash of its values, or a test that sweeps nothing into `.astli/build/verilator/<profile>/<package>/<test>/`. A point is not built again when nothing it reads has changed. Only the points selected are checked and built.

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

A test passes when its simulation exits with status 0: it reaches `$finish` with no `$error` or `$fatal`. No `PASS` line is needed. It fails on any other status, if a signal ends it, or if it runs past the package's `[test] timeout`; without one it may run as long as it likes, and a run past 60 seconds is reported as still running.

Each run starts in a directory of its own, `run/<seed>/` in the point's build directory, so what one writes, waveforms included, never overwrites another's. It is given `+astli_root=<path>`, the root of the test's package, to open its memory images and stimuli by, and a seed, `+verilator+seed+N`, which comes from the point's name unless `--seed` gives one, so a rerun is the same run. What follows `--` on the command line is passed to every run after these, as `astli run` passes it, and the same for every run. Its output is always kept in `output.log` there, and `astli clean` deletes it with the builds.

A failure shows its output and where its log is, and the command that repeats it: with its seed and `--trace`, and every flag of the run that changes what is built or how it runs, `--release`, `--sim`, `-I`, `-D`, `-G`, `--features`, `--no-features` and `--all-features`, and what followed `--`, quoted so that it can be pasted into a shell:

```text
      Passed tb_ok in 0.2s
error: tb_fifo failed: the simulation exited with status 1, after 0.3s
  = note: its output is in .astli/build/verilator/dev/fifo/tb_fifo/run/341838526/output.log
  = help: astli test --exact tb_fifo --seed 341838526 --trace repeats it
error: 1 of 2 tests failed: tb_fifo
```

Every test is built and run, whatever failed before it, and `astli test` fails if any test did.

### Tests that should fail

A test that checks the design catches a mistake, such as an assertion in the design or a VIP's `$fatal` firing on a bad handshake, passes only when its run fails. Say what the failure writes with `astli_should_fail`:

```systemverilog
(* astli_test, astli_should_fail = "[AXI_HS]" *)
module tb_axi_bad_handshake;
```

Each run of the test then passes only if it fails and its output contains the text. A run that passes, or fails without the text, fails; one that runs past its timeout fails whatever it wrote. The text is required, since any crash would otherwise pass. The standard requires every `$error` and `$fatal` message to hold your text and the scope it was called in, but not an exit status, so name the scope, `tb.dut.u_checker`, or a tag you put in the message, `$fatal(1, "[AXI_HS] AWVALID dropped before AWREADY")`.

Where the design reports the mistake itself, on an error output or as an error response, check that in the bench and `$finish` instead: the test then passes normally, and can check when the error came.

## Several seeds

`astli test --seeds N`, or `seeds = N` in `[test]`, runs each point under N random seeds in place of the one its name gives, all from one build, each in its own `run/<seed>/` in the point's build directory. Each run is shown with its seed, as `tb_fifo[Depth=16] seed 1234`, and a failure's repeat command carries it as `--seed`, which takes the place of `seeds` in the manifest. `--seeds` with `--seed` is refused.

## Reports for CI

`--junit PATH` writes a JUnit report as well as the summary, for the test views of GitHub, GitLab and Jenkins: a test suite for each package, and a test case for each run, named by its point, as `tb_fifo[Depth=16]`, and with several seeds by its seed too. A failure holds why it failed and the run's output. A point that did not build is a failure too, and one whose package failed its check an error; under `--no-run` every point built is skipped, so the report names every point selected however it ended.

## In a workspace

`astli test -p NAME` runs the tests of one package of the workspace, and `astli test --workspace`, or `astli test` at a virtual root, those of every package, under one `-j` and one summary. Two members may each have a `tb_top`, so each test is shown with its package's name before it, and the command repeating a failure names the package with `-p`, as it does after `-p NAME`. The package's name is no part of what a filter matches. `[test]` is each package's own; a virtual root's is not read.
