# Filelists and pickling

Modern SystemVerilog designs often accumulate hundreds of files across numerous IP repositories. Managing, trimming, and ordering these files for synthesis and simulation can quickly become a maintenance headache.

`astli` provides two dedicated commands to automate this process: `astli files` to prune and order filelists, and `astli pickle` to bundle designs into a single self-contained source file.

---

## Pruning and ordering filelists (`astli files`)

`astli files` analyzes the symbol dependencies across your design, identifying which modules, packages, interfaces, and classes are declared and referenced by each file.

```sh
# Emit only the files strictly required to build 'soc_top'
astli files -f design.f --top soc_top

# Emit the trimmed files in correct topological compilation order
astli files -f design.f --top soc_top --order

# Discover candidate top-level modules (modules not instantiated by any other file)
astli files -f design.f --emit tops

# Trace why a specific file is included in the build
astli files -f design.f --top soc_top --why rtl/fifo.sv

# Generate an ordered filelist directly from the local package
astli files --top soc_top > ordered_soc.f
```

!!! warning "Strict error handling"
    If any source file read by `astli files` contains preprocessor or syntax errors, `astli files` prints the diagnostics, exits with status 1, and refuses to emit a filelist. Because unparsable blocks might contain module instantiations, generating a filelist from broken sources could silently omit required dependencies.

### Topological ordering (`--order`)

Legacy EDA simulators and synthesis tools often require compilation in strict topological order—packages and base definitions must be compiled before the modules that import or instantiate them. Passing `--order` ensures that packages and dependencies are placed before their dependents.

Run from a package with no filelist, `astli files` always orders its output, since the files it discovers have no order of their own. A filelist keeps the order it gives unless `--order` is passed, since that order may carry something `astli` cannot see, such as a defines file that must come first.

!!! tip "Auditing dependencies with `--why`"
    If you're wondering why an unexpected file or legacy module is being pulled into your build, pass `--why <file>`. `astli` will print the exact chain of instantiations and package imports connecting that file back up to `--top`.

---

## Single-file bundling (`astli pickle`)

`astli pickle` bundles all source files required by a top module into a single, unified SystemVerilog file. By default, it inlines referenced headers (` `include `) while preserving original formatting and comments:

```sh
# Generate a single self-contained source file
astli pickle -f design.f --top soc_top --order > soc_bundle.sv
```

### Macro expansion and namespace prefixing

You can instruct `astli pickle` to fully expand preprocessor directives and rename declared symbols using `--prefix` or `--suffix`:

```sh
# Expand macros and prefix all module/package/class names with 'v2_'
astli pickle -f design.f --top soc_top --order --expand --prefix v2_ > soc_v2.sv
```

!!! note "Multi-version co-simulation"
    The `--prefix` and `--suffix` flags rename all modules, interfaces, packages, and classes—along with all their instantiation sites—across the generated file. This allows you to compile two different versions or revisions of the same IP core into a single simulation testbench without symbol collisions.

    If certain top-level interfaces or external library names should not be modified, use `--exclude-rename <NAME>` to keep them unchanged.

---

## Interoperability with Bender

If your workflow currently uses [Bender](https://github.com/pulp-platform/bender), you can use `astli` as a post-processor to trim Bender's output down to what a specific top-level module requires:

```sh
# Generate a full filelist from Bender, then let astli trim and sort it
bender script flist-plus -t rtl > full_design.f
astli files -f full_design.f --top soc_top --order > trimmed_soc.f
astli check -f trimmed_soc.f --top soc_top
```

To learn how `astli` can completely replace Bender for package resolution, see [Coming from Bender](bender.md).
