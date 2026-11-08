# Diagnostic formats

Every command writes its diagnostics to `stderr`, so its output on `stdout` is unaffected. `--diag <FORMAT>`, or the `ASTLI_DIAGNOSTICS` environment variable, chooses how they are written. The flag wins over the variable.

| Format | Writes |
| :--- | :--- |
| `pretty` | The default: each diagnostic with the source it points at, and the macro calls and includes it came through. A file shows its first 20 and says how many more it has. |
| `short` | One `file:line:col: severity[code]: message` line per diagnostic, every one of them. Suits editors and log scrapers. |
| `github` | GitHub Actions workflow commands, which annotate a pull request. |
| `json` | One JSON record per line. |
| `gitlab` | A GitLab Code Quality report. |

## `github`

Each diagnostic becomes an `::error`, `::warning` or `::notice` command, which GitHub turns into an annotation on the line it is about. The log still shows the `short` line.

GitHub places an annotation by the file's path in the checkout. A package run writes full paths, so they always match. A file named on the command line keeps the path it was given, which matches only when `astli` runs from the repository root.

## `json`

Each line is one record:

```json
{"severity":"error","code":"undefined-macro","message":"`NOPE is not defined",
 "path":"macro.sv","start":{"line":2,"column":11,"offset":28},"end":{"line":2,"column":15,"offset":32},
 "label":"not defined here","labels":[],"notes":["the reference stands as written"],"help":[],
 "spelled":{"path":"macro.sv","start":{"line":1,"column":13,"offset":12},"end":{"line":1,"column":18,"offset":17}},
 "expansion":[{"macro":"`BAD","path":"macro.sv","start":{"line":2,"column":11,"offset":28},"end":{"line":2,"column":15,"offset":32}}],
 "included_from":[],"fix":[]}
```

The example is wrapped here, but each record is a single line.

- `start` and `end` are where the diagnostic is reported. When a macro wrote the text, that is the macro call, and `spelled` is where the text is spelled in the macro's body.
- `expansion` lists the macro calls the text came through, and `included_from` the includes.
- `fix` lists the edits [`lint --fix`](../linting.md#applying-fixes) would make, each as a `text` and the range it replaces.
- A problem with no place in a source, such as a file that can't be read, has `code`, `path`, `start` and `end` set to `null`.

Nothing else goes to `stderr`: there is no summary line and no progress, so every line parses.

```sh
astli lint --diag json 2>&1 | jq -r 'select(.severity == "error") | .path'
```

## `gitlab`

The whole run writes one JSON array, `[]` when there is nothing to report. Each finding has a fingerprint, so GitLab can track it across pipelines. Two identical findings in one file are told apart by their order, not their line, so adding a line above them doesn't make them new. Paths are relative to `CI_PROJECT_DIR`.

A problem with no place in a source, such as `--locked` refusing a stale `astli.lock`, still fails the run but isn't in the report. [CI](../ci.md#gitlab-ci) shows the job that collects it.
