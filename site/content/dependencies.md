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

### Inspecting the dependency tree

`astli tree` prints the resolved dependencies, each package under the ones that need it, with the version it resolved to:

```console
$ astli tree
soc
├── axi 0.39.6
│   └── common_cells 1.37.0
└── common_cells 1.37.0
```

A package without a version tag shows its commit instead, and a local or patched one where it is read from. A package needed in more than one place lists its own dependencies only where it first appears, and is marked `(*)` elsewhere.

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
    Cached dependencies are marked read-only to prevent accidental edits from leaking across projects. Never manually modify files inside the cache; if you need to alter or customize a dependency's module, use [overrides](features.md#overrides) in your project, or work on it in a clone with [`astli patch`](#working-on-a-dependency-in-place).

---

## Describing a dependency without an `astli.toml`

A dependency that has no `astli.toml` is read with every default: all of its files, and only its `include` directory exported to you. Many IPs keep files for several flows side by side, such as FPGA and ASIC versions of the same cell, or keep their headers elsewhere, so a design built on them sees a name declared twice or misses a header.

Until the dependency ships a manifest, your root `astli.toml` can describe its files for it:

```toml
[described.tech_cells_generic]
exclude = ["src/fpga/**"]

[described.idma]
include-dirs = ["src/include", "target/rtl/include"]
```

A description holds the `sources`, `include-dirs` and `exclude` that the dependency's own `[package]` would, with paths relative to the dependency's root. It changes which files are read, never which versions are chosen, so `astli.lock` is unaffected.

A description is a stopgap for a manifest upstream, and astli keeps it in sight:

- Every run that reads the dependencies names the described ones in one line (hidden by `-q`), and `astli tree` marks them `(described)`.
- Once the dependency has its own `astli.toml`, that manifest is read instead, and astli warns you to remove the description.
- A description of a package the design does not use, or an `exclude` pattern that matches none of the dependency's files, is a warning.
- Only the root's descriptions count, so a library cannot pass its descriptions on to the designs that use it.

To fix it upstream, copy the description under `[package]` in an `astli.toml` in the dependency's repository, for example in a clone from [`astli patch`](#working-on-a-dependency-in-place), and propose it there.

---

## Working on a dependency in place

When iterating on hardware IP alongside a top-level SoC or testbench, you often need to make and test edits in a dependency without publishing premature Git tags or making temporary edits to your manifest.

`astli patch` (aliased as `astli clone`) lets you temporarily override any dependency with a local working directory:

```sh
# Clone common_cells at its locked commit into .astli/patch/common_cells and redirect builds to it
astli patch common_cells

# Or point the dependency at an existing local checkout (or clone it there if the path doesn't exist)
astli patch common_cells ../common_cells

# Remove the patch and revert to the locked remote dependency (your local clone is preserved)
astli patch --remove common_cells
```

### How local patches work

When a dependency is patched:

- **Instant offline clone**: If the target directory does not exist, `astli` clones it instantly from your local cache mirror at the exact commit pinned in `astli.lock`. The clone's Git `origin` is automatically configured to the upstream repository URL, ready for committing and pushing.
- **Transitive discovery**: The patched directory's own `astli.toml` or `Bender.yml` is parsed in place, so any new dependencies or headers you introduce in the IP block are resolved seamlessly into the top-level design.
- **Zero manifest pollution**: Patches are stored locally in `.astli/patch.toml`. The `.astli` directory automatically ignores itself via an internal `.gitignore`, guaranteeing that local development overrides are never accidentally committed to source control.
- **CI safety**: While a patch is active, `astli.lock` is treated as strictly read-only to preserve reproducible states for team members. In addition, `--locked` and `astli update` fail with an error to prevent accidental CI or production builds against uncommitted local patches.

---

## Freezing changes with `astli snapshot`

Once you have tested your modifications in the patched repository, committed them, and pushed them to your remote Git branch, use `astli snapshot` to update your root project:

```sh
astli snapshot common_cells
```

`astli snapshot` performs the following automated steps:

1. **Validates Git state**: Checks that the patched repository has no uncommitted changes and verifies that the current `HEAD` commit has been pushed to the remote repository so collaborators can fetch it.
2. **Updates `astli.toml`**: Replaces the dependency's existing version requirement in `astli.toml` with an exact `rev = "<commit-hash>"` pointing to the new commit.
3. **Pins when necessary**: If other transitive packages depend on version tags for this IP, `astli snapshot` automatically adds the package to `[resolve] pinned`, ensuring your exact commit takes precedence without causing version conflict errors.
4. **Removes the patch**: Cleans up the local override in `.astli/patch.toml` and updates `astli.lock` to the new commit, leaving your local cloned directory untouched on disk.

!!! tip "Migrating Bender projects"
    If your root project's dependencies come from its `Bender.yml`, because it has no `astli.toml` or one without a `[dependencies]` table, `astli snapshot` changes nothing. It prints the commit to require; write it into your `Bender.yml` by hand, then drop the patch with `astli patch --remove`.
