---
name: astli
description: Format, lint and check SystemVerilog with the `astli` CLI, and manage astli packages and dependencies. Use when editing `.sv`, `.svh`, `.v` or `.vh` files, when a repository has an `astli.toml`, `astli.lock` or `Bender.yml`, or when asked to format, lint, check, elaborate, or build a filelist for a SystemVerilog design, or to migrate a project from Bender.
---

# astli

`astli` is a SystemVerilog toolchain: a formatter, a linter, a semantic
checker, filelist tools, and a package manager. One binary, one subcommand
per job.

This skill describes the common workflows. The installed binary is the
authority on its own flags: when `astli <command> --help` disagrees with this
file, follow `--help`. The full guide is at <https://fischeti.github.io/astli/>.

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
```

`check` reports only what a conforming compiler would also reject: undeclared
names, missing modules, wrong ports and parameters, multiple drivers. Treat
every error as real. Without `--top`, instances inside generate blocks are
checked leniently, so pass `--top` (or set `[top.NAME]` in `astli.toml`) for
a complete check.

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

## Dependencies

Dependencies live under `[dependencies]` in `astli.toml`, or come from
`Bender.yml` when `astli.toml` has no `[dependencies]` table. Commands that
need them fetch into a shared cache and pin commits in `astli.lock`.

```sh
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
- In CI, pass `--locked` so a stale lockfile fails instead of changing.

## Coming from Bender

To move a project from `Bender.yml` to `astli.toml`, or to translate a
Bender command, read [references/bender.md](references/bender.md) first.

## Reading diagnostics

`--diag short` prints one `file:line:col: severity[code]: message` line per
diagnostic, and `--diag json` one JSON object per line on stderr, with ranges
and any fix. Use one of these when parsing output. `-q` drops the status
lines and keeps errors and results.
