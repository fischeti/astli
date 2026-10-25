# Dependencies

`astli` provides built-in dependency management designed specifically for hardware designs. It fetches packages from Git repositories or local directories, resolves compatible versions, and records exact snapshots in an `astli.lock` lockfile.

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38" }
idma = { git = "https://github.com/pulp-platform/iDMA.git", rev = "main" }
tech = { path = "../tech" }
```

### Specifying dependency sources

- **`git` with `version`**: Resolves semver requirements (e.g. `^1.38` or `1.38.0`) by selecting the highest matching `vX.Y.Z` tag available in the remote repository.
- **`git` with `rev`**: Pins to an explicit Git branch, tag name, or commit SHA.
- **`path`**: Points to a local directory relative to the current package root. Ideal for monorepos or in-tree development.

Each dependency is treated as an independent `astli` package. If a dependency includes its own `astli.toml`, its source boundaries, features, and transitives are respected; otherwise, standard default conventions apply. Code quality findings (linting and checks) originating from third-party dependencies are ignored by default.

---

## Version resolution and pinning

`astli` requires a unified dependency graph: exactly one version of any given package is selected for the entire design.

If two dependencies require mutually incompatible versions of the same package, `astli` raises a version conflict error.

??? info "Resolving version conflicts with `pinned`"
    If two upstream packages specify conflicting requirements, the root package can force a specific version using `[resolve] pinned`:

    ```toml
    [resolve]
    pinned = ["common_cells"]
    ```

    When a package is pinned, the root's requirement takes absolute precedence. `astli` will select the root package's chosen version and downgrade conflicting upstream requirements to warnings rather than fatal errors.

---

## Lockfiles (`astli.lock`)

When dependencies are resolved, the exact commits and revisions chosen are written to `astli.lock` next to your `astli.toml`.

```sh
# Re-resolve all dependencies to latest compatible versions and update the lockfile
astli update

# Update only a single specific dependency
astli update common_cells
```

!!! tip "Commit your lockfile"
    Always check `astli.lock` into source control. Future builds will reproduce the exact same commit revisions across machines without re-querying Git remotes.

### Enforcing lockfiles in CI

In automated CI environments, you want to guarantee that dependencies are never updated implicitly. Pass the `--locked` flag to any command:

```sh
astli check --locked --top soc_top
```

If `astli.lock` is missing or out of sync with `astli.toml`, `astli` exits with an error instead of modifying the lockfile.

---

## The shared cache

To save disk space and network bandwidth, `astli` downloads and checks out dependencies into a single shared cache on your machine, accessible across all your local projects.

```sh
# Pre-fetch all dependencies into cache (useful for offline work or container caching)
astli fetch

# Print the resolved filesystem path of a dependency
astli path common_cells

# Show the location of the local cache directory
astli cache dir

# Clear the cached repositories
astli cache clean
```

The default cache directory is located at:
- **Linux**: `$XDG_CACHE_HOME/astli` (typically `~/.cache/astli`)
- **macOS**: `~/Library/Caches/astli`
- **Windows**: `%LOCALAPPDATA%\astli`

You can customize this location by setting the `ASTLI_CACHE_DIR` environment variable.

!!! note "Read-only cache protection"
    Cached dependencies are marked read-only to prevent accidental edits from leaking across projects. Never manually modify files inside the cache; if you need to alter or customize a dependency's module, use [overrides](features.md#overrides) in your project.
