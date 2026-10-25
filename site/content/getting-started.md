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

As your project grows, create an `astli.toml` file at your repository root to configure source directories, exclude patterns, formatting guidelines, and linter settings. `astli init` writes one with only the package's name in it, taken from the directory or from `Bender.yml`:

```sh
astli init
```

A fuller manifest looks like this:

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

`astli` makes pulling in external SystemVerilog IP as straightforward as adding dependencies in Cargo. Declare git repositories or local directory paths under `[dependencies]`:

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38" }
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

In your continuous integration workflows, pin the `astli` version and provide the `--locked` flag. This ensures that any unexpected lockfile discrepancies or dependency drift cause immediate, visible build failures:

```yaml
- uses: astral-sh/setup-uv@v7
- run: uvx astli@0.3.0 fmt --check
- run: uvx astli@0.3.0 lint --locked
- run: uvx astli@0.3.0 check --locked --top my_soc_top
```

For more detailed workflows and caching strategies, see the [CI Guide](ci.md).
