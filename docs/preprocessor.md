# Preprocessing

This is the hard part, and getting it wrong invalidates everything downstream.
It is written up separately from [`plan.md`](plan.md) because it will get long
and because it is the part most likely to be revised.

## The problem

A compiler consumes preprocessed text and is, in that one respect, in an easy
position: macros are gone, inactive `` `ifdef `` branches are gone, includes
are inlined. A formatter is never handed preprocessed text. It has to format
source that may not be simultaneously valid under any single set of macro
definitions, and it has to leave the preprocessed meaning untouched for *every*
set of definitions at once.

## What `slang` does

Worth knowing precisely, because the shape of it is right.

The preprocessor sits between the lexer and the parser and emits `Trivia` for
every directive. Its trivia kinds are:

```
Unknown  Whitespace  EndOfLine  LineComment  BlockComment
DisabledText  SkippedTokens  SkippedSyntax  Directive
```

A `Directive` trivium carries a full syntax node (`DefineDirectiveSyntax`,
`IncludeDirectiveSyntax`, …), so directives are structured, not opaque text. An
inactive `` `ifdef `` branch becomes a single opaque `DisabledText` trivium.

The consequence is elegant: the CST is *simultaneously* preprocessed and
losslessly round-trippable. `SyntaxPrinter` replays the original bytes exactly,
and can filter by location to include or exclude macro-expanded and included
content.

**What it cannot do is format inside a disabled branch.** `DisabledText` is a
blob. That is precisely the line we have to draw somewhere else.

## The design: three levels

### Level A — directive as opaque trivia

`rdlfmt`'s model, extended to handle `\` line continuations so a multi-line
`` `define `` body is a single trivium. Sound and cheap for `` `define ``,
`` `include ``, `` `undef ``, `` `timescale ``, `` `line ``,
`` `default_nettype ``, `` `resetall ``, `` `celldefine ``, `` `pragma ``.

The soundness argument from `rdlfmt` carries over unchanged: preprocessing
depends on nothing but the token sequence and the rule that a directive owns
its line, so any whitespace change that preserves both is invisible to it.

### Level B — macro invocations are grammar atoms

Non-negotiable for SystemVerilog, and the part SystemRDL gave no practice in.
UVM code puts macro calls in every syntactic position:

```systemverilog
class my_txn extends uvm_sequence_item;
  `uvm_object_utils_begin(my_txn)        // class member position
    `uvm_field_int(m_addr, UVM_ALL_ON)
  `uvm_object_utils_end

  function void go();
    `uvm_info("TAG", "msg", UVM_LOW)     // statement position
  endfunction
endclass
```

So a `MacroCall` node must be admissible at **item, member, statement,
expression, port-list-element, and type** position. Two rules:

- **Macro arguments are balanced token soup, not expressions.** Parse them as
  token lists with paren/bracket/brace matching and nothing more. Anything
  cleverer is wrong, because a macro argument is text.
- A macro *reference* is never trivia. It is an atom that may stand for a
  value, a name, a type, or a whole declaration.

Getting this wrong is what makes most SV tooling useless on verification code.

### Level C — conditionals as structured regions

The real problem. Model `` `ifdef / `ifndef / `elsif / `else / `endif `` as a
CST node with branch children, then classify each region with a cheap
token-level pre-pass:

> **Is every branch self-delimiting?** That is: does each branch have balanced
> `()`, `[]`, `{}`, `begin`/`end`, `module`/`endmodule`, `case`/`endcase`,
> `fork`/`join*`, `function`/`endfunction`, … and does every branch start and
> end at the same kind of boundary (item, member, statement, list element)?

**If yes** — the overwhelming majority, where `` `ifdef `` wraps whole ports,
items, or statements — parse each branch independently in the enclosing parse
context and format all of them normally:

```systemverilog
module foo #(
`ifdef WIDE
  parameter int W = 64,
`else
  parameter int W = 32,
`endif
) ( ... );
```

**If no** — the ragged case, where a delimiter is handed across branches —
freeze the region verbatim, byte-for-byte, and resync after `` `endif ``:

```systemverilog
`ifdef SYNTHESIS
  always_comb begin
`else
  always_ff @(posedge clk) begin
`endif
```

This two-tier behaviour is the main formatting-quality differentiator against
`verible-verilog-format`, which handles neither case well.

> **Measure this before building on it.** The assumption that self-delimiting
> regions dominate is falsifiable in an afternoon during M2: classify every
> conditional region in the corpus and count. If ragged regions turn out to be
> common in real code, Level C needs rethinking before the formatter is
> written, not after.

## The transparency invariant

The formatter must be **preprocessor-transparent**:

> for every macro environment *E*, `preprocess(format(src), E)` equals
> `preprocess(src, E)`.

Environments can't be enumerated — but the property follows from a purely
syntactic one that is checkable on a single file with no environment at all:

1. the raw token sequence (directives included, whitespace excluded) is
   unchanged; **and**
2. every directive still owns its own line; **and**
3. no `\` line-continuation boundary moved.

Because preprocessing is a function of exactly those three things, preserving
them preserves the result under every environment simultaneously.

**Enforce it as an assertion inside `format()`, not only as a test.** It is
cheap (one re-lex) and it converts an entire class of silent corruption into a
loud refusal. `rdlfmt` already does the weaker version of this; here it is
load-bearing.

### Two exceptions that will bite

- **Never reformat a `` `define `` body.** The body is *text*. If the macro
  uses `` `" `` stringification, whitespace inside the body is observable in
  the program's output. Treat bodies as verbatim unless it can be proven that
  no stringification occurs — and it usually can't be, cheaply. Reindenting a
  continued body also violates invariant (3).
- **Escaped identifiers** (`` \foo.bar[3] ``) are terminated by whitespace, so
  their trailing whitespace is significant and cannot be collapsed. A lexer
  trap and a formatter trap both.

## Two output modes

`svirig-preproc` produces one of two token streams from the same machinery. The
parser is parameterised over the source and does not know which it got.

| | **Raw** | **Expanded** |
| --- | --- | --- |
| Consumer | formatter, LSP, linter | compiler / semantic analysis |
| Macros | surfaced as `MacroCall` atoms | expanded |
| `` `include `` | not followed | followed |
| Conditionals | all branches present, as regions | evaluated, inactive branches dropped |
| Origin map | not needed | required (`svirig-text`) |

The origin map is `slang`'s definition-location vs. expansion-location
distinction, and every diagnostic in the compiler path needs it ("this token
came from `` `FOO `` expanded at line 40, defined at line 12"). It must exist
before the expanded mode has any users; retrofitting it is painful. Give it a
name that isn't `SourceManager`.

## Smaller things not to forget

- `` `__FILE__ `` and `` `__LINE__ ``, and interaction with `` `line ``.
- `` `pragma protect `` encrypted IP envelopes — verbatim passthrough, and a
  lexer mode.
- `` `begin_keywords `` / `` `end_keywords `` change the keyword set mid-file.
- `` `"``, `` `\`" ``, and ``` `` ``` inside macro bodies (stringification and
  token pasting).
- Macros that expand to other macros, and recursion detection.
- Numbers may contain whitespace: `8 'h FF` is legal. `1step` is one token.
- CRLF line endings are common from Windows-based EDA flows.
