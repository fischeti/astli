# Workspaces

A repository often holds more than one package: an SoC and the IP blocks it is built from, or a library of blocks that are released together. A **workspace** lets those packages share one lock, so they agree on one version of every dependency, and lets you run a command on any of them, or all of them, from anywhere in the repository.

## When to use one

Use a workspace when the packages of a repository are developed and tested together: each block has its own testbench and lint settings, but they should all build against the same `common_cells`, and CI should check every one of them.

Keep separate packages, without `[workspace]`, when the directories are unrelated: vendored IP that ships its own `astli.toml`, say, or a block that pins different versions than the rest. A nested `astli.toml` without a workspace is a package of its own, with a lock of its own.

## Setting one up

Take a repository with an SoC at the top and two blocks below it:

```text
my-soc/
├── astli.toml
├── rtl/soc.sv
└── ip/
    ├── dma/
    │   ├── astli.toml
    │   ├── rtl/dma.sv
    │   └── tb/tb_dma.sv
    └── uart/
        ├── astli.toml
        └── rtl/uart.sv
```

The `astli.toml` at the top becomes the **workspace root** by holding a `[workspace]` table. An empty one is enough:

```toml
# my-soc/astli.toml
[package]
name = "soc"

[workspace]

[dependencies]
dma = { path = "ip/dma" }
uart = { path = "ip/uart" }
```

Every `astli.toml` below it in the repository is then a **member**. A member is an ordinary package, with its own dependencies, features, tops and settings:

```toml
# my-soc/ip/dma/astli.toml
[package]
name = "dma"
dev = ["tb/**"]

[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38" }
```

`astli tree --workspace` shows what each package depends on, the root first:

```console
$ astli tree --workspace
soc
├── dma (ip/dma)
│   └── common_cells 1.40.0
│       ├── common_verification 0.2.5
│       └── tech_cells_generic 0.2.14
│           └── common_verification 0.2.5
└── uart (ip/uart)

dma
└── common_cells 1.40.0
    ├── common_verification 0.2.5
    └── tech_cells_generic 0.2.14
        └── common_verification 0.2.5

uart
```

There is one `astli.lock`, beside the root's manifest, and it is the same whichever package's command writes it.

## Members

`astli` finds the members by walking down from the root. It leaves out:

- a directory a `.gitignore` names, as it does when [finding a package's files](packages.md#source-file-discovery);
- a directory holding a `.git` of its own, such as a submodule, which is another repository;
- a directory that `[workspace] exclude` matches, such as vendored IP with a manifest of its own:

```toml
[workspace]
exclude = ["vendor/**"]
```

A member may sit inside another. Two packages of one workspace may not share a name, and a `[workspace]` table inside a workspace that does not exclude it is an error: a workspace has one root.

One member uses another the way it would use any directory on disk, with a [`path` dependency](dependencies.md#specifying-dependency-sources). The root above depends on both blocks so that its own design holds them; a member that no package requires is still a member, resolved and checked with the rest.

## What the root holds and what each member keeps

What has to be the same for the whole workspace lives in the root's manifest and directory. The rest is each package's own.

| The workspace root's alone | Each package's own |
| :--- | :--- |
| `astli.lock` | `[package]`, its files and `default-top` |
| `[resolve] pinned` | `[dependencies]` and `[dev-dependencies]` |
| patches, `.astli/patch.toml` | `[features]` and `[overrides]` |
| `[described.NAME]` | `[top.NAME]` |
| `[build]` and `[profile.NAME]` | `[fmt]` and `[lint]` |
| `.astli/build` | |

A member's manifest holding one of the root's tables, or a member's own patches, gets a warning that they are not read: move what should count to the root.

## One resolution

Every command resolves the whole workspace, from whichever package it runs in. So the workspace holds **one version of each package**, even between members that are never built together, and two members whose requirements no version meets both of are an error that names them both. Where a dependency's own requirements disagree, [pin](dependencies.md#version-resolution-and-pinning) the package in the root's `[resolve] pinned`.

Each package is still a **design of its own**: run inside `ip/dma`, a command builds `dma` and what it requires, with `dma`'s features, tops and lint levels, and warns of a pin overruling a requirement only for the packages that design holds.

`astli update`, `astli fetch`, `astli patch`, `astli snapshot` and `astli clean` act on the whole workspace from any member. A patch is the root's, whichever member you run `astli patch` in, and `astli snapshot` writes the commit into the manifest of each package that requires it.

## Running commands

A command acts on the package the current directory is in. Two flags choose others.

**`-p NAME`** (`--package NAME`) acts on the workspace package of that name, the root or a member, from anywhere in the workspace, as if run inside it:

```sh
astli build -p dma --top tb_dma   # a member's testbench, from the root
astli add -p uart --path ip/dma   # requires dma in ip/uart/astli.toml
```

`fmt`, `lint`, `check`, `tree`, `files`, `elab`, `build`, `run`, `path`, `add`, `remove` and `bender export` take it.

**`--workspace`** runs `fmt`, `lint`, `check`, `tree` or `bender export` once for each package, the root first and the members in the order of their paths. A package that fails is named, the others still run, and the command fails at the end:

```console
$ astli check --workspace
[undeclared-name] Error: `idle` is not declared
   ,-[ /work/my-soc/ip/uart/rtl/uart.sv:4:15 ]
   |
 4 |   assign tx = idle;
   |               ^^|^
   |                 `--- declared nowhere in scope
---'
error: `soc`: 1 of 3 files has errors
error: 2 of 3 packages failed
```

`soc` depends on `uart`, so its run checks `uart`'s files too, as a dependent [checks a `path` dependency](dependencies.md#specifying-dependency-sources). The finding is told once, by the first run that meets it, and fails both packages that hold it. The runs share what they read alike, so `--workspace` costs little more than the root's own run.

`files`, `build` and `run` describe one design, so they take `-p` and not `--workspace`. Outside a workspace, `--workspace` runs on the one package, so a script can always pass it.

!!! tip "Several tops"
    A package with several `[top.NAME]` tables needs a `default-top` for a run that names no top, and under `--workspace` the run names none. Give each such member a `default-top`, or check its tops in steps of their own with `-p NAME --top NAME`.

## Virtual roots

A repository of IP blocks often has no design at the top. Leave out `[package]`, and the root is **virtual**:

```toml
# astli.toml at the top of the repository
[workspace]

[resolve]
pinned = ["common_cells"]
```

A virtual root holds no files and depends on nothing. Of its tables, only `[workspace]`, `[resolve]`, `[described.NAME]`, `[build]` and `[profile.NAME]` are read; `[dependencies]`, `[features]`, `[top.NAME]`, `[fmt]`, `[lint]` and the like are warned of, and belong in a member.

Run at a virtual root, `fmt`, `lint`, `check`, `tree` and `bender export` act on every member, as `--workspace` does. `files`, `build`, `run`, `add` and `remove` there need `-p` to say which member.

## In CI

`--workspace` makes each check one step for the whole repository:

```yaml
      - name: Format check
        run: astli fmt --check --workspace

      - name: Lint
        run: astli lint --locked --workspace

      - name: Check
        run: astli check --locked --workspace
```

`--diag json` and `--diag gitlab` still write one stream: every package's records in one, and one Code Quality report for all of them. See [Continuous integration](ci.md) for the rest of the workflow.

## Coming from Bender

A Bender repository with a `Bender.yml` in each block moves to a workspace the way a single package does, [one step at a time](bender.md#step-by-step-migration):

1. Run `astli init` at the top and in each block's directory. Each `astli.toml` takes its name from the `Bender.yml` beside it, and reads its dependencies from it until you import them.
2. Add `[workspace]` to the top `astli.toml`, and move the pins Bender needed into its `[resolve] pinned`.
3. Run `astli bender import` in each block to copy its dependencies into its `astli.toml`. A block whose dependencies still come from its `Bender.yml` keeps working in the meantime.
4. If Bender still builds the blocks, write their `Bender.yml` files from the `astli.toml` files with `astli bender export --workspace`, and check them in CI with `astli bender export --check --workspace`. A block that has not been imported yet is left alone.

Bender reads one block's `path` dependency on another from that block's own `Bender.yml`, which is why every member gets one.

## What a workspace does not do yet

- **Other repositories require the root.** A `git` dependency is a repository's root package, so another repository cannot yet require a single member; it gets the members the root requires by `path`. A virtual root, which requires nothing, gives it nothing.
- **Members do not inherit settings.** Each member has its own `[fmt]` and `[lint]`; the root's are not passed down.
