# Coming from Bender

[Bender](https://github.com/pulp-platform/bender), from the PULP Platform, manages dependencies and generates filelists. `astli` reads a Bender project as it is, and you can move to `astli` one step at a time.

## What `astli` reads from `Bender.yml`

When a package has a `Bender.yml` and its `astli.toml` has no `[dependencies]` table, `astli` reads the dependencies from `Bender.yml`: `git` ones with a version or a `rev`, `path` ones, and named remotes. Package names are lowercased, as Bender does.

!!! info "What `astli` ignores in `Bender.yml`"
    `astli` does not read Bender's `sources`, `defines` or `targets`. It finds a package's files itself (see [Packages](packages.md)), and [features](features.md) take the place of targets. It ignores `Bender.lock` too, and keeps its own [`astli.lock`](dependencies.md#lockfiles-astlilock).

## Step-by-step migration

A Bender project moves to `astli` in four steps, each of which works on its own:

### Step 1: Use `astli` on Bender's filelist

Without changing the repository, `astli` can check, lint or trim the filelist Bender writes:

```sh
bender script flist-plus -t rtl > design.f

astli check -f design.f --top soc_top
```

### Step 2: Add an `astli.toml` for formatting and linting

`astli init` writes an `astli.toml` with the package name from `Bender.yml`. Add layout, lint levels or source directories to it as you need them:

```toml
[package]
name = "my_soc"

[fmt]
width = 100
indent = 2
```

With no `[dependencies]` in `astli.toml`, `astli` still reads them from `Bender.yml`, and `astli fmt`, `astli lint` and `astli check` now run on the package with no filelist.

### Step 3: Transition dependencies to `astli.toml`

When you are ready to manage dependencies natively with `astli`, run `astli bender import` (or manually copy your dependencies). This copies Bender's dependency definitions into `[dependencies]` in `astli.toml`, creating the file if there is none:

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38" }
axi = { git = "https://github.com/pulp-platform/axi.git", version = "0.39" }
```

Once `[dependencies]` is defined in `astli.toml`, `astli` exclusively uses its own native resolver and lockfile.

!!! note "Target-specific dependencies"
    `astli bender import` omits dependencies that are restricted to specific Bender `target`s (such as simulation testbenches or FPGA builds). In `astli`, flow-specific dependencies and conditional compilation are managed declaratively using [Features](features.md).

### Step 4: Export `Bender.yml` for downstream compatibility

If external projects still consume your package using Bender, maintain `astli.toml` as your single source of truth and export `Bender.yml` automatically:

```sh
astli bender export
```

Commit the generated `Bender.yml` to version control so that downstream Bender consumers can resolve your package at its tagged releases. In CI, enforce that `Bender.yml` stays in sync:

```sh
astli bender export --check
```

The export maps `astli`'s package model onto Bender's:

| `astli.toml` | `Bender.yml` |
| :--- | :--- |
| `[dependencies]` | `dependencies` |
| Feature-gated dependency | Dependency with `target: <features>` |
| `[dev-dependencies]` | Dependency with `target: <package>_dev` |
| Feature dependencies | `pass_targets` |
| `include-dirs` (and feature-specific ones) | `export_include_dirs` (under corresponding target) |
| Package sources | `sources`, in topological compilation order |
| Feature sources | Source group with `target: <feature>` |
| `dev` sources | Source group with `target: <package>_dev` |
| Default feature sources | Sources without a target |
| Files declared in `[overrides]` | Source group with `override_files: true` |

`dev` is scoped using `<package>_dev` because Bender targets set on the command line apply globally across all packages; this ensures `-t soc_dev` compiles only `soc`'s testbenches without building testbenches of dependencies.

`astli` verifies compatibility before writing: it checks that file replacements match Bender's naming rules and include search scopes, and reports an error if Bender would compile the files differently.

!!! tip "Organize sources into directories per target"
    While Bender projects often place all sources in a single directory and list individual files explicitly under targets, `astli` selects feature, `dev`, and excluded files using directory glob patterns. Organizing files into dedicated directories (e.g. `rtl/fpga/`, `rtl/asic/`, and `tb/`) turns long file lists into concise patterns and ensures new files are categorized automatically without updating `astli.toml`.

## Handling version conflicts (`[resolve] pinned`)

When dependency version requirements conflict, Bender interactively prompts the user in the terminal to pick a version (or requires a local override file such as `Bender.local`).

`astli` is strictly non-interactive and scriptable: it never halts to prompt for manual user input during dependency resolution. If two packages request incompatible versions, `astli` stops with an explicit conflict error by default.

To resolve a conflict without modifying transitive upstream manifests, pin the package in `[resolve] pinned` in your `astli.toml`:

```toml
[resolve]
pinned = ["common_cells"]
```

When a package is pinned:

- `astli` forces the dependency to whichever version or revision the root package specifies.
- Any conflicting downstream requirements that cannot be met are downgraded to informative warnings rather than fatal errors.

!!! note "Projects with only a `Bender.yml`"
    A project that relies solely on `Bender.yml` has no mechanism to declare pins. If your design encounters version conflicts across its dependencies, you will need to add an `astli.toml` and declare `[resolve] pinned` to explicitly resolve them.
