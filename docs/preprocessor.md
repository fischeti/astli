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

One trivium per directive, covering its whole line, with `\` continuations
folded in so a multi-line `` `define `` body stays one. Sound and cheap for
`` `define ``, `` `include ``, `` `undef ``, `` `timescale ``, `` `line ``,
`` `default_nettype ``, `` `resetall ``, `` `celldefine ``, `` `pragma ``.

The soundness argument: preprocessing depends on nothing but the token
sequence and the rule that a directive owns its line, so any whitespace change
that preserves both is invisible to it.

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

**Built.** The node is a parser-level one — `MACRO_CALL` over a `TICK_IDENT`
token and an optional `MACRO_ARG_LIST` of `MACRO_ARG`s, all in the same flat
kind enum. The lexer keeps lexing every `` `name `` alike, which is why the
token is named for how it is written; separating an invocation from a real
directive is a lookup against the ~25 known directive names, and everything
else is a call.

A call is built **wherever it stands**, including in the middle of a `VERBATIM`
run, which at this stage is most places. That is what makes the claim above
real rather than aspirational: the positions a macro is written in are exactly
the positions the grammar has not reached yet.

Getting this wrong is what makes most SV tooling useless on verification code.
It is also the common case by a wide margin. Over the same corpus commits as
[Measured](#measured) below, deduplicated, a `` `name `` token is a macro
reference four times out of five:

| | count |
| --- | --- |
| macro references | 39526 |
| `` `include `` | 3524 |
| `` `define `` | 1820 |
| conditionals (`` `ifdef ``, `` `ifndef ``, `` `elsif ``, `` `else ``, `` `endif ``) | 3241 |
| `` `undef `` | 382 |
| everything else | 76 |

Those are `` ` `` *tokens*. The reference count below is smaller, because it
counts what one scan reports as items: a reference inside a `` `define `` body,
or inside another reference's arguments, belongs to that text and is found by
scanning it rather than by appearing in the flat list.

Only 14 of the 22 standard directives appear at all. The eight that do not are
`` `begin_keywords ``, `` `end_keywords ``, `` `celldefine ``,
`` `endcelldefine ``, `` `line ``, `` `unconnected_drive ``,
`` `nounconnected_drive `` and `` `undefineall ``.

Arity is not always knowable. A macro defined in a header the formatter will
never follow (decision D6) has no entry at all, and with every `` `ifdef ``
branch present at once two branches may define one name with different formals
— so the table needs an *unknown* state rather than a last-write-wins guess.
Guessing wrong costs tree shape and nothing more, because macro arguments are
byte-preserved either way; on the expanded path the includes have been followed
and the arity is never in doubt.

**Unknown is the common case in raw mode, not the corner.** Over the corpus
commits below, deduplicated to 4475 files, only 1482 of 29724 references have
their definition in their own file. Arity is unknown for 95% of them, and
always will be, because D6 says the includes are never followed. So the
fallback rule is the *main* rule and deserves to be chosen on evidence.

The rule taken is: **a `(` anywhere on the same line opens an argument list,
unless a definition in scope says the macro is nullary.** Requiring adjacency
instead — which is what 22.5.1 requires of a *formal* list, but not of an
actual one — misreads 256 calls on 24 distinct names, because both house styles
align the parenthesis of a repeated call:

```systemverilog
`uvm_field_int   (is_active,   UVM_DEFAULT)
`AXI_ASSIGN (slink_slv_mux[0], slink_mst_ext)
```

Reading to the end of the line misreads 12, all of them one macro in one file:
`` `define WITH iff `` is a nullary stand-in for a keyword, and
`` `WITH (!rs3_valid) `` hands it a parenthesised expression. So the rule is
wrong twenty times less often — and wrong in the cheaper direction, since a
spurious argument list still reproduces its own bytes while a missed one leaves
a parenthesised expression in item position, where the parser can only fall
back to verbatim. Stopping at the newline costs nothing measurable: no call in
the corpus puts its `(` on the next line.

Recorded in [`limitations.md`](limitations.md), because 12 is not zero.

### Level C — conditionals as structured regions

**Built**, as far as the preprocessor and the tree go: see
[Evaluating a conditional](#evaluating-a-conditional) for the first, and
`CONDITIONAL_REGION` / `CONDITIONAL_BRANCH` for the second — every branch a
region writes is in the tree, because raw mode cannot evaluate the condition.

Reading the region as one **atom** has a second effect worth naming. The
fallback run around it never looks inside, so a ragged region hands it no
unbalanced delimiter, and what raggedness costs is the one construct enclosing
the region rather than the rest of the file.

The classification below is what the parser does *inside* a region, and M3
built it: `RegionShape::live`, worked out over raw tokens when the stream is
built, and acted on in `svirig-parse`'s `preprocessor.rs`. **1,571 of the parser's 1,660 corpus regions are
live**; the figure differs from the 96.4% below because that one is measured
over a deduplicated set that includes `.v` and `.vh`.

The real problem. Model `` `ifdef / `ifndef / `elsif / `else / `endif `` as a
CST node with branch children, then classify each region with a cheap
token-level pre-pass:

> **Is every branch self-delimiting?** That is: does each branch have balanced
> `()`, `[]`, `{}`, `begin`/`end`, `module`/`endmodule`, `case`/`endcase`,
> `fork`/`join*`, `function`/`endfunction`, … and does every branch start and
> end at the same kind of boundary (item, member, statement, list element)?

The implementation asks the first half and not the second, and it leaves
`function` and the four keywords like it out of the count, because each has a
prototype form with no closer at all — the same five the fallback has to guess
about, and [written up as a limitation](limitations.md) for the same reason.

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
leave the region to the fallback and resync after `` `endif ``. The branches
are not frozen quite byte-for-byte in the end: the preprocessor's own rules and
declarations still run inside one, because both are self-contained and
all-or-nothing, and neither can go looking for the `end` that the *next* branch
writes.

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
That made the figure a lower bound on raggedness.

**Re-measured with the macros expanded, and the bound is tight.** The example
now measures every region twice, the second time expanding each branch before
counting its delimiters. **Not one of the 1366 regions reads differently.** No
`` `MY_BEGIN `` hides in a branch of the corpus, so 96.4% is the number and not
a floor under it. The example prints how many regions the two readings disagree
about for exactly this reason: two columns agreeing proves nothing unless the
second column can move.

Two things the expanded reading does differently, neither of which changed a
verdict here. A macro defined in a header the file includes is still opaque
unless the branch pulls the header in itself. And a *nested* region resolves
against the file's own definitions rather than being read as its first branch,
which is the more honest question -- a nested region is one branch in any given
build -- but a different one.

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
loud refusal.

### Two exceptions that will bite

- **Never reformat a `` `define `` body.** The body is *text*. If the macro
  uses `` `" `` stringification, whitespace inside the body is observable in
  the program's output. Treat bodies as verbatim unless it can be proven that
  no stringification occurs — and it usually can't be, cheaply. Reindenting a
  continued body also violates invariant (3).
- **Escaped identifiers** (`` \foo.bar[3] ``) are terminated by whitespace, so
  their trailing whitespace is significant and cannot be collapsed. A lexer
  trap and a formatter trap both.

## Substitution

**Built.** The expanded mode. `scan` has already found every reference and
split its arguments, and the table already knows what each name means, so what
is left is the substitution and the provenance that makes the result
diagnosable.

### Rescanning by recursion, not by re-lexing

A macro body is contiguous text in a file, and so is an argument. So a
reference nested in either is delimited *in place* — against the same token
slice, with the same table — and expanded by recursing into the range that
holds it. Nothing is re-lexed and no intermediate token stream exists, which is
what keeps every token's spelling a real location in a real buffer.

The state of the art rescans differently, and has to: a preprocessor that
re-emits text produces text, and text has to be lexed again. The choice here
follows from emitting tokens, which is the same root as
[D9](plan.md#per-token-provenance).

What it costs is a call assembled out of two pieces of text — a name from one,
an argument list from another. Recorded in
[`limitations.md`](limitations.md); the corpus contains none.

### Scope and placement are different questions

Substituting a formal splices in text written at the *call site*, so the names
in it mean what they mean there: an identifier that happens to match a formal
of the macro being expanded is not that formal, and

```systemverilog
`define INNER(x) [x]
`define OUTER(x) `INNER(x + 1)
```

must not let `INNER`'s `x` capture what `OUTER` was passed. But those tokens
are *placed* by the outer expansion, which is what a message about them has to
say.

So the two are carried separately: a binding resolves through the caller's
frame while the expansion a token points at stays the current one. That is also
what makes a macro expanding to a macro read back as a chain of calls rather
than as a flattened result.

### The two operators make text that is in no file

``` `` ``` fuses the tokens either side of it; `` `" `` turns a stretch of body
into one string literal. Neither result is spelled anywhere, so both need a
buffer built for them — which is what `Origins::add_synthesised` is for, and
why both are recorded as expansions even where no `` `define `` directly
supplies them.

A paste resolves against the tokens *already emitted*, not against the body
text, because either side may itself be a formal or a nested call:
`` `define REG(n) reg_``n``_q `` pastes what the argument expanded to. The
result is re-lexed, since fusing is the point — `reg_` and `q` are two
identifiers apart and one identifier together.

## Following an `` `include ``

**Built.** Expanded mode only: the formatter never follows one
([D6](plan.md#4-decisions)), so raw mode sees a header's name and not its
contents.

### It is one file walked inside another

An included file is spliced in where the directive was, and walked **at the top
level** however deeply nested the include is. That is not a shortcut: the text
is a *file*, not substitution text. Its comments are its own, its tokens are
written where they are used, and nothing in it is a formal of whatever macro
the `` `include `` may have been written inside.

What placed it is therefore recorded on the *file* rather than on each token.
`Origins::add_included` keeps the `` `include `` that pulled it in and
`include_trace` walks back up the chain, which is the same shape as the
expansion chain and a different axis of it: a token has an expansion chain
saying which macros placed it and a file has an include chain saying how that
file was reached.

This is also what finally makes the table cross a file boundary. A header's
`` `define `` is in scope below the include, and a header sees the definitions
the file above it had already made — so the expanded path now builds one table
over the whole tree rather than one per file. That means walking a file token
by token against the running table rather than against a per-file scan, which
is what `expand` does; `scan` stays the per-file pass raw mode wants.

### Where it looks

22.4 gives the quoted form the including file's own directory and then an
implementation-defined search, and reserves the angle form for files the
implementation supplies. We supply none, so the angle list is empty until a
driver fills it. A `+incdir+`, a filelist, or `bender` is what fills either,
and that is driver work rather than preprocessor work.

An `` `include `` inside a macro body resolves **where the macro is used**, not
where it is written. That is the same rule `reported_at` applies to
`` `__LINE__ ``, and for the same reason: the directive is executed at the call
site, and the relative name the reader wrote is relative to the file they wrote
it in.

### The name may arrive by expansion

22.4's syntax allows only a quoted or an angled literal. Real code does not
stop there, and the corpus has

```systemverilog
`define include_file(f) `include `"f`"
```

— the name is a formal until the body is substituted, and a stringification
turns it into the literal the directive wants. So anything that is not a
literal is kept as `IncludePath::Expanded` and expanded before it is read.

Its extent is **one token**, not the rest of the line. A macro body that puts
an `` `include `` between an `` `ifdef `` and an `` `endif `` is the ordinary
way to write a conditional include, and the `` `endif `` is not part of the
file name.

### Cycles and depth

22.4 asks for at least 15 levels of nesting, so both a cycle check and a depth
limit are needed. The cycle check is the honest one: following a name that
would re-enter a file already open above it does nothing. It compares *cleaned
paths* — `.` and `..` resolved textually — because canonicalising means asking
the filesystem, and reading is deliberately behind a trait so that the
preprocessor can be tested without one. Two names that reach one file another
way still read as two, and the depth limit is the backstop for those.

Nothing is evaluated yet, so a header pulled in twice through two branches of
an `` `ifdef `` is read twice. Include guards are conditionals, which is the
next rung.

## Evaluating a conditional

**Built.** A conditional is not one directive: the five that build it only
mean anything together, and what they delimit is *text*. `conditional` nests
the flat directive stream into regions with branches, and both readings are
built on that one structure -- the expanded mode takes the branch that is
taken, raw mode keeps them all, because a formatter has to lay out code it
cannot evaluate and does not know what a build system will define.

### A branch not taken is not text

Its `` `define ``s never reach the table, its `` `include ``s are never
followed, and its macro references are never expanded -- an undefined macro
inside one is not an error, because nothing reads it. That is what makes an
include guard a guard, and it is why evaluating conditionals is what finally
let a header be pulled in twice without being read twice.

### A region does not cross a file boundary

A region is read within the one stretch of text it opens in, so an `` `ifdef ``
in a file and an `` `endif `` in a file it includes do not pair. That is a
reading rather than an omission: the `` `include `` that would join them sits
*inside* the region, so whether it is even followed is the question the region
was supposed to answer. An unclosed region runs to the end of its own text; an
`` `endif `` with nothing above it is consumed like any other directive. The
corpus has zero of either.

The same boundary applies to a macro body, and there it is load-bearing rather
than defensive. A body is substitution text, so the region in

```systemverilog
`define GUARD(x) `ifdef E x `endif
```

is the body's own, evaluated wherever the macro is used and against the table
as it stands there. Reading the scan token by token rather than region by
region is what makes that fall out: the directives are met where the text is
walked, and the text is walked where it is used.

### The oracle

`slang -E --comments` over the corpus, in
`crates/svirig-preproc/tests/differential.rs`. Both outputs are lexed and the
token sequences compared, comments included and whitespace dropped: a token a
macro placed brings no whitespace with it, so how much ends up between two
tokens is a property of whoever wrote them out rather than of the expansion.
Lexing says which differences matter without guessing, and still catches
separation that was needed and not written — it shows up as two tokens fused
into one.

**Nothing is filtered out any more.** Every file the reference will preprocess
is compared. Neither side is given an include path or a predefined macro, so a
file that needs one is a file the reference *declines* rather than one we skip,
and what would widen this further is a driver handing both sides what a build
actually passes.

| | conditionals | + `` `include `` | + expansion only |
| --- | --- | --- | --- |
| agree | 1843 | 1507 | 1502 |
| agree but for a reference defect | 153 | — | — |
| the reference declines | 3630 | 3260 | 2244 |
| skipped as not comparable | 0 | 859 | 1880 |

**The oracle is not infallible, and saying so is cheaper than pretending.** The
153 differ from us only where the reference is wrong, and both defects are
reproducible in three lines and confirmed against a *third* preprocessor, which
agrees with us:

- whitespace before a `\` continuation in a macro body leaks into the next
  `` `" `` in that body, so a stringified name comes back indented;
- a `//` comment in a continued macro body survives expansion, which 22.5.1
  says it may not — "comments shall not be considered part of the substitution
  text".

They are counted apart rather than excused quietly, and both counts are floors,
so a defect that widens moves a number instead of going unnoticed. Anything
else is still a disagreement and still fails.

**Widening the comparison is what found our own bugs**, which is the argument
for doing it. Two, both in code that had passed every targeted test: ``` `` ```
was deleting the whitespace around it — C's `##` rule, which fuses `force` onto
a signal name in a macro the corpus actually has — and a directive was
consuming the comment that followed its operands on the same line.

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
came from `` `FOO `` expanded at line 40, defined at line 12").

**Built, as `svirig-text`.** `Origins` holds every buffer -- files, files
reached through an `` `include ``, and buffers that ``` `` ``` or `` `" ``
synthesised, which are in no file at all. A `Span` is a byte range in one of
them. A `TokenOrigin` pairs the span a token's bytes live at with the
`Expansion` that placed it; expansions chain through a parent, so a macro
expanding to a macro reads back as a chain of calls, and `reported_at` walks it
to the outermost call -- the `` `FOO `` the reader actually wrote.

It records provenance **per token rather than per byte**, which is what makes a
macro argument ordinary instead of a special case. See
[D9](plan.md#per-token-provenance).

Diagnostic rendering is deliberately not in it yet: `trace` and `reported_at`
carry everything a renderer needs, and what to do with them waits for there
being a diagnostics layer to do it in.

## A table is built per file, and that is what it costs

Expansion starts from an empty table for each file, which is each file standing
as its own compilation unit (3.12.1). Raw mode now takes a seeded table too --
not to change a token it emits, but to know arities -- and the driver fills
that seed by expanding the file and keeping only the table the expansion ended
with. So `svirig parse -I ...` runs the whole expanded pipeline to learn six
numbers, and does it again for the next file.

Measured on `cc_fifo.sv`, six arities out of the two headers it
includes -- one of which includes a third:

| | |
| --- | --- |
| the file being parsed | 6,079 bytes, 990 raw tokens |
| headers it reaches | 27,372 bytes, 3,291 tokens, three files |
| tokens the expansion emits and we discard | 2,022 |
| reading and lexing the headers | 1.7 ms |
| the whole seeding pass | 3.4 ms |

Over half of it is reading and lexing bytes that are identical on every file of
the run. Across twenty files of one library the seeding costs about as much as
all twenty parses together, and every one of those computes the same table from
the same two headers.

### The obvious cache is unsound

One table per build, reused for every file, does not work. A table is not a
function of the build: one file includes two headers, its neighbour includes
none. And a header's contribution is not a function of the header either,
because the guard every real header opens with

```systemverilog
`ifndef COMMON_CELLS_ASSERTIONS_SVH
`define COMMON_CELLS_ASSERTIONS_SVH
```

is a top-level conditional over the table *at the include site*. The honest key
is the whole incoming table, and hashing it costs what the walk costs.

### What `slang` does

It caches the bytes and nothing above them. `SourceManager` holds one map from
canonical path to file data, shared across the compilation behind a lock, and
an include resolves through it; a path that failed to open is cached as a
failure too, so a header that is not on the search path is not re-probed once
per file. Each inclusion still gets a buffer entry of its own, so the bytes are
stored once while locations stay distinct per inclusion.

Above that it caches nothing. A pushed buffer gets a newly constructed lexer,
so a header is re-lexed on every inclusion, and every syntax tree gets a
preprocessor whose macro map starts empty -- for the soundness reason above.

What it offers instead is a *mode*: all inputs treated as one compilation unit,
one preprocessor walking every file in sequence, macros carrying from one file
to the next. The include guards then do the work a cache would have done, since
each header is read and expanded once for the whole run. A companion option
letting library files inherit macros is rejected unless that mode is on.

So the answer to recomputation is not a cache at all. It is the other reading
of 22.3.

### What follows for us

None of these is worth building until something consumes expanded mode. The
formatter never will ([D6](plan.md#4-decisions), [D15](plan.md#4-decisions)),
so the preprocessor stays as it is through M4.

Three levers, in the order they pay:

1. **A store shared across a run.** Unconditionally correct -- bytes are bytes
   and tokens are tokens, no table involved -- because the directive walk still
   runs per file against the same tokens. Nothing is skipped, only the
   re-reading. The cost is real: [D13](plan.md#4-decisions) gets thread safety
   by sharing nothing, and a session's lex cache is `Rc` precisely so it is
   built and dropped on one thread. Sharing means `Arc`, a reachable `Origins`,
   and an answer for `expand(&mut self)`.
2. **A single-unit mode.** The fork is already named where the table is
   created: carrying one file's definitions into the next is the other reading
   of 22.3 and wants a driver to say so. It is a thing real builds ask for, and
   it happens to make the seeding cost vanish at filelist scale. It needs the
   shared store to be worth anything, so it stacks with the first rather than
   replacing it.
3. **A directive-only walk**, following includes and evaluating conditionals
   without substituting or emitting. Saves the 2,022 discarded tokens and no
   more -- a constant factor, not a file-count one. The state of the art does
   not bother, which is mild evidence it is not where the money is.

Both of the first two want the build to be something a session *has* rather
than something each caller assembles; [`api.md`](api.md) has that shape. That
is the connection: the build is not a performance feature, but it is where a
shared store and a single-unit mode would both have to hang.

## Smaller things not to forget

- ~~`` `__FILE__ `` and `` `__LINE__ ``.~~ **Done.** Both answer for the
  *outermost* call site, which the origin map already computes: a
  `` `__LINE__ `` in a body reports the line the macro was used on. Their
  interaction with `` `line `` remains open, and `` `line `` occurs zero times
  ([`limitations.md`](limitations.md)).
- `` `pragma protect `` encrypted IP envelopes — verbatim passthrough, and a
  lexer mode.
- `` `begin_keywords `` / `` `end_keywords `` change the keyword set mid-file.
- ~~`` `"``, `` `\`" ``, and ``` `` ``` inside macro bodies (stringification and
  token pasting).~~ **Done.** See
  [the two operators](#the-two-operators-make-text-that-is-in-no-file).
- ~~Macros that expand to other macros, and recursion detection.~~ **Done.** A
  macro that reaches itself, directly or through others, stands as written;
  nothing else terminates.
- ~~A `` `define `` body line ending in `\` **inside a `//` comment** still
  continues.~~ **Done in the lexer.** The comment rule swallows the backslash,
  so reading the token stream naively ends the body a line early. Continuation
  wins over the comment. A newline inside a *block* comment does not end the
  body either (22.5.1), which needs no code: a block comment is one token.
- Numbers may contain whitespace: `8 'h FF` is legal. `1step` is one token.
- CRLF line endings are common from Windows-based EDA flows.
