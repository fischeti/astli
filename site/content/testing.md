# Testing

`astli test` finds the testbenches your package marks, builds each with Verilator as `astli build` builds a top, runs them in parallel, and reports each one on its own. You keep no list of testbenches in a script: a testbench says it is a test.

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
astli test --nocapture          # show every run's output, not only a failure's
```

A filter that matches no test is an error, so a misspelt name fails CI rather than running nothing. Quote a name that holds `[`, which zsh takes for a pattern: `astli test --exact 'tb_fifo[Depth=16]'`.

## Which tests

The tests are the marked modules in any file of your package, its `dev` sources included. A dependency's tests are its own, and so are those of a [workspace](workspaces.md) member your package requires by `path`: run them in that member. Each file is read as written, every branch of an `` `ifdef `` included, so a test that only a `[top.NAME]` table's defines declare is still found.

## How a test is built

A test is a top. If a `[top.NAME]` table has the test's name, the test is built under it: its `defines`, its `params`, and the `timeout` below. Tests whose tables define the same are checked together, and the others apart, every one before any is built, exactly as [`astli check`](checking.md) checks them; an error stops every build. Each test is then built into `.astli/build/verilator/<profile>/<package>/<test>/` as `astli build` builds a top, and is `Fresh` when nothing it reads has changed.

```toml
[top.tb_fifo]
defines = { FIFO_DEPTH = 16 }
params = { Mode = 1 }
timeout = 600           # seconds, for this test alone

[test]
timeout = 120           # seconds, for every test of the package
```

Builds and runs are parallel under `-j`, which the Verilator builds share.

## Passing and failing

A test passes when its simulation exits with status 0: it reaches `$finish` with no `$error` or `$fatal`. No `PASS` line is needed. It fails on any other status, if a signal ends it, or if it runs past its `timeout`; without one it may run as long as it likes, and a run past 60 seconds is reported as still running.

Each run starts in a directory of its own, `run/<seed>/` in the test's build directory, so what one writes, waveforms included, never overwrites another's. It is given `+astli_root=<path>`, the root of the test's package, to open its memory images and stimuli by, and a seed, `+verilator+seed+N`, which comes from the test's name unless `--seed` gives one, so a rerun is the same run. Its output is always kept in `output.log` there, and `astli clean` deletes it with the builds.

A failure shows its output and where its log is, and the command that repeats it, with its seed and `--trace`:

```text
      Passed tb_ok in 0.2s
error: tb_fifo failed: the simulation exited with status 1, after 0.3s
  = note: its output is in .astli/build/verilator/dev/fifo/tb_fifo/run/341838526/output.log
  = help: astli test --exact tb_fifo --seed 341838526 --trace repeats it
error: 1 of 2 tests failed: tb_fifo
```

Every test is built and run, whatever failed before it, and `astli test` fails if any test did.

## In a workspace

`astli test -p NAME` runs the tests of one package of the workspace, and `astli test --workspace`, or `astli test` at a virtual root, those of every package, under one `-j` and one summary. Two members may each have a `tb_top`, so each test is shown with its package's name before it, and the command repeating a failure names the package with `-p`. The package's name is no part of what a filter matches. `[test]` is each package's own; a virtual root's is not read.
