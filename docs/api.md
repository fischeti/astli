# The public API

What a caller outside the crates writes. The internal types (`Tokens`,
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
I/O failure can say why. The session stays private: a tool that needs the tokens or other files is a tier 2 tool.
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

## Text and spans

`astli-text` is what every crate locates things with, and depends on
nothing:

```rust
let mut origins = Origins::new();                 // every buffer, and how includes and macros connect them
let file = origins.add_file("top.sv", text);      // SourceId: a buffer as placed
let span = Span::new(file, start, end);           // a byte range in one buffer
origins.slice(span);                              // &str
origins.line_col(file, span.start);               // LineCol: 1-based, counted in characters
origins.path(file);                               // Option<&Path>: none for a buffer astli made, a command-line define
origins.spelled(span);                            // the span as written, its expansion dropped
origins.reported_at(span);                        // the outermost call: where a message points
origins.include_trace(file);                      // the includes that reached it, outward
origins.trace(file);                              // &Expansion: the calls that placed it, innermost first
origins.placed_through(file);                     // ExpansionId: the same, by id
origins.expansion(id);                            // &Expansion: its name, call, and the `define it read

Diagnostic::error(code, span, "message")          // also `warning`; a `Severity` orders by how much it matters
    .pointing("label under the caret")
    .label(other, "a second place")
    .note("…").help("…")
    .fix(span, "replacement");                    // an Edit, which `apply` writes into the text
Disk.read(&path)?;                                // Reader: how a file is read, swapped for an editor's buffers
clean(&path);                                     // `.` and `..` taken out without asking the file system
```

A span is a buffer and bytes, never a line, so it is a `Copy` and compares
cheaply. A line is looked up when a message needs one, in a table built
when the buffer is added ([D10](plan.md#4-decisions)). A buffer seen
through an expansion has an id of its own, so one span says both where its
bytes are written and which call placed them ([D9](plan.md#4-decisions)).
A caller rarely fills an `Origins` itself: a session and a tree each hand
one out.

## Tokens and trees

`astli-syntax` defines the tree and never builds one from source; that is
`astli-parse`'s.

```rust
tokenize(text);                     // Vec<Token>: a kind and a byte range each, trivia included, ending in EOF
SyntaxKind::MODULE_KW.is_keyword(); // one enum for every token kind and every node kind
token.kind.is_trivia();             // whitespace, comments, an encrypted envelope
name_text(r"\cpu3 ");              // "cpu3": a name as the language compares it

node.kind();                        // SyntaxNode, SyntaxToken: rowan's, bound to SyntaxKind
significant_tokens(&node);          // its tokens, trivia left out; `significant_child_tokens` its own
first_significant(&node);           // also `last_significant`, and `significant_range` between them
child_token(&node, &[IDENT]);       // the first child token of one of the kinds

ast::ModuleDecl::cast(node);        // a typed view, its kind checked
module.name(); module.items();      // an accessor per part, in the order `astli.ungram` gives
```

A token holds no text, so the lexer allocates nothing per token; a tree's
tokens hold theirs. A typed view is a node whose kind is checked and nothing
more: each accessor walks the children when it is called, and `None` is a
part the source leaves out or the parser did not reach. The views are
generated from `astli.ungram` ([D16](plan.md#4-decisions)); an accessor is
written by hand only where two parts are of one kind and their place alone
tells them apart. A node's
first and last significant tokens are found from its ends, not by walking
what is between them, since every crate asks for a node's range.

## The raw scan

What a file writes, before anything is expanded, as the preprocessor reads
it: its tokens, its directives and macro references, and its conditional
regions with every branch at once. The parser shapes raw mode from it, the
formatter's transparency check compares directives by it, and `pickle` and
the package walk find each `` `include `` with it.

```rust
let input = session.input(file);            // Input: the file's text and tokens, by index
input.kind(at);                             // SyntaxKind; also `text`, `token`
let all = input.span(0..input.len());       // TokenSpan: token indices in one file

let found = scan(&input);                   // Scan: directives and macro references, in order
let found = scan_seeded(&input, expanded.macros);  // the same, arities known from a table
session.scan(file);                         // scan, of the session's file
found.directives();                         // &Directive: its type, operands and tokens
found.references();                         // &MacroRef: a call, with its arguments
found.includes(&input);                     // each `include, and the name it writes if not a macro's

all_regions(&input, all);                   // Vec<Region>: every `ifdef … `endif, nested ones too
region.branches[0].taken;                   // Taken: the condition its Branch is taken under
span.bytes(&input.tokens);                  // Span: the bytes a TokenSpan covers
```

A span here is token indices, not bytes, since the readers walk tokens:
`bytes` gives the `Span` a diagnostic points at. A reference's arguments
depend on its macro's arity, which raw mode rarely knows, so `scan_seeded`
starts from the table an expansion ends with. `all_regions` finds nested
regions in the same pass as those around them, in the order they open, so a
reader that needs a region's branches before the region around it walks the
list backwards.

## What an expansion recorded

From a place in the text to what expansion made of it, for a reader such
as a language server:

```rust
let expanded = session.expand(file);
expanded.calls;                             // Vec<ExpansionId>: each call a file writes, in order
expanded.call_at(origins, file, offset);    // Option<ExpansionId>: the call over that byte
let wrote = expanded.written_by(origins, id);  // Vec<ExpandedToken>: what it wrote, nested calls too
render(origins, &wrote);                    // as text
origins.expansion(id).def;                  // Option<Span>: the `define it read; none for a built-in
expanded.skipped;                           // Vec<Span>: each branch body not taken, as written
```

The listing is the expansion's rather than the origins', so a file
expanded twice in one session finds each run's calls in its own result.
Only a call a file writes is listed: one a body or an argument writes is
placed by the call around it, and its tokens are among that call's. A call
that writes nothing is still listed, which a token's trace cannot find. A
call that builds an `` `include ``'s name is listed, and wrote nothing,
since its tokens became the name. What
a call wrote is filtered from the tokens rather than kept per call, since a
reader asks about one call at a time. A header's spans are its inclusion's,
so a header read twice has two sets of calls and skipped branches, and a
reader that wants one per path chooses, say greying what every inclusion
skipped.

## Diagnostics

A diagnostic points where a token is placed, which inside a macro is text
the user never wrote. `astli-diag` maps it to where it is written, and
renders it for a person:

```rust
let resolved = resolve_all(tree.origins(), tree.diagnostics());  // in order, each said once
let one = &resolved[0];
one.at;                  // the Span the user wrote, the outermost macro call
one.place;               // Place: its path, and the start and end LineCol
one.spelled;             // the text a call stands for, if not at `at`
one.through;             // the calls it came through, innermost first
one.included_from;       // the includes that reached the file
Place::of(origins, span);  // any other span, named as `place` is
write(&mut out, &mut Sources::new(origins), one, Style::plain())?;  // with the source under it
colour(Severity::Error);  // the colour a report of a severity is drawn in; `EXPLAINING` a note's
```

That is all of it: resolving, and the rendering a language server or a
tool calling astli shows a person. The colours are exported so that what
the driver writes besides a report, a status line or a problem that has no
place, matches the reports beside it. The formats `astli --diag` writes for a
program, `short`, `github`, `json` and `gitlab`, are the command's, in the
driver: what they write is what the command promises, versioned with it
([D18](plan.md#4-decisions)), and a tool that wants a format of its own
builds it from a `Resolved`, which holds every place it would name.

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
position in the summaries. `Summary::new` reads a tree in either mode,
with the `Origins` its spans resolve against, the one part of a session it
needs; `summarize` expands and parses first, which is what a filelist needs.
`names` is the walk a `Summary` is read from, and `renamed` writes a tree's
text with the names its closure answers for replaced.

A `Summary` holds no attribute. The `astli_test`, `astli_should_fail` and
`astli_sweep` marks are the driver's to read, the one reader of them, which
also refuses what is wrong with one: a summary made to find a design's files
would otherwise walk attributes it never needs.

## Semantics

```rust
let hir = astli_sema::lower(&parsed);  // Hir, from a tree in either mode
for member in &hir[hir.root()].members { … }
hir.expr_children(expr, |child| …);  // each Node, Expr or Stmt, it is made of
println!("{hir}");                     // a line per member or statement
```

A `Hir` is arenas addressed by `ScopeId`, `SymbolId`, `StmtId`, `ExprId`,
`BindId` and `EnumId`, indexed on the `Hir` itself. It holds spans and no syntax node, so
files lower in parallel and a HIR outlives its tree; its spans resolve
against the session that parsed it. The types are the API, with public
fields and no accessors, since every analysis walks them whole. What
lowering knows it records, so that no reader works it out again: the scope
declaring each symbol, or holding the `bind` that adds an instance, whether
a scope holds what is not lowered and so may declare anything, how a
parameter was declared, which of a definition's parameters an instance may
override, the enum declaring each member and the typedef naming it. What
each statement and expression is made of is listed once, by
`stmt_children` and `expr_children`, so that a walk matches only the kinds it
treats apart and recurses into the rest. A type
is a value, copied into each declarator it is written for, which adds its
unpacked dimensions; an enum is the one written, whatever the copies, so
that its members know it. `hir.types()` lists every type, wherever it is
written. What takes the whole design it leaves as written: a name alone
after `bind` may be a definition or an instance. `Display` is for tests and
debugging, and its format may change.

```rust
let design = Design::new(hirs);        // Vec<Arc<Hir>>, in filelist order
let names = design.names(file);        // &Names, for one FileId, resolved when first asked
names.expr(id);                        // Option<Resolution>, a name's, an opaque region's or a hierarchical member's
design.symbol(symbol_ref).scope;       // ScopeId: where it is declared
hir.enclosing(scope, ScopeKind::Class); // Option<ScopeId>: the class around it, or definition, generate block
accesses(&design, file, names);        // Vec<Access>: what reads and writes what, how, and where
```

A `Design` holds what files share: the definitions namespace and the
packages, and so what each `bind` binds into, decided once for all who ask.
It shares its HIRs, so a design made again after an edit keeps
the files that did not change, and resolves each file once, from whichever
thread asks first. A
`Resolution` is `Declared(SymbolRef)`, `Implicit`, `Unknown` or
`Undeclared(Searched)`, which says where the name was looked for: the scopes
around it, a package, or a class. The lints and `check` read it, and only
`check` reports the last, worded by where it looked. Every name is an
expression, so `expr` is the one lookup: an implicit `.name` connects an
`ExprKind::Name`, and a name an opaque region spells is an
`ExprKind::Loose`, which says what it may refer to but not how it is used.
`names.loose()` lists those.

```rust
let found = astli_sema::check(&elaboration, file, None);  // Vec<Diagnostic>
let origins = session.into_origins();                // what they render against

let built = Built::new(&elaboration, &tops);         // what the tops build
let found = astli_sema::check(&elaboration, file, Some(&built));
let found = astli_sema::check_built(&elaboration, &built);  // Vec<(FileId, Diagnostic)>
built.frames();                                      // &[Frame]: each body built, then its blocks, each naming its parent
```

`check` resolves the file, compares each instance with its definition, and
each variable's drivers in the definitions no top reaches, at their
defaults.
Its spans are those of the file's session; `Session::into_origins` keeps what
they resolve against once the session is gone, and is `Send` where a session
is not, so the driver lowers files in parallel and renders afterwards.
`Built` holds what the tops build and no elaboration: which generate
blocks are built, which definitions are reached, and a frame for each body
built and each block built in one. `check_built` reports what only those
bodies' values reveal, a variable two things drive, a value whose type
does not convert or an operand no operator takes, what each `$fatal`,
`$error` and `$warning` built says, a parameter without a default left
without a value; each is one pass over all of it, which `check --top`
reports beside what `check` finds in each file. `Built` holds no check
itself, so what a walk of the tree found is apart from what is reported
of it, and the width lints read the same frames.

```rust
let elaboration = Elaboration::new(Arc::new(design));  // queries over the design, which it owns
let elaboration = Elaboration::with_tops(Arc::new(design), &["soc_top"]);  // the same, applying only the `bind`s the tops reach
let body = elaboration.body(definition);      // Option<BodyId>: a package, a definition at its defaults
elaboration.value(body, parameter);           // Value: Bits, Real, String or Unknown
elaboration.type_of(body, parameter);         // Ty
elaboration.expr_type(body, expr);            // Ty, by itself before its context sizes it
elaboration.eval(body, expr);                 // Value, at the type it has by itself
let walked = elaboration.walk(body);          // Walked: expr_type and eval, each expression once
elaboration.conversions(frame, |conversion| …);  // each value a frame gives to a type, or compares
elaboration.symbol_type(body, symbol_ref);    // Ty of a declaration the body sees
elaboration.names(file);                      // &Names, the design's
elaboration.reach(body, access.place?);       // Option<Range<u64>>: the bits a select names

let top = elaboration.top("soc_top")?;        // Option<BodyId>, a definition at its defaults
let set = [("Depth", TopSetting::Literal("16".into()))];  // the text of a literal, as `-G` takes it
let top = elaboration.top_with("tb_fifo", &set)?;          // Result<BodyId, TopError>
elaboration.top_values(top, "Mode")?;         // Result<Vec<TopValue>>: an enum's members, a bit's 0 and 1
let mode = elaboration.top_value(top, "Mode", "Bypass")?;  // a number or a member's name, held to the type
(mode.short(), mode.integer());               // "Bypass" for a point's name, "1" for `-G`
elaboration.children(top);                    // Arc<[Child]>: instances, generate blocks built
elaboration.child(body, instance);            // Option<(SymbolRef, BodyId)>: its definition, and the body it builds in `body`
elaboration.connections(instance);            // Vec<(SymbolId, Bound)>: each port connected, and to what
elaboration.overrides(instance);              // Vec<(SymbolId, ExprId)>: each parameter overridden, and with what
elaboration.interface_port(port);             // Option<Option<SymbolRef>>: the interface an interface port names
elaboration.dump(top);                        // the tree as `astli elab` prints it
blocks.enter(block);                          // bool: whether a walk keeping `blocks` may enter one more generate block
```

An `Elaboration` computes each answer when first asked and keeps it, so a
query takes `&self` and one elaboration serves a whole design. It is
`Sync`: threads share it and run its queries at once, each answer computed once or, when two ask at the same time, by both. An expression's own type and value are not kept, as most are asked once; a walk that asks at every level of an expression asks a `Walked` instead, which keeps them for one body and one thread. `conversions` walks one frame of a `Built` and gives each place a value is given to something of a type, or two are compared, with what it stands in, which is what typing checks and the width rules size. A body is where values are computed, addressed by `BodyId`. A
`Value` is `Unknown` where sema cannot evaluate, which is not the `x` a
`Bits` may hold: `Bits` is an integral value of any width with four states
a bit, and carries the standard's operators. A `Ty` is structural: a
`Packed` type is its width and `Shape` (scalar, vector, packed array,
struct, enum), and `ArrayTy` and `StructTy` are unpacked; its composite
parts sit behind an `Arc`, so a `Ty` clones cheaply and crosses threads. A
struct or enum knows its declaration: the typedef's `SymbolRef`, each
member's span, an enum's members as symbols. Two of one shape declared
apart are different types. A handle's `ClassTy` is its class's
`SymbolRef`, every specialization of a parameterized class the one class,
and `Ty::Null` is `null`'s.

A top's parameters are set by name, as a simulator's `-G` names them,
not by `SymbolId`, since a driver has names from a command line or an
attribute and no HIR. A setting is the text of a literal or a `TopValue`,
one entry point for both, so a sweep's value and a `-G` share the body
they build alike. `top_values` and `top_value` take a top's body rather
than its name, because a parameter's type may depend on one before it:
the caller reads each in the body that sets the ones before. Each refusal
is a `TopError` of its own, for the driver to word.

The instance tree is a walk, not a structure: `children` lists what a body
holds once built, each `Child` an instance of a definition with the body its
overrides give it, a generate block with its own body, a definition no file
declares, or a construct sema cannot decide. An instance says whether its
definition is certain, which it is not where two files declare its name,
so no walker tests that again. A walker stops at `Elaboration::DEPTH`, as a
definition may instantiate itself, and every walk stops there, so what
`check` sees is what `elab` shows. A walker also keeps a `Blocks`, which
lets it enter `Elaboration::BLOCKS` generate blocks and no more: a body's
children are built for whoever asks first, so the bound on nested loops
is the walk's, and what one walk sees does not depend on another. Instances whose parameters are equal
share a body, so a walk is cheap where the tree is wide. A walk over a
body's members finds an instance's body with `child` rather than by
searching `children`.

Arguments are given to formals by one function, `bind`, for a call's
arguments, an instance's overrides and its connections alike: what a
formal is given is a `Bound`, an expression or what the name of a `.name`
or a `.*` refers to, with the span the name is written at, so a rule
reporting on a connection needs no binding of its own. `connections` and
`overrides` are `bind` over an instance's ports and the parameters it may
override.

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
let (waivers, invalid) = Waivers::read(&tree);  // Waivers, Vec<Diagnostic>
let found = astli_lint::lint(&tree, &waivers, &Config::default());  // Vec<Diagnostic>
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
let linter = Linter::new(&elaboration, Some(&built));  // once: what every file spells, and what `built` builds
linter.lint(file, &origins, &waivers, &config)  // design and built rules
config.needs(Layer::Design)          // whether to build a design at all
config.needs(Layer::Built)           // whether `--top` gives a rule more
```

A rule's `layer()` is what it reads: a `Tree`, a file of a `Design`, or
what is `Built`, each made from the one before, so `needs` asks for a
layer or one above it. `lint` runs the tree rules, and `Linter::lint` the
design rules, and the built rules when the linter was given a `Built`.
`Linter::new` walks `Built::frames` through `Elaboration`'s public
queries for the widths the built rules read, a frame on each thread, so
the elaboration behind them is one pass and the rules run in parallel:
what a width rule tolerates is the linter's, not sema's. `Waivers` holds
no tree, so it is read off the tree as written in the parallel pass and
used after every tree is gone. It is read apart from `lint`, and the
waivers that do not parse come with it, so a file's are read once and
what is wrong with them reported once, however many layers they waive.

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
let root = Root::find(&dir)?;            // Result<Option<Root>, Error>: the package around dir, as a run's root
let root = Root::read(&manifest)?;       // Result<Root, Error>: one named outright
let workspace = Workspace::find(&root)?; // Result<Option<Workspace>, Error>: its root, members and patches
root.tables;                             // RootTables: pins, descriptions, tops, tests, simulator, profiles
root.workspace;                          // Option<WorkspaceTable>: `[workspace]`
let package = &root.package;             // Package: what a dependent sees
package.root;                            // what every path in the manifest is relative to
package.sources;                         // absolute, inside the root
package.manifest;                        // Manifest: what its astli.toml says, as written
package.manifest.fmt;                    // toml::Table, the driver's to read
package.walk(&Walk::own(package, &workspace.patches))?;  // Result<Vec<PathBuf>, Error>: its files, sorted
package.manifest.dependencies;           // BTreeMap<String, Requirement>: from astli.toml or Bender.yml
package.manifest.dev_dependencies;       // the same, for the package's own runs only
Package::at(&dir)?;                      // Result<Package, Error>: a dependency, read as a dependent sees it
bender_dependencies(&text)?;             // the dependencies of a Bender.yml's text
Manifest::parse(astli, bender)?;         // Result<Manifest, Malformed>: the same from text, as a mirror holds it
let deps = Dependencies::ordered(&package, found, requires)?;  // what resolution found, in bender's order
let enabled = deps.select(&package, &selection)?;   // the features each enables, settled once for the design
deps.replaced(&package, &declared)?;    // Vec<bool>: which units an override replaces
let files = package.files(&walk, &deps.include_dirs())?;  // Files: walked, units, headers, and the build
let dep = &deps.packages[0];             // Dependency: a name, a Package as a dependent sees it, what it needs, its Walk
dep.package.files(&dep.walk, &deps.include_dirs_of(dep))?;
```

A `Package` holds its `Manifest`, what `[package]` and the tables of a
design say as written, and beside it the sources and include directories
it names, checked and made absolute, with public fields as a `Hir` has.
`Manifest::parse` is the one reading of a manifest's text, so a mirror's
and a checkout's say the same. `[fmt]` and `[lint]` stay
tables, since what their keys mean is the driver's, and the formatter and
the linter take no `serde` to read them. What only a run at the package
reads is a `Root`'s, beside its `Package`, and what a run decides of a
package, the features enabled, the files a checkout holds and the patches
in force, is a `Walk`, which a walk takes beside the package: the
manifest's type holds what the manifest says.

A `Dependency` is a package like the root, read by `Package::at` wherever
it was located, so its files are walked and told apart by the same code
([P17](packages.md#decisions)). The driver resolves them with
`astli-resolve` and hands each, with the names it depends on, to
`Dependencies::ordered`. `Files::build` holds the package's
include directories, then its dependencies', and after them its walked
files by path suffix (`Build::include_files`). `Build::resolve` is the
one search for an `` `include ``, asking a `Reader` which candidate it
holds: the expander asks its session's, and discovery, telling headers
from units, and `pickle`, inlining them, ask `Disk`, each taking the
directives from `Scan::includes`. Given the same reader they find the same
header. A file that is found and cannot be read is the expander's
`include-not-found`, saying why, rather than a reason to search on.

`astli-resolve` fetches what a dependency names by `git` and chooses its
versions. It and `astli-package` serve the driver, and their API follows
what the driver needs, so the umbrella re-exports neither.

```rust
let store = Store::from_env()?;          // the shared cache, ASTLI_CACHE_DIR or the user's
let mirror = store.mirror("axi", url)?;  // Mirror: cloned on first use, locked while written to, its fetches reported by name
let mirror = store.fetched("axi", url)?;  // the same, fetched unless this run cloned or fetched it already
mirror.versions()?;                      // Vec<(Version, ObjectId)>: its vX.Y.Z tags
mirror.rev("main");                      // Option<ObjectId>: a branch, a tag or a commit
mirror.file(commit, "astli.toml")?;      // Option<Vec<u8>>: read without a checkout
mirror.manifests(commit, &dir)?;         // (Option<String>, Option<String>): its astli.toml and Bender.yml in dir
let lock = Lock::read(&path)?;           // Option<Lock>: astli.lock, the commit each package resolved to
let resolution = resolve(&store, &roots, &Overrides { pinned, patches }, &lock)?;  // Resolution: one version of each, for the root and then a workspace's members, preferring the lock
resolution.packages[0];                  // Resolved: name, origin (with its commit, from a repository), version, what it requires directly, revs
resolution.packages[0].checkout(&store)?;  // Located: where its files are, checked out on demand, written once and read-only
resolution.reached(&package);            // BTreeSet<&str>: the packages its design holds, by name
resolution.load(&store, &root, &package, note)?;  // Loaded: those dependencies, read and ordered, which are described and which on disk, each warning given to `note`
Lock::of(&resolution);                   // the lock it makes, to write

```

## Rules

- **Modules are private.** Each library crate is `mod x;` plus a curated
  `pub use` list, which *is* the API. The exceptions are namespaces whose
  names would crowd the crate root and collide with its own:
  `astli_syntax::ast`, a hundred typed views, and `astli_package::mark`,
  what marks a span of a message as a name, a version, a path or a code.
- **A test is not a reason to export.** An integration test that needs an
  internal is a unit test in the wrong file. `astli-parse`'s integration
  tests go through `SyntaxTree` and `parse_expanded` alone; what tests a rule or the event list
  directly lives beside it in `src/`.
- **One umbrella crate.** `astli` re-exports each library crate whole, as a
  module, with no features yet, but the two that serve only the driver,
  `astli-package` and `astli-resolve`; the driver is `astli-cli`. The
  crates it re-exports share one version, so the umbrella's version names
  a set that fits.
- **Each library crate stays usable alone.** `astli preprocess` needs no
  grammar, for example.
