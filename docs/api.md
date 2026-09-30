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
tree.path();            // &Path, as read or named
tree.source();          // &str
tree.line_col(offset);
tree.diagnostics();     // &[Diagnostic]
tree.unparsed();        // Vec<Diagnostic>, a warning per VERBATIM run
tree.origins();         // what astli-diag renders them against
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

**Expanded mode** parses the expansion itself:

```rust
let parsed = parse_expanded(&session, &expanded.tokens);
parsed.span(&token);  // Option<Span>: placed, or None for an added separator
```

Both return `Parsed`. `span` is how a caller finds where a token came from
without knowing which mode made the tree: in raw mode it is the token's range
in the file. `unparsed` is asked for rather than among the diagnostics,
because a tool that only reads names, like the index, has no use for it.

## Names across files

```rust
let (summary, diagnostics) = astli_index::summarize(&mut session, file);
let index = Index::new(summaries);    // Vec<Summary>, in filelist order
index.reachable(&["soc_top"])?;       // Result<Vec<usize>, UnknownTop>
index.ordered(&files);                // dependencies first

astli_index::names(&parsed.root);     // Vec<Name>: token and Role, in order
astli_index::renamed(&parsed.root, |name| Some(format!("p_{name}")));
```

A `Summary` holds names and `path:line:col` locations, not spans, so it is
`Send` and outlives its session: summaries are made per file in parallel, as
each file is its own unit, and one index is built from them. A file is its
position in the summaries. `Summary::new` reads a tree in either mode;
`summarize` expands and parses first, which is what a filelist needs.
`names` is the walk a `Summary` is read from, and `renamed` writes a tree's
text with the names its closure answers for replaced.

## Semantics

```rust
let hir = astli_sema::lower(&parsed);  // Hir, from a tree in either mode
for member in &hir[hir.root()].members { … }
println!("{hir}");                     // a line per member or statement
```

A `Hir` is arenas addressed by `ScopeId`, `SymbolId`, `StmtId` and `ExprId`,
indexed on the `Hir` itself. It holds spans and no syntax node, so files lower
in parallel and a HIR outlives its tree; its spans resolve against the session
that parsed it. The types are the API, with public fields and no accessors,
since every analysis walks them whole. `Display` is for tests and debugging,
and its format may change.

```rust
let design = Design::new(hirs);        // Vec<Hir>, in filelist order
let names = design.resolve(file);      // Names, for one FileId
names.expr(id);                        // Option<Resolution>
```

A `Design` holds what files share: the definitions namespace and the
packages. `resolve` takes `&Design`, so files resolve in parallel. A
`Resolution` is `Declared(SymbolRef)`, `Implicit`, `Unknown` or `Undeclared`;
the lints and `check` read it, and only `check` reports the last.

## Formatting

```rust
let text = astli_fmt::format(&tree)?;  // Result<String, Refusal>
```

A `Refusal` is the transparency check failing: a formatter bug, caught before
the text is returned. It names the input offset where the output departs.
There are no options yet ([D7](plan.md#4-decisions)).

`astli_fmt::unformatted(&tree)` returns the nodes `format` writes as they
were read, for lack of a rule. The `unformatted` example sums them over the
corpus, which is how the next rule is chosen.

## Linting

```rust
let found = astli_lint::lint(&tree, &Config::default());  // Vec<Diagnostic>
config.set("lowrisc", Level::Allow)?;  // a group, or one rule by name
```

A rule's name is its diagnostics' code. `RULES` lists every rule with its
`Group`, which sets its default level, so `--list` is read off it. A rule
reports through `SyntaxTree::span`, which gives a node's range in the file.

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
job, not `astli-preproc`'s.

## Rules

- **Modules are private.** Each library crate is `mod x;` plus a curated
  `pub use` list, which *is* the API. The one exception is
  `astli_syntax::ast`, a namespace of a hundred typed views that would
  crowd the crate root and collide with its names.
- **A test is not a reason to export.** An integration test that needs an
  internal is a unit test in the wrong file. `astli-parse`'s integration
  tests go through `SyntaxTree` alone; what tests a rule or the event list
  directly lives beside it in `src/`.
- **One umbrella crate.** `astli` re-exports each library crate whole, as a
  module, with no features yet; the driver is `astli-cli`. The crates are
  versioned in lockstep, so the umbrella's version names a set that fits.
- **Each library crate stays usable alone.** `astli preprocess` needs no
  grammar, for example.
