# Coming from Bender

[Bender](https://github.com/pulp-platform/bender), from the PULP Platform, manages dependencies and generates filelists. `astli` does both, built on the same ideas, and also reads the code: it formats, lints and checks the design, and builds and tests it with Verilator. It reads a Bender project as it is, so you can move to `astli` one step at a time.

## At a glance

| | Bender | `astli` |
| :--- | :--- | :--- |
| Source files | Listed in `Bender.yml`, in compile order | Found below the package root; ordered by what each file declares and uses |
| Build variants | Targets, set for every package at once with `-t` | Features, each package's own, enabled by name across dependencies |
| Version conflicts | The root's requirement wins, or Bender asks at the terminal | An error, unless `[resolve] pinned` in `astli.toml` settles it, with a warning for each requirement it overrules |
| Checkouts | In `.bender/` of each project | One cache per machine, one read-only checkout per commit |
| A local copy of a dependency | `Bender.local`, which each developer keeps out of git | `astli patch`, kept out of git by `astli` itself |
| Tool flows | A script per tool, from `bender script` | One filelist every tool reads; `astli build`, `run` and `test` drive Verilator |
| Reading the code | No | `fmt`, `lint`, `check` and a language server, over one parser |

## What `astli` does differently

Each example below shows a Bender project and the same project in `astli`.

### Files are found, not listed

A `Bender.yml` lists every file, and its order is the compile order: a new file is a new line, and a package listed after the module that imports it fails in the simulator. `astli` takes the files in a directory, and puts each after the files it depends on:

=== "Bender"

    ```yaml
    sources:
      - include_dirs: [include]
        files:
          - rtl/soc_pkg.sv
          - rtl/soc_regs.sv
          - rtl/soc_xbar.sv
          - rtl/soc_top.sv
      - target: test
        files:
          - tb/tb_soc.sv
    ```

=== "astli"

    ```toml
    [package]
    name = "soc"
    sources = ["rtl"]
    include-dirs = ["include"]
    dev = ["tb"]
    ```

`astli files --top soc_top` writes the list, holding only what `soc_top` reaches, and `--why rtl/soc_regs.sv` says which instance or import needs a file. See [Packages](packages.md) and [Filelists](filelists.md).

### Features instead of targets

A Bender target set with `-t` holds in every package of the design at once, so `-t test` brings in every dependency's testbenches, and a project names its targets so that they don't collide. A [feature](features.md) belongs to its package: a dependent asks for a dependency's feature by name, and a package's own testbenches (`dev`) are never a dependent's.

=== "Bender"

    ```yaml
    dependencies:
      axi: { git: "https://github.com/pulp-platform/axi.git", version: 0.39.0 }
      tech_cells: { git: "https://github.com/acme/tech_cells.git", version: 1.0.0, target: asic }

    sources:
      - target: fpga
        files: [rtl/fpga/sram_wrap.sv]
      - target: asic
        files: [rtl/asic/sram_wrap.sv]
    ```

    ```sh
    bender script vsim -t fpga -t test
    ```

=== "astli"

    ```toml
    [dependencies]
    axi = { git = "https://github.com/pulp-platform/axi.git", version = "0.39" }
    tech_cells = { git = "https://github.com/acme/tech_cells.git", version = "1.0" }

    [features]
    default = ["fpga"]
    fpga = { sources = ["rtl/fpga/**"] }
    asic = { sources = ["rtl/asic/**"], dependencies = ["tech_cells"] }

    [top.soc_asic]
    top = "soc_top"
    features = ["asic"]
    ```

    ```sh
    astli files --top soc_asic
    ```

`fpga` and `asic` each declare `sram_wrap`, so they are alternatives: a design that enables both is an error naming the two declarations, where a filelist with both leaves it to the tool which one it builds. `tech_cells`, which only `asic` names, is fetched and read only while `asic` is on. Without `--top`, `astli check` checks every `[top.NAME]` table, each with its own features, so a mistake in the ASIC variant fails CI even when nobody builds it.

Defines are the root's alone: a package exports none, and a `[top.NAME]` table sets the ones its build needs. Code that differs by tool keeps its `` `ifdef VERILATOR ``, which every tool already understands.

### Conflicts are errors, and a pin says so in the manifest

Bender lets the root's requirement decide a conflict, or asks at the terminal, so a design can build against a version its dependency never asked for without anyone seeing. `astli` never prompts: when two packages require versions no commit meets, it stops and names both requirements.

To build anyway, pin the package in `[resolve] pinned`. The root's requirement then decides, and each requirement it overrules is a warning on every run, so the conflict stays in sight until upstream fixes it:

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38" }

[resolve]
pinned = ["common_cells"]
```

`Bender.yml` has no way to pin a package, so a project with only a `Bender.yml` whose dependencies conflict needs an `astli.toml` for its pins. See [Version resolution and pinning](dependencies.md#version-resolution-and-pinning).

### One cache, shared and sealed

Bender clones a project's dependencies into its own `.bender/`, so every checkout of a project clones them again. `astli` keeps one mirror per repository in a cache the whole machine shares, and checks each commit out once, for every design that uses it:

```sh
astli fetch              # check out everything the lock names, ahead of time
astli check --offline    # then run without the network, as on a build node
astli path common_cells  # where a dependency's checkout is
```

A checkout's files are read-only, and a file put into one by hand is refused where a run would read it: a register file generated into a dependency's checkout would change that dependency for every design on the machine. Generate it into your own package and [override](features.md#overrides) the dependency's module instead.

Submodules and Git LFS files are checked out only when a requirement [asks for them](dependencies.md#submodules), so a dependency is the files its own repository holds, the same on every machine.

### Working on a dependency in place

To fix a bug in a dependency where your design uses it, Bender points the dependency at a local clone through `Bender.local`, which must not be committed. `astli patch` clones the dependency at its locked commit and points the design at the clone, in `.astli/`, which `astli` keeps out of git itself:

```sh
astli patch common_cells     # clone it into .astli/patch/common_cells, and build from there
# fix it, commit and push in the clone
astli snapshot common_cells  # require that commit in astli.toml, and drop the patch
```

While a patch is in place, `astli.lock` is never written, and `--locked` fails, so CI never builds against one developer's clone. See [Working on a dependency in place](dependencies.md#working-on-a-dependency-in-place).

### One filelist, and Verilator driven for you

`bender script` writes a script for each tool it knows. `astli files` writes the one list every simulator and synthesis tool reads with `-f`, and a flow with a script of its own wraps that:

```sh
astli files --top soc_top > soc.f
vcs -sverilog -f soc.f
```

A filelist has one include path for every file, while each package searches its own. Where two packages each hold a `defs.svh`, the tool would read the wrong one for some files, and `astli files` warns with `include-differs` before it does.

For Verilator, `astli` runs the tool itself: `astli build` checks the design, compiles it and skips the build when nothing it reads changed, and `astli test` runs every testbench the package marks, across the parameter values it sweeps. See [Building](building.md) and [Testing](testing.md).

### It reads the code

Bender knows files; `astli` also knows what they say. The same parser behind dependency resolution formats the package, lints it, and checks every instance against its definition, before a simulator ever sees it:

```sh
astli fmt --check
astli lint
astli check --top soc_top
```

Your editor gets the same answers from [`astli lsp`](editors.md).

### Dependencies without an `astli.toml`

Most dependencies have a `Bender.yml` and no `astli.toml`. `astli` reads such a package by its defaults, every file below its root, which is wrong for one that keeps two versions of each cell side by side, as `tech_cells_generic` keeps its generic cells and their Xilinx versions. Until it gets a manifest upstream, the root can describe it:

```toml
[described.tech_cells_generic]
exclude = ["src/fpga/**"]   # its Xilinx versions of the same cells
```

A description says only what the dependency's own manifest would, so it moves there unchanged, and it is ignored, with a warning, once the dependency has an `astli.toml`. See [Describing unmanifested dependencies](dependencies.md#describing-unmanifested-dependencies-described).

### What `astli` does not do

- It writes no per-tool scripts: a flow other than Verilator reads the filelist.
- It reads no `sources`, `defines` or `targets` of a `Bender.yml`, only its dependencies.
- It has no counterpart to `bender vendor`, which copies a foreign repository into yours with patches applied.

## What `astli` reads from `Bender.yml`

When a package has a `Bender.yml` and its `astli.toml` has no `[dependencies]` table, `astli` reads the dependencies from `Bender.yml`: `git` ones with a version or a `rev`, `path` ones, and named remotes. Package names are lowercased, as Bender does.

!!! info "What `astli` ignores in `Bender.yml`"
    `astli` does not read Bender's `sources`, `defines` or `targets`. It finds a package's files itself (see [Packages](packages.md)), and [features](features.md) take the place of targets. It ignores `Bender.lock` too, and keeps its own [`astli.lock`](dependencies.md#lockfiles-astlilock).

    Bender checks out every submodule of a dependency; `astli` checks out none unless a requirement in an `astli.toml` [takes it](dependencies.md#submodules). Likewise, `astli` fetches a dependency's [Git LFS](dependencies.md#git-lfs) files only with `lfs = true`.

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

`astli bender import` copies the dependencies of `Bender.yml` into `[dependencies]` in `astli.toml`, creating the file if there is none:

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38" }
axi = { git = "https://github.com/pulp-platform/axi.git", version = "0.39" }
```

Once `astli.toml` has `[dependencies]`, `astli` reads no dependency from `Bender.yml`.

!!! note "Target-specific dependencies"
    `astli bender import` omits dependencies that are restricted to specific Bender `target`s (such as simulation testbenches or FPGA builds). In `astli`, flow-specific dependencies and conditional compilation are managed declaratively using [Features](features.md).

### Step 4: Export `Bender.yml` for downstream compatibility

Where other projects still read your package with Bender, keep `astli.toml` as the one source of truth and write `Bender.yml` from it:

```sh
astli bender export
```

Commit the generated `Bender.yml` to version control so that downstream Bender consumers can resolve your package at its tagged releases. In CI, enforce that `Bender.yml` stays in sync:

```sh
astli bender export --check
```

In a [workspace](workspaces.md#coming-from-bender), `--workspace` writes or checks every package's `Bender.yml` in one step, since Bender reads a member another depends on from its own. A package whose dependencies its `Bender.yml` still lists is left alone.

The export maps `astli`'s package model onto Bender's:

| `astli.toml` | `Bender.yml` |
| :--- | :--- |
| `[dependencies]` | `dependencies` |
| Feature-gated dependency | Dependency with `target: <features>` |
| `[dev-dependencies]` | Dependency with `target: <package>_dev` |
| Feature dependencies | `pass_targets` |
| `dev-features` | Its own feature's target as `any(<feature>, <package>_dev)`, a dependency's in `pass_targets` under `<package>_dev` |
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
