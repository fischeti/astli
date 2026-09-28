# astli

A SystemVerilog formatter, and the preprocessor and lossless parser behind it,
in pure Rust.

```systemverilog
module counter #(
  parameter int Width = 8
) (
  input  logic             clk_i,
  input  logic             rst_ni,
  input  logic             en_i,
  output logic [Width-1:0] count_o
);
  always_ff @(posedge clk_i or negedge rst_ni) begin
    if (!rst_ni) count_o <= '0;
    else if (en_i) count_o <= count_o + 1;
  end
endmodule
```

The `astli` command:

- **formats** SystemVerilog in the
  [lowRISC style](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md),
  and refuses a file rather than change what it means;
- **trims and orders filelists** to what a top module needs.

The libraries it is built on are on
[crates.io](https://crates.io/crates/astli), documented on
[docs.rs](https://docs.rs/astli).

## Install

Prebuilt for Linux, macOS and Windows.

=== "Installer"

    ```sh
    curl --proto '=https' --tlsv1.2 -LsSf https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.sh | sh
    ```

    On Windows:

    ```powershell
    powershell -ExecutionPolicy Bypass -c "irm https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.ps1 | iex"
    ```

=== "uv"

    ```sh
    uv tool install astli
    ```

    Or run it once, without installing:

    ```sh
    uvx astli fmt top.sv
    ```

=== "pip"

    ```sh
    pip install astli
    ```

    Or with [pipx](https://pipx.pypa.io), which installs it in an environment
    of its own and puts `astli` on your `PATH`, or runs it once without
    installing:

    ```sh
    pipx install astli
    pipx run astli fmt top.sv
    ```

=== "cargo"

    A prebuilt binary through
    [cargo-binstall](https://github.com/cargo-bins/cargo-binstall), or from
    source:

    ```sh
    cargo binstall astli-cli
    cargo install astli-cli
    ```

## Scope

astli lexes, preprocesses and parses; it does not elaborate or type-check.
Constructs the parser does not handle yet, such as `specify` blocks, gate
primitives and UDPs, are kept as written, so the formatter never loses code.
