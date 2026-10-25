# Continuous Integration (CI)

Integrating `astli` into your CI pipeline provides fast, automated checks on every pull request. Because `astli` exits with code `1` whenever an error or unformatted file is encountered, wiring it into automated workflows is straightforward.

---

## Example GitHub Actions workflows

Choose either `uv` (recommended for speed) or `pipx` (pre-installed on standard GitHub-hosted runners):

=== "GitHub Actions with uv"

    ```yaml
    name: SystemVerilog CI

    on: [push, pull_request]

    jobs:
      check:
        runs-on: ubuntu-latest
        steps:
          - uses: actions/checkout@v4

          - name: Set up uv
            uses: astral-sh/setup-uv@v7

          # Cache downloaded Git dependencies between workflow runs
          - name: Cache astli dependencies
            uses: actions/cache@v4
            with:
              path: ~/.cache/astli
              key: astli-${{ runner.os }}-${{ hashFiles('astli.lock') }}
              restore-keys: |
                astli-${{ runner.os }}-

          - name: Format check
            run: uvx astli@0.3.0 fmt --check

          - name: Lint
            run: uvx astli@0.3.0 lint --locked

          - name: Semantic and elaboration check
            run: uvx astli@0.3.0 check --locked --top soc_top
    ```

=== "GitHub Actions with pipx"

    ```yaml
    name: SystemVerilog CI

    on: [push, pull_request]

    jobs:
      check:
        runs-on: ubuntu-latest
        steps:
          - uses: actions/checkout@v4

          - name: Cache astli dependencies
            uses: actions/cache@v4
            with:
              path: ~/.cache/astli
              key: astli-${{ runner.os }}-${{ hashFiles('astli.lock') }}

          - name: Format check
            run: pipx run --spec astli==0.3.0 astli fmt --check

          - name: Lint
            run: pipx run --spec astli==0.3.0 astli lint --locked

          - name: Semantic and elaboration check
            run: pipx run --spec astli==0.3.0 astli check --locked --top soc_top
    ```

---

## CI best practices

### 1. Pin the `astli` executable version

!!! important "Prevent unexpected CI breaks"
    Always pin the exact version of `astli` in your workflow (e.g. `astli@0.3.0`). As new linter rules or formatter refinements are released, an unpinned version could unexpectedly fail builds on unchanged legacy code.

### 2. Enforce lockfiles with `--locked`

Passing `--locked` instructs `astli` to verify that `astli.lock` exists and matches the dependency definitions in `astli.toml`. If a dependency has drifted or the lockfile is missing, `astli` fails the build immediately rather than silently modifying files in CI.

### 3. Caching dependencies

`astli` stores downloaded repositories in `$HOME/.cache/astli` (or the path defined by `ASTLI_CACHE_DIR`). Caching this directory with `actions/cache` keyed on `astli.lock` avoids redundant Git clones across workflow runs, accelerating CI runtimes.

---

## Pull request annotations

To have GitHub Actions show each diagnostic on the line of the pull request's diff it is about, write them as workflow commands:

```sh
astli lint --diag github
```

Each diagnostic becomes an `::error`, `::warning` or `::notice` command, which the runner turns into an annotation; the log still shows its `file:line:col: severity[code]: message` line. The runner places an annotation by the file's path inside the checkout, which a package run writes in full; a file named relative to the working directory, as passed on the command line, is placed only when `astli` runs from the repository's root. To use the format for every step of a job, set it once:

```yaml
env:
  ASTLI_DIAGNOSTICS: github
```

Other CI systems and editors read the plain one-line format, `--diag short`.
