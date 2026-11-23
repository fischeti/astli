# Features and overrides

One design is often built several ways: with simulation models, for an FPGA prototype, or with a foundry's technology cells. `astli` expresses these with **features** and **overrides**.

## Features

As Cargo's features do for Rust, a feature turns on source files, include directories, optional dependencies, or features of a dependency.

Features only add: enabling one adds files to the build and never removes any. Two features that declare the same module are alternatives, such as a behavioural memory model and the wrapper of an SRAM macro, and a design that enables both is an error naming both declarations.

A feature's `sources` claim its files wherever they are below the package root, as `dev` does: a file a feature names is the feature's, inside `[package] sources` or outside it, and is in the design only while the feature is on. A pattern that matches no file is an error, even while the feature is off.

```toml
[features]
# Features enabled by default when another package depends on this one
default = ["vip"]

# A custom feature that introduces extra sources and activates a dependency feature
vip = { sources = ["hw/vip/**"], include-dirs = ["hw/vip/include"], dependencies = ["axi/vip"] }
```

### Controlling features on the command line

A command run in a package enables its `default` features, as a package depending on it would get them. Flags choose others:

```sh
# Disable the default features
astli files --no-features

# Enable only the 'vip' feature
astli files --no-features --features vip

# Enable every feature, alternatives together
astli files --all-features
```

### Choosing between alternatives

A [`[top.NAME]`](reference/manifest.md#topname) table can name the features its top is built with, in place of `default`. Keep the alternatives in one package and give each a table:

```toml
[features]
default = ["behav"]
behav = { sources = ["hw/models/**"] }
asic = { sources = ["hw/macros/**"], dependencies = ["tech_cells/asic"] }

[top.tb_soc]                 # the default features: the behavioural models

[top.soc_asic]
top = "soc"
features = ["asic"]          # the macros' wrappers, and not the models
```

`astli check` and `astli lint` without `--top` check every table, each design with its own features, so a mistake only one alternative holds is still found. `astli files --top soc_asic` writes the filelist of the ASIC design. A package depending on this one gets `behav`, and asks for the other with `default-features = false, features = ["asic"]`.

`astli fmt` and lint's per-file rules read every feature's files, whichever a design enables.

### Features for your own tests

Your testbenches may need what a dependent should only get by asking: test drivers you ship for other blocks' testbenches, or a library's VIP. Name them in `dev-features`, which only runs in your own package enable, whatever `--no-features` or a table selects:

```toml
[package]
dev = ["test/**"]
dev-features = ["test", "axi/vip"]   # your own feature, and a dependency's

[features]
test = { sources = ["src/mem_test.sv"] }   # still a dependent's to ask for
```

A package depending on yours never enables them.

### Configuring dependency features

A dependency comes with its `default` features. A requirement can ask for more, or turn the defaults off:

```toml
[dependencies]
axi = { git = "https://github.com/pulp-platform/axi.git", version = "0.39", features = ["vip"], default-features = false }
```

### Optional dependencies

A dependency a feature's `dependencies` names, as `dependencies = ["tech_foundry"]`, is in the design only while the feature is on:

```toml
[features]
asic = { dependencies = ["tech_foundry"] }

[dependencies]
tech_foundry = { git = "https://github.com/my-org/tech_foundry.git", version = "1.0" }
```

!!! tip "Development dependencies"
    A package only your own verification needs, such as a UVM harness or a VIP, goes in `[dev-dependencies]`. A package depending on yours never fetches it.

## Overrides

Generic code instantiates generic primitives, such as clock gates, reset synchronizers or SRAMs, which a technology's build replaces with its own cells. `[overrides]` lets the root package replace modules, interfaces or packages a dependency provides:

```toml
[features]
ihp13 = { sources = ["target/asic/ihp13/**"] }

[overrides]
# Replace these modules of tech_cells_generic
tech_cells_generic = ["tc_sram_impl", "tc_clk_*"]
```

### How overrides work

1. **Symbol matching**: When `astli` encounters an override pattern (supporting `*` wildcards), it identifies which source files in the specified dependency declare matching modules or interfaces.
2. **File substitution**: If your local package declares a module with the same name, `astli` excludes the dependency's file containing the generic implementation from the build, substituting your local implementation instead.
3. **Feature gating**: If the overriding source file belongs to a feature (such as `ihp13` above), the override is only applied when that feature is active.

!!! warning "File granularity and collisions"
    - `astli` excludes files at the whole-file level. If a dependency file defines both an overridden module and an unrelated module that your design still needs, `astli` will flag an error. Keep overridable cells isolated in individual files.
    - Two packages cannot attempt to override the same symbol. Overrides must be unambiguous.
    - Header files (included via `` `include ``) cannot be overridden; dependencies always resolve their own internal headers first.
