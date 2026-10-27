# Terminal output

> **Status:** built. Milestone M15 in [`plan.md`](plan.md#5-milestones).

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
| Problem | an error, warning or note with no place to show | ``error: `astli.lock` would change, and `--locked` keeps it as it is`` |
| Status | something the run did | `    Fetching common_cells (github.com/pulp-platform/common_cells)` |
| Progress | something the run is doing, redrawn in place | `  ⠙ Fetching axi (github.com/pulp-platform/axi) 1,204 objects, 2.1 MiB` |
| Summary | a tally, once at the end | `3 errors, 1 warning` |

A run that fetches and then fails reads as below. At a terminal the
backticks give way to a style for each kind of span (C10).

```text
    Fetching common_cells (github.com/pulp-platform/common_cells)
    Fetching axi (github.com/pulp-platform/axi)
    Updating axi 0.39.1 -> 0.39.4
warning: `[described.idma]` describes a package the design does not hold
error: `astli.lock` would change, and `--locked` keeps it as it is
  = help: `astli update` writes it
```

## Decisions

| # | Decision | Why |
| --- | --- | --- |
| C1 | The look is the driver's: a `term` module in `astli-cli`. `astli-diag` keeps rendering diagnostics, and nothing else | `astli-diag` is a library whose readers, a language server or bender calling astli, want a diagnostic rendered and no terminal. A progress bar or a status verb is how one program talks to a person, and would bring its dependencies to every reader of the library. |
| C2 | `term` owns stderr. Every message goes through it, `say!` for a line in place of `eprintln!`, and a worker's buffered diagnostics are handed to it whole | One place decides colour and `-q`, and clears a progress line before writing above it, so the two never interleave. A file's report written in one piece keeps parallel workers from tearing it. |
| C3 | Colour is `--color auto\|always\|never`, global, with `NO_COLOR` and `CLICOLOR_FORCE` honoured; `auto` colours a stream only if it is a terminal, stdout and stderr each decided alone. `anstream` and `anstyle` write it, and `astli-diag`'s `Style` is set from the same choice | The conventions every other tool reads. A diff piped to `git apply` or a pager stays a patch while the diagnostics beside it keep their colour. `anstream` strips what a stream cannot take, so no message checks for itself, and `render.rs` stops writing escapes by hand. |
| C4 | The palette is the diagnostics': an error red, a warning yellow, a note or help ariadne's advice shade, what explains it blue. A status verb is bold green, and bold cyan with its spinner while its step runs; a summary is bold. A problem's message is not, as a diagnostic's is not, so that a span in it stands out | A warning looks the same whether or not it has a snippet, so the eye learns one set of colours. |
| C5 | A problem is `error: message` (or `warning:`, `note:`), with indented `= note:` and `= help:` lines under it. What to do about it is a `help`, never a clause after a semicolon. No `astli:` prefix. A note is left out under `-q`, an error or a warning never. A library's error is reported as its `message()` says, with no `caused by:` chain | The severity is what a reader scans for, and it is the shape of `rustc`'s and `ariadne`'s reports. A hint on its own line can be read, or skipped, alone. `-q` keeps what may need acting on. `astli-package`'s and `astli-resolve`'s errors already name their cause in their message, so a chain would say it twice. |
| C6 | A problem at a line of a file is a diagnostic, rendered with the file under it, and written as one line under `--diag short`: a name declared twice, a reference declared in no file, a filelist's error, a manifest's | `--diag short` then covers every place an editor can jump to, and the snippet shows what the message describes. |
| C7 | A status is a verb right-aligned in twelve columns, then what it acted on: `Cloning`, `Fetched`, `Adding`, `Updating` and `Removing` a lock's packages, `Patched`, `Unpatched`, `Required`, `Created`, `Removed`, `Fixed`. `status!` writes it unless `-q`, piped or not, so no call site asks. A count is spelled as its noun wants, never `file(s)` | Cargo's shape, which most readers already know: the column makes a long run scannable, and a CI log keeps the record of what was fetched. |
| C8 | Progress is drawn only on a terminal and never under `-q`, the status line the step ends as with a spinner in the margin left of its verb, then how far a transfer has come, and replaced by that line when it ends. Steps running at once each have a line, below everything written so far. Anywhere else the status line is written as the step begins. `indicatif` draws it | A redrawn line in a log is noise; the status line that replaces it is the record. In the margin the verb keeps its column, so the line does not shift when it ends, and the counts after it can grow without moving the spinner. Written first in a log, it is among the last lines while the step runs, so a run that hangs or fails says what it was doing. A line for each step shows which fetch a run still waits on, as one line for all of them could not. `indicatif` can hide its spinners around a line written above them, which C2 needs. |
| C9 | `astli-resolve` reports what it does through `Report`, which the driver implements, and a `Store` reports nothing until given one: a step is begun and ended, several at once for different repositories, for fetching a mirror, made first if there is none, or checking a package's commit out, and the transfer's counts within it; it writes nothing itself | The resolver stays free of terminal code. A mirror is bare, so making one is a fetch too, and said as one. A fetch is named by the dependency it is for, then, dimmed, the URL its mirror is keyed by, and a checkout by its package: the name is what a reader wrote, and the URL says which fork, and why one fetch may serve two packages of one repository. Counts come from the progress tree `gix` fills as it receives a pack, read ten times a second while a fetch runs, and the task deepest in it is what `advance` says. |
| C10 | A name, a version, a path or what a person types is a span of a message marked by its kind: a package's, a feature's or a design unit's name bold, a version, branch, tag or commit cyan, a path or URL italic, and a command, a flag or a manifest's key or table bold cyan. Where stderr takes no colour each is in backticks. `astli-package`'s `mark` marks them, for its own messages, `astli-resolve`'s and the driver's, and the driver styles them. A library's error keeps the marks in `message()`, and displays with backticks. A path leading a message as its place, `astli.toml: …`, is not a span, and a diagnostic in a source file keeps its backticks | The eye finds a name in a line without the clutter of backticks, and learns one look for each kind. With no colour nothing else sets a name apart from the words around it. A message is a `String` from the library to the driver, built into longer ones at every level, so a mark is control characters inside it rather than a type each level would carry, and `Display` shows a reader outside the driver what it always did. A source diagnostic quotes SystemVerilog, as `rustc` quotes Rust, and `--diag short` writes it for tools that take no style. |
| C11 | `--diag github` writes each diagnostic as a GitHub Actions workflow command, `::error` (or `::warning`, `::notice`) with the file, its start and end, and the code as the title; the message is the line `--diag short` writes. It is chosen, never detected from `GITHUB_ACTIONS`. A problem with no place stays as C5 has it | The runner turns the command into an annotation on the pull request's diff, which a problem matcher, a pattern registered beside the workflow, would do only once someone writes one. The log shows a command's message alone, so the message carries the place too. Detecting the runner would take the snippets out of every existing workflow's log, where a reader wants them; a setup action can set `ASTLI_DIAGNOSTICS`. An annotation needs a line to hang on, so a problem without one is left to the log. |
| C12 | `--diag json` and `--diag gitlab` make stderr one document for a program, and nothing else: no status, progress or note, as under `-q`, no colour, a name in backticks, and no tally of the files with errors. `json` writes a line for each diagnostic as it is reported, its place, label, notes, help, the text a macro call stands for, the calls it came through, the includes it was reached from and the edits that fix it, every key in every record; a problem with no place is a record with the place null. `gitlab` keeps every diagnostic to the end of the run and writes a Code Quality report, one array, `[]` for none, its paths from `CI_PROJECT_DIR`; a problem with no place is left out | Stdout holds the result of `files`, `pickle` and `fmt --diff`, so a diagnostic there would mix with it; on stderr every format is redirected alike, `2> gl-code-quality-report.json`. A status line, a colour or a tally in it would leave a reader a document it cannot parse, and the tally only counts what the reader holds. A key always present lets `jq` read a record without asking first. A Code Quality report is one array, so it can only be written once all of it is known. GitLab compares a merge request's findings with its target's by fingerprint, so a fingerprint hashes the path, code and message, and the count of alike findings before it, but not the line, which an edit above it would move; FNV-1a, since `std`'s hasher may change between builds. GitLab places a finding by its path in the checkout, which a package run writes in full. |

## Prior art

Cargo's `Shell` is the model for C2 and C7: one owner of stderr, colour
decided once, a status verb column. `uv` draws its fetches with `indicatif`
as C8 does. `rustc` shapes a problem with no snippet as C5 does.
