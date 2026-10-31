# Getting started

## Installation

`astli` is distributed as a single static binary for Linux, macOS, and Windows. Choose your preferred installation method below:

=== "Installer"

    On Linux and macOS:

    ```sh
    curl --proto '=https' --tlsv1.2 -LsSf https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.sh | sh
    ```

    On Windows (PowerShell):

    ```powershell
    powershell -ExecutionPolicy Bypass -c "irm https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.ps1 | iex"
    ```

    The installer places `astli` in `~/.local/bin`, or `$XDG_BIN_HOME` if set.

    Each archive on the [releases page](https://github.com/fischeti/astli/releases) carries a build attestation, which the [GitHub CLI](https://cli.github.com) checks:

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

---

## Quick start in an existing project

`astli` works out of the box with zero configuration required. If your repository doesn't have an `astli.toml` manifest yet, `astli` simply treats the repository root as the package root, automatically gathering all `.sv`, `.svh`, `.v`, and `.vh` files while respecting `.gitignore`.

You can immediately format and lint your project:

```sh
# Check if any files require formatting (exits non-zero if unformatted)
astli fmt --check

# Format all files in place
astli fmt

# Run the linter across your sources
astli lint
```

If you prefer passing explicit files or already maintain an EDA filelist (`.f`), you can point `astli` directly to them:

```sh
astli lint rtl/*.sv
astli check -f design.f
```

---

## Adding an `astli.toml` manifest

As your project grows, create an `astli.toml` file at your repository root to configure source directories, exclude patterns, formatting guidelines, and linter settings.

You can generate a starter manifest automatically with `astli init`:

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

---

## Adding dependencies

`astli` makes pulling in external SystemVerilog IP as straightforward as adding dependencies in Cargo. `astli add` requires a git repository at its latest release, or a local directory:

```sh
astli add https://github.com/pulp-platform/common_cells.git
astli add --path ../tech_lib
```

Each writes an entry under `[dependencies]` in `astli.toml`, which you can also edit by hand:

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38.2" }
tech_lib = { path = "../tech_lib" }
```

When you run a command that operates on the design, `astli` fetches dependencies into a shared local cache, determines compatible versions, and records exact pinned commits in `astli.lock`:

```sh
# Verify your design, including resolved dependency sources
astli check --top my_soc_top

# View the full elaborated instance hierarchy
astli elab --top my_soc_top
```

!!! tip "Commit `astli.lock` to version control"
    Always check `astli.lock` into git. This guarantees that your team members and CI pipelines build against the exact same commit revisions every time. Use `astli update` whenever you intentionally want to bump dependencies.

!!! note "Bender compatibility"
    If your repository already uses `Bender.yml` and does not define a `[dependencies]` table in `astli.toml`, `astli` automatically imports the dependencies declared in your `Bender.yml`. Read the [Coming from Bender guide](bender.md) to learn more.

---

## Setting up CI

On GitHub Actions, [`astli-action`](https://github.com/fischeti/astli-action) installs a pinned `astli` and caches your dependencies. Provide the `--locked` flag, so that any unexpected lockfile discrepancies or dependency drift cause immediate, visible build failures:

```yaml
- uses: fischeti/astli-action@v0.4.0
- run: astli fmt --check
- run: astli lint --locked
- run: astli check --locked --top my_soc_top
```

For other CI systems, upgrades and pull request annotations, see the [CI Guide](ci.md).

---

## Using astli from an AI agent

The repository is also a [Claude Code](https://code.claude.com) plugin marketplace. Its `astli` plugin holds a skill that tells the agent which command to run for formatting, linting, checking and dependencies, and which mistakes to avoid, such as running `astli fmt` with no files to format a snippet:

```sh
/plugin marketplace add fischeti/astli
/plugin install astli@astli
```

The plugin's version is astli's, and a new skill comes with each release. Run `/plugin marketplace update astli` after upgrading `astli` to fetch it, or turn on auto-update for the marketplace under **Marketplaces** in `/plugin`. Where the skill and your installed `astli` disagree, the skill tells the agent to trust `astli --help`.

Other agents that read the [Agent Skills](https://agentskills.io) format can use the same skill: copy the directory `plugin/skills/astli/` into that agent's skills directory.
