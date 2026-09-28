# Filelists

`astli files` reads the files of a design and works out which top-level
modules, packages and classes each one declares and uses.

```sh
astli files -f design.f --top soc_top          # only what soc_top needs
astli files -f design.f --top soc_top --order  # packages before their users
astli files -f design.f --emit tops            # modules nothing instantiates
astli files -f design.f --top soc_top --why rtl/fifo.sv
```

Each file is expanded as it would be compiled, with the filelist's
`+incdir+`s and `+define+`s, so a module a macro instantiates counts. A name
used and declared in no file is a warning.

## Pickling

`astli pickle` takes the same selection and writes the files as one source,
each as written with its headers inlined. With `--expand`, each is written as
a compiler reads it instead, and `--prefix` and `--suffix` can rename every
module, interface, program, package and class the files declare, wherever it
is named, so that two designs can share a compilation:

```sh
astli pickle -f design.f --top soc_top --order > soc.sv
astli pickle -f design.f --top soc_top --order --expand --prefix v2_ > soc.sv
```

`--exclude-rename NAME` keeps a name as it is.

## Reading filelists

Every command that reads source takes files, filelists, or both:

- `-f design.f` resolves relative paths against the current directory;
- `-F design.f` resolves them against the filelist's own directory.

Filelists may carry `+incdir+DIR` and `+define+NAME=VALUE`; `-I` and `-D` do
the same on the command line.
