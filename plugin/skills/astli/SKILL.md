---
name: astli
description: Format, lint and check SystemVerilog with the `astli` CLI, and manage astli packages and dependencies. Use when editing `.sv`, `.svh`, `.v` or `.vh` files, when a repository has an `astli.toml`, `astli.lock` or `Bender.yml`, or when asked to format, lint, check, elaborate, build a filelist for, or simulate a SystemVerilog design, or to migrate a project from Bender.
---

# astli

`astli` is a SystemVerilog toolchain: a formatter, a linter, a semantic
checker, filelist tools, and a package manager. One binary, one subcommand
per job.

This skill describes the common workflows. The installed binary is the
authority on its own flags: when `astli <command> --help` disagrees with this
file, follow `--help`. The full guide is at <https://astli.dev/>.

If `astli` is not on `PATH`, `uvx astli ...` runs it without installing.

## Files or a package

With no files on the command line, every command works on the package around
the working directory: the nearest `astli.toml` going up, stopping at the
repository root, or the repository root itself if there is none. It finds
every `.sv`, `.svh`, `.v` and `.vh` file, respecting `.gitignore`, and pulls
in the package's dependencies.

Naming files or a filelist turns that off, and astli reads only what it was
given:

```sh
astli lint rtl/alu.sv
astli check -f design.f -I include -D SIM
astli check +incdir+include rtl/*.sv
```

`-f` resolves a filelist's paths from the working directory, `-F` from the
filelist's own directory.

## Formatting

```sh
astli fmt --check        # exit non-zero if any file would change
astli fmt --diff top.sv  # show the change without writing it
astli fmt top.sv         # rewrite the named files
astli fmt -              # stdin to stdout
```

- **`astli fmt` with no arguments rewrites every file in the package in
  place.** To format text you are holding, pipe it through `astli fmt -`. To
  see what would change, use `--check` or `--diff`.
- The style is fixed, apart from `width`, `indent`, `max-pad` and `exclude`
  under `[fmt]` in `astli.toml`. There are no style flags; don't look for
  them.
- Format code after writing it rather than hand-aligning it: astli aligns
  declarations, ports, assignments and trailing comments into columns, and a
  blank line starts a new alignment group.
- `(* astli_fmt_skip *)` on an item keeps its text as written.
- astli refuses to write output whose tokens differ from the input. If it
  reports that, the formatter has a bug; leave the file alone and report the
  diagnostic.

## Linting

```sh
astli lint                       # the package and its dependencies
astli lint --list                # every rule, its group and level
astli lint --fix rtl/*.sv        # apply the fixes rules offer
astli lint --top chip_top        # also run the width rules, which elaborate
astli lint --allow lowrisc --warn parameter-name-style rtl/*.sv
```

- Rules sit in groups: `correctness` (deny), `suspicious` and `lowrisc`
  (warn), `restriction` (allow). A `deny` finding exits 1.
- `--allow`, `--warn` and `--deny` have no short forms: `-D` defines a macro.
- Run `astli fmt` after `--fix`, which makes minimal edits.
- Waive a rule on one construct with `(* astli_allow = "rule-name" *)`, or
  several with a comma-separated list. For whole directories, use
  `[lint.paths]` in `astli.toml`. Prefer fixing the code over waiving it, and
  ask before adding waivers.

## Checking a design

```sh
astli check                  # the package and its dependencies
astli check --top soc_top    # elaborate from a top and check what it reaches
astli elab --top soc_top     # print the instance tree, with parameters
astli check --top tb_fifo -G Depth=8   # with a top's parameter set
```

`check` reports only what a conforming compiler would also reject: undeclared
names, names used before their declaration or declared twice, missing modules, wrong ports and parameters, nets assigned in procedures, multiple drivers. Treat
every error as real. It reports in the package's files and in those of its
`path` dependencies, so an error there may be the package's to fix, such as
a parameter it passes; a Git dependency's findings are not shown. Without `--top`, instances inside generate blocks are
checked leniently, so pass `--top` (or set `[top.NAME]` in `astli.toml`) for
a complete check. A `[top.NAME]` table holds what its top needs:
`defines = { SIM = true, W = 8 }` and `params = { Depth = 8, Mode = "fast" }`.
`-G NAME=VALUE` sets a parameter of every top, over the table, as Verilator's
`-G` does; a string needs its quotes, `-G 'Mode="fast"'`. To check one
testbench at several configurations, give each its own table with
`top = "tb_fifo"` rather than writing a wrapper module. A hard macro linked only at synthesis, or a standard cell or analog
block a library supplies, is declared by an `extern module` of its ports,
which `check` holds its instances to; that is the fix for an
`unknown-definition` on one. A file of such declarations alone is a stub,
which `files` and `build` leave out of the list.

## Filelists

```sh
astli files --top soc_top            # files the top needs, in compile order
astli files -f all.f --top soc_top --order
astli files -f all.f --emit tops     # modules nothing instantiates
astli files --top soc_top --why rtl/fifo.sv
astli files --emit incdirs
astli pickle --top soc_top --order > bundle.sv
```

`files` refuses to write a filelist when any source has an error, since an
unparsed region may hold an instance. Fix the error first.

## Building a testbench

```sh
astli build                  # the default top, dev profile, with Verilator
astli build --top tb_xbar    # a top module or [top.NAME] table
astli build --release        # optimised for simulation speed
astli run --trace            # a build that can write waveforms
astli run -- +SEED=3         # build, then run with plusargs after --
```

`build` needs Verilator 5 or later on the `PATH`, and a testbench written in
SystemVerilog. It runs `check` on the top first and stops at an error, so fix
what `check` reports before looking at Verilator's output. It writes to
`.astli/build/verilator/<profile>/<package>/<top>/`, at the workspace root
in a workspace: `files.f`, Verilator's output and
the executable `V<module>`. Profiles are `[profile.dev]` and
`[profile.release]` in `astli.toml`; add Verilator flags with
`[profile.NAME.verilator] args`, never by editing `files.f`. A build made
from the same files, flags and versions as the last one is skipped as
`Fresh`; `astli clean` deletes every build. `run` runs the simulation in the
current directory and exits with its status, so a `$fatal` fails it; pass
defines with `-D` and parameters with `-G` before `--`, since everything after
it is the simulation's.

## Dependencies

Dependencies live under `[dependencies]` in `astli.toml`, or come from
`Bender.yml` when `astli.toml` has no `[dependencies]` table. Commands that
need them fetch into a shared cache and pin commits in `astli.lock`.

```sh
astli add https://github.com/pulp-platform/common_cells.git  # latest release
astli add URL --rev main  # track a branch; also --version REQ, --path DIR, --dev
astli add axi --features vip  # require a package the design already holds
astli remove common_cells # drop it, and what features and pins say of it
astli update [NAME]       # re-resolve and rewrite astli.lock
astli tree                # the resolved dependency tree
astli path common_cells   # where a dependency is checked out
astli patch common_cells  # work on a dependency in a local clone
astli patch --remove common_cells
astli snapshot common_cells  # pin the pushed patch commit, drop the patch
```

- Never edit files in the cache (`astli cache dir`); they are read-only and
  shared between projects. To change a dependency, use `astli patch`.
- Don't edit `astli.lock` by hand; run `astli update`.
- A git dependency's submodules are empty directories. If the design reads
  files in one, add `submodules = ["PATH"]` to its entry (a nested one by its
  full path), or `submodules = true` for all of them. An error naming a
  submodule that is not checked out says which one to add.
- A git dependency's Git LFS files are pointers. If an error says a file is a
  Git LFS pointer, add `lfs = true` to the dependency's entry (it needs
  `git-lfs` installed); for the project's own files, run `git lfs pull`.
- Prefer `astli add` and `astli remove` over editing `[dependencies]` by
  hand: they take the name from the dependency's manifest and update the
  lock in one step. Neither works while the dependencies come from
  `Bender.yml`.
- In CI, pass `--locked` so a stale lockfile fails instead of changing.
  Add `--offline` where the cache was filled by an earlier `astli fetch`
  and the run must not clone or fetch.
- An `astli.toml` with a `[workspace]` table is a workspace root: every
  `astli.toml` below it is a member, but those `[workspace] exclude` names.
  The lock, `[resolve]`, patches, `[described]`, `[build]`, `[profile]` and
  `.astli/build` are the root's alone; put them there, not in a member.
  A command inside a member resolves the whole workspace and builds that
  member's design. `-p NAME` acts on the workspace package of that name
  from anywhere in it, as if run there (`fmt`, `lint`, `check`, `tree`,
  `files`, `elab`, `build`, `run`, `path`, `add`, `remove`,
  `bender export`). `--workspace` runs `fmt`,
  `lint`, `check` or `tree` once per package, root included, and fails if
  any fails: `astli check --workspace` at the root checks every member.
  A root with `[workspace]` and no `[package]` is virtual: it holds no
  files and depends on nothing, `fmt`/`lint`/`check`/`tree` there act on
  every member, and `files`/`build`/`run`/`add`/`remove` there need `-p`.
  Only its `[workspace]`, `[resolve]`, `[described]`, `[build]` and
  `[profile]` are read; dependencies, tops, `[fmt]` and `[lint]` go in a
  member.

## Coming from Bender

To move a project from `Bender.yml` to `astli.toml`, or to translate a
Bender command, read [references/bender.md](references/bender.md) first.

## Reading diagnostics

`--diag short` prints one `file:line:col: severity[code]: message` line per
diagnostic, and `--diag json` one JSON object per line on stderr, with ranges
and any fix. Use one of these when parsing output. `-q` drops the status
lines and keeps errors and results.
