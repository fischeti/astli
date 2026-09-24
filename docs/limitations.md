# Limitations

Conformance we chose to trade away, and what would justify closing each gap.
Unfinished grammar belongs in [`grammar-coverage.md`](grammar-coverage.md)
instead. Where a limitation can announce itself at runtime (a diagnostic, a
refusal), make it do that as well.

## Lexer

### Only the IEEE 1800-2023 keyword set

`` `begin_keywords `` lexes and round-trips, but its effect is ignored. It
never occurs in the corpus. **Revisit when** it shows up in real input. The
lookup already takes a `KeywordVersion`, so closing this means adding a table.
**Where** `svirig-syntax/src/keyword.rs`

### Token kinds have no external oracle

Round-trip proves every byte lands in one token, not that the kind is right.
Kind audits in `svirig-syntax/tests/lexer.rs` and the parser stand in for
one. **Revisit when** a wrongly kinded token reaches formatter output.

### No lexer modes

UDP `table` bodies and `` `pragma protect `` envelopes lex as ordinary code.
Their bytes survive, but the kinds inside are meaningless. The corpus has zero
tables and one envelope. **Revisit when** real input has either.
**Where** `svirig-syntax/src/kind.rs`

### Triple-quoted strings are not lexed

`"""…"""` (1800-2023) lexes as an empty string followed by its contents, so a
formatter could reflow it, and inside a `` `define `` its first newline ends the
body. It never occurs in the corpus. **Revisit when** one does.
**Where** `svirig-syntax/src/kind.rs`

### An escaped identifier must be followed by whitespace

`\foo` at the very end of a file with no newline does not lex as an
identifier. Such a file is malformed anyway; the only cost is a poor message.
**Where** `svirig-syntax/src/kind.rs`

## Preprocessor

### `` `line `` does not move reported line numbers

Operands are kept but have no effect. It never occurs in the corpus, even in
generated code. **Revisit when** it does. Closing this means a sorted
per-buffer list consulted by `line_col`. **Where** `svirig-text/src/origins.rs`

### Macro arity is guessed when unknown

Raw mode sees no definition for 95% of references. A `(` on the same line opens
an argument list unless a definition in scope says the macro is nullary. That
is wrong 12 times in the corpus, all from one `` `define WITH iff ``
([`preprocessor.md`](preprocessor.md#raw-mode-three-levels)). A wrong guess is
the wrong tree over the right bytes. `svirig parse -I/-D` seeds arities by
expanding first, and the driver assembles that seed by hand because `Session`
carries no `Build` yet. The formatter never seeds ([D15](plan.md#4-decisions)),
so for it the guess is permanent. **Revisit when** a misread call costs
formatting on real input. **Where** `svirig-parse/src/source.rs`,
`svirig/src/cmd/parse.rs`

### A directive with a defined end is given the whole line

`` `timescale ``, `` `default_nettype `` and four others take the rest of their
line as operands, so code following them on that line would sit inside the
`DIRECTIVE` node. Bytes survive. There are 16 occurrences in the corpus, none
followed by code. **Revisit when** something reads those operands.
**Where** `svirig-preproc/src/directive.rs`

### A macro call cannot be assembled from two pieces of text

In `` `define A(x) x(1) `` invoked as `` `A(`FOO) ``, `` `FOO `` is expanded as
nullary where it is written, and `(1)` is left as body text. Closing this means
rescanning a mixed-origin stream, which is a much larger machine. None in the
corpus. **Revisit when** one appears. **Where** `svirig-preproc/src/expand.rs`

### An `` `include `` cycle is caught by path, not identity

Paths are cleaned textually, because reading goes through `Reader` and cannot
assume a real filesystem. Symlinks and hard links read as different files, and
the depth limit of 200 is the backstop. **Revisit when** a real tree loops
through a symlink. **Where** `svirig-text/src/origins.rs`,
`svirig-preproc/src/include.rs`

### An `` `include `` name that expands is read as one token

`` `include `PATH(a, b) `` reads `` `PATH `` alone. Delimiting the argument
list would need the macro table where directives are parsed. None in the
corpus. **Revisit when** one appears. **Where** `svirig-preproc/src/directive.rs`

### A conditional region does not cross a file boundary

An `` `ifdef `` in a file and an `` `endif `` in a file it includes do not
pair. The include is inside the region, so whether it is followed at all is the
region's own question. None in the corpus. **Revisit when** real input does
this. **Where** `svirig-preproc/src/conditional.rs`

## Parser

### The verbatim fallback guesses which keywords open a body

`function`, `class`, `interface`, `property` and `sequence` only sometimes open
a body (`extern function`, `typedef class`, `virtual interface`, `assert
property`). Nearby tokens decide. A wrong guess is bounded: a mismatched closer
ends the run, so at most the enclosing construct goes verbatim. **Revisit when**
a file's verbatim rate stands out from its neighbours'.
**Where** `svirig-parse/src/verbatim.rs`

### A construct missing its closer falls back whole

When a shell never reaches its closer, the parser rolls back to the opening
keyword and the whole construct becomes one `VERBATIM` node, after its body was
already parsed. The closer can be missing because it is in a ragged region,
because a macro supplies it, or because the buffer is half-typed. Better:
speculate only over short ambiguous prefixes, commit once a construct's
keyword is consumed, and let the formatter decide what to do with a node that
has errors. **Revisit when** an editor or LSP needs structure from incomplete
buffers, or when the formatter shows it costing real input.
**Where** `svirig-parse/src/item.rs` (`close`, `generate_region`, `class`)

### A type is decided by shape, never resolved

Two names in a row are a declaration, and a `(` after the second makes it an
instantiation, because nothing else in the language is written that way. So
`nonexistent_t x;` parses silently, and `INSTANTIATION` does not say whether
it is a module or an interface. **Revisit when** something needs to know what a
name means. That is name resolution over a compilation unit.
**Where** `svirig-parse/src/decl.rs`

### Six constructs are left to the fallback on purpose

Concurrent assertions, `specify`, `covergroup`, `clocking`, `bind`, and the
inside of `constraint` have no rules. Together they make up about three
quarters of the remaining verbatim rate. Each is large and rare in RTL.
`constraint` gets a shell only because otherwise the fallback runs past its
`}`. **Revisit when** someone formats verification code in earnest.
**Where** `svirig-parse/src/item.rs`

### A parenthesised header is taken whole when its rule stops early

When the rule inside `if (…)`, `foreach (…)`, `@(…)` and similar stops before
the `)`, the rest is taken as plain tokens so that the node covers its own
parentheses. These tokens are neither verbatim nor understood: 0.3% of the
corpus. **Revisit when** that grows. **Where** `svirig-parse/src/stmt.rs`,
`svirig-parse/src/decl.rs`

### Region classification counts eight delimiter pairs, not thirteen

The same five keywords the fallback guesses about are left out, since their
prototype forms have no closer. A region handing a `function` across branches
is called live. A branch is parsed against a position bound, so the damage
stays inside the region. **Revisit when** such a region appears.
**Where** `svirig-parse/src/source.rs`

### An assignment is not an expression

`(a = b)` as a primary is legal but not parsed. Including `=` in the
precedence table would swallow the right-hand side of every assignment
statement. None in the corpus. **Revisit when** one appears.
**Where** `svirig-parse/src/expr.rs`

## Formatter

### Macro arguments are written as they were read

An argument is balanced text the grammar never parses, so `` `CHECK(a==b) ``
keeps `a==b`, and an argument written over several lines keeps its shape,
moved as a block. Only the space around each argument is the formatter's.
They are 3.0% of the corpus's tokens. **Revisit when** that share is the
largest left: parsing an argument that is one whole expression, and keeping
the rest as text, would close most of it. **Where**
`svirig-fmt/src/rules.rs`

### Small node kinds move as a block

`GENERATE_REGION` 0.3%, `INSIDE_EXPR` 0.2% and `STREAM_EXPR` have no rule, and
a `DIRECTIVE` 1.6% needs none: the fallback places it right, and a
`` `define `` body must stay byte for byte. `LITERAL_EXPR` 0.8% is literals
written in pieces, left as they are on purpose. **Revisit when** one of them
is the largest share left. **Where** `svirig-fmt/src/rules.rs`

### A long statement after `always_ff @(…)` aligns far right

It breaks inside its expression, aligned under the chain's start: a chain
with no opener has no indented fallback, and the `begin`/`end` the guide wants
are tokens the formatter may not add. **Revisit when** a chain gets an
indented form. **Where** `svirig-fmt/src/rules.rs`

### A modport's ports are not a table

Broken one per line, they are not aligned as a module's ports are. **Revisit
when** a corpus diff shows it. **Where** `svirig-fmt/src/rules.rs`

### A broken assignment pattern's keys are not padded

Values do not line up under each other. **Revisit when** a corpus diff shows
it. **Where** `svirig-fmt/src/rules.rs`

## Driver

### A filelist carries four things

Sources, `+incdir+`, `+define+` and nested `-f`/`-F`. `-y`, `-v`, `+libext+`
and paths containing whitespace are rejected by name rather than skipped.
Library lookup is elaboration's job. **Revisit when** something can resolve a
module name to a file. **Where** `svirig/src/filelist.rs`

### `-D` and `+define+` on one command line ignore their relative order

The argument parser does not report argv positions, so the dash form always
wins. It only matters when one command line defines the same name both ways.
**Revisit when** the parser reports positions. **Where** `svirig/src/sources.rs`

### A parallel run holds a wave of files in memory

Output is ordered, so each file's output waits for the ones named before it.
Waves are sized against a 64 MB budget, which was picked rather than measured.
**Revisit when** a run holds more than printed output, as the formatter's
rewritten buffers will. **Where** `svirig/src/cmd/mod.rs`
