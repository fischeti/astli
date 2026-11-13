# Dependencies

`astli` fetches packages from Git repositories or local directories, chooses a version of each that every requirement accepts, and records the commits it chose in `astli.lock`.

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38" }
idma = { git = "https://github.com/pulp-platform/iDMA.git", rev = "main" }
tech = { path = "../tech" }
```

### Adding and removing dependencies

`astli add` writes a dependency into `astli.toml` and updates `astli.lock` in one step:

```sh
astli add https://github.com/pulp-platform/common_cells.git   # latest semver release
astli add https://github.com/pulp-platform/iDMA.git --rev main
astli add --path ../tech
astli add axi --features vip     # a package already in the design
astli add tb_utils --dev         # add to [dev-dependencies]
```

By default, the package is named using the name declared in its `astli.toml` or `Bender.yml` (falling back to the Git repository name from the URL); use `--name` to override this. When called without `--version` or `--rev`, `astli add` queries remote tags and pins the latest semver release as a caret requirement (e.g. `version = "1.38.2"`).

Passing a bare package name adds a dependency already present in your lockfile (as a transitive dependency), pinning it at its current version and source. Adding an existing direct dependency updates its configuration in place while preserving other options.

To remove dependencies, run `astli remove`:

```sh
astli remove common_cells
astli remove --dev tb_utils
```

`astli remove` removes the entries from `astli.toml`, cleans up references in feature definitions or `[resolve] pinned`, and prunes unused packages from `astli.lock`. If removing a dependency would break dependency resolution, `astli` reports the conflict and leaves `astli.toml` unchanged.

### Specifying dependency sources

- **`git` with `version`**: Resolves semver requirements (e.g. `^1.38` or `1.38.0`) by selecting the highest matching `vX.Y.Z` tag available in the remote repository.
- **`git` with `rev`**: Pins to an explicit Git branch, tag name, or commit SHA.
- **`path`**: Points to a local directory relative to the current package root. Ideal for monorepos or in-tree development.

Each dependency is a package of its own: its `astli.toml`, if it has one, says what its files, features and dependencies are. `astli check` reports findings in the files of a `path` dependency, or of one that a `path` dependency names by `path`, as it does in the package's own: such a package usually belongs to the same people, and only the package that instantiates it checks it with the parameters it is given. With `--top`, that means only the files the tops reach. A dependency fetched from Git, or patched with `astli patch`, has its findings left to its authors, and so has one that a patched dependency names by `path`, except that with `--top`, an `` `include `` in one of its files that finds no header, or more than one, is an error, since a tool given the file list could not find it either. `astli lint` reports on the package's own files only. For a dependency without an `astli.toml`, [`[described.NAME]`](#describing-unmanifested-dependencies-described) can name its include directories.

### Submodules

A dependency's submodules are left out of its checkout: each is an empty directory. Most hold software, verification suites or generators a design never reads, and what a design does need from one is usually a dependency of its own. Where a design reads files from a submodule, `submodules` takes it:

```toml
[dependencies]
core = { git = "https://github.com/example/core.git", version = "2.1", submodules = ["vendor/fpu"] }
foo = { git = "https://github.com/example/foo.git", version = "1.2", submodules = true }
```

- **A list** takes the submodules at those paths, relative to the repository's root. A submodule inside one is taken only if its own full path is listed too, such as `"vendor/fpu/third_party/x"`. A path that is not a submodule at the chosen commit is an error.
- **`true`** takes every submodule, and every one inside those.

Each requirement of a package adds what it takes, as features do: if your design and a library it uses both depend on `core` and each takes a submodule, the checkout holds both. The commit chosen fixes each submodule's commit and URL, so `astli.lock` holds nothing more. A dependency read from a `Bender.yml` takes no submodules; to take one, depend on the package in your own `astli.toml` with `submodules`. [`astli bender export`](bender.md) leaves the key out, since bender checks out every submodule anyway.

If a manifest's `sources` or `include-dirs` names a path inside a submodule that is not checked out, `astli` says which submodule it is and what checks it out: in a dependency, `submodules` on its requirement; in your own package, `git submodule update --init`.

### Git LFS

A file a repository stores with [Git LFS](https://git-lfs.com) is a small pointer in the commit, and that pointer is what a dependency's checkout holds. Where a design reads such files, `lfs = true` fetches them:

```toml
[dependencies]
models = { git = "https://github.com/example/models.git", version = "1", lfs = true }
```

The checkout then holds each file's content, in the submodules it takes too. It runs the [`git-lfs`](https://git-lfs.com) program, which has to be installed, and a run that needs it fails if it is not. `git-lfs` fetches over HTTPS or SSH as `git` would, with the same credentials, and keeps what it fetched in the cache, so a later checkout needs no network; `--offline` fails, naming the files, if the cache lacks any. As with `submodules`, each requirement of a package adds to it, and `astli.lock` holds nothing more, since each pointer names its file by hash. [`astli bender export`](bender.md) leaves the key out.

Without `lfs`, a file `astli` reads that is an LFS pointer is an error saying so, rather than a file that does not parse: set `lfs = true` on the dependency, or for a file of your own package, run `git lfs pull`.

### Choosing a requirement: `version` vs `rev`

- **`version`**: Specifies a semver requirement. A bare version string acts as a caret requirement: `version = "1.38"` matches compatible releases `^1.38` (admitting versions `>=1.38.0, <2.0.0`). Running `astli update` will automatically advance to non-breaking fixes and additions without requiring changes in downstream manifests. For 0.x releases, caret treats the minor version as breaking (e.g. `^0.39.8` permits `0.39.9` but not `0.40.0`).
- **`rev`**: Pins to an exact Git commit, tag, or branch. Use `rev` for unreleased in-tree work or tracking an active upstream branch. Because `rev` does not allow automatic non-breaking updates via `astli update`, prefer `version` for packages published for others to depend on.

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

## Lockfiles (`astli.lock`)

When dependencies are resolved, the exact commits and revisions chosen are written to `astli.lock` next to your `astli.toml`, or in a [workspace](packages.md#workspaces) next to the workspace root's, for every member at once.

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

In CI, dependencies should never move without a commit saying so. Pass `--locked`:

```sh
astli check --locked --top soc_top
```

If `astli.lock` is missing or out of sync with `astli.toml`, `astli` exits with an error instead of modifying the lockfile.

## The shared cache

`astli` checks dependencies out into one cache that every project on the machine shares.

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

`--offline` keeps a run from cloning or fetching: it resolves over what the cache holds, and fails naming a dependency or locked commit the cache lacks. Without `--locked`, it may lock another version the cache has.

`ASTLI_CACHE_DIR` moves the cache; its default for each OS is in the [environment variables reference](reference/environment.md).

!!! note "Read-only cache protection"
    Cached dependencies are marked read-only to prevent accidental edits from leaking across projects. Never manually modify files inside the cache; if you need to alter or customize a dependency's module, use [overrides](features.md#overrides) in your project, or work on it in a clone with [`astli patch`](#working-on-a-dependency-in-place).

## Describing unmanifested dependencies (`[described]`)

A dependency without an `astli.toml` takes the defaults: every file in the repository is a source, and its headers are found in `include/`.

Many IPs keep files for different flows side by side, such as FPGA and ASIC primitives, formal properties, or testbenches, or keep their headers elsewhere. Read with the defaults, they declare names twice, miss includes, or bring in files their own build leaves out. A `bind` among them applies wherever its file is compiled, so with `--top`, `astli files` and `astli check` note each such file that binds into the design. A `[described.NAME]` table in the root's `astli.toml` says what the dependency's own manifest would:

```toml
[described.tech_cells_generic]
exclude = ["src/fpga/**"]

[described.idma]
include-dirs = ["src/include", "target/rtl/include"]
```

A `[described.NAME]` block accepts the same `sources`, `include-dirs`, and `exclude` keys that the dependency would declare in its own `[package]` manifest table, evaluated relative to the dependency's root directory. Because descriptions only filter file discovery and include paths, they never alter version resolution or lockfile checksums.

### Guardrails for temporary descriptions

`[described]` stands in until the dependency has a manifest of its own, and `astli` keeps it from going stale. A run that resolves dependencies notes which are described (unless `-q`), and `astli tree` marks them. Once the dependency has its own `astli.toml`, that is read, with a warning to remove the table. Describing a package not in the design, or an `exclude` that matches nothing, is a warning too. Only the root's descriptions are read: a dependency cannot describe another.

!!! tip "Upstreaming a manifest"
    The lasting fix is an `astli.toml` in the dependency's repository. [`astli patch`](#working-on-a-dependency-in-place) gives you a clone to try one in before proposing it.

## Working on a dependency in place

To change a dependency and try the change in your design, without tagging a release or editing your manifest, `astli patch` (or `astli clone`) reads the dependency from a directory of yours:

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

- A directory that does not exist yet is cloned from the cache, at the commit `astli.lock` names, with `origin` set to the upstream URL, ready to commit and push.
- The directory's own `astli.toml` or `Bender.yml` is read, so a dependency or header you add there is part of the design.
- Patches are recorded in `.astli/patch.toml`, and `.astli/` keeps itself out of git with a `.gitignore` of its own.
- While a patch is active, `astli.lock` is not written, and `--locked` and `astli update` fail, so nothing is built or locked against your local copy by accident.

## Freezing changes with `astli snapshot`

Once you have tested your modifications in the patched repository, committed them, and pushed them to your remote Git branch, use `astli snapshot` to update your root project:

```sh
astli snapshot common_cells
```

`astli snapshot`:

1. Checks that the clone has no uncommitted changes and that its `HEAD` is pushed, so others can fetch it.
2. Replaces the dependency's requirement in `astli.toml` with `rev = "<commit>"`. In a [workspace](packages.md#workspaces), it does so in every package of the workspace that requires it, so that they agree on the commit. If none does, only a dependency, the requirement is added to the package you run it in, which must then be one whose design holds the dependency.
3. Adds the package to `[resolve] pinned`, in the workspace root's `astli.toml` in a workspace, if other packages require it by version, so the commit wins over their requirements.
4. Removes the patch and locks the new commit. The clone stays where it is.

!!! tip "Migrating Bender projects"
    If the dependencies of your root project, or of a workspace package `astli snapshot` would change, come from its `Bender.yml`, because it has no `astli.toml` or one without a `[dependencies]` table, `astli snapshot` changes nothing. It prints the commit to require; write it into your `Bender.yml` by hand, then drop the patch with `astli patch --remove`.
