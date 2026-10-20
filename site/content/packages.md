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
common_cells = { features = ["asic"] }
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
astli check --features axi/vip          # a dependency's too
```

A dependency enables its `default` features, and every feature something in
the design asks of it, through `[dependencies]`, a feature, or
`--features`.

## Headers and units

A file another one includes is a header, whatever its extension; any other
`.sv` or `.v` file is compiled on its own. An `` `include `` is looked for
beside the file, then in `include-dirs`, then as the one file of the package
whose path ends in the name. Two such files are an error where the include is
expanded.

`files` writes the include directories a tool needs to find every header
without that last search.

## Dependencies

A package with a `Bender.yml` has its dependencies located by `bender`, which
fetches them if need be. Each is read as a package itself: its own
`astli.toml`, or the defaults without one, less what its `dev` names. Their
findings are not reported.
A module two files declare is an error if one of the package's files uses it,
so a dependency with several implementations of one module needs an
`astli.toml` that leaves all but one out.
