# The public API

What a caller outside the crates writes. The internal types (`Input`, `Tokens`,
`Events`) are documented next to their code.

## Two tiers

**A file, a tree.** For a tool that reads one file at a time, the formatter
included:

```rust
let tree = SyntaxTree::read("top.sv")?;        // io::Result
let tree = SyntaxTree::parse("top.sv", text);  // text already in hand

tree.root();            // &SyntaxNode
tree.source();          // &str
tree.line_col(offset);
tree.diagnostics();     // &[Diagnostic]
tree.session();
```

`SyntaxTree` owns a private `Session<'static>` and reads the file itself, so an
I/O failure can say why. The `Reader` trait returns only `Option`.

**An explicit session.** For expanded mode, a custom `Reader` (an editor's
unsaved buffers, the differential harness), or spans compared across files:

```rust
let mut session = Session::new().searching(includes);
let file = session.open("top.sv")?;   // Option<SourceId>
let parsed = parse(&session, file);   // raw mode
let expanded = session.expand(file);  // tokens + diagnostics
```

`parse` takes `&Session` because the tree does not borrow the session: tier 2
addresses files by `SourceId`, so more files can be added with `&mut` while
earlier trees stay alive.

## Formatting

```rust
let text = svirig_fmt::format(&tree)?;  // Result<String, Refusal>
```

A `Refusal` is the transparency check failing: a formatter bug, caught before
the text is returned. It names the input offset where the output departs.
There are no options yet ([D7](plan.md#4-decisions)).

`svirig_fmt::unformatted(&tree)` returns the nodes `format` writes as they
were read, for lack of a rule. The `unformatted` example sums them over the
corpus, which is how the next rule is chosen.

## What a build passes

Include directories and `+define+`s arrive together from a filelist or a
command line. Today the driver holds them as its own `Build`
(`crates/svirig/src/sources.rs`). `Session` takes only `Includes`, so the
driver seeds definitions itself: it lexes a synthesised `<command-line>` buffer
of `` `define `` lines, which gives command-line macros provenance for free.
The intended shape moves `Build` onto the session, so that every caller stops
repeating that step.

Expanded mode uses a build to decide what the text *is*. Raw mode can use one
only to learn macro arities (`parse_seeded`), and the formatter never does
([D15](plan.md#4-decisions)). Reading filelists and manifests is the driver's
job, not `svirig-preproc`'s.

## Rules

- **Modules are private.** Each library crate is `mod x;` plus a curated
  `pub use` list, which *is* the API. The one exception is
  `svirig_syntax::ast`, a namespace of a hundred typed views that would
  crowd the crate root and collide with its names.
- **A test is not a reason to export.** An integration test that needs an
  internal is a unit test in the wrong file. `svirig-parse`'s integration
  tests go through `SyntaxTree` alone; what tests a rule or the event list
  directly lives beside it in `src/`.
- **No facade crate.** The `svirig` name belongs to the driver.
- **Each library crate stays usable alone.** `svirig preprocess` needs no
  grammar, for example.
