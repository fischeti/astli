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

See [Packages](../packages.md) for details.

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

Describes the files of a dependency that has no `astli.toml` of its own, as its manifest's `[package]` would. Only the root package's descriptions are read, and only while the dependency has no `astli.toml`. A description is a stopgap: move it into the dependency's own manifest.

| Key | Default | Description |
| :--- | :--- | :--- |
| `sources` | `["."]` | Directories of the dependency scanned for source files, relative to its root. |
| `include-dirs` | `["include"]` | The dependency's header search directories exported to its dependents, relative to its root. |
| `exclude` | `[]` | Glob patterns of the dependency's files to ignore, relative to its root. |

See [Describing a dependency](../dependencies.md#describing-a-dependency-without-an-astlitoml).

---

## `[fmt]`

Sets automated code layout and formatting rules for `astli fmt`.

| Key | Default | Description |
| :--- | :--- | :--- |
| `width` | `100` | Maximum line width in columns before wrapping. |
| `indent` | `2` | Number of spaces per indentation level. |
| `max-pad` | `12` | Maximum padding spaces inserted to vertically align tokens in columns. |

See [Formatting](../formatting.md#configuration).

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
