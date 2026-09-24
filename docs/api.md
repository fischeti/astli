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
tree.origins();         // what svirig-diag renders them against
```

`SyntaxTree` owns a private `Session<'static>` and reads the file itself, so an
I/O failure can say why. The `Reader` trait returns only `Option`. The session
stays private: a tool that needs the tokens or other files is a tier 2 tool.
The transparency check lexes the input again rather than reach into it, which
costs one lex per file.

**An explicit session.** For expanded mode, a custom `Reader` (an editor's
unsaved buffers, the differential harness), or spans compared across files:

```rust
let build = Build::new().include_dir("rtl/include").define("WIDTH", "32");
let mut session = Session::new().building(build);
let file = session.open("top.sv")?;   // Option<SourceId>
let expanded = session.expand(file);  // from the build's definitions
let parsed = parse(&session, file, expanded.macros);  // raw, arities seeded
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
command line, and a `Build` holds both. `Session::building` lexes the
definitions once, as `` `define `` lines in a `<command-line>` buffer, which
gives them provenance for free, and every `expand` starts from them. Turning
`+define+NAME=VALUE` into a name and a body is the driver's, like the rest of
filelist syntax.

Expanded mode uses a build to decide what the text *is*. Raw mode can use one
only to learn macro arities (`parse`'s `seed`), and the formatter never does
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
