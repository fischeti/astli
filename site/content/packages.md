# Packages

With no file named, `fmt`, `lint`, `check` and `files` read the package the
current directory is in:

```sh
astli fmt --check   # every file the package holds
astli lint          # its files, its dependencies in the design
astli check         # its design, its dependencies' files included
astli files --top soc_top --order > design.f
```

## Finding the package

The package's root is the nearest directory above the current one with an
`astli.toml`, up to the repository's root; without one, the repository's
root. Every path in `astli.toml` is relative to it:

```toml
[package]
name = "my_soc"
sources = ["hw"]               # where to look; the root by default
include-dirs = ["hw/include"]  # headers a dependent sees; "include" by default
exclude = ["hw/scripts/**"]    # on top of .gitignore
dev = ["hw/tb/**"]             # read here, never by a dependent

[features]
default = ["vip"]              # what a dependent gets without asking
vip = { sources = ["hw/vip/**"], include-dirs = ["hw/vip/include"], dependencies = ["axi/vip"] }

[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38", features = ["asic"] }

[dev-dependencies]
common_verification = { git = "https://github.com/pulp-platform/common_verification.git", version = "0.2" }

[overrides]
tech_cells_generic = ["tc_sram", "tc_clk_*"]  # names it may replace there
```

The files are every `.sv`, `.svh`, `.v` and `.vh` under `sources`, less what
the `.gitignore` files inside the package and `exclude` name. A directory with
an `astli.toml` of its own is another package, and is left out.

## Features

A feature adds files, include directories, and features of the dependencies.
The files its `sources` name are read only when it is enabled; a feature
never takes a file away, so any set of features is a design. The package's
own runs enable all of its features:

```sh
astli files --no-features               # none of the package's own
astli files --no-features --features vip
```

`--features` names the package's own features. To select a dependency's
feature in some runs and not others, give the package a feature that
enables it, as `vip` enables `axi/vip` above.

A dependency enables its `default` features, and every feature something in
the design asks of it, through `[dependencies]`, a feature, or
`--features`. Naming features adds to the defaults; they are off only where
every package depending on it says `default-features = false`.

A feature can also bring in a whole dependency: one it names alone, as
`dependencies = ["tech"]`, is in the design only while that feature is
enabled. `[dev-dependencies]` are for the package's own runs, such as a
testbench's verification IP; a package depending on it never sees them.

## Overrides

`[overrides]` names what the package may replace in a dependency, by the
modules, packages and interfaces declared there; `*` matches any run of
characters. Where one of the package's own files declares such a name, the
dependency's files declaring it are left out. A file is left out whole, so
one that also declares a name nothing replaces is an error, as is a name two
packages replace. An override follows the feature that brings in its file:

```toml
[features]
ihp13 = { sources = ["target/asic/ihp13/**"] }

[overrides]
tech_cells_generic = ["tc_sram_impl", "tc_clk_*"]
```

Headers are not replaced: a dependency finds its own headers first.

## Headers and units

A file another one includes is a header, whatever its extension; any other
`.sv` or `.v` file is compiled on its own. An `` `include `` is looked for
beside the file, then in `include-dirs`, then as the one file of the package
whose path ends in the name. Two such files are an error where the include is
expanded.

`files` writes the include directories a tool needs to find every header
without that last search.

## Dependencies

A package's dependencies are its `[dependencies]`, or without that table
the `dependencies` of its `Bender.yml`. astli fetches them into a cache
every project shares and chooses one version of each, then reads each as a
package itself: its own `astli.toml`, or the defaults without one, less what
its `dev` names. Their findings are not reported.

```toml
[dependencies]
common_cells = { git = "https://github.com/pulp-platform/common_cells.git", version = "1.38" }
idma = { git = "https://github.com/pulp-platform/iDMA.git", rev = "main" }
tech = { path = "../tech" }

[resolve]
pinned = ["common_cells"]   # the root's requirement is the only one
```

A requirement two dependencies cannot both meet is an error, unless the root
names the package in `[resolve] pinned`. The versions chosen go to
`astli.lock`, which later runs keep to, without fetching, until
`astli update` moves them; `astli update NAME` moves only that one. A
project without an `astli.toml` gets no lock unless `astli update` writes
one. `--locked` fails rather than write the lock, for CI.
A module two files declare is an error if one of the package's files uses it,
so a dependency with several implementations of one module needs an
`astli.toml` that leaves all but one out.
