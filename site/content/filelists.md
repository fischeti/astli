# Filelists and pickling

`astli files` writes the files a top needs, in an order they compile in. `astli pickle` writes them as one source file.

## Pruning and ordering filelists (`astli files`)

`astli files` reads which modules, interfaces, packages and classes each file declares and uses.

```sh
# The files soc_top needs
astli files -f design.f --top soc_top

# The same, packages and definitions before what uses them
astli files -f design.f --top soc_top --order

# Modules nothing instantiates
astli files -f design.f --emit tops

# Why soc_top needs this file
astli files -f design.f --top soc_top --why rtl/fifo.sv

# An ordered filelist from the current package
astli files --top soc_top > ordered_soc.f
```

!!! warning "A file with errors fails the run"
    If a file has a preprocessor or syntax error, `astli files` writes no list. Code that does not parse may hide an instance, so a list made without it could miss a file.

### Topological ordering (`--order`)

Many tools need a package compiled before what imports it. `--order` puts every file after the files it depends on.

In a package, `astli files` always orders its output, since files it found have no order of their own. Each package's files are listed together, after those of the packages it depends on. A filelist keeps its own order unless `--order` is given, since it may encode an order `astli` cannot see, such as a header of defines that must come first.

`--why <file>` prints the chain of instances and imports that leads from the top to the file.

## Single-file bundling (`astli pickle`)

`astli pickle` writes the files a top needs as one SystemVerilog file, with the headers they include inlined, and their text and comments as written:

```sh
# Generate a single self-contained source file
astli pickle -f design.f --top soc_top --order > soc_bundle.sv
```

### Macro expansion and namespace prefixing

`--expand` writes the preprocessed text instead, and `--prefix` or `--suffix` renames what the file declares:

```sh
# Expand macros and prefix all module/package/class names with 'v2_'
astli pickle -f design.f --top soc_top --order --expand --prefix v2_ > soc_v2.sv
```

!!! note "Multi-version co-simulation"
    `--prefix` and `--suffix` rename every module, interface, package and class, and every use of it. Two versions of one IP can then be compiled into one simulation. `--exclude-rename <NAME>` keeps a name as it is.

## Interoperability with Bender

With [Bender](https://github.com/pulp-platform/bender), `astli files` can trim Bender's list to what a top needs:

```sh
bender script flist-plus -t rtl > full_design.f
astli files -f full_design.f --top soc_top --order > trimmed_soc.f
astli check -f trimmed_soc.f --top soc_top
```

To have `astli` resolve the dependencies instead, see [Coming from Bender](bender.md).
