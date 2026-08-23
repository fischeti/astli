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
- **A call's shape depends on the macro table, so raw mode has to build one.**
  `` `FOO (a + b) `` is a call with one argument if `FOO` was defined with
  formals, and a nullary call followed by an unrelated parenthesised expression
  if it was not. Those are different trees. Raw mode therefore tracks
  `` `define `` and `` `undef `` even though it expands nothing: *raw* means
  unexpanded, not unprocessed.

The node is a parser-level one — `MACRO_CALL` over a `DIRECTIVE` token and an
optional `MACRO_ARG_LIST`, all in the same flat kind enum. The lexer keeps
lexing every `` `name `` alike; separating an invocation from a real directive
is a lookup against the ~25 known directive names, and everything else is a
call.

Arity is not always knowable. A macro defined in a header the formatter will
never follow (decision D6) has no entry at all, and with every `` `ifdef ``
branch present at once two branches may define one name with different formals
— so the table needs an *unknown* state rather than a last-write-wins guess.
Where it is unknown, treat an immediately adjacent `(` as an argument list.
Guessing wrong costs tree shape and nothing more, because macro arguments are
byte-preserved either way; on the expanded path the includes have been followed
and the arity is never in doubt.

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

### Measured

`cargo run --release --example conditionals` classifies every conditional
region in the corpus: a region is self-delimiting when each branch balances on
its own, counting `()`, `[]`, `{}`, `begin`/`end`, `case`/`endcase`,
`fork`/`join*`, `module`/`endmodule` and `generate`/`endgenerate`.

The commits are written out here rather than pointed at, because `corpus/` is
gitignored: `corpus/MANIFEST` is overwritten by the next fetch, so a number
that cites it is traceable to nothing. Any measurement quoted in these
documents has to carry its own inputs.

| | commit | regions | self-delimiting |
| --- | --- | --- | --- |
| `axi` | `4da1597974` | 124 | 100.0% |
| `cheshire` | `6234e9e989` | 61 | 100.0% |
| `common_cells` | `db42769334` | 71 | 100.0% |
| `FlooNoC` | `2fa02eb23c` | 3 | 100.0% |
| `iDMA` | `2e0b0fe53b` | 14 | 100.0% |
| `snitch_cluster` | `f78a978343` | 26 | 100.0% |
| `cva6` | `6cb200105f` | 298 | 95.6% |
| `opentitan` | `34ceb5eb56` | 772 | 94.9% |
| `ibex` | `8b8ee086ae` | 295 | 87.5% |
| **deduplicated** | 2026-08-23 | **1366** | **96.4%** |

**The assumption holds.** Freezing 4% of regions verbatim is a cost worth
paying for formatting the other 96% properly.

Read the per-repo column rather than the pooled figure. Six of the nine repos
are perfect, and they are perfect because the PULP and lowRISC house styles
wrap whole items — including `axi`, which is the densest macro user in the
corpus at a dozen invocations per file and gates four different simulators.
Adding them raised the pooled number without testing it: every ragged region
still comes from verification or vendored code, and `ibex` remains the
outlier at 87.5%. The corpus has no genuinely hostile conditional code in it
yet, so treat 96.4% as "clean code is clean" rather than as a bound on what
SystemVerilog can do.

Two findings matter more than the headline number.

**The ragged cases are few and repetitive.** All 49 live in 21 files, and 45
of those are one generated file replicated across 15 target directories. Three
idioms account for every one of them:

- an `` `ifdef `` swapping two spellings of the same declaration head and
  leaving a `{` open across the boundary — `implemented_csr[] = {` against
  `const implemented_csr[] = {`;
- the same trick on a module instantiation, one branch carrying a parameter
  list and the other not, both leaving the port list's `(` open;
- a region whose entire content is a bare `end`.

A fourth turned up in the declaration keywords, which are counted separately
because their prototype forms have no closer: two `interface` headers swapped
between branches, closed by one `endinterface` after the `` `endif ``.

**Ragged does not mean arbitrary.** In 48 of the 49, *every branch agrees on
the same non-zero delta* — the region uniformly opens something that closes
after `` `endif ``. Exactly one region in the corpus has branches that
disagree with each other. So the ragged set is not a grab-bag; it is almost
entirely one shape, and a later version could plausibly format that shape
rather than freeze it. v0 should still freeze it.

**What this cannot see.** A macro that expands to a delimiter is one opaque
token to a pre-pass, so a region split by one counts as self-delimiting here.
This is a lower bound on raggedness, and re-measuring once expansion works is
the obvious M2 follow-up.

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
| Consumer | formatter, linter, LSP *syntactic* requests | compiler, LSP *semantic* requests |
| Macros | surfaced as `MacroCall` atoms | expanded |
| `` `include `` | not followed | followed |
| Conditionals | all branches present, as regions | evaluated, inactive branches dropped |
| Origin map | not needed | required (`svirig-text`) |

**An LSP is a client of both.** The division is not formatter-versus-compiler
but buffer-versus-design: a request answered in the editor's own coordinates
needs the raw stream, one answered about the elaborated program needs the
expanded one. Formatting, folding ranges, selection ranges, document symbols,
semantic tokens and in-file rename are the first kind, and they *cannot* be
served from the expanded stream — it has dropped inactive branches and inlined
other files, and there is no folding a region that is no longer there, nor
greying out a branch that was deleted. Go-to-definition, hover, completion,
cross-file references and semantic diagnostics are the second kind. So an LSP
holds the raw tree as its spine and hangs analysis off it, joined through the
origin map. That is a second and independent reason the map has to exist early,
and it makes raw the more load-bearing of the two modes rather than the cheap
one.

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
- A `` `define `` body line ending in `\` **inside a `//` comment** still
  continues. The comment rule swallows the backslash, so reading the token
  stream naively ends the body a line early — which real code does not
  survive, and `opentitan` has one that would break. Continuation wins over
  the comment.
- Numbers may contain whitespace: `8 'h FF` is legal. `1step` is one token.
- CRLF line endings are common from Windows-based EDA flows.
