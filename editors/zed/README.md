# astli for Zed

Starts `astli lsp` for Verilog and SystemVerilog files. The languages come
from the [`verilog`](https://github.com/someone13574/zed-verilog-extension)
extension, which has to be installed too.

## Install

The extension is not published. In Zed, run `zed: install dev extension` and
choose this directory. Zed builds it itself, which needs Rust installed
through `rustup`.

## Settings

Zed starts every server an extension names for a language, so say which one
it is to be:

```json
{
  "languages": {
    "SystemVerilog": { "language_servers": ["astli"] },
    "Verilog": { "language_servers": ["astli"] }
  }
}
```

The server is the `astli` on `PATH`. To try a build from a checkout without
replacing it, name the binary and its arguments: with a `path` set, Zed
starts it itself, with the arguments the settings give and none of its own.

```json
{
  "lsp": {
    "astli": {
      "binary": {
        "path": "/path/to/astli/target/release/astli",
        "arguments": ["lsp"]
      }
    }
  }
}
```

`"formatter": "language_server"` in a language's settings formats through
the server, which reads the package's `[fmt]`.
