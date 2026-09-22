# The public API

What a caller outside these crates writes, and why it is shaped that way. The
internal shapes — `Input`, `Tokens`, `Events` — are documented next to the code
that uses them; this file is about the door.

---

## What was wrong with the first shape

Parsing one file read:

```rust
let mut pp = Preprocessor::new();
let file = pp.add(path, text);
let tokens = pp.tokens(file);
let source = pp.origins().text(file);
let tree = parse(pp.input(file));
```

Three defects, and they compound:

**`parse`'s only parameter was a type the caller could not construct.**
`Input::new` is crate-private, so the sole route to an `Input` was through a
`Preprocessor`. The signature advertised a free function and then sent you
looking for its factory.

**`Preprocessor` was named for one of the four things it held.** It is the
`Origins` store, the `Reader`, the include path and the lexed-token cache — a
compilation session, which is what its own prose called it throughout. Named
for preprocessing, `parse(pp.input(file))` reads as though parsing needed a
preprocessor, which it does not.

**The tree came back detached from what explains it.** `parse` returned a bare
`SyntaxNode`, so every caller kept the session and the `FileId` alive by hand
to recover the source text or a line and column.

---

## Two tiers

### Tier 1 — a file, a tree

The whole API for a tool that reads one file at a time:

```rust
let tree = SyntaxTree::read("top.sv")?;        // io::Result
let tree = SyntaxTree::parse("top.sv", text);  // text already in hand

tree.root();            // &SyntaxNode
tree.source();          // &str, for the round-trip invariant
tree.line_col(offset);  // for a message that points somewhere
tree.diagnostics();     // &[Diagnostic], and usually empty
tree.session();         // for what only the session answers
```

A `SyntaxTree` owns a private session, always `Session::new`'s, reading from
disk. It reads the file itself rather than going through that session's
[`Reader`]: the trait answers `Option` on purpose, and a tool handed a path can
say *why* it could not be opened. A caller who wants a reader of its own is
holding unsaved buffers or several files, and wants tier 2.

This is the tier the formatter lives in. [D6](plan.md#4-decisions) has it never
follow an `` `include ``, so each file is formatted alone, and a corpus walk
builds one session per file because each file is its own compilation unit
(3.12.1).

### Tier 2 — an explicit session

```rust
let mut session = Session::new().searching(build);
let file = session.open("top.sv")?;
let parsed = parse(&session, file);   // raw: what the formatter reads
parsed.root;                          // SyntaxNode
parsed.diagnostics;                   // what the rules found wrong
```

Not a second API so much as the one tier 1 is three lines over. It stays public
for the three callers that genuinely need the session itself:

- **Expanded mode**, which follows includes and writes synthesised buffers into
  the store. A header pulled in by twenty files should be read, lexed and
  stored once; that is the whole reason a session exists.
- **A custom `Reader`** — an editor answering out of unsaved buffers, and the
  differential harness.
- **Cross-file spans**, so that a span from one file is comparable with one
  from another. Diagnostics over a compilation, and whatever semantic work
  follows.

`parse(&session, file)` takes the two things a caller actually holds. `Input`
survives unchanged as the currency between the preprocessor and the parser — it
is `Copy` and it is right for that job — it just stops being the door.

### There is no `parse_expanded` yet

The grammar already runs over the expanded stream — that is what `Expanded` is
for — but the tree builder is raw-only, and deliberately: losslessness is a
raw-mode idea, and an expanded stream's tokens are not one file's, so there is
nothing for them to round-trip against.

So an expanded parse has no tree to return, and the entry point waits for a
builder that knows what an expanded tree is. It belongs beside the compiler
that wants one. When it lands it takes `&mut Session`, because expansion writes
into the store and the signature should say so.

---

## What a build passes

Include directories and `+define+` arrive together, from the same place — a
filelist, a `bender` manifest, a command line — and neither is a property of a
file. So they are one value on the session rather than two parameters on a
call:

```rust
#[derive(Clone, Default)]
pub struct Build {
    pub includes: Includes,
    /// As written: "SYNTHESIS", "WIDTH=8".
    pub defines: Vec<String>,
}
```

One struct because `+libext+` and `-y` library directories are then a field
rather than a third parameter, and because a filelist reader wants to return
one value.

### Defines are seeded by a synthesised file

`+define+WIDTH=8` is text, and making an entry out of it means lexing it. So
build a `<command-line>` buffer holding the `` `define `` lines, add it to
`Origins`, and scan it with the machinery that is already there. Nothing new in
`expand.rs`, and `Origins::trace` then explains a command-line macro for free:
a diagnostic about one points at `<command-line>`, which is what a C compiler
has always done.

The alternative — a second constructor into `MacroTable` — is a parallel path
into the one structure whose contents everything downstream trusts, and it
would have no provenance to report.

### Why it sits on the session and not on `expand`

Raw mode wants it too, for a different reason, except when the formatter is
the one reading it: [D15](plan.md#4-decisions) keeps every build input away
from `fmt`.

Neither field changes what raw mode emits, because raw mode never follows an
include and cannot evaluate a conditional. But both tell it macro **arities**,
which is what decides whether a `` `name `` takes an argument list — and
guessing that wrong puts a `MACRO_ARG_LIST` over a parenthesised expression
that is nobody's argument. See
[`limitations.md`](limitations.md#a-macro-references-arguments-are-guessed-when-its-arity-is-unknown).

So: expanded mode uses a `Build` to decide what the text *is*, raw mode uses it
only to know what is a call and how wide. Hanging it off the call site would
deny raw mode the half it can use.

### Where a `Build` comes from

Not from `svirig-preproc`. The driver reads the `.f` file or the `Bender.yml`
and produces a `Build` and a list of paths. Filelist syntax has nothing to do
with SystemVerilog, and keeping it out is the same line the preprocessor
already holds about grammar.

---

## The modules are private

Every library crate is `mod x;` plus a curated `pub use` list. Nothing else
leaves the crate, so the re-export list *is* the API rather than a suggested
reading of it.

It was the other way around — `pub mod` on everything, with the same `pub use`
list on top — and the cost was that roughly two thirds of the crates' public
functions were reachable by a path nobody ever wrote. Closing the modules broke
exactly two references across the workspace, which is the measure of how much
of that surface was load-bearing.

The rule this leaves: an item becomes public by being named in the `pub use`
list, which is one line to read and one place to argue about.

### A test is not a reason to export

`KEYWORDS_1800_2023` was public so that an integration test could check the
table was sorted and the keyword variants were one contiguous run. Those are
invariants of the table, not of the crate, and a `tests/` file can only see
them by widening the door. They moved into a `#[cfg(test)]` module beside the
table.

An integration test reaching for an internal is the signal that it is a unit
test in the wrong file. `svirig-parse` still has several — `Parser`, `Events`
and the grammar entry points are public for `tests/` and for nothing else, and
`Events::snapshot` returns a type the door does not let a caller name.

---

## What is deliberately not here

**No facade crate.** A `svirig` library crate whose contents are `pub use`
would exist only to host the one-liner, and `SyntaxTree` already hosts it from
`svirig-parse`. The `svirig` name belongs to the driver binary.

**No `SyntaxTree` borrowing its session.** `Session::add` takes `&mut self`, so
a tree holding `&Session` would block the next file from being added — the
reason tier 2 addresses files by `FileId` against a shared `&Session` instead.

**No lifetime on `SyntaxTree`.** It holds a `Session<'static>`, because the
session it owns is always `Session::new`'s. A caller wanting a tree over its
own `Reader` has several files and wants tier 2; if that stops being true, the
parameter goes back in.
