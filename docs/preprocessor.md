# Preprocessing

A compiler sees preprocessed text. A formatter never does: it has to lay out
source that may not be valid under any single set of definitions, and leave
its preprocessed meaning unchanged under *every* set at once.

## Two output modes

`svirig-preproc` produces two token streams from the same machinery.

| | **Raw** | **Expanded** |
| --- | --- | --- |
| Consumer | formatter, in-buffer LSP requests | compiler, semantic LSP requests |
| Macros | `MACRO_CALL` atoms | expanded |
| `` `include `` | not followed | followed |
| Conditionals | every branch, as regions | evaluated |
| Origin map | not needed | per token ([D9](plan.md#4-decisions)) |

An LSP needs both. Folding, semantic tokens, document symbols and formatting
cannot come from the expanded stream: it has dropped branches and inlined
other files. The raw tree is the spine, joined to analysis through the origin
map.

## Raw mode: three levels

**A — directives are structure, not trivia.** A directive owns its line, with
`\` continuations folded in. Preprocessing depends only on the token sequence
and line ownership, so whitespace changes that keep both are invisible to it.

**B — macro references are grammar atoms.** UVM code puts calls in item,
member, statement, expression, port and type position, so a `MACRO_CALL`
(`TICK_IDENT` plus an optional `MACRO_ARG_LIST`) is admissible in all of them,
including inside a `VERBATIM` run. Arguments are balanced token soup, never
parsed as expressions. Four in five `` ` `` tokens in the corpus are macro
references.

A call's shape depends on arity: `` `FOO (a + b) `` is a call if `FOO` has
formals and a nullary reference plus an expression if not. So raw mode tracks
`` `define `` and `` `undef `` without expanding anything. Arity is unknown for
95% of references, since their definitions are in headers D6 never follows. The
rule for those: **a `(` anywhere on the same line opens an argument list,
unless a definition in scope says the macro is nullary.** Requiring the `(` to
touch the name misreads 256 calls, because house styles align them
(`` `uvm_field_int   (x, UVM_DEFAULT) ``). The same-line rule misreads 12, all
one `` `define WITH iff `` in one file. It is also wrong in the cheaper
direction, because a spurious argument list still reproduces its bytes.

**C — conditionals are structured regions.** `CONDITIONAL_REGION` holds every
`CONDITIONAL_BRANCH`. A region is **live** when each branch balances its
delimiters on its own. A live region's branches are each parsed in the
enclosing context. A **ragged** one is left to the fallback, except for
directives and declarations inside it, which are self-contained. 96.4% of
1366 corpus regions are live. The measurement is `examples/conditionals.rs`,
and it gives the same figure with macros expanded. The ragged ones are three
idioms, and in 48 of 49 every branch opens the same thing that closes after
`` `endif ``, so a later version could format them.

```systemverilog
`ifdef SYNTHESIS
  always_comb begin        // ragged: `begin` is closed after `endif
`else
  always_ff @(posedge clk) begin
`endif
```

## The transparency invariant

> For every macro environment *E*, `preprocess(format(src), E) ==
> preprocess(src, E)`.

This follows from three properties checkable on one file without any
environment:

1. the raw token sequence, directives included and whitespace excluded, is
   unchanged;
2. every directive still owns its line;
3. no `\` continuation boundary moved.

Assert it inside `format`. It costs one re-lex and turns silent corruption into
a refusal. Two traps:

- **Never reformat a `` `define `` body.** `` `" `` makes whitespace inside it
  observable, and reindenting a continued body breaks (3).
- **Escaped identifiers** end at whitespace, which is therefore significant.

## Expanded mode

**Rescanning by recursion.** A body and an argument are each contiguous text in
some buffer. So a nested reference is delimited in place and expanded by
recursing into its range. Nothing is re-lexed, and every token's spelling
stays a real location. The cost: a call cannot be assembled from two pieces of
text ([limitation](limitations.md#a-macro-call-cannot-be-assembled-from-two-pieces-of-text)).

**Scope and placement are separate.** Substituted argument tokens resolve names
through the caller's frame, so `` `define OUTER(x) `INNER(x + 1) `` cannot let
`INNER`'s `x` capture them. They are still placed by the current expansion,
which is what a diagnostic reports.

**` `` ` and `` `" `` make text in no file.** The result goes into a synthesised
buffer (`Origins::add_synthesised`). A paste resolves against tokens already
emitted, since either side may be a formal or a call, and is re-lexed.

**Includes** are walked at top level wherever the directive is, because the
text is a file and not substitution text. The include chain is recorded per
file (`include_trace`); the expansion chain is recorded per token. One macro
table spans the whole include tree. Quoted names search the including file's
directory first. An include inside a macro body resolves where the macro is
used. A name that is not a literal is expanded first, and is taken as one
token. Cycles are caught by cleaned path, with a depth limit of 200 as the
backstop.

**Conditionals.** A branch not taken is not text: its defines never register,
its includes are never followed, and an undefined macro inside it is no error.
That is what makes include guards work. A region does not cross a file
boundary, and a macro body's region is evaluated where the macro is used.

**Recoveries.** Each one keeps the surrounding tokens rather than guessing at
intent, and reports a diagnostic (`svirig-preproc/src/diagnostics.rs`):

| Problem | Recovery |
| --- | --- |
| undefined macro; call missing its required arguments | the reference's tokens stand |
| too many arguments | extras dropped |
| formal with no argument and no default | expands to nothing |
| a macro that reaches itself | stands as written |
| unclosed `` `" `` | quotes to the end of the body |
| ` `` ` with nothing on one side | operator dropped |
| include not found, cyclic, or deeper than 200 | directive expands to nothing |
| conditional with no name | branch never taken |
| region with no `` `endif `` | runs to the end of its text |
| stray `` `endif `` / `` `else `` | consumed |

In raw mode, an undefined macro is the normal case and is not reported.

## The oracle

`svirig-preproc/tests/differential.rs` compares token sequences against
`slang -E --comments` over the corpus, comments included and whitespace
dropped. Nothing is filtered out. Files `slang` declines for want of include
paths or defines are counted, not skipped. 1843 files agree, and 153 differ
only through two `slang` defects that a third preprocessor confirms: whitespace
before a continuation leaking into a later `` `" ``, and a `//` comment
surviving in a continued body. Widening the comparison found two real bugs that
targeted tests had missed, which is the argument for running the oracle
against everything.

## Cost of a table per file

Each file starts from an empty table, as its own compilation unit (3.12.1).
`svirig parse -I` seeds raw mode's arities by running a full expansion and
keeping only the final table. On a typical file that is 3.4 ms, over half of
it re-reading the same headers. Caching the table per build is unsound,
because include guards make a header's effect depend on the table at the
include site. The levers are a byte store shared across a run and a
single-compilation-unit mode. Neither is worth building until something
consumes expanded mode.
