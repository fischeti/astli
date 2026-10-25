# Terminal output

> **Status:** built but for a transfer's counts in progress (C9). Milestone
> M15 in [`plan.md`](plan.md#5-milestones).

How `astli` talks to the person running it. A diagnostic in a source file
already has a look, `astli-diag`'s: a severity in its colour, the code, the
source under it. Everything else the driver writes to stderr is a bare line,
prefixed `astli:` or not, with a hint after a semicolon, and a fetch from git
says nothing until it is done. This is the one look all of it shares.

## What is written where

The result of a command goes to stdout: formatted text, a diff, a file list,
a tree, a dump. Everything about the run goes to stderr, in one of five
kinds:

| Kind | Is | Example |
| --- | --- | --- |
| Diagnostic | a problem at a place in a file, with the file under it | `[undefined-macro] Error: ...` (`astli-diag`) |
| Problem | an error, warning or note with no place to show | `error: astli.lock would change, and --locked keeps it as it is` |
| Status | something the run did | `    Fetching common_cells (github.com/pulp-platform/common_cells)` |
| Progress | something the run is doing, redrawn in place | `  ⠙ Fetching axi (github.com/pulp-platform/axi) 1,204 objects, 2.1 MiB` |
| Summary | a tally, once at the end | `3 errors, 1 warning` |

At a terminal a run that fetches and then fails reads as:

```text
    Fetching common_cells (github.com/pulp-platform/common_cells)
    Fetching axi (github.com/pulp-platform/axi)
    Updating axi 0.39.1 -> 0.39.4
warning: `[described.idma]` describes a package the design does not hold
error: astli.lock would change, and `--locked` keeps it as it is
  = help: `astli update` writes it
```

## Decisions

| # | Decision | Why |
| --- | --- | --- |
| C1 | The look is the driver's: a `term` module in `astli-cli`. `astli-diag` keeps rendering diagnostics, and nothing else | `astli-diag` is a library whose readers, a language server or bender calling astli, want a diagnostic rendered and no terminal. A progress bar or a status verb is how one program talks to a person, and would bring its dependencies to every reader of the library. |
| C2 | `term` owns stderr. Every message goes through it, `say!` for a line in place of `eprintln!`, and a worker's buffered diagnostics are handed to it whole | One place decides colour and `-q`, and clears a progress line before writing above it, so the two never interleave. A file's report written in one piece keeps parallel workers from tearing it. |
| C3 | Colour is `--color auto\|always\|never`, global, with `NO_COLOR` and `CLICOLOR_FORCE` honoured; `auto` colours a stream only if it is a terminal, stdout and stderr each decided alone. `anstream` and `anstyle` write it, and `astli-diag`'s `Style` is set from the same choice | The conventions every other tool reads. A diff piped to `git apply` or a pager stays a patch while the diagnostics beside it keep their colour. `anstream` strips what a stream cannot take, so no message checks for itself, and `render.rs` stops writing escapes by hand. |
| C4 | The palette is the diagnostics': an error red, a warning yellow, a note or help ariadne's advice shade, what explains it blue. A status verb is bold green, and bold cyan with its spinner while its step runs; a summary is bold. | A warning looks the same whether or not it has a snippet, so the eye learns one set of colours. |
| C5 | A problem is `error: message` (or `warning:`, `note:`), with indented `= note:` and `= help:` lines under it. What to do about it is a `help`, never a clause after a semicolon. No `astli:` prefix. A note is left out under `-q`, an error or a warning never. A library's error is reported as it displays itself, with no `caused by:` chain | The severity is what a reader scans for, and it is the shape of `rustc`'s and `ariadne`'s reports. A hint on its own line can be read, or skipped, alone. `-q` keeps what may need acting on. `astli-package`'s and `astli-resolve`'s errors already name their cause in their message, so a chain would say it twice. |
| C6 | A problem at a line of a file is a diagnostic, rendered with the file under it, and written as one line under `--diag short`: a name declared twice, a reference declared in no file, a filelist's error, a manifest's | `--diag short` then covers every place an editor can jump to, and the snippet shows what the message describes. |
| C7 | A status is a verb right-aligned in twelve columns, then what it acted on: `Cloning`, `Fetched`, `Adding`, `Updating` and `Removing` a lock's packages, `Patched`, `Unpatched`, `Required`, `Created`, `Removed`. `status!` writes it unless `-q`, piped or not, so no call site asks. A count is spelled as its noun wants, never `file(s)` | Cargo's shape, which most readers already know: the column makes a long run scannable, and a CI log keeps the record of what was fetched. |
| C8 | Progress is drawn only on a terminal and never under `-q`, the status line the step ends as with a spinner in the margin left of its verb, and replaced by that line when it ends. Anywhere else the status line is written as the step begins. `indicatif` draws it | A redrawn line in a log is noise; the status line that replaces it is the record. In the margin the verb keeps its column, so the line does not shift when it ends. Written first in a log, it is the last line while the step runs, so a run that hangs or fails says what it was doing. `indicatif` can hide its spinner around a line written above it, which C2 needs. |
| C9 | `astli-resolve` reports what it does through `Report`, which the driver implements, and a `Store` reports nothing until given one: a step is begun and ended, one at a time, for fetching a mirror, made first if there is none, or checking a package's commit out, and the transfer's counts within it; it writes nothing itself | The resolver stays free of terminal code. A mirror is bare, so making one is a fetch too, and said as one. A fetch is named by the dependency it is for, then, dimmed, the URL its mirror is keyed by, and a checkout by its package: the name is what a reader wrote, and the URL says which fork, and why one fetch may serve two packages of one repository. Counts come from the `Progress` `gix` already takes, which today is `Discard`. |

## Prior art

Cargo's `Shell` is the model for C2 and C7: one owner of stderr, colour
decided once, a status verb column. `uv` draws its fetches with `indicatif`
as C8 does. `rustc` shapes a problem with no snippet as C5 does.
