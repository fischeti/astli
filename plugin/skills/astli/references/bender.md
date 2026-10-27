# Migrating a Bender project to astli

astli reads the `dependencies` of a `Bender.yml` by itself, so a Bender
project works with astli before anything changes. Migrating moves the rest
into `astli.toml` one step at a time, and each step can stop there. Ask the
user how far to go: formatting and linting only, astli resolving the
dependencies, or replacing Bender.

## What carries over and what does not

astli takes from `Bender.yml` only its dependencies: `git` with `version` or
`rev`, `path`, and named remotes. It ignores the rest:

| Bender | astli |
| :--- | :--- |
| `sources` | found by scanning `[package] sources`, minus `exclude` |
| `export_include_dirs` | `[package] include-dirs` |
| `include_dirs` (private) | headers next to the including file, or `include-dirs` |
| `defines` | `defines` in a `[top.NAME]` table |
| `target: test` / `simulation` sources | `[package] dev` |
| other `target`-gated sources | a feature in `[features]` |
| target-gated dependencies | an optional dependency of a feature, or `[dev-dependencies]` |
| `Bender.lock` | `astli.lock`, resolved anew |
| `Bender.local` overrides | `[resolve] pinned`, or a `path` dependency |
| a dependency without a `Bender.yml` | `[described.NAME]` in the root `astli.toml` |

| Bender command | astli command |
| :--- | :--- |
| `bender update` | `astli update` |
| `bender checkout` | `astli fetch` |
| `bender path NAME` | `astli path NAME` |
| `bender clone NAME` | `astli patch NAME` |
| `bender parents`, `bender packages` | `astli tree` |
| `bender script flist` | `astli files --top TOP` |

## Steps

1. **Take a baseline from Bender.** Write the filelist of the target the
   user builds, and check it with astli before changing anything:

   ```sh
   bender script flist-plus -t rtl > bender.f
   astli check -f bender.f --top TOP
   ```

   Errors here are in the design or in astli, not in the migration. Report
   them, and don't count them against the later steps.

2. **Add an `astli.toml`.** `astli init` writes one, naming the package from
   `Bender.yml`. Leave out `[dependencies]` for now, so astli keeps reading
   Bender's. Fill in `[package]` from the table above: `include-dirs` from
   `export_include_dirs`, testbenches under `dev`, and anything Bender never
   lists (scripts, generated or vendored copies, other flows) under
   `exclude`. Then `astli fmt --check`, `astli lint` and `astli check` work
   on the repository with no filelist.

3. **Compare the two file sets.** List what each tool builds for the top and
   compare paths, ignoring order:

   ```sh
   astli files --top TOP > astli.f
   ```

   A file only Bender lists is usually missing from `sources` or caught by
   `exclude`. A file only astli lists is one Bender left out by `target` or
   by never naming it: exclude it, or move it into `dev` or a feature. A
   duplicate module across dependencies (ASIC and FPGA variants side by side)
   needs `[described.NAME] exclude` for a dependency without an
   `astli.toml`, or `[overrides]` when the root package supplies its own
   version. `astli files --why FILE` explains why a file is in.

   Features, `dev` and `exclude` select files by glob, so they are only as
   clean as the directory layout. Bender projects often keep every file in
   one directory and sort them into targets by listing each by name; moving
   a target's files into a directory of their own (`rtl/fpga/`, `rtl/asic/`,
   `tb/`) turns a long list of names into one pattern, and keeps the feature
   right when files are added later. Propose the move, with `git mv` so
   history follows, rather than translating the list name by name, and ask
   before doing it: other tools and scripts may name those paths.

4. **Move the defines.** Put each target's `defines` in a `[top.NAME]`
   table, so `--top NAME` brings them and `astli files` writes them out as
   `+define+`.

5. **Move the dependencies, if the user wants astli to resolve them.**
   `astli init --bender` copies them into `[dependencies]`; from then on
   astli ignores `Bender.yml` for dependencies. It leaves out those behind a
   `target`: add those back as optional dependencies of a feature, or as
   `[dev-dependencies]` when only testbenches use them. A version conflict
   stops resolution instead of prompting; pin the package the root decides
   with `[resolve] pinned`. Commit `astli.lock`.

6. **If both tools keep running**, add `astli init --bender --check` to CI.
   It fails when `astli.toml` and `Bender.yml` list different dependencies,
   or take one from different places.

Re-run step 3 after each change, and stop at the first step the user did
not ask for.
