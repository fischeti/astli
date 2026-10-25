# Features and overrides

In hardware design, different build targets often require different source configurations—for example, switching between simulation verification models, FPGA prototypes, and ASIC foundry technology cells.

`astli` manages these variations cleanly through **features** and **overrides**.

---

## Features

Much like Cargo features in Rust, an `astli` feature allows you to conditionally activate additional source files, include directories, optional dependencies, or downstream dependency features.

Features are additive: enabling a feature adds sources to your build, but never removes base sources. This guarantees that combining features always yields a well-defined design.

```toml
[features]
# Features enabled by default when another package depends on this one
default = ["vip"]

# A custom feature that introduces extra sources and activates a dependency feature
vip = { sources = ["hw/vip/**"], include-dirs = ["hw/vip/include"], dependencies = ["axi/vip"] }
```

### Controlling features on the command line

When working inside a package directly, `astli` enables **all** declared features by default. You can fine-tune this on the command line:

```sh
# Disable all features declared in this package
astli files --no-features

# Disable defaults and explicitly enable only the 'vip' feature
astli files --no-features --features vip
```

### Configuring dependency features

When you declare a dependency in `[dependencies]`, it automatically activates that dependency's `default` features. You can request additional features or turn off defaults:

```toml
[dependencies]
axi = { git = "https://github.com/pulp-platform/axi.git", version = "0.39", features = ["vip"], default-features = false }
```

### Optional dependencies

You can make an entire dependency conditional on a feature. When you reference a dependency name in a feature's `dependencies` list (e.g. `dependencies = ["tech_foundry"]`), that dependency is only pulled into the design when the feature is enabled:

```toml
[features]
asic = { dependencies = ["tech_foundry"] }

[dependencies]
tech_foundry = { git = "https://github.com/my-org/tech_foundry.git", version = "1.0" }
```

!!! tip "Development dependencies"
    For packages only needed during internal verification (like UVM test harnesses or mock VIP), place them in `[dev-dependencies]`. Downstream dependents will never download or compile them.

---

## Overrides

In ASIC and FPGA development, high-level code often instantiates generic primitives (such as clock gates, reset synchronizers, or SRAM macros) that must be replaced by technology-specific macros in foundry builds.

The `[overrides]` table allows a root package to substitute module, interface, or package implementations provided by a dependency:

```toml
[features]
ihp13 = { sources = ["target/asic/ihp13/**"] }

[overrides]
# Instructs astli to replace matching modules in tech_cells_generic
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
