# Dependencies

`astli` provides built-in dependency management designed specifically for hardware designs. It fetches packages from Git repositories or local directories, resolves compatible versions, and records exact snapshots in an `astli.lock` lockfile.

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38" }
idma = { git = "https://github.com/pulp-platform/iDMA.git", rev = "main" }
tech = { path = "../tech" }
```

### Adding and removing dependencies

`astli add` writes a dependency into `astli.toml` and updates `astli.lock` in one step:

```sh
astli add https://github.com/pulp-platform/common_cells.git   # highest tagged release
astli add https://github.com/pulp-platform/iDMA.git --rev main
astli add --path ../tech
astli add axi --features vip     # a package the design already holds
astli add tb_utils --dev         # into [dev-dependencies]
```

A repository is named as its own `astli.toml` or `Bender.yml` names it, or else after the URL; `--name` chooses another. Without `--version` or `--rev`, `astli add` requires the highest `vX.Y.Z` tag as a caret requirement, such as `version = "1.38.2"`, and refuses a repository with no such tag. A bare name requires a package that another dependency already brings in, from the same source and at the version it is locked at. Adding a dependency that is already there changes its entry in place, keeping its other keys.

`astli remove NAME...` drops dependencies again, along with what the root's features and `[resolve] pinned` say about them, and removes from the lock whatever nothing needs any more. Both commands keep every other package at its locked commit, and leave `astli.toml` as it was if the dependencies no longer resolve.

### Specifying dependency sources

- **`git` with `version`**: Resolves semver requirements (e.g. `^1.38` or `1.38.0`) by selecting the highest matching `vX.Y.Z` tag available in the remote repository.
- **`git` with `rev`**: Pins to an explicit Git branch, tag name, or commit SHA.
- **`path`**: Points to a local directory relative to the current package root. Ideal for monorepos or in-tree development.

Each dependency is treated as an independent `astli` package. If a dependency includes its own `astli.toml`, its source boundaries, features, and transitives are respected; otherwise, standard default conventions apply. Code quality findings (linting and checks) originating from third-party dependencies are ignored by default.

### Choosing a requirement

A bare version is a caret requirement: `version = "1.38"` admits every release from `1.38.0` up to, but not including, `2.0.0`. A release inside that range reaches every design that depends on the package through `astli update`, and the packages in between need no change.

A `rev` admits exactly one commit, or wherever a branch points. Every fix to that dependency then has to be written into each manifest that names it, and each of those released, before a design further up can use it. Prefer `version` in a package others depend on, and keep `rev` for a root that tracks work no release holds yet.

Caret treats the first non-zero number as the major version, so below `1.0.0` the minor version breaks: `^0.39.8` admits `0.39.9` but not `0.40.0`. Releases decide how far a requirement reaches:

| Change | From `1.0.0` | Below `1.0.0` |
| --- | --- | --- |
| Breaking | major | minor |
| Addition | minor | patch |
| Fix | patch | patch |

A package that releases every addition as a new minor version below `1.0.0` makes each release look breaking to everything that depends on it, and each of those must raise its requirement by hand.

!!! info "Why not a lower bound alone"
    `version = ">=0.39.8"` would admit every later release, but the highest admitted version is the one chosen, so a release that breaks the package's interface would reach the design through `astli update` unseen. A caret requirement states the series the dependent was checked against, and a breaking release becomes a resolution error rather than a failure in simulation.

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

### Visualizing the dependency tree

Run `astli tree` to inspect the resolved dependency hierarchy and see exactly which packages pull in each dependency:

```console
$ astli tree
soc
├── axi 0.39.6
│   └── common_cells 1.37.0
└── common_cells 1.37.0
```

- **Revisions**: Packages tracked by branch or commit hash display their short Git commit SHA instead of a version tag.
- **Local & patched checkouts**: Dependencies resolved from a local `path` or active `astli patch` display their source directory path.
- **Deduplication**: When a shared package appears multiple times in the tree, its downstream dependencies are listed only upon its first occurrence and marked with `(*)` on subsequent occurrences to keep output concise.
- **Described packages**: Upstream dependencies configured via `[described.NAME]` (see below) are annotated with `(described)`.

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

## Describing unmanifested dependencies (`[described]`)

When consuming a third-party dependency that does not yet have an `astli.toml`, `astli` falls back to sensible package defaults: all files in the repository are scanned as sources, and headers are exported exclusively from the `include/` directory.

However, many real-world IPs bundle conflicting files for different EDA flows side-by-side (for example, FPGA vs. ASIC primitives), or locate their header files in non-standard directories. Without a manifest, this can cause duplicate symbol collisions or missing include files.

To support unmanifested upstream repositories cleanly, you can provide temporary package descriptions in your root `astli.toml` using `[described.NAME]`:

```toml
[described.tech_cells_generic]
exclude = ["src/fpga/**"]

[described.idma]
include-dirs = ["src/include", "target/rtl/include"]
```

A `[described.NAME]` block accepts the same `sources`, `include-dirs`, and `exclude` keys that the dependency would declare in its own `[package]` manifest table, evaluated relative to the dependency's root directory. Because descriptions only filter file discovery and include paths, they never alter version resolution or lockfile checksums.

### Guardrails for temporary descriptions

`[described]` is intended as a temporary stopgap while upstream packages adopt `astli`. To prevent stale configurations from lingering silently, `astli` provides clear guardrails:

- **Visibility**: Every run that resolves dependencies reports which packages are currently described (suppressed with `-q`), and `astli tree` tags them with `(described)`.
- **Automatic upstream handoff**: As soon as the upstream repository adds its own `astli.toml`, `astli` automatically uses the upstream manifest and issues a warning prompting you to remove the redundant `[described]` block.
- **Validation**: `astli` emits warnings if you describe a package that is not part of your design, or if an `exclude` pattern does not match any files in the dependency checkout.
- **Root-only scope**: Only descriptions in your root project's `astli.toml` are evaluated. Downstream libraries cannot impose file descriptions on parent designs.

!!! tip "Upstreaming a manifest"
    To resolve the issue permanently for the entire community, propose an `astli.toml` upstream in the dependency's repository. You can use [`astli patch`](#working-on-a-dependency-in-place) to test your `[package]` configuration in a local checkout before submitting a pull request.

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
