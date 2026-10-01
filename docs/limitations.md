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
**Where** `astli-syntax/src/keyword.rs`

### Token kinds have no external oracle

Round-trip proves every byte lands in one token, not that the kind is right.
Kind audits in `astli-syntax/tests/lexer.rs` and the parser stand in for
one. **Revisit when** a wrongly kinded token reaches formatter output.

### No lexer modes

UDP `table` bodies and `` `pragma protect `` envelopes lex as ordinary code.
Their bytes survive, but the kinds inside are meaningless. An expansion makes
an encrypted envelope trivia ([D21](plan.md#4-decisions)) and the formatter
leaves a file with one as it is, but a raw tree still holds its ciphertext as
code. The corpus has zero tables and one envelope, not encrypted. **Revisit
when** real input has either.
**Where** `astli-syntax/src/kind.rs`

### Triple-quoted strings are not lexed

`"""…"""` (1800-2023) lexes as an empty string followed by its contents, so a
formatter could reflow it, and inside a `` `define `` its first newline ends the
body. It never occurs in the corpus. **Revisit when** one does.
**Where** `astli-syntax/src/kind.rs`

### An escaped identifier must be followed by whitespace

`\foo` at the very end of a file with no newline does not lex as an
identifier. Such a file is malformed anyway; the only cost is a poor message.
**Where** `astli-syntax/src/kind.rs`

## Preprocessor

### `` `line `` does not move reported line numbers

Operands are kept but have no effect. It never occurs in the corpus, even in
generated code. **Revisit when** it does. Closing this means a sorted
per-buffer list consulted by `line_col`. **Where** `astli-text/src/origins.rs`

### Macro arity is guessed when unknown

Raw mode sees no definition for 95% of references. A `(` on the same line opens
an argument list unless a definition in scope says the macro is nullary. That
is wrong 12 times in the corpus, all from one `` `define WITH iff ``
([`preprocessor.md`](preprocessor.md#raw-mode-three-levels)). A wrong guess is
the wrong tree over the right bytes. `astli parse -I/-D` seeds arities by
expanding first, and the driver assembles that seed by hand because `Session`
carries no `Build` yet. The formatter never seeds ([D15](plan.md#4-decisions)),
so for it the guess is permanent. **Revisit when** a misread call costs
formatting on real input. **Where** `astli-parse/src/source.rs`,
`astli/src/cmd/parse.rs`

### A directive with a defined end is given the whole line

`` `timescale ``, `` `default_nettype `` and four others take the rest of their
line as operands, so code following them on that line would sit inside the
`DIRECTIVE` node, and an expansion would keep it as trivia that no rule reads
([D20](plan.md#4-decisions)). Bytes survive. There are 16 occurrences in the
corpus, none followed by code. **Revisit when** something reads those
operands. **Where** `astli-preproc/src/directive.rs`

### A directive a macro writes is dropped from the expansion

Only one written in a file is kept ([D20](plan.md#4-decisions)): a macro's
would carry its body's line continuations. None of the corpus's 24 kept
directives is in a macro body. **Revisit when** one is. **Where**
`astli-preproc/src/expand.rs`

### A macro call cannot be assembled from two pieces of text

In `` `define A(x) x(1) `` invoked as `` `A(`FOO) ``, `` `FOO `` is expanded as
nullary where it is written, and `(1)` is left as body text. Closing this means
rescanning a mixed-origin stream, which is a much larger machine. None in the
corpus. **Revisit when** one appears. **Where** `astli-preproc/src/expand.rs`

### An `` `include `` cycle is caught by path, not identity

Paths are cleaned textually, because reading goes through `Reader` and cannot
assume a real filesystem. Symlinks and hard links read as different files, and
the depth limit of 200 is the backstop. **Revisit when** a real tree loops
through a symlink. **Where** `astli-text/src/origins.rs`,
`astli-preproc/src/include.rs`

### An `` `include `` name that expands is read as one token

`` `include `PATH(a, b) `` reads `` `PATH `` alone. Delimiting the argument
list would need the macro table where directives are parsed. None in the
corpus. **Revisit when** one appears. **Where** `astli-preproc/src/directive.rs`

### No macro a tool predefines is defined

The standard has tools predefine some macros, such as the coverage
constants of §20.14 (`` `SV_COV_CHECK ``), and simulators add their own.
Expansion starts from an empty table and the build's defines, so these are
undefined. None in the corpus. **Revisit when** one appears; a build can
define them meanwhile. **Where** `astli-preproc/src/session.rs`

### A conditional region does not cross a file boundary

An `` `ifdef `` in a file and an `` `endif `` in a file it includes do not
pair. The include is inside the region, so whether it is followed at all is the
region's own question. None in the corpus. **Revisit when** real input does
this. **Where** `astli-preproc/src/conditional.rs`

### Every `` `include `` reads and lexes its file again

Each inclusion is a new buffer, in the same session and across the sessions of
a parallel run, even when an include guard then skips it all. Expanding
opentitan re-lexes about 12 MB of headers, some 50 ms of 0.66 s single-threaded;
walking their `` `define ``s into each unit's table costs more, and a cache
cannot skip that. `fmt` never follows includes. A shared store would hold each
path's text, lines and tokens behind an `Arc`, with placement left per session.
**Revisit when** an LSP needs one file store across units, or a UVM-heavy run
shows lexing matters. **Where** `astli-text/src/origins.rs`,
`astli-preproc/src/expand.rs`

## Parser

### The verbatim fallback guesses which keywords open a body

`function`, `class`, `interface`, `property` and `sequence` only sometimes open
a body (`extern function`, `typedef class`, `virtual interface`, `assert
property`). Nearby tokens decide. A wrong guess is bounded: a mismatched closer
ends the run, so at most the enclosing construct goes verbatim. **Revisit when**
a file's verbatim rate stands out from its neighbours'.
**Where** `astli-parse/src/verbatim.rs`

### A construct missing its closer falls back whole

When a shell never reaches its closer, the parser rolls back to the opening
keyword and the whole construct becomes one `VERBATIM` node, after its body was
already parsed. The closer can be missing because it is in a ragged region,
because a macro supplies it, or because the buffer is half-typed. Better:
speculate only over short ambiguous prefixes, commit once a construct's
keyword is consumed, and let the formatter decide what to do with a node that
has errors. **Revisit when** an editor or LSP needs structure from incomplete
buffers, or when the formatter shows it costing real input.
**Where** `astli-parse/src/item.rs` (`close`, `generate_region`, `class`)

### A type is decided by shape, never resolved

Two names in a row are a declaration, and a `(` after the second makes it an
instantiation, because nothing else in the language is written that way. So
`nonexistent_t x;` parses silently, and `INSTANTIATION` does not say whether
it is a module or an interface. **Revisit when** something needs to know what a
name means. That is name resolution over a compilation unit.
**Where** `astli-parse/src/decl.rs`

### `specify` is left to the fallback on purpose

It has no rule. Timing paths and their delays are written for gate-level
netlists and cell libraries, and no source file in the corpus has one.
Each is large and rare in RTL. **Revisit when** someone formats verification
code in earnest.
**Where** `astli-parse/src/item.rs`

### A parameter's value is one expression, never `min:typ:max`

`specparam tRise = 1:2:3;` leaves `:2:3` to the fallback, though any
parameter's value may take that form. A declarator is shared with variables,
whose value is never one. None in the corpus. **Revisit when** one appears.
**Where** `astli-parse/src/decl.rs`

### Region classification counts eight delimiter pairs, not thirteen

The same five keywords the fallback guesses about are left out, since their
prototype forms have no closer. A region handing a `function` across branches
is called live. A branch is parsed against a position bound, so the damage
stays inside the region. **Revisit when** such a region appears.
**Where** `astli-parse/src/source.rs`

### Nesting past 256, and a tree past 2048 deep, is left as written

Rules recurse once per level, so past 256 open nodes the construct at the
cursor becomes one flat `VERBATIM` with a `nested-too-deep` warning. A long
left-associative chain nests without recursing, so the builder also flattens
anything below depth 2048 into a `VERBATIM`, with the same warning, wherever
it falls, even where `astli.ungram` names no `Verbatim`. The corpus peaks at
584 (a generated `|` chain). **Revisit when** real code reaches either limit.
**Where** `astli-parse/src/lib.rs` (`MAX_NESTING`), `astli-parse/src/build.rs`
(`MAX_DEPTH`)

## Formatter

### A chain of postfixes or mixed operators 2048 deep overflows the formatter

A chain of one operator is walked in a loop, but `a.b.c…`, `a[0][0]…`,
`f()()…` and `a + b - c + …` recurse once per link. On the 2 MB a thread gets,
a release build overflows somewhere past 1000 links, and on the 1 MB of the
Windows main thread, which formats a single file, past about half that. Real
code chains a handful. **Revisit when** a real file does, or the formatter runs
where a crash costs more than one command, such as a language server.
**Where** `astli-fmt/src/rules.rs`

### A generate block named before its `begin` is laid out as a statement

`if (P) gen_a : begin ... end else ...` puts `else` on the line after `end`,
since the rule for `if` sees a labelled statement, not a block. Neither the
corpus nor sv-tests names a generate block that way. **Revisit when** real
code does. **Where** `astli-fmt/src/rules.rs`

### The formatter takes no options

Width 100 and indent 2 are constants, alignment is always on, and `format`
takes nothing but the tree. **Revisit when** someone needs another value:
[D7](plan.md#4-decisions) names the three knobs to add, and no others.
**Where** `astli-fmt/src/lib.rs`

### Some parentheses, argument lists and calls are left unformatted

`PAREN_EXPR` 0.4%, `ARG_LIST` 0.3% and `CALL_EXPR` 0.1% of the corpus's tokens
are written as they were read where their rules give up on a shape.
**Revisit when** they are the largest share left: find which shapes first,
then write rules for them. **Where** `astli-fmt/src/rules.rs`

### Macro arguments are written as they were read

An argument is balanced text the grammar never parses, so `` `CHECK(a==b) ``
keeps `a==b`, and an argument written over several lines keeps its shape,
moved as a block. Only the space around each argument is the formatter's.
They are 3.0% of the corpus's tokens. **Revisit when** that share is the
largest left: parsing an argument that is one whole expression, and keeping
the rest as text, would close most of it. **Where**
`astli-fmt/src/rules.rs`

### A `` `define `` with a `\` that cannot move keeps its `\`s

A `\` inside `` `"…`" ``, right after ``` `` ``` or right after a line comment
cannot move without changing the tokens or the expansion, and its
`` `define `` is then left as written, so its other `\`s do not line up
around a fixed one. 12 of 1745 continued `` `define ``s in the corpus, all
with a comment. **Revisit when** a style lines up the rest around it.
**Where** `astli-fmt/src/verbatim.rs`

### Small node kinds move as a block

`GENERATE_REGION` 0.3%, `INSIDE_EXPR` 0.2% and `STREAM_EXPR` have no rule, and
a `DIRECTIVE` 1.6% needs none: the fallback places it right, and a
`` `define `` body must stay byte for byte. `LITERAL_EXPR` 0.8% is literals
written in pieces, left as they are on purpose. **Revisit when** one of them
is the largest share left. **Where** `astli-fmt/src/rules.rs`

### A long statement after `always_ff @(…)` aligns far right

It breaks inside its expression, aligned under the chain's start: a chain
with no opener has no indented fallback, and the `begin`/`end` the guide wants
are tokens the formatter may not add. **Revisit when** a chain gets an
indented form. **Where** `astli-fmt/src/rules.rs`

### A modport's ports are not a table

Broken one per line, they are not aligned as a module's ports are. **Revisit
when** a corpus diff shows it. **Where** `astli-fmt/src/rules.rs`

## Lint

### Rules that need a list from the project are left out

verible's `forbidden-macro` and `banned-declared-name-patterns` check against
names a project supplies, and `astli.toml` sets only levels. **Revisit when**
a project asks; it means options per rule, `[lint.rules.<name>]`.

### Rules that need more than the raw tree are left out

`disable-statement` asks whether a label names a `fork`, which is name
resolution (M9). `macro-string-concatenation` reads a `` `define ``'s body,
which is never parsed. `mismatched-labels` checks what the standard makes an
error, so it belongs to the parser. **Revisit when** their layer exists.

### `signal-name-style` allows a name ending in `_` and a number

lowRISC's guide forbids `foo_1`, since synthesis names a bus's nets that way;
verible's rule of the same name does not check it, and astli keeps the name's
meaning. **Revisit when** someone asks, as a rule of its own.

## Semantics

### Only the head of a dotted name is resolved

In `a.b.c`, `a` resolves and the rest does not: a struct's member needs
its type, an instance's needs the instance tree. A class's members are not
modelled at all ([S9](sema.md#decisions)). **Revisit when** types exist
(M10). **Where** `astli-sema/src/resolve.rs`

### Declaration order is not checked

A name declared after its use in the same scope resolves, which the
standard allows only for some names. It finds more than a compiler would,
never less. **Revisit when** `check` wants the error.

### `unused-import` does not tell imports of one package apart

A use of a package's member keeps every import of that package in the file
quiet, though only one of them brings it into scope. Accesses do not record
which import a name resolved through. **Revisit when** a file with two
modules importing one package shows it.

### `$unit` is the file's own

Each file is its own compilation unit ([D17](plan.md#4-decisions)), so a
name declared at the top of one file is not seen from another.
**Revisit when** one unit over all files is built.

## Index

### A reference is a name in a place, not a resolved name

The head of a type and the name left of `::` count as references even where
they name a local typedef or class, so a file declaring a module of the same
name is kept without need. The error only ever keeps a file, but `pickle`
renames such a local name along with the declaration it shares a name with.
**Revisit when** name resolution exists. **Where** `astli-index/src/names.rs`

### A file that declares nothing is never needed

`--top` drops a file of `` `define ``s alone. Each file is its own unit
([D17](plan.md#4-decisions)), so its definitions reached no other file
anyway. **Revisit when** one compilation unit over all files is built.
**Where** `astli-index/src/index.rs`

## Driver

### A filelist carries four things

Sources, `+incdir+`, `+define+` and nested `-f`/`-F`. `-y`, `-v`, `+libext+`
and paths containing whitespace are rejected by name rather than skipped.
Library lookup is elaboration's job. **Revisit when** something can resolve a
module name to a file. **Where** `astli/src/filelist.rs`

### `-D` and `+define+` on one command line ignore their relative order

The argument parser does not report argv positions, so the dash form always
wins. It only matters when one command line defines the same name both ways.
**Revisit when** the parser reports positions. **Where** `astli/src/sources.rs`

### A raw pickle's `` `undefineall `` may clear a compiler's own definitions

Each file ends with it, so that the next starts from the `+define+`s alone,
as its own unit would. The standard clears what `` `define `` defined, and a
compiler may count its command line's `+define+`s among them. **Revisit
when** a design pickled raw needs a definition given where it is compiled.
**Where** `astli-cli/src/cmd/pickle.rs`

### `pickle` holds every file's tree until it writes

With `--expand`, which names to rename is known only once every file is
summarized, so each tree is kept, as a green node that can cross threads,
rather than expanded twice. `cheshire` writes 430 of its 578 files, 9 MB.
**Revisit when** a design is too large for that. **Where**
`astli-cli/src/cmd/pickle.rs`

### A parallel run holds a wave of files in memory

Output is ordered, so each file's output waits for the ones named before it.
Waves are sized against a 64 MB budget, which was picked rather than measured.
**Revisit when** a run holds more than printed output, as the formatter's
rewritten buffers will. **Where** `astli/src/cmd/mod.rs`
