# Language server

> **Status:** the loop is built: the handshake, the buffers, positions,
> cancellation and panics; and each open file's design is loaded when it is
> opened and again after edits, reusing every unit the edit left alone.
> Formatting is answered from the buffer, document symbols and folding
> from its raw tree, and workspace symbols from the loaded designs' indexes,
> on the pool; diagnostics are published from the buffer and the snapshot.
> A position is looked up in the snapshot, and go to definition answers
> from it.
> Milestone M12 in [`plan.md`](plan.md#5-milestones).

`astli lsp` is a language server for reading a design more than for writing
one. More and more of a design is written by an agent and read by a person,
and what a reader asks of an editor is to follow a name to its definition, to
see a type, a width, a parameter's value or what a macro call turns into, and
to have the problems marked where they are. An agent reads the same way:
Claude Code calls definition, references, hover and symbols as a tool, and
takes the diagnostics a server publishes after each of its edits.

It needs no setup. A server has to know a design's files, include directories
and defines before it can say anything across files, and the others ask the
user for them, as a filelist or as flags. astli already finds them: the
server reads the package the open file is in as `check` with no arguments does
([`packages.md`](packages.md)), and `astli.toml` is all the configuration
there is.

## What a request reads

A request is answered from one of two places. The buffer's raw tree has every
conditional branch and every macro call as written
([`preprocessor.md`](preprocessor.md)), is parsed on each edit in milliseconds,
and needs nothing but the buffer. A snapshot is the whole design, expanded,
lowered and elaborated, as `check` builds it.

| Request | Reads | Gives |
| --- | --- | --- |
| Document symbols, folding | the raw tree | definitions, classes, subroutines, declarations, instances, blocks, each with its range; every branch of a conditional |
| Formatting | the buffer | `astli fmt`'s output under the package's `[fmt]`, as an edit of the lines that differ |
| Diagnostics | both | the raw parse's and the tree lint rules' at once; the preprocessor's, the expanded parse's, `check`'s and the design lint rules' from the snapshot |
| Code actions | a diagnostic's fix | the edits `lint --fix` would apply, one finding or the whole file |
| Workspace symbols | each snapshot's index | every module, interface, program, checker, primitive, package and class of the packages and their dependencies |
| Definition, references | the snapshot | where a name is declared and used, a macro's `` `define ``, an `` `include ``'s header |
| Hover | the snapshot | a declaration's type and width, a parameter's value, a macro call's expansion |
| Inlay hints | the snapshot | the formal a positional connection lands on, what `.*` connects, an override's value |
| Semantic tokens | the snapshot | what a name is, and the branches the build did not take |

## Buffers, units and snapshots

The server holds three things.

**Buffers.** The text of each open file, as the editor last sent it, and its
raw tree. They are a `Reader` that prefers a buffer to the disk, which every
read of a source file goes through. A tree is not `Send`, so the trees are
held on the main loop beside the buffers rather than in them, each parsed
when a request first asks for it and parsed again once its buffer's text
differs.

**Units.** A compilation unit, lowered: its HIR, its origins, its summary and
its parse diagnostics. A unit is a function of its own text, its build, and
the text of every file it included, and of nothing else, since units share no
macro state. That is the cache's key.

**A snapshot.** The design, the index and the elaboration over one set of
units, never changed once built. A request reads the snapshot that was
current when it started.

An edit is debounced, then the units that read the changed file are lowered
again and a new snapshot is built from them and from every unit the edit left
alone. A rebuild an edit overtakes is abandoned. A change to `astli.toml`, to
`astli.lock` or to which files exist runs discovery again.

## Where a snapshot ends

A snapshot is one design root's: the package the file is in, found from its
directory as `check` with no arguments finds it
([P1](packages.md#decisions)), or outside any package the file alone, as
`check` reads a file named. Every open file with the same root shares its
snapshot, and files of several packages, or of none, have one each. A
workspace member is a root of its own, read as `check` in its directory reads
it, its dependencies and the siblings it requires among them; a sibling's
file opened is the sibling's snapshot. A header belongs to the root of the
directory it is in, and is read through the units that include it.

A snapshot holds every design `check` analyses of its package: one for each
set of defines the package's tops give, which is one for most packages. Each
has its elaboration, its index, and each unit's path, origins and parse
diagnostics, and beside them is what the loader found wrong, as data: a
manifest that does not read, a dependency the store lacks, a file that cannot
be read. It also carries, as data, what only the manifest says of what is
reported ([Diagnostics](#diagnostics)). It is made of library types alone,
so a handler in `astli-lsp` reads it without the driver.

## Staying current

The server asks the driver two things of a design, through the trait `astli-lsp`
defines and `astli-cli` implements: which design root a file is in, asked
once when it is opened, and to load a root through a `Reader`. A load is
handed a copy of the buffers as they were when it started, each buffer's
text shared rather than copied, and whether to discover again.

An open, a change or a close of a buffer overtakes at once every load of the
design the buffer is in and of every design whose last load read the file or
asked whether it exists, and makes each due 100 ms later, a due time the
next edit pushes back. A load runs on a thread of its own, not on the pool
its units are lowered on: it waits for them, and a pool thread that waited
would hold one they need. It asks between units whether a later edit
overtook it; one that was stops lowering and is dropped. One that was not
is swapped in whole, and the snapshot it replaces is dropped on the pool.
If a buffer the load read changed before the load reached it, as a file
outside the root opened while the load had yet to read it, the snapshot is
still taken, as the nearest there is, and the design is due again. A load
not taken that discovered the package leaves the next to discover it.

Generations are counted for the whole server, never per design, so that a
load of a design closed is never taken for one of the design opened again.
Once no open file is in a design, its load running is overtaken and the
driver forgets what its loads kept for the next.

A root's plan, the packages, their files and builds, is kept from one load
to the next, and discovery runs again only when the client says a manifest,
the lock or a dependency's `Bender.yml` changed, or that a source file was
created or deleted. The server registers for `workspace/didChangeWatchedFiles`
where the client can register it dynamically. A source changed on disk that
is not open makes due the designs that read it. A client that cannot
register tells the server nothing of the disk: a design is discovered again
only when it is made again, after its files are closed and one is reopened.

The driver keeps each unit lowered, per root, keyed by its path and its
build, and with what lowering it read: every file's text, and the answer to
every question of whether a file exists, which is how an include directory
is searched. A unit is reused while every one of those reads gives the same
answer again, compared exactly; the text a unit's origins hold is what a
read is compared with, so the cache keeps no second copy of the sources. A
load keeps the units it used or made and lets the rest go; one that stopped
short keeps them over the units kept. Each load takes a ticket as it starts,
and what it keeps, plan or units, is kept only if no load that started
later kept its own already, and the root has not been forgotten since.

### What a rebuild costs

On OpenTitan, 1953 units, on ten cores, release, as the server logs each
load (`scripts/lsp-rebuild.py`, five servers of five edits each; the 100 ms
debounce comes on top):

| Load | Units lowered | Median | Range |
| --- | --- | --- | --- |
| Cold, on opening a unit | 1953 | 637 ms | 626–688 ms |
| After an edit to one unit | 1 | 57 ms | 55–65 ms |
| After an edit to `prim_assert.sv`, which 595 units include | 595 | 305 ms | 294–347 ms |
| Cold `astli check`, for comparison | 1953 | 1.10 s | |

The server holds 0.80 GB once loaded, where a cold `check` peaks at 0.83
GB. Edits to one unit leave it near 0.85 GB. Edits to the header raise it to
a plateau: over a hundred edits, every other one to the header, it stays
between 1.16 and 1.34 GB and ends at 1.20 GB. While a header's snapshot is
swapped in, the old and the new copies of its 595 units are alive at once,
and the allocator keeps that peak's pages for the next rebuild; telling it
to return freed memory at once does not lower the plateau. Of the 57 ms
after one unit's edit, about 40 go to reading every source again to see
that the kept units still hold, and lowering the one that does not; 13 to
15 to `Design::new`, 3 to the index and 2 to the elaboration. That is fast
enough that nothing finer is built ([L6](#decisions)). Comparing each file's
modification time rather than its text would save most of the 40, but
would no longer be exact.

Finding the open files' diagnostics follows each load
([Diagnostics](#diagnostics)), and the script logs it apart: about 60 ms
with `uart_core.sv` open, most of it the linter's view of the whole design,
and about 330 ms once `prim_assert.sv` is open too, since each of the 595
units that include it is checked and linted
([limitations](limitations.md#each-load-makes-its-designs-linter-and-what-its-tops-build-again)).

## From a cursor to a meaning

Sema answers by id: what an expression resolves to, what type a symbol has in
a body. A request starts from a path and an offset. Between the two is a
lookup `astli-sema` builds per file, the spelled span of every name of the
file's HIR, sorted by offset, which says what stands there: a use and what it
resolves to, a declaration, or one of the names that is not an expression, an
instance's definition, the formal a connection names, an import's package or
item, a `bind`'s target, a struct's member.

Three things make it more than a table of spans.

- **A name a macro wrote** is spelled in the `` `define ``'s body or in an
  argument, so one offset there may stand for a use in every expansion. The
  answer is each of them.
- **A header** has no unit of its own. It is read through a unit that
  includes it, and a place in it is its path, whichever unit's origins found
  it.
- **A macro call** means its expansion. A unit's expansion lists the calls
  its files write, so the call under a cursor is found, one that expands to
  nothing too, and with it the tokens it wrote and the `` `define `` it read.

A value depends on the instance. A definition's body at its defaults always
exists; where the manifest names tops ([P32](packages.md#decisions)), the
bodies the definition has under them are found by walking down from each.

### The lookup

Every request that starts at a position goes through one lookup. On the
loop it takes the snapshot current for the file's design and the buffer's
text; on the pool it finds each unit, of each design of the snapshot, whose
origins hold the file, its own text or each inclusion of a header, and in
each the byte the position names and what stands there: each name the
unit's sites give, the macro call whose `` `NAME `` the cursor is on, and
the `` `include `` it is on. Each place is answered per unit, since a
header's name may mean something else in each unit that includes it, and a
request says what several agree on once.

A unit's sites are made the first time a request asks for them, and kept
with the unit for as long as its snapshot lives. Its macro calls, and the
branch bodies its expansion skipped, the driver keeps in the snapshot as it
lowers the unit: they are a few ids and spans a unit, where its expanded
tokens are the size of the text it read ([L25](#decisions)). A call is
found among the calls a file writes outside any other, so one written in
another's argument is not found
([limitations](limitations.md#a-macro-call-in-another-calls-argument-is-not-found-from-a-cursor)).

A position is in the buffer's text, and the snapshot may have read older
text. Where a unit read the file otherwise than the buffer holds it now,
the edit has made the design due already, and the lookup answers
`ContentModified`, which tells the client to ask again, rather than answer
from places the text no longer has ([L26](#decisions)). A file not open,
one whose design has not loaded, and one no unit of its design holds,
answer nothing.

A place the snapshot holds leaves the server as a `file:` URI and a range in
the negotiated encoding, counted in the text the unit's origins hold of the
file, as diagnostics are placed; an open file is named by the URI the client
opened it with.

On OpenTitan, release, a lookup in `uart_core.sv` takes 3 ms, most of it
finding the unit among the 1953. The first in `prim_assert.sv` after a load
takes 45 ms, making the sites of the 595 units that include it, and each
after it 4 ms.

## The loop

One thread runs the server (`astli-lsp`), started by `astli lsp`, which
hands it the driver's host and points `term` at the client's log. It takes
each message from the client in turn, owns the buffers, and sends every
response. A request that reads only the buffers is answered there; one over
the design is handed to the pool with the snapshot it reads, and its answer
comes back on a channel the loop waits on beside the client's. So does each
load of a design when it ends, and the loop wakes when the next design is
due. A handler is an arm of the server's dispatch, answering at once or
handing its work to the pool.

`initialize` announces only what is built, each capability added by the
change that implements it: so far the position encoding, incremental
text document sync, document symbols, folding ranges, formatting,
workspace symbols and definition. A
request the server does not know is answered `MethodNotFound`, and a
notification it does not know is dropped. After `shutdown` every request is
refused as `InvalidRequest`, and `exit` ends the process with 0, or with 1
if `shutdown` did not come first, as the protocol has it.

A buffer is held under the path its `file:` URI names, made absolute and
cleaned, and the store is the `Reader` every read of a source file goes
through. A URI of another scheme, a client's `untitled:` buffer, is not held,
since no design reads it. Each change of an incremental `didChange` is read
against the text the one before it left. A position past the end of its line
is the line's end, before its `\r\n`, and one past the last line the end of
the text; one inside a character is that character's start.

## Formatting

Formatting reads the open buffer and nothing else of the design, so it is
answered on the loop. The layout is the `[fmt]` of the file's package, which
the driver reads for the server, since only it reads manifests: the package
the file is in, as `astli fmt` run beside the file finds it, so a dependency's
file takes its own package's, and a file outside any package the defaults.
The manifest is read on each request, so an edit to it counts from the next
format on. The client's formatting options, a tab size and the like, are
ignored ([L2](#decisions)).

The answer is an edit per run of lines that differ between the buffer and
`astli fmt`'s output, each replacing whole lines, and no edit for a file that
is formatted, not open, or one `[fmt] exclude` names. A refusal, and a
manifest that does not read, give no edit and a `window/showMessage` with
what `astli fmt` would say, since a format on save that does nothing would
otherwise say nothing.

## Symbols and folds

Both read the buffer's raw tree alone, so they answer before any design is
loaded, and a declaration in a branch no build takes is listed like any
other. A symbol's range is its construct, without the comments ahead of it,
and its selection range its name.

| Listed | Kind | Detail |
| --- | --- | --- |
| module, program, checker, primitive | module | the keyword, but for `module` |
| interface, package, class, covergroup | interface, package, class, class | `covergroup` |
| function, task; a method, a constructor in a class | function, method, constructor | a function's return type, or `task` |
| property, sequence, `let` | function | the keyword |
| modport | interface | `modport` |
| typedef: enum, struct or union, other; nettype | enum, struct, type parameter | `nettype` |
| enum variant, struct member | enum member, field | a member's type |
| parameter, localparam; type parameter | constant; type parameter | what is written before the name |
| port, in a header or the body | property | its direction and type, a header's taken from the port before it; one a header only names and the body declares is the body's alone |
| net, variable, genvar | variable | what is written before the name |
| instance, named gate | object | the definition's name, or the gate |
| labelled block, labelled statement | namespace | |
| `always`, `initial`, `final` | event, named by its block's label or its keyword | the keyword, when labelled |

Each name of a declaration of several is a symbol whose range is the whole
declaration, so the cursor anywhere in it finds one. A block without a label
is not a symbol, and what it declares is listed in the symbol around it, as
is a generate `if`, `for` and `case`. Not listed: what a macro call stands
for and what it is given, a `VERBATIM` region, a macro's `` `define ``, an
`extern` prototype of a module, an import, a constraint, a clocking block,
and the variables a `for` header declares. A client without
`hierarchicalDocumentSymbolSupport` is given the same symbols flat, as
`SymbolInformation`, each naming the one around it, since such a client may
not read a nested answer at all.

A fold covers a definition's or a subroutine's body from the line its header
ends on, a parameter or port list, an instance's connections, a block, a
`case`, a `generate` region, an enum's or a struct's body, each branch of a
conditional, a `` `define `` over several lines, and a run of `//` lines or a
`/* */` over several, as a comment. One closed by a token of its own at the
start of its last line, `endmodule`, `end`, `)`, ends at that token; for a
client that folds whole lines only, on the line before it, so the closer
stays in view.

## Workspace symbols

Workspace symbols read the index of every design of every snapshot loaded
when the request arrives, the package and its dependencies alike, and run
on the pool, stopping between designs once the request is cancelled. Before
any design has loaded the answer is empty. Listed are the declarations at
the top level: modules, programs, checkers and primitives as modules, and
interfaces, packages and classes, each kind as document symbols give it. A
declaration several designs or roots hold, or a header several units
include, is answered once. Not listed: an `extern` prototype, which its
definition stands in for, and a macro, which no index holds; what a
declaration holds inside it is the file's document symbols'.

The query matches a name ignoring case when its characters are in the name
in order, and an empty query matches every name. Matches are sorted by how
they match: the name itself, then a name it starts, then one that holds it
whole, then one that holds its characters apart; then shorter names first,
then by name, file and place. An answer holds at most 128, a screen of a
fuzzy finder, since a short query over a large design matches thousands.

A symbol's range is its name. The index gives a line and a column counted in
characters, so the column is counted out in the text the unit's origins hold
of the file, a header's included, and the range converted to the negotiated
encoding from there. An escaped name's range covers its backslash. A name a
macro wrote is placed at the outermost call, where the name is not spelled,
and is an empty range there.

## Diagnostics

Each open file's findings are published as `textDocument/publishDiagnostics`
under the buffer's version, in two steps that read different things and
find different things, so that what the buffer alone can say is shown at
once and what needs the design follows it.

**From the buffer**, on the loop, when the file is opened, on each change,
and whenever a snapshot of its design is swapped in: what the raw parse
found malformed, the waivers that do not read, and the tree lint rules, at
the levels of the file and under its waivers, as `astli lint` in the file's
directory reads it. The raw parse keeps a region its grammar does not cover
as written; that is not published, since a macro standing for syntax is one,
and the expanded parse says what a compiler would not read. The levels are
the snapshot's, so until the design is first loaded a buffer publishes what
its raw parse found and nothing else.

**From the snapshot**, on the load's thread once the driver hands the
snapshot back, before it is swapped in: what each unit's expansion and
expanded parse reported, `check` of the unit, and the design lint rules; where
the manifest names tops, also what only what the tops build gets wrong and
the built rules, and among the load's problems, a name declared more than
once. Only the units that hold an open file are checked, the file itself or
a header it includes, and of each only what lies in an open file of the
same design root is kept ([L21](#decisions)). A header's findings are those of every unit that
includes it and is reported, each published once
([L22](#decisions)). What is built from the tops and the linter's view of
the design are made once a load, for the whole design, and only if a unit
needs them. An edit that overtakes the load stops this step too, between
units and before each of those is made. How long this step took is logged
on a line of its own, after the load's, and not counted in it. A panic in
this step is caught apart from the load and logged: the snapshot is swapped
in all the same, with nothing from its design, and its problems placed
unless placing them is what panicked, since a snapshot lost would be lost
again on every load while the file that makes it panic is open.

Which units are reported, at which levels, and from which tops is the
driver's, read from `astli.toml` ([P8](packages.md#decisions)): it hands the
server, in the snapshot, whether `check` and whether `lint`'s design rules
report each unit, the levels `lint` reads each file of the root package at,
and each top's body. The server runs the checks the commands run over them
([L23](#decisions)).

The two steps overlap in nothing but a parse error both parses find, and in
what they read: the buffer step reads the text as it is now, the snapshot's
the text its load read. So what is published for a file is both sets, a
finding with the same range, code and message once, and

- a snapshot's findings, and the load's problems placed in an open file,
  are taken for the file only if its load read the buffer's text as it is
  now; otherwise the design is due again already, and the file keeps what
  it had;
- a change drops those of the changed file that do not end before the
  first character it changed, since their places may no longer be theirs,
  and keeps the rest until the next snapshot, so that typing at the end of
  a file does not blank everything above it. A finding with a related
  place in the same file that does not end before it is dropped too, since
  for a client that takes no related information the place is a line of
  the message ([L22](#decisions));
- a file that closes, or whose findings are gone, is published an empty
  list.

The load's problems that have a place are published at their file, open or
not, usually `astli.toml`: they cost nothing more than the load, and the
manifest's is often what explains the rest. One without a place is shown
the user with `window/showMessage` when a load first finds it, and only
then, since a design is loaded on every edit; a note is only logged. Where
the dependencies could not be read, the package is read without any of them
([limitations](limitations.md#a-load-that-cannot-read-one-dependency-reads-the-package-without-any)),
and every name they declare would be unknown. Why is placed in the
manifest, at the line its dependencies start on, since a resolution that
fails says what it lacks but not where it is required. The snapshot says
so, and then only its problems and each buffer's own findings are
published, and the design's held back ([L24](#decisions)).

A finding is placed as the CLI places it ([C1](cli.md#decisions)): at the
outermost macro call when a macro wrote what it is about, through the
unit's origins and in the negotiated encoding. Its labels, the text a macro
call stands for, each call of the macro chain and each `` `include `` that
reached the file are related information, each at its place; for a client
that takes none, a line of the message each. Notes and help join the
message, a line each. An error is an error, a warning a warning, a note
information. The `source` is `astli` and the `code` the finding's, and a lint
rule's code links to its group on the rules page of the site, which lists
it.

An open file's diagnostics are published under the URI the client opened it
with, and a related place in an open file names that URI too: a client
matches a URI to its documents as written, and may escape a character, or
write a drive letter, otherwise than the path's own URI does. A file not
open is published under the URI its path makes, and cleared under the one
it was last published under.

## Definition

`textDocument/definition` is answered on the pool from the lookup, and
stops between units once the request is cancelled. What it goes to is the
place a name is declared, its name and not the whole declaration, which
lowering does not keep ([S74](sema.md#decisions)).

| At the cursor | Goes to |
| --- | --- |
| a name used that sema resolves | its declaration; an implicit net, its first use |
| a declaration, a struct's member declared | itself, so that a client can tell it is there |
| an instance's definition | the definition |
| `.name(…)` or `.name` of a connection or an override | the port or the parameter; `.name` also the name it connects |
| a positional connection or override | the formal it lands on, only where no name inside it is under the cursor |
| an import's package and item, a `bind`'s target | theirs |
| a struct's member used | its declaration in the struct, in the body of the definition around it at its defaults |
| `` `NAME `` of a macro call | the name its `` `define `` defines; nothing for a built-in, a `+define+`, or a macro no `` `define `` defines |
| an `` `include `` | the start of the header it read |

A definition or a package that several files declare goes to each
declaration, as the index groups them. A name a macro wrote goes,
from the `` `define ``'s body, to what it is in each expansion, and from a
call's argument, to what it is at that call. A name nothing declares goes
nowhere, and so does a name in what the parser kept as written, which may
stand for several things. A target in a dependency's checkout is a file
like any other.

Each place is answered once, in the order found, as a `LocationLink` where
the client takes links, whose origin is what the cursor was on, and as a
`Location` otherwise ([L27](#decisions)).

## Decisions

| # | Decision | Why |
| --- | --- | --- |
| L1 | The server is for reading: diagnostics, navigation, hover, inlay hints. Rename, completion, and structure kept from a half-typed buffer are not part of this design | They are what a reader and an agent use. Completion needs a tree from a buffer missing its closers, which the parser gives up whole ([limitations](limitations.md#a-construct-missing-its-closer-falls-back-whole)), and rename needs every reference to be certain. Both build on what is here, so they can follow it. |
| L2 | No setup: the package the open file is in, found and read as `check` with no arguments does ([P1](packages.md#decisions), [P17](packages.md#decisions)), with its `[fmt]`, `[lint]` and `[top.*]`. A file outside any package is read alone. The server has no configuration file and no settings of its own | Include directories and defines are what a server cannot guess and a user will not keep current by hand. Discovery has them. A second place to configure them would disagree with the first. |
| L3 | Standard protocol only: no request, view or command a client must be written for | Every editor and every agent's client then has all of it. A hierarchy view or an instance picker is a client per editor, and the first is `astli elab`'s job. |
| L4 | A crate of its own, `astli-lsp`, which the driver starts behind `astli lsp`, over stdio. It depends on library crates alone: what it needs of a loaded design, choosing the package, settling dependencies, the analysis, it defines as a trait, which `astli-cli` implements and hands in. It writes nothing to the terminal; the driver sends what `term` writes to the client's log through a handle the crate gives it, and maps how the server ended to an exit code | The driver is large, and the server will grow to thousands of lines. Loading a design is the driver's ([D18](plan.md#4-decisions)), and turning the dependency around keeps it there without lifting it into a library first. Like `astli-package` and `astli-resolve`, the crate serves the driver alone and promises no API. |
| L5 | `lsp-server` with `gen-lsp-types`. The main loop is synchronous, and owns the buffers and their raw trees; work over the design runs on the pool, against a snapshot | The pipeline is synchronous and its parallelism is rayon's; a tree is not `Send` ([D13](plan.md#4-decisions)), so it stays on the thread that parsed it. An async framework brings a runtime for nothing that waits. |
| L6 | A cache of lowered units under a whole rebuild: an edit lowers again the units that read the changed file, and the design, index and elaboration are built anew. No `salsa` | Expanding, parsing and lowering is what the cache keeps, and its key is exact. Cold, `check` takes 30 ms on ibex, 0.2 s on ara and 1.1 s on OpenTitan on ten cores, so the rest is measured before anything finer is built. [S4](sema.md#decisions)'s queries stay functions of ids, which is what a finer scheme would need. |
| L7 | A snapshot is immutable and swapped whole. A request reads the one it started on, a rebuild checks between units whether an edit has overtaken it, and the snapshot it replaces is dropped off the request's path | No request sees half of an edit, and none takes a lock to read. Dropping a design takes tens of milliseconds, which is why a command leaks it. |
| L8 | A place leaves the server as a path and a range. A `SourceId` and a `FileId` mean something within one snapshot alone, and each unit keeps its own origins | A `FileId` is a position in the design's list, which shifts with the file set, and a header has a `SourceId` per unit that includes it. With nothing outliving a snapshot, no identity has to be stable, and one store across units is not needed. |
| L9 | Positions are UTF-8 where the client accepts it, and converted to UTF-16 code units through the line table otherwise | Spans are bytes. The protocol's default is UTF-16, and a diagnostic's column, which counts characters, is neither. |
| L10 | Dependencies are read from the lock and the store, offline. The server never fetches and never writes `astli.lock`; a dependency it cannot read is a diagnostic on `astli.toml`, and the rest of the design is still loaded | Opening a file should not change the repository or reach the network. `astli fetch` is the command that does, and the diagnostic names it. |
| L11 | Diagnostics are published, for the packages [P8](packages.md#decisions) reports on, in two steps: the buffer's own at once, the design's when its snapshot is built. A finding in what a macro wrote is at the outermost call, and its labels and its macro and include chain are related information | The first step is as fast as typing and needs no design. Published diagnostics are what an agent's client takes. One place per finding is what the CLI already settled ([C1](cli.md#decisions)). |
| L12 | No active instance. A value is shown at its definition's defaults, and where the manifest names tops, with the distinct values it takes under them and the instances that have each; an inlay hint shows a value only where they agree | Which instance a reader means is client state the protocol has no place for. A definition's bodies are few, since instances with equal parameters share one ([S22](sema.md#decisions)), so all of them fit in a hover. |
| L13 | A panic in a request is caught and answered as an error. The formatter's stack overflow on a chain a thousand links long stays ([limitations](limitations.md#a-chain-of-postfixes-or-mixed-operators-2048-deep-overflows-the-formatter)) | A command that panics ends; a server that does takes every open file's state with it. An overflow is not caught, but only a fuzzer writes such a chain, and a client starts a server that died again. |
| L14 | Formatting is the whole file, returned as an edit of the lines that differ. No range formatting | The formatter lays out a file, not a part of one. A whole-document edit moves the cursor and reopens folds in most editors. |
| L15 | Every editor but VS Code starts `astli lsp` from its own configuration, upstreamed where the editor keeps a list of servers. The Claude Code plugin names it in a `.lsp.json`. VS Code gets a thin extension that ships the binary and has no view, command or setting beyond the binary's path | The binary is the product, and is already built for each platform. VS Code starts no server without an extension, and one that downloads nothing and configures nothing is one a user never has to think about. |
| L16 | Only work on the pool is cancelled: `$/cancelRequest` sets the request's flag, which its work reads between steps, and a request cancelled before its answer is sent is answered `RequestCancelled`, whatever its work made. A request answered on the main loop has been answered before the cancellation is read | Requests over the design are the ones that take long. One answered on the loop has already been answered by the time the loop reads the next message. A flag the work reads costs nothing until it is set, and the work decides where it can stop. |
| L17 | A panic is caught around each message on the main loop, and around each piece of work on the pool, and told to the client's log; a request's answer is `InternalError`. What `term` writes, which a command writes to stderr, goes to the client's log as `window/logMessage`, without colour. The standard panic hook still writes to stderr | A panic on a rayon thread that reached rayon would abort the process. Stdout is the protocol's, and nothing in astli writes there but a command's result; stderr a client may not show. The hook's line names where the panic was, which a bug report wants. |
| L18 | A snapshot is one design root's: the package an open file is in, as `check` with no arguments finds it, or the file alone outside any. Files of one root share it; a workspace member is a root of its own | It is what `check` would say in the file's directory, which is what a user compares the editor with. A root is cheap to find from a path and never depends on what else is open, so opening a file never changes another's design. A member's file read inside a sibling's design would be read with the sibling's build, and which sibling would depend on the order files were opened. |
| L19 | A unit is cached under its path and build, valid while every file it read reads the same and every file it asked after exists as it did. No hash, and no knowledge of the preprocessor | What a unit read is all it depends on, and recording it at the `Reader` catches what a list of included files would miss: a header created earlier on the include path. Comparing with the text its origins already hold is exact and costs no memory. |
| L20 | A root's plan is kept until a manifest, the lock or the set of source files changes, as `workspace/didChangeWatchedFiles` says; edits debounce for 100 ms | Discovery walks the package and settles its dependencies, which an edit to a source file does not change. 100 ms is longer than the pause between keys and short enough that the design is current by the time the reader looks. |
| L21 | Diagnostics are published for open files only, and the load's problems at their file. Of a design, only the units that hold an open file are checked and linted | `check` over a whole package is what a cold run costs, 1.1 s on OpenTitan, and a design is loaded again on every edit. The files open are the ones being read; CI's `check` covers the rest. A problem is the load's, found whatever is open, and the manifest's is the one that explains why the rest is missing. |
| L22 | What is published for a file is the buffer's findings and the snapshot's together, one with the same range, code and message once. A snapshot's, and the load's problems placed in an open file, are taken for the file only if its load read the buffer's text as it is now; a change keeps those of the changed file that end before the first character it changed, related places in the file and all, until the next snapshot. A header's are those of every reported unit that includes it | Neither step shows a finding at a place its text no longer has, and neither waits for the other. The text before an edit is unchanged, so what is found there is placed exactly, and the design's findings above the cursor do not blink out at each key. A header means something in each unit that includes it, as `check` reports it through each. |
| L23 | What the driver's policy decides comes as data: the snapshot says whether `check` and whether `lint`'s design rules report each unit, the levels `lint` reads each root file at, and each top's body; the server runs the checks the commands run over them. The tree rules' levels are the snapshot's too | Which files are reported ([P8](packages.md#decisions)), the levels, the waivers and the tops are the manifest's, which only the driver reads, and levels can extend a style that is a dev dependency, which only discovery finds. As data, a finding is the same whether `check` or the server runs it, and no second copy of the policy can disagree with the first. |
| L24 | A snapshot whose dependencies could not be read says so, and then only its problems and the buffers' own findings are published; why is a diagnostic in the manifest | The package is then read without any dependency, so every name one declares would be reported unknown, a wall of findings that says nothing. A fresh clone opened before `astli fetch` is that case, and the manifest's diagnostic says which dependency the store lacks, beside the requirements, and names the command. |
| L25 | A request at a position goes through one lookup, which gives each unit that holds the file, the byte, the sites there, the macro call and the `` `include ``. A unit's sites are made on the first ask and kept with it; its macro calls and skipped branches the driver keeps in the snapshot, and not its expanded tokens | Definition, references, hover and inlay hints all start from what stands at a place, and finding it is the same for each. Sites take a walk of the unit's HIR, which a header's 595 units make 45 ms of, so they are made once a snapshot rather than once a request, and only for the units asked about. A unit's calls are ids into the origins the snapshot keeps already. Its tokens would cost as much as its text again, for every unit, to show the expansion of a call a reader hovers, which expanding that one unit again from its build, kept in the snapshot, gives when asked. |
| L26 | A lookup in a file whose buffer differs from the text its snapshot read answers `ContentModified` | The position is in the buffer's text, and the snapshot's places are in the text it read, so any answer would be placed where the text no longer is. The edit has made the design due already, and the client asks again. Mapping the position back through the edit would need the edits kept since the load. |
| L27 | Definition answers every place what stands at the cursor leads to, each once: a name's declaration, each declaration of a definition several files declare, what a macro-written name is in each expansion. A declaration answers itself, as links where the client takes them | A name in a header or a macro's body means one thing per inclusion or expansion, and a reader asking where it is declared wants all of them, not the first unit's. An answer of itself lets a client say "this is the definition" rather than nothing. A link's origin tells the client what the answer was for, which a macro call or a positional connection spans more widely than the word under the cursor. |

## Prior art

slang-server is the closest in what it shows, and the model for hover and
inlay hints: a symbol's type, width and value, the macro a token came from,
port and parameter names at a positional connection. It reads its design from
`.slang/server.json`: slang's flags, a `.f` file or a command that prints one,
and include directories. Without them it indexes the workspace and compiles
only the definitions the open file names, which its documentation says misses
macros an earlier file of the unit defined, and deep or upward hierarchical
names. A full design is elaborated again on each save. Its hierarchy view and
its choice of an active instance per module are its VS Code client's, which is
what L3 and L12 decline.

verible's server needs a `verible.filelist` at the root before it follows a
name across files. svls is svlint in an editor, and configures include paths
and defines in `.svls.toml`. svlangserver indexes by glob settings. Sigasi
and DVT read a project file or an argument file.

rust-analyzer, ruff and ty are the shape of L5: `lsp-server`, the generated
protocol types, and scheduling and cancellation written on top. ruff and ty
are also L15's: the server is a subcommand of the one binary, and their
VS Code extensions bundle it per platform.
