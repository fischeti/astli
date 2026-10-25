# Terminal output

> **Status:** designed; nothing built. Milestone M15 in
> [`plan.md`](plan.md#5-milestones).

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
| Status | something the run did | `    Fetching common_cells v1.38.0` |
| Progress | something the run is doing, redrawn in place | `    Fetching axi ⠙ 1,204 objects, 2.1 MiB` |
| Summary | a tally, once at the end | `3 errors, 1 warning` |

At a terminal a run that fetches and then fails reads as:

```text
    Fetching common_cells v1.38.0
    Fetching axi v0.39.4
    Updating axi v0.39.1 -> v0.39.4
warning: `[described.idma]` describes a package the design does not hold
error: astli.lock would change, and `--locked` keeps it as it is
  = help: `astli update` writes it
```

## Decisions

| # | Decision | Why |
| --- | --- | --- |
| C1 | The look is the driver's: a `term` module in `astli-cli`. `astli-diag` keeps rendering diagnostics, and nothing else | `astli-diag` is a library whose readers, a language server or bender calling astli, want a diagnostic rendered and no terminal. A progress bar or a status verb is how one program talks to a person, and would bring its dependencies to every reader of the library. |
| C2 | One `Shell` owns stderr. Every message goes through it, never `eprintln!`; a worker's buffered diagnostics are handed to it whole | One place decides colour and `-q`, and clears a progress line before writing above it, so the two never interleave. A file's report written in one piece keeps parallel workers from tearing it. |
| C3 | Colour is `--color auto\|always\|never`, global, with `NO_COLOR` and `CLICOLOR_FORCE` honoured; `auto` colours a stream only if it is a terminal, stdout and stderr each decided alone. `anstream` and `anstyle` write it, and `astli-diag`'s `Style` is set from the same choice | The conventions every other tool reads. A diff piped to `git apply` or a pager stays a patch while the diagnostics beside it keep their colour. `anstream` strips what a stream cannot take, so no message checks for itself, and `render.rs` stops writing escapes by hand. |
| C4 | The palette is the diagnostics': an error red, a warning yellow, a note or help ariadne's advice shade, what explains it blue. A status verb is bold green, a summary bold | A warning looks the same whether or not it has a snippet, so the eye learns one set of colours. |
| C5 | A problem is `error: message` (or `warning:`, `note:`), with indented `= note:` and `= help:` lines under it, and a library error's `source()` chain as `caused by:` lines. What to do about it is a `help`, never a clause after a semicolon. No `astli:` prefix | The severity is what a reader scans for, and it is the shape of `rustc`'s and `ariadne`'s reports. A hint on its own line can be read, or skipped, alone. |
| C6 | A problem at a line of a file is a diagnostic, rendered with the file under it, and written as one line under `--diag short`: a name declared twice, a reference declared in no file, a filelist's error, a manifest's | `--diag short` then covers every place an editor can jump to, and the snippet shows what the message describes. |
| C7 | A status is a verb right-aligned in twelve columns, then what it acted on: `Fetching`, `Cloning`, `Checking out`, `Adding`, `Updating`, `Removing`, `Patched`, `Created`. It is written unless `-q`, piped or not, and a count is spelled as a word wants, never `file(s)` | Cargo's shape, which most readers already know: the column makes a long run scannable, and a CI log keeps the record of what was fetched. |
| C8 | Progress is drawn only on a terminal and never under `-q`, and when it ends is replaced by its status line. `indicatif` draws it | A redrawn line in a log is noise; the status line that replaces it is the record. `indicatif` can hide its bar around a line written above it, which C2 needs. |
| C9 | `astli-resolve` reports what it does through a trait the driver implements, with an event per step (cloning a mirror, fetching one, checking a commit out) and the transfer's counts within it; it writes nothing itself | The resolver stays free of terminal code, and its tests pass a reporter that records. Counts come from the `Progress` `gix` already takes, which today is `Discard`. |

## Prior art

Cargo's `Shell` is the model for C2 and C7: one owner of stderr, colour
decided once, a status verb column. `uv` draws its fetches with `indicatif`
as C8 does. `rustc` shapes a problem with no snippet as C5 does.
