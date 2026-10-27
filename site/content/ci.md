# Continuous Integration (CI)

Integrating `astli` into your CI pipeline provides fast, automated checks on every pull request. Because `astli` exits with code `1` whenever an error or unformatted file is encountered, wiring it into automated workflows is straightforward.

---

## GitHub Actions

The [`astli-action`](https://github.com/fischeti/astli-action) sets `astli` up in a job: it installs the prebuilt binary, caches the Git dependencies `astli` downloads, and has every diagnostic annotate the pull request.

```yaml
name: SystemVerilog CI

on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: fischeti/astli-action@v0.4.0

      - name: Format check
        run: astli fmt --check

      - name: Lint
        run: astli lint --locked

      - name: Semantic and elaboration check
        run: astli check --locked --top soc_top
```

It runs on Linux, macOS and Windows runners. Each tag of the action installs the `astli` release of the same version, so the tag pins `astli`, and [Dependabot](https://docs.github.com/en/code-security/dependabot/working-with-dependabot/keeping-your-actions-up-to-date-with-dependabot) upgrades it through a pull request, where a change in the formatter's output or the lint rules shows up as a failing check:

```yaml
# .github/dependabot.yml
version: 2
updates:
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
```

The action's [README](https://github.com/fischeti/astli-action#inputs) lists its inputs, which choose another version, turn the cache or the annotations off, or name another lock file to key the cache on.

---

## Other CI systems

Elsewhere, run `astli` with `uvx` or `pipx`, pinned to a version:

```sh
uvx astli@0.4.0 fmt --check
uvx astli@0.4.0 lint --locked
uvx astli@0.4.0 check --locked --top soc_top
```

To skip cloning the dependencies on every run, cache the directory `ASTLI_CACHE_DIR` names, keyed on `astli.lock`. Its default depends on the OS, as the [environment variables reference](reference/environment.md) lists, so set it to a path of your own. Diagnostics in the plain one-line format, `--diag short` or `ASTLI_DIAGNOSTICS=short`, suit a log that tools read.

---

## CI best practices

### 1. Pin the `astli` version

!!! important "Prevent unexpected CI breaks"
    Always pin the exact version of `astli` in your workflow, through the action's tag or `astli@0.4.0`. As new linter rules or formatter refinements are released, an unpinned version could unexpectedly fail builds on unchanged legacy code.

### 2. Enforce lockfiles with `--locked`

Passing `--locked` instructs `astli` to verify that `astli.lock` exists and matches the dependency definitions in `astli.toml`. If a dependency has drifted or the lockfile is missing, `astli` fails the build immediately rather than silently modifying files in CI.

---

## Pull request annotations

With `--diag github`, `astli` writes each diagnostic as an `::error`, `::warning` or `::notice` workflow command, which GitHub Actions turns into an annotation on the line of the pull request's diff it is about; the log still shows its `file:line:col: severity[code]: message` line. The action sets `ASTLI_DIAGNOSTICS=github` for the rest of the job, so every step uses the format. Without the action, set it yourself:

```yaml
env:
  ASTLI_DIAGNOSTICS: github
```

The runner places an annotation by the file's path inside the checkout, which a package run writes in full; a file named relative to the working directory, as passed on the command line, is placed only when `astli` runs from the repository's root.

## Merge request annotations on GitLab

When running in GitLab CI, `--diag gitlab` formats diagnostics as a GitLab [Code Quality](https://docs.gitlab.com/ci/testing/code_quality/) artifact. GitLab parses this report to render findings directly on modified lines within merge request diffs.

Diagnostic output is emitted as a pure JSON array on `stderr`, allowing you to redirect it directly to an artifact file:

```yaml
lint:
  script:
    - uvx astli@0.4.0 lint --locked --diag gitlab 2> gl-code-quality-report.json
  artifacts:
    when: always
    reports:
      codequality: gl-code-quality-report.json
```

- **Failure artifacts**: Using `when: always` ensures the Code Quality report is preserved even when `astli` exits with an error upon finding violations.
- **Repository paths**: File paths in the report are automatically normalized relative to `CI_PROJECT_DIR`.
- **Global diagnostics**: Non-source failures (such as `--locked` rejecting an out-of-sync `astli.lock`) still cause the job to fail with a non-zero exit code, but are omitted from line-level diff annotations.
