# Limitations

Conformance we chose to trade away, and what would justify closing each gap.
Unfinished grammar belongs in [`grammar-coverage.md`](grammar-coverage.md)
instead. Where a limitation can announce itself at runtime (a diagnostic, a
refusal), make it do that as well.

## Lexer

### `` `begin_keywords `` holds within one file

The lexer keeps the stack of versions as it reads a buffer, so a version a
file names does not reach a file it includes, or the next file of a
compilation unit, as the standard has it. A version it does not define is
read as the one in force, without an error. **Revisit when** real input
relies on either. **Where** `astli-syntax/src/lexer.rs`

### Token kinds have no external oracle

Round-trip proves every byte lands in one token, not that the kind is right.
Kind audits in `astli-syntax/tests/lexer.rs` and the parser stand in for
one. **Revisit when** a wrongly kinded token reaches formatter output.

### A `` `pragma protect `` envelope lexes as code

An encrypted envelope lexes as ordinary code: its bytes survive, but the
kinds inside are meaningless. An expansion makes it trivia
([D21](plan.md#4-decisions)) and the formatter leaves a file with one as it
is, but a raw tree still holds its ciphertext as code. So does a UDP's table
a macro writes, whose `endtable` may be in another macro or in none. The
corpus has one envelope, not encrypted, and no table. **Revisit when** real
input has either.
**Where** `astli-syntax/src/lexer.rs`

## Preprocessor

### Diagnostics report physical positions

`` `line `` renumbers what `` `__LINE__ `` and `` `__FILE__ `` expand to, but
diagnostics, the tree and every other position stay where the text physically
is, so a user can open the file at the line reported. It never occurs in the
corpus, even in generated code. **Revisit when** a generator relies on it to
point diagnostics at its own source. **Where** `astli-text/src/origins.rs`

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

### Only the standard's predefined macros are defined

Expansion starts from the coverage constants of §40.3.1
(`` `SV_COV_CHECK `` and the rest) and the build's defines. What a simulator
adds of its own, such as `` `VERILATOR `` or `` `XCELIUM ``, is undefined,
so an `` `ifdef `` on one takes its `` `else ``. **Revisit when** a
design needs one; a build can define it meanwhile. **Where**
`astli-preproc/src/build.rs`

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

### The formatter indents with spaces only

Alignment pads with spaces, and tabs would bring a tab width into every
width it measures; the lowRISC guide asks for spaces anyway. **Revisit when**
a project asks: tabs to indent, spaces to align. **Where**
`astli-fmt/src/doc.rs`

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

`macro-string-concatenation` reads a `` `define ``'s body, which is never
parsed. `mismatched-labels` checks what the standard makes an error, so it
belongs to the parser. **Revisit when** their layer exists.

### `signal-name-style` allows a name ending in `_` and a number

lowRISC's guide forbids `foo_1`, since synthesis names a bus's nets that way;
verible's rule of the same name does not check it, and astli keeps the name's
meaning. **Revisit when** someone asks, as a rule of its own.

## Semantics

### A class's body is opaque

A class is a declared type, and nothing in its body is lowered
([S9](sema.md#decisions)): its members resolve nowhere, `check` reports
nothing inside it, and each name it spells may be a use of anything so
named. In UVM code that is most names, so a signal a testbench reads through
a virtual interface is never reported unused. The constraints of an
object's `randomize() with {…}` are opaque too, since they name its
members first ([S40](sema.md#decisions)), and so are a method and a
constraint defined outside their class, `C::f`. **Revisit when** a rule needs
class members, or `check` is to run on testbenches. **Where**
`astli-sema/src/lower.rs`

### Nothing in a property is typed or sized

Its names resolve and are read ([S37](sema.md#decisions)), but no width
lint compares inside it, an instance's arguments are not matched to its
formals, and a formal typed `sequence` or `property` has no type sema
models. **Revisit when** a rule needs what a property means, or `check` is
to report what a compiler rejects in one. **Where**
`astli-sema/src/lower/body.rs`

### Nothing in a covergroup is evaluated

Its names resolve and are read ([S39](sema.md#decisions)), but a bin's
values, ranges and transitions are not evaluated, so no bin is checked
against what its cover point can take, and a bin is no name: `cp.low` in
a cross's `binsof` resolves nothing past `cp`. A covergroup sampled by
`@@(begin f)` leaves its event opaque, and one that `extends` another is
opaque whole, as the class body it stands in is. **Revisit when** a rule
checks coverage, or `check` is to report what a compiler rejects in a
covergroup. **Where** `astli-sema/src/lower.rs`

### A clocking block's signal is found only beside it

`cb.name` resolves to what the block names it for only in the block's own
file, after the block ([S38](sema.md#decisions)); through an interface
port or a virtual interface, `bus.cb.valid`, it is unknown, so a testbench
driving an interface through its clocking block drives nothing sema sees.
**Revisit when** a drive through a clocking block is to be checked from
another file. **Where** `Resolver::hierarchical`

### A declaration a macro wrote gets no name lint

Its user can change it only in the macro ([S10](sema.md#decisions)), often
a library's, so `unused-*` and `undriven-signal` skip it. A macro that declares a signal no one reads goes unreported.
**Revisit when** a project asks for the macro's definition to be reported
once instead. **Where** `astli-lint/src/rules/`

### A name spelled anywhere sema cannot see counts as a use everywhere

Whether another file reads a signal through a `bind`, a class, or a
hierarchical name that does not resolve ([S30](sema.md#decisions)), such
as one going up the tree, is known only by name: any file spelling `sig`
in an opaque region, or naming `up.sig`, keeps every signal called `sig`
from `unused-signal` and `undriven-signal`. **Revisit when** names up the
instance tree resolve against the tree `--top` builds. **Where**
`Linter::maybe_used`

### `check` does not know which generate branches exist

Without a top, an instance in a generate construct may never be built, so
`check` looks for no definition of it, and what its connections get wrong
is a warning ([S13](sema.md#decisions)); `check --top` knows for what its
tops build ([S26](sema.md#decisions)). **Revisit when** the tops are known
without being named: a package's manifest (M10).

### A generate loop's block is not found by name from outside it

`g_loop[0].sig` from beside the loop is unknown: the loop's label is
declared in its header's scope, where the genvar is, not in the scope
around it ([S30](sema.md#decisions)). So is a member of an unnamed
generate block, which no path names but `genblk`. A struct's member has a
type, which `Elaboration::expr_type` gives, and no symbol. **Revisit
when** a lint misses a signal read this way. **Where**
`astli-sema/src/lower.rs`

### Declaration order is not checked

A name declared after its use in the same scope resolves, which the
standard allows only for some names. It finds more than a compiler would,
never less. **Revisit when** `check` wants the error.

### `unused-import` does not tell imports of one package apart

A use of a package's member keeps every import of that package in the file
quiet, though only one of them brings it into scope. Accesses do not record
which import a name resolved through. **Revisit when** a file with two
modules importing one package shows it.

### A select is compared only where it evaluates

A write to `x[i]` by a genvar is placed per iteration in what `--top`
builds, but at a definition's defaults, where the lint reads it, its bits
are unknown, so it clashes only with a write to all of `x`, and a read of
`x[sel]` keeps `x`'s bits from being compared at all
([S31](sema.md#decisions)). A read of any part of a signal still counts as
using it for `unused-signal`. **Revisit when** the lint reads what `--top`
builds. **Where** `astli-lint/src/rules/drivers.rs`

### `check --top` builds nothing for sure below a definition two files declare

Its generate blocks are undecided and its drivers not compared
([S28](sema.md#decisions)), and so for everything below it, though every
copy may build the same. **Revisit when** a design's filelist, or a
package's manifest, says which one it keeps. **Where**
`astli-sema/src/built.rs`

### Width lints compare a statement's widths, not an expression's

`width-expand` and `width-trunc` compare an assignment's sides, a port and
its connection, and a comparison's operands ([S29](sema.md#decisions)). An
operand of `a & b` narrower than the other, or an arm of `?:`, is not
reported on its own, as Verilator would; neither is an instance array's
connection, which may be split across its elements. **Revisit when** a
design's bug hides in one. **Where** `astli-lint/src/widths.rs`

### Dynamic arrays, queues, associative arrays and unpacked unions have no value

Their type is unknown, and so is a constant computed from one
([S21](sema.md#decisions)). **Revisit when** a design's constant needs one.
**Where** `astli-sema/src/elab.rs`

### A genvar has a value only in the tree

A parameter in a generate loop that reads its genvar is unknown at a
definition's defaults, outside any iteration; in the tree, each iteration
gives it one ([S23](sema.md#decisions)). **Revisit when** a reader without
a top needs it. **Where** `astli-sema/src/elab.rs`

### The tree has no `defparam`, `config` or `bind` into chosen instances

A `defparam`'s override, a `config`'s library choices, and an instance of a
checker a package declares or a procedure holds are in regions the HIR
leaves opaque, so the tree has none of them. A `bind` into the instances a
list or a path chooses is lowered, but not in the tree, since a body is
shared by siblings that lack it ([S35](sema.md#decisions)). The corpus has
no `defparam` or `config`, and 20 of its 712 `bind`s choose. **Revisit
when** a design needs one in its tree. **Where** `astli-sema/src/lower.rs`,
`astli-sema/src/elab/tree.rs`

### Nothing is known through an interface port

A body is keyed by its own parameters' values alone, so a module sees
nothing of the interface instance connected to its port: `bus.data` has no
type, and a width or select through it is silent ([S3](sema.md#decisions)).
A `bus_if #(.W(16))` connected to a module that assigns `bus.data` to 8
bits is not reported. **Revisit when** a design's interfaces carry widths
a lint should see: key a body by the interface bodies connected to it too.
**Where** `astli-sema/src/elab/tree.rs`

### An unnamed generate block after a region left as written may be misnumbered

A generate construct the parser kept as written is not counted, so an
unnamed block after it is named `genblk` and too small a number
([S25](sema.md#decisions)). **Revisit when** a hierarchical name through one
needs resolving. **Where** `astli-sema/src/elab/tree.rs`

### A generate loop builds at most 65,536 blocks

Past that, or past 4M blocks in one elaboration, the loop is undecided
([S24](sema.md#decisions)), so a loop that never ends costs no more.
**Revisit when** a design needs more. **Where** `astli-sema/src/elab/tree.rs`

### A `` `timescale `` reaches only the rest of its file

A file that sets no time unit takes 1ns/1ns ([S41](sema.md#decisions)),
where a compiler reading a filelist takes the last `` `timescale `` of a
file before it, so a testbench relying on another file's `1ns/1ps` gets
`1.5ns` rounded to 2. **Revisit when** a design's values depend on a
`` `timescale `` another file sets: a build's order would decide it.
**Where** `astli-sema/src/lower.rs`

### `inside` is not evaluated

The HIR keeps a range `[lo:hi]` in the set as its two bounds, which a
value cannot be told from. **Revisit when** a constant needs it. **Where**
`astli-sema/src/lower/body.rs`

### A constant function runs at most a million statements

Past that, or 64 calls deep, its value is unknown
([S17](sema.md#decisions)). **Revisit when** a design needs more.
**Where** `astli-sema/src/elab/call.rs`

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

## Packages

### Only what the lock names is fetched in parallel

Before resolution, every repository the lock still reaches is fetched,
eight at a time: cheshire's 26 take 12 seconds from a cold cache rather
than 29, whatever the number of cores, since a fetch mostly waits on the
network. A repository the lock does not name is fetched when resolution
first needs it, in turn, so a run without a lock, or one that resolves
past it to new packages, waits on each. **Revisit when** such runs matter:
fetching a package's dependencies in parallel as soon as its manifest
names them overlaps those waits too. **Where** `astli-resolve/src/resolve.rs`

### `upstream_name` in a `Bender.yml` is ignored

A bare version's URL is its remote's with the dependency's own name, even
where bender would put `upstream_name` there instead. No corpus manifest
needs it: the one that writes it gives a `git` URL anyway. **Revisit when**
one does. **Where** `astli-package/src/requirement.rs`

### A header a macro names is not seen by discovery

Discovery reads each `` `include `` as written, in every branch, so it cannot
know what `` `include `HEADER `` names. A `.sv` file included only that way
is taken for a unit. The corpus has one such include. **Revisit when** a
package has a unit that fails alone for this reason. **Where**
`astli-package/src/files.rs`

### Only walked files are found by path suffix

The suffix index holds the files the walk yields, which are `.sv`, `.svh`,
`.v` and `.vh`. A header of another extension, such as `.inc`, resolves only
beside its includer or through `include-dirs`. **Revisit when** a package
relies on one by suffix; walking every file a package holds would close it.
**Where** `astli-package/src/walk.rs`

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
