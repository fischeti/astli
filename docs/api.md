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
let design = Design::new(hirs);        // Vec<Arc<Hir>>, in filelist order
let names = design.names(file);        // &Names, for one FileId, resolved when first asked
names.expr(id);                        // Option<Resolution>, a name's or a hierarchical member's
design.home(symbol_ref);               // ScopeId: where it is declared
accesses(&design, file, names);        // Vec<Access>: what reads and writes what, and where
```

A `Design` holds what files share: the definitions namespace and the
packages. It shares its HIRs, so a design made again after an edit keeps
the files that did not change, and resolves each file once, from whichever
thread asks first. A
`Resolution` is `Declared(SymbolRef)`, `Implicit`, `Unknown` or `Undeclared`;
the lints and `check` read it, and only `check` reports the last.

```rust
let found = astli_sema::check(&elaboration, file, None);  // Vec<Diagnostic>
let origins = session.into_origins();                // what they render against

let built = Built::new(&elaboration, &tops);         // what the tops build
let found = astli_sema::check(&elaboration, file, Some(&built));
built.multiple_drivers(&elaboration);                // Vec<(FileId, Diagnostic)>
built.frames();                                      // &[Frame]: each body built, its blocks inside
```

`check` resolves the file, compares each instance with its definition, and
each variable's drivers in the definitions no top reaches, at their
defaults.
Its spans are those of the file's session; `Session::into_origins` keeps what
they resolve against once the session is gone, and is `Send` where a session
is not, so the driver lowers files in parallel and renders afterwards.
`Built` holds what the tops build and no elaboration; the drivers it
compares are one pass over all of it.

```rust
let elaboration = Elaboration::new(Arc::new(design));  // queries over the design, which it owns
let body = elaboration.body(definition);      // Option<BodyId>: a package, a definition at its defaults
elaboration.value(body, parameter);           // Value: Bits, Real, String or Unknown
elaboration.type_of(body, parameter);         // Ty
elaboration.expr_type(body, expr);            // Ty, by itself before its context sizes it
elaboration.eval(body, expr);                 // Value, at the type it has by itself
elaboration.symbol_type(body, symbol_ref);    // Ty of a declaration the body sees
elaboration.names(file);                      // &Names, the design's
elaboration.reach(body, access.place?);       // Option<Range<u64>>: the bits a select names

let top = elaboration.top("soc_top")?;        // Option<BodyId>, a definition at its defaults
elaboration.children(top);                    // Arc<[Child]>: instances, generate blocks built
elaboration.dump(top);                        // the tree as `astli elab` prints it
```

An `Elaboration` computes each answer when first asked and keeps it, so a
query takes `&self` and one elaboration serves a whole design. It is
`Sync`: threads share it and take turns at its queries. A body is where values are computed, addressed by `BodyId`. A
`Value` is `Unknown` where sema cannot evaluate, which is not the `x` a
`Bits` may hold: `Bits` is an integral value of any width with four states
a bit, and carries the standard's operators. A `Ty` is structural: a
`Packed` type is its width and `Shape` (scalar, vector, packed array,
struct, enum), and `ArrayTy` and `StructTy` are unpacked; its composite
parts sit behind an `Arc`, so a `Ty` clones cheaply and crosses threads. A
struct or enum knows its declaration: the typedef's `SymbolRef`, each
member's span, an enum's members as symbols. Two of one shape declared
apart are different types.

The instance tree is a walk, not a structure: `children` lists what a body
holds once built, each `Child` an instance of a definition with the body its
overrides give it, a generate block with its own body, a definition no file
declares, or a construct sema cannot decide. A walker bounds its depth, as a
definition may instantiate itself. Instances whose parameters are equal
share a body, so a walk is cheap where the tree is wide.

## Formatting

```rust
let text = astli_fmt::format(&tree, &Options::default())?;  // Result<String, Refusal>
```

A `Refusal` is the transparency check failing: a formatter bug, caught before
the text is returned. It names the input offset where the output departs.
`Options` holds the knobs of [D7](plan.md#4-decisions) as public fields; the
driver reads them from `astli.toml`, so the library has no `serde`.

`astli_fmt::unformatted(&tree)` returns the nodes `format` writes as they
were read, for lack of a rule. The `unformatted` example sums them over the
corpus, which is how the next rule is chosen.

## Linting

```rust
let found = astli_lint::lint(&tree, &Config::default());  // Vec<Diagnostic>
let fixes: Vec<&[Edit]> = found.iter().map(|it| it.fix.as_slice()).collect();
let (fixed, applied) = astli_text::apply(tree.source(), &fixes);  // String, usize
config.set("lowrisc", Level::Allow)?;  // a group, or one rule by name
config.set_option("port-name-suffix", "input", &["i_*"])?;  // as `astli.toml` spells it
```

A rule's name is its diagnostics' code. A diagnostic's `fix` is the edits
that do away with it, empty when it has none; it lives on `Diagnostic`
rather than in the linter, since an editor offering it reads only
diagnostics, and `apply` takes it whole or not at all. `RULES` lists every rule with its
`Group`, which sets its default level, so `--list` is read off it, and the
`RuleOption`s it takes. An option's values are strings, as configuration
writes them, so a driver reading `astli.toml` needs to know no option's
kind, and `astli-lint` no `serde`. A rule
reports through `SyntaxTree::span`, which gives a node's range in the file.

```rust
let linter = Linter::new(&elaboration);  // once: what every file spells
linter.lint(file, &origins, &Waivers::of(&tree), &config)  // design rules
let widths = astli_lint::widths(&elaboration, &built);    // Vec<Width>, each with its FileId
linter.lint_built(&origins, &waivers, &config, &widths)    // the file's `Width`s
config.needs_design()                // whether to build a design at all
config.needs_tops()                  // whether `--top` gives a rule more
```

A rule that `needs_design` reads a file of a `Design` rather than a tree.
`lint` skips those, and `Linter::lint` runs only those. A rule that
`needs_tops` reads what `Built` finds for the file, which `lint_built`
takes, so the elaboration behind it is one pass and the rules run in
parallel. `widths` walks `Built::frames` through `Elaboration`'s public
queries: what a width rule tolerates is the linter's, not sema's. `Waivers` holds no
tree, so it is read off the tree as written in the parallel pass and used
after every tree is gone.

## What a build passes

Include directories and `+define+`s arrive together from a filelist or a
command line, and a `Build` holds both. `Session::building` lexes the
definitions once, as `` `define `` lines in a `<command-line>` buffer, which
gives them provenance for free, and every `expand` starts from them, over
the coverage constants every tool predefines, in a `<predefined>` buffer.
Turning `+define+NAME=VALUE` into a name and a body is the driver's, like the
rest of filelist syntax.

Expanded mode uses a build to decide what the text *is*. Raw mode can use one
only to learn macro arities (`parse`'s `seed`), and the formatter never does
([D15](plan.md#4-decisions)). Reading filelists is the driver's job, not
`astli-preproc`'s, and reading a manifest is `astli-package`'s.

## Packages

```rust
let package = Package::find(&dir)?;      // Result<Option<Package>, Error>: the package around dir
let package = Package::read(&manifest)?; // Result<Package, Error>: one named outright
package.root;                            // what every path in the manifest is relative to
package.sources;                         // absolute, inside the root
package.fmt;                             // toml::Table, the driver's to read
package.walk()?;                         // Result<Vec<PathBuf>, Error>: its files, sorted
package.dependencies;                    // BTreeMap<String, Requirement>: from astli.toml or Bender.yml
package.dev_dependencies;                // the same, for the package's own runs only
bender_dependencies(&text)?;             // the dependencies of a Bender.yml's text
manifest(astli, bender)?;                // Manifest: dependencies and what features bring in, from text, as a mirror holds it
let deps = Dependencies::ordered(&package, found)?;  // what resolution found, in bender's order
deps.select(&mut package, &selection)?; // the features each enables, settled once for the design
deps.replaced(&package, &declared)?;    // Vec<bool>: which units an override replaces
let files = package.files(&deps.include_dirs())?;  // Files: walked, units, headers, and the build
let dep = &deps.packages[0];             // Dependency: a name, a Package as a dependent sees it, what it needs
dep.package.files(&deps.include_dirs_of(dep))?;
```

A `Package` holds what `[package]` says, its paths checked and made
absolute, with public fields as a `Hir` has. `[fmt]` and `[lint]` stay
tables, since what their keys mean is the driver's, and the formatter and
the linter take no `serde` to read them.

A `Dependency` is a package like the root, read by `Package::at` wherever
it was located, so its files are walked and told apart by the same code
([P17](packages.md#decisions)). The driver resolves them with
`astli-resolve` and hands each, with the names it depends on, to
`Dependencies::ordered`. `Files::build` holds the package's
include directories, then its dependencies', and after them its walked
files by path suffix (`Build::include_files`). The expander resolves
an `` `include `` through it, and `Build::resolve` answers the same without
reading: how discovery tells headers from units, and how `pickle` inlines
them.

`astli-resolve` fetches what a dependency names by `git` and chooses its
versions, and the umbrella does not re-export it: it and `astli-package`
serve the driver, and their API follows what the driver needs.

```rust
let store = Store::from_env()?;          // the shared cache, ASTLI_CACHE_DIR or the user's
let mirror = store.mirror(url)?;         // Mirror: cloned on first use, locked while held
mirror.versions()?;                      // Vec<(Version, ObjectId)>: its vX.Y.Z tags
mirror.rev("main");                      // Option<ObjectId>: a branch, a tag or a commit
mirror.file(commit, "astli.toml")?;      // Option<Vec<u8>>: read without a checkout
mirror.checkout(commit)?;                // Checkout: its dir, written once and read-only, and the files it holds
let lock = Lock::read(&path)?;           // Option<Lock>: astli.lock, the commit each package resolved to
let resolution = resolve(&store, &root, &pinned, &lock)?;  // Resolution: one version of each, preferring the lock
resolution.packages[0];                  // Resolved: name, origin, commit, version, needs, revs
resolution.packages[0].checkout(&store)?;  // Located: where its files are, checked out on demand
Lock::of(&resolution);                   // the lock it makes, to write

```

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
