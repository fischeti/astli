# Linting

```sh
astli lint top.sv                   # report what the rules find
astli lint -f src.f                 # every file in a filelist
astli lint -W correctness rtl/*.sv  # report a group as warnings
astli lint --list                   # every rule, its group and its level
```

Each file is linted on its own, as written, as the formatter reads it:
includes are not followed, no `+define+` reaches a rule, and every branch of
an `` `ifdef `` is read. What a macro call might write is not looked into, so
no rule reports on it.

A finding at the `deny` level is an error, and fails the run; one at `warn` is
reported and does not.

## Rules and groups

Every rule belongs to a group, which sets its level unless you set one:

- **`correctness`**, denied: almost certainly a bug, such as a blocking
  assignment in `always_ff` or a `case` label written twice.
- **`suspicious`**, warned: legal, and more often wrong than meant, such as
  `always @*` or a `case` without `default`.
- **`lowrisc`**, warned: departures from
  [lowRISC's style guide](https://github.com/lowRISC/style-guides/blob/master/VerilogCodingStyle.md),
  such as a parameter neither `CamelCase` nor `ALL_CAPS`.
- **`restriction`**, allowed: what a project may choose to forbid.

Where [verible](https://github.com/chipsalliance/verible) has the same rule,
it has the same name and checks the same thing. `astli lint --list` lists them
all.

`-A`, `-W` and `-D` set a rule, or every rule of a group, to `allow`, `warn`
or `deny`. A rule named on its own wins over its group:

```sh
astli lint -A lowrisc -W parameter-name-style rtl/*.sv
```

## Waivers

To turn a rule off for one construct, put an attribute on it naming the rule,
or its group:

```systemverilog
(* astli_allow = "always-ff-non-blocking" *)
always_ff @(posedge clk_i) begin
  q = d;
end
```

It covers the construct it stands on and everything inside it: a statement, a
procedural block, a declaration, an instance, a port, a module. Several names
go in one string, separated by commas. A name that is no rule or group is
reported, so a misspelt waiver does not pass unnoticed. Other tools ignore an
attribute they do not know.

## `astli.toml`

What no attribute reaches, such as a header of `` `define ``s or vendored code
nobody edits, gets its levels by path in an `astli.toml`:

```toml
[lint]
warn = ["restriction"]

[lint.paths]
"hw/vendor/**" = { allow = ["lowrisc"] }
"hw/dv/sv/dv_utils/dv_macros.svh" = { allow = ["macro-name-style"] }
```

`[lint]` sets levels for every file, as the flags do. `[lint.paths]` sets them
for the files a pattern matches, relative to the `astli.toml`; `*` does not
cross a `/` and `**` does. The file applies first, then the flags, then the
patterns, in the order written: a waiver for a path holds whatever the
command line asks.

`astli lint` reads the first `astli.toml` in the current directory or above
it, or the one `--config` names.

## In CI

The run fails on a finding at `deny`, so a workflow step is enough:

```yaml
- uses: astral-sh/setup-uv@v7
- run: uvx astli@0.3.0 lint -f src.f
```

Pin the version: a new rule, or a rule that finds more, would fail the step on
code that has not changed.
