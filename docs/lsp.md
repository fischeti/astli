# Language server

> **Status:** designed; nothing is built. Milestone M12 in
> [`plan.md`](plan.md#5-milestones).

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
| Diagnostics | both | the raw parse's and the tree lint rules' at once; the preprocessor's, `check`'s and the design lint rules' from the snapshot |
| Code actions | a diagnostic's fix | the edits `lint --fix` would apply, one finding or the whole file |
| Workspace symbols | the snapshot's index | every definition, package and class of the package and its dependencies |
| Definition, references | the snapshot | where a name is declared and used, a macro's `` `define ``, an `` `include ``'s header |
| Hover | the snapshot | a declaration's type and width, a parameter's value, a macro call's expansion |
| Inlay hints | the snapshot | the formal a positional connection lands on, what `.*` connects, an override's value |
| Semantic tokens | the snapshot | what a name is, and the branches the build did not take |

## Buffers, units and snapshots

The server holds three things.

**Buffers.** The text of each open file, as the editor last sent it, and its
raw tree. They are a `Reader` that prefers a buffer to the disk, which every
read of a source file goes through.

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
- **A macro call** means its expansion. The origins list a unit's expansions
  by their call, so the call under a cursor is found, one that expands to
  nothing too, and with it the tokens it wrote and the `` `define `` it read.

A value depends on the instance. A definition's body at its defaults always
exists; where the manifest names tops ([P32](packages.md#decisions)), the
bodies the definition has under them are found by walking down from each.

## Decisions

| # | Decision | Why |
| --- | --- | --- |
| L1 | The server is for reading: diagnostics, navigation, hover, inlay hints. Rename, completion, and structure kept from a half-typed buffer are not part of this design | They are what a reader and an agent use. Completion needs a tree from a buffer missing its closers, which the parser gives up whole ([limitations](limitations.md#a-construct-missing-its-closer-falls-back-whole)), and rename needs every reference to be certain. Both build on what is here, so they can follow it. |
| L2 | No setup: the package the open file is in, found and read as `check` with no arguments does ([P1](packages.md#decisions), [P17](packages.md#decisions)), with its `[fmt]`, `[lint]` and `[top.*]`. A file outside any package is read alone. The server has no configuration file and no settings of its own | Include directories and defines are what a server cannot guess and a user will not keep current by hand. Discovery has them. A second place to configure them would disagree with the first. |
| L3 | Standard protocol only: no request, view or command a client must be written for | Every editor and every agent's client then has all of it. A hierarchy view or an instance picker is a client per editor, and the first is `astli elab`'s job. |
| L4 | A module in `astli-cli` behind `astli lsp`, over stdio | What it needs, choosing the package, settling dependencies, the analysis, is the driver's ([D18](plan.md#4-decisions)). A crate of its own would have no second reader, and would need those lifted into a library first. |
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
