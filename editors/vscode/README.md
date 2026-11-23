# astli for VS Code

Starts [astli](https://astli.dev/)'s language server, `astli lsp`, for
SystemVerilog and Verilog files. It reads the package around the file you
open, the way `astli check` does with no arguments, so it needs no setup
beyond `astli.toml`, if the package has one.

The server provides:

- the problems `astli lint` and `astli check` report, as you type, and the
  fixes lint rules offer as quick fixes;
- formatting, under the package's `[fmt]`;
- go to definition, find references, and highlighting of a name's other
  uses, across the package and its dependencies;
- hover: a name's type and width as elaborated, a parameter's value, what a
  macro call expands to;
- inlay hints: the port or parameter a positional connection lands on, the
  ports a `.*` connects, and what a parameter's expression evaluates to;
- colours for what each name is: a module, a port, a parameter, a type;
- faded `` `ifdef `` branches that the build leaves out, under the package's
  defines;
- symbols in a file, and across the workspace.

The extension itself adds the languages, `.sv` and `.svh` as SystemVerilog
and `.v` and `.vh` as Verilog, with their syntax highlighting.

## The binary

The extension starts the first of:

1. the binary `astli.path` names;
2. an `astli` on `PATH`;
3. the binary the extension ships with.

An install of your own wins over the shipped one, so that the server is the
version the command line runs. The extension ships a binary for macOS on
Apple silicon, Linux and Alpine on x64 and arm64, and Windows on x64; on any
other platform, [install astli](https://astli.dev/) yourself.

## Settings

- `astli.path`: the `astli` binary to start, to try a build from a checkout,
  say. Empty by default. The server restarts when it changes.

## Alongside the Verilog-HDL extension

[Verilog-HDL/SystemVerilog/Bluespec SystemVerilog](https://marketplace.visualstudio.com/items?itemName=mshr-h.VerilogHDL)
gives the languages the same names, so the two work side by side. Turn off
its ctags, so that hover and go to definition come from astli alone, and
format through astli:

```json
{
  "verilog.ctags.enabled": false,
  "[systemverilog]": { "editor.defaultFormatter": "fischeti.astli" },
  "[verilog]": { "editor.defaultFormatter": "fischeti.astli" }
}
```
