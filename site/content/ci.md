# Continuous integration

`astli` exits with code 1 when a file has errors or isn't formatted, so each command can be a CI step as it is.

## GitHub Actions

[`astli-action`](https://github.com/fischeti/astli-action) installs the prebuilt binary, caches the Git dependencies `astli` fetches, and turns every diagnostic into an annotation on the pull request.

```yaml
name: SystemVerilog CI

on: [push, pull_request]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: fischeti/astli-action@v0.5.0

      - name: Format check
        run: astli fmt --check

      - name: Lint
        run: astli lint --locked

      - name: Check
        run: astli check --locked --top soc_top
```

In a [workspace](packages.md#workspaces), `--workspace` on `fmt`, `lint` and `check` covers every package in one step, and the step fails if any package does.

It runs on Linux, macOS and Windows runners. Each tag of the action installs the `astli` release of the same version, so the tag pins `astli`. [Dependabot](https://docs.github.com/en/code-security/dependabot/working-with-dependabot/keeping-your-actions-up-to-date-with-dependabot) can then upgrade it through a pull request, where any change to the formatter's output or the lint rules shows up as a failing check:

```yaml
# .github/dependabot.yml
version: 2
updates:
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
```

The action sets `ASTLI_DIAGNOSTICS=github` for the rest of the job. Without the action, set it yourself. The action's [README](https://github.com/fischeti/astli-action#inputs) lists its inputs, which choose another version, turn off the cache or the annotations, or key the cache on another lock file.

## GitLab CI

`--diag gitlab` writes a [Code Quality](https://docs.gitlab.com/ci/testing/code_quality/) report, which GitLab shows on the lines of a merge request's diff:

```yaml
lint:
  script:
    - uvx astli@0.5.0 lint --locked --diag gitlab 2> gl-code-quality-report.json
  artifacts:
    when: always
    reports:
      codequality: gl-code-quality-report.json
```

`when: always` keeps the report when `astli` fails the job, which is when it has something to show.

## Other CI systems

Run `astli` with `uvx` or `pipx`:

```sh
uvx astli@0.5.0 fmt --check
uvx astli@0.5.0 lint --locked
uvx astli@0.5.0 check --locked --top soc_top
```

To avoid cloning the dependencies on every run, cache the directory `ASTLI_CACHE_DIR` names, keyed on `astli.lock`. Its default differs by OS (see [environment variables](reference/environment.md)), so set it to a path of your own. `--diag short` gives one line per diagnostic, which suits a log other tools read.

## Pin the version and the lock

Pin `astli` to an exact version, through the action's tag or `astli@VERSION`. A new release may add lint rules or change the formatter's output, and an unpinned run would fail on code nobody changed.

Pass `--locked` to every command that reads dependencies. It fails the run if `astli.lock` is missing or doesn't match `astli.toml`, instead of writing a new one.

[Diagnostic formats](reference/diagnostics.md) describes `github`, `gitlab` and the other formats in full.
