# Getting started

## Installation

`astli` is one binary, for Linux, macOS and Windows.

=== "Installer"

    On Linux and macOS:

    ```sh
    curl -LsSf https://astli.dev/install.sh | sh
    ```

    On Windows (PowerShell):

    ```powershell
    powershell -ExecutionPolicy Bypass -c "irm https://astli.dev/install.ps1 | iex"
    ```

    Both run the installer attached to the [latest release](https://github.com/fischeti/astli/releases/latest), which you can also fetch from there directly:

    ```sh
    curl --proto '=https' --tlsv1.2 -LsSf https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.sh | sh
    ```

    The installer places `astli` in `~/.local/bin`, or `$XDG_BIN_HOME` if set.

    Release archives on the [releases page](https://github.com/fischeti/astli/releases) include build provenance attestations, which can be verified with the [GitHub CLI](https://cli.github.com):

    ```sh
    gh attestation verify astli-cli-x86_64-unknown-linux-gnu.tar.xz --repo fischeti/astli
    ```

=== "uv"

    Install globally using [uv](https://docs.astral.sh/uv/):

    ```sh
    uv tool install astli
    ```

    Or run it directly on demand without installing:

    ```sh
    uvx astli fmt top.sv
    ```

=== "pip"

    Install with standard Python package managers:

    ```sh
    pip install astli
    ```

    Or with [pipx](https://pipx.pypa.io), which installs `astli` into an isolated environment and exposes it on your `PATH`:

    ```sh
    pipx install astli
    pipx run astli fmt top.sv
    ```

=== "cargo"

    Install a precompiled binary using [cargo-binstall](https://github.com/cargo-bins/cargo-binstall), or compile directly from source:

    ```sh
    # Fast installation via prebuilt binary
    cargo binstall astli-cli

    # Or build from source
    cargo install astli-cli
    ```

## Quick start in an existing project

`astli` needs no configuration to start. Without an `astli.toml`, the repository root is the package root, and its `.sv`, `.svh`, `.v` and `.vh` files that `.gitignore` does not ignore are the package's files:

```sh
# Check if any files require formatting (exits non-zero if unformatted)
astli fmt --check

# Format all files in place
astli fmt

# Run the linter across your sources
astli lint
```

Files and filelists can be named instead:

```sh
astli lint rtl/*.sv
astli check -f design.f
```

## Adding an `astli.toml` manifest

An `astli.toml` at the repository root configures where sources are, what to exclude, the layout and the lint levels. `astli init` writes one:

```sh
astli init
```

`astli init` infers the package name from the current directory (or from an existing `Bender.yml` if present). A customized manifest might look like this:

```toml
[package]
name = "my_soc"
sources = ["hw"]             # Search within hw/ rather than the entire repo
exclude = ["hw/scripts/**"]  # Skip auxiliary or third-party script directories

[fmt]
width = 100                 # Maximum line width
indent = 2                  # Number of spaces per indentation level
```

See [Packages](packages.md) for details on package layout and visibility.

## Adding dependencies

`astli add` adds a dependency from a Git repository or a local directory:

```sh
astli add https://github.com/pulp-platform/common_cells.git
astli add --path ../tech_lib
```

This records the dependency under `[dependencies]` in `astli.toml` and updates `astli.lock`. You can also configure dependencies manually:

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38.2" }
tech_lib = { path = "../tech_lib" }
```

A command that reads the design fetches the dependencies into a shared cache, chooses their versions, and records the commits in `astli.lock`:

```sh
# Verify your design, including resolved dependency sources
astli check --top my_soc_top

# View the full elaborated instance hierarchy
astli elab --top my_soc_top
```

!!! tip "Commit `astli.lock` to version control"
    Commit `astli.lock`, so every machine and every CI run builds the same commits. `astli update` moves them when you mean to.

!!! note "Bender compatibility"
    With a `Bender.yml` and no `[dependencies]` table in `astli.toml`, `astli` reads the dependencies from `Bender.yml`. See [Coming from Bender](bender.md).

## Setting up CI

On GitHub Actions, [`astli-action`](https://github.com/fischeti/astli-action) installs a pinned `astli` and caches the dependencies. `--locked` fails the run if `astli.lock` does not match `astli.toml`:

```yaml
- uses: fischeti/astli-action@v0.5.0
- run: astli fmt --check
- run: astli lint --locked
- run: astli check --locked --top my_soc_top
```

For other CI systems, upgrades and pull request annotations, see the [CI Guide](ci.md).

## AI agent integration

A plugin for [Claude Code](https://code.claude.com), and any agent that reads [Agent Skills](https://agentskills.io), tells the agent how to use `astli`'s commands.

To install the plugin in Claude Code:

```sh
/plugin marketplace add fischeti/astli
/plugin install astli@astli
```

For other agents supporting Agent Skills, copy the `plugin/skills/astli/` directory into your agent's skills directory.
