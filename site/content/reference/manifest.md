# `astli.toml` manifest reference

The `astli.toml` file at your package root defines metadata, source search paths, features, dependencies, and toolchain configurations.

All keys in `astli.toml` are optional. Relative paths are resolved against the directory containing `astli.toml`. File patterns support standard glob syntax: `*` matches within a single path segment, while `**` crosses directory separators.

```toml
[package]
name = "my_soc"
sources = ["hw"]
include-dirs = ["hw/include"]
exclude = ["hw/scripts/**"]
dev = ["hw/tb/**"]
default-top = "tb_soc"

[top.tb_soc]
defines = ["SIMULATION"]

[top.soc_fpga]
top = "soc"
defines = ["FPGA"]

[features]
default = ["vip"]
vip = { sources = ["hw/vip/**"], include-dirs = ["hw/vip/include"], dependencies = ["axi/vip"] }

[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38", features = ["asic"] }
idma = { git = "https://github.com/pulp-platform/iDMA.git", rev = "main" }
tech = { path = "../tech" }

[dev-dependencies]
common_verification = { git = "https://github.com/pulp-platform/common_verification.git", version = "0.2" }

[overrides]
tech_cells_generic = ["tc_sram", "tc_clk_*"]

[resolve]
pinned = ["common_cells"]

[described.tech_cells_generic]
exclude = ["src/fpga/**"]

[fmt]
width = 100
indent = 2
max-pad = 12
exclude = ["hw/vendor"]

[lint]
warn = ["restriction"]

[lint.paths]
"hw/vendor/**" = { allow = ["lowrisc"] }
```

---

## `[package]`

Defines basic package identity and file discovery boundaries.

| Key | Default | Description |
| :--- | :--- | :--- |
| `name` | *(optional)* | Name identifier of the package. |
| `sources` | `["."]` | Directories scanned recursively for `.sv`, `.svh`, `.v`, and `.vh` source files. |
| `include-dirs` | `["include"]` | Header search directories exported to downstream packages depending on this one. |
| `exclude` | `[]` | Glob patterns of files to ignore (applied in addition to `.gitignore`). |
| `dev` | `[]` | Glob patterns of internal files (such as testbenches) compiled only when working directly in this package, never by downstream dependents. |
| `default-top` | *(optional)* | Default top module or `[top.NAME]` table elaborated from when `--top` is omitted. |

See [Packages](../packages.md) for details.

---

## `[top.NAME]`

Configures elaboration roots and target-specific preprocessor defines for top-level modules. Used by `check`, `lint`, and `files` to eliminate repetitive command-line flags. When generating filelists, `astli files` outputs these defines as `+define+` plusargs.

| Key | Default | Description |
| :--- | :--- | :--- |
| `top` | `NAME` | The root module, interface, or program to elaborate from. |
| `defines` | `[]` | Preprocessor macros defined for every file in the hierarchy (`NAME` or `NAME=VALUE`). |

- **Top resolution**: Passing `--top NAME` selects the `[top.NAME]` table if defined, or falls back to resolving a module named `NAME`.
- **Default tops**: If `--top` is omitted, `astli` elaborates using `default-top` if specified in `[package]`, or automatically selects the table if only one `[top.NAME]` exists.
- **Macro precedence**: Command-line `-D` flags override macros defined in manifest tables. If multiple tops are elaborated together, their defines are merged; contradictory macro values will trigger an error.
- **Root-only**: Only `[top]` tables in the root manifest are evaluated; dependency tops are private to those dependencies.

---

## `[features]`

Defines optional or target-specific compilation configurations.

The special `default` key lists features automatically enabled when another package imports this project without specifying features. Every other key defines a feature table:

| Key | Description |
| :--- | :--- |
| `sources` | Glob patterns of files added to the design when this feature is activated. |
| `include-dirs` | Additional header include paths activated with this feature. |
| `dependencies` | Activates dependent features (`"dep/feature"`) or makes an entire dependency optional (`"dep"`). |

See [Features and overrides](../features.md).

---

## `[dependencies]` and `[dev-dependencies]`

Declares external IP libraries and sub-packages.

| Key | Description |
| :--- | :--- |
| `git` | Remote Git repository URL. |
| `version` | Semver version requirement matching repository `vX.Y.Z` tags. |
| `rev` | Specific Git branch name, tag, or commit hash (alternative to `version`). |
| `path` | Filesystem path to a local package directory (alternative to `git`). |
| `features` | List of features to activate on the dependency. |
| `default-features` | Set to `false` to disable the dependency's `default` feature set. |

`[dependencies]` are exposed transitively to parent projects, while `[dev-dependencies]` are only compiled for internal tests and simulations within the local package.

!!! note "Bender fallback"
    If no `[dependencies]` table is present in `astli.toml`, `astli` will automatically look for and parse dependencies from an adjacent `Bender.yml`. See [Coming from Bender](../bender.md).

---

## `[overrides]`

Enables foundry or platform cell substitutions. Each key names a dependency, mapped to an array of module, package, or interface names to substitute. Wildcards (`*`) are supported:

```toml
[overrides]
tech_cells_generic = ["tc_sram_*", "tc_clk_gate"]
```

See [Overrides](../features.md#overrides).

---

## `[resolve]`

Configures dependency resolution solver preferences.

| Key | Description |
| :--- | :--- |
| `pinned` | Array of package names where the root package's requirement strictly overrides all downstream versions. |

---

## `[described.NAME]`

Describes source files and header directories for an upstream dependency that lacks an `astli.toml` manifest of its own.

Only descriptions declared in the root package's manifest are evaluated, and they are automatically disabled once the dependency provides its own `astli.toml`.

| Key | Default | Description |
| :--- | :--- | :--- |
| `sources` | `["."]` | Directories within the dependency scanned for source files, relative to the dependency root. |
| `include-dirs` | `["include"]` | Header search directories exported to dependents, relative to the dependency root. |
| `exclude` | `[]` | Glob patterns of files to ignore within the dependency, relative to the dependency root. |

See [Describing unmanifested dependencies](../dependencies.md#describing-unmanifested-dependencies-described).

---

## `[fmt]`

Sets automated code layout and formatting rules for `astli fmt`.

| Key | Default | Description |
| :--- | :--- | :--- |
| `width` | `100` | Maximum line width in columns before wrapping. |
| `indent` | `2` | Number of spaces per indentation level. |
| `max-pad` | `12` | Maximum padding spaces inserted to vertically align tokens in columns. |
| `exclude` | `[]` | Glob patterns of files to leave unformatted, relative to the manifest; a directory excludes everything below it. Applies to files passed on the command line as well. |

See [Formatting](../formatting.md#configuration) and [Excluding files](../formatting.md#excluding-files-from-formatting).

---

## `[lint]`

Configures project-wide lint rule severities and waivers.

| Key | Description |
| :--- | :--- |
| `allow` | List of rules or rule groups to silence across the package. |
| `warn` | List of rules or rule groups to report as warnings without failing builds. |
| `deny` | List of rules or rule groups to enforce strictly as fatal errors (exit code 1). |
| `paths` | Map of file glob patterns to nested `{ allow, warn, deny }` severity tables for targeted path-based waivers. |

See [Linting](../linting.md#path-based-configuration).
