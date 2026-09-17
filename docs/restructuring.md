# Restructuring

> **Status:** proposal. Nothing here is built. It supersedes the crate table
> in [`plan.md`](plan.md#crates), which sketches a `svirig-lexer` and folds
> the parser into `svirig-syntax`; §2 argues that split runs the wrong way.

Two questions, asked together because their answers depend on each other:
where the crate boundaries go, and what the preprocessor's entry points look
like. The boundary question is cheap — nothing is published, so any API may
break at any time — and the entry-point question is the one with content.
Parallelism (§5) is neither, but it constrains both answers, and two of its
constraints are cheap now and awkward later.

---

## 1. What is wrong now

Nothing is *hurting*. `svirig-syntax` compiles from clean in 0.66 s and the
module layering is already acyclic: nothing in `kind`, `keyword`, `lexer` or
`tree` reaches into `preproc` or `parser`, and nothing in `preproc` reaches
into `parser`. The case for moving is not pain.

What is untidy is the preprocessor's surface. It has three different answers
to "how does a file become tokens here":

| door | who loads the text | who lexes |
| --- | --- | --- |
| `scan(&Input)` | caller; it never enters `Origins` at all | caller |
| `expand(&mut Origins, file, &Includes)`, root file | caller, via `add_file` | preproc, out of `origins.text(file)` |
| `` `include `` reached while expanding | preproc: `resolve` → `Reader::read` → `add_included` | preproc |

The root file and an included file take different roads into the same store.
An editor can install a `Reader` impl serving unsaved buffers and have it
honoured for every header but not for the file being edited. `scan` sits
outside the store entirely, so using both modes on one file lexes it twice.

The cost shows up at the call sites. In `examples/conditionals.rs`:

```rust
let mut origins = Origins::new();
let file = origins.add_file(path, source);
// Held apart from the store, so that expanding a branch can write to it.
let tokens = tokenize(&source);
```

The text is kept alive beside the store because the caller lexes but the store
owns the bytes, and a slice of `origins` cannot be held across the `&mut
origins` that expansion needs. `expand_span` then lexes the same file again
into an `Expander.lexed` cache that dies with the call. In the parser,
`Shapes::of` runs `scan(input)` twice over the same input twenty-five lines
apart, because there is nowhere to keep the first result.

---

## 2. The crates

Target layout:

| crate | contents | src lines today |
| --- | --- | --- |
| `svirig-text` | spans, `Origins`, the file store, file reading | 349 |
| `svirig-syntax` | `kind`, `keyword`, `lexer`, `tree` (the `rowan` `Language`) | ~1,480 |
| `svirig-preproc` | `preproc/` | ~2,220 |
| `svirig-parse` | `parser/` | ~3,900 |

Then `svirig-fmt` on top of `svirig-parse`, as before.

### The parser moves out, not the lexer

The obvious split — lift the lexer into its own crate — founders on
`SyntaxKind`. One enum covers trivia, tokens, keywords *and* nodes, and
`is_node` is a range check against `FIRST_NODE`. A lexer crate would have to
carry the whole enum, node kinds included, and would then be a vocabulary
crate with a 182-line lexer stapled on. The alternative, two enums with a
`From` impl, pays a conversion at every token push and leaves 936 lines of
enum to keep in sync across a crate boundary.

Both are answers to the wrong question. `SyntaxKind` **is** the lexer:
`logos` derives on it, `keyword::lookup` maps into it, and `tree.rs`
implements `rowan`'s `Language` over it. Those four files are one thing, and
that thing is honestly named `svirig-syntax`. What wants to leave is the
parser — the largest piece, the one still changing, and the one nothing else
depends on.

### Why `svirig-text` stays separate

Not for layering hygiene. `Origins` is the file store, not merely a
provenance map: the expander reads source through `origins.text(file)` and
registers synthesised buffers in it. It is *upstream* of the tree in the data
flow, not beneath it. A raw-mode consumer — a lint over directives, a macro
dumper — uses `Origins` end to end and never builds a tree. Once
`svirig-preproc` exists, two crates need it and neither is the parser.

### Diagnostics do not go in `svirig-text`

`plan.md` says rendering will land there. Split it by dependencies instead:

- **`svirig-text` holds the data** — `Span`, `LineCol`, `Origins`, `trace`,
  `reported_at`, and eventually a plain `Diagnostic` / `Severity` / `Label`.
  Every crate that *produces* an error needs that type, so it sits at the
  bottom and stays dependency-free.
- **A separate crate renders it.** Terminal output wants colour, unicode
  width and snippet framing; LSP output wants none of that and a different
  shape. Putting either in `svirig-text` makes the lexer depend
  transitively on a terminal renderer to report an unterminated comment.

---

## 3. Where `` `include `` splits

Reading a file is generic. *Where* `` `include `` looks is 22.4, which is
language semantics. `include.rs` currently holds both.

To `svirig-text`, beside `Origins`, which is already the store:

- the `Reader` trait and `Disk`
- `clean()`, textual path normalisation, which exists to serve the cycle check
- a new `Origins::load_included(&dyn Reader, candidates, site)`, so that the
  one operation that admits text from outside also decides whether it may be
  admitted
- `reenters` and `depth`, today methods on `Expander` but written entirely in
  terms of `include_trace` and `path`

Staying in `svirig-preproc`:

- `Includes { quoted, angle }` and `search()` — the candidate list and the
  order it is tried in, which is the part the standard specifies. It produces
  paths and no longer reads anything or mutates the store.
- `MAX_DEPTH`, since the fifteen-level floor is 22.4 as well, though the check
  that consults it becomes a store query.

Not to be changed while in there: two `` `include ``s of one header correctly
produce two buffers with different `included_from`. Loading must not dedup by
path.

---

## 4. One entry point per mode

The state that has to outlive a single call — and be shared between the two
modes — gets owned rather than re-threaded:

```rust
pub struct Preprocessor<'a> {
    origins: Origins,
    reader: &'a dyn Reader,
    includes: Includes,
    lexed: FxHashMap<FileId, Rc<[Token]>>,
}

impl Preprocessor<'_> {
    pub fn add(&mut self, path: impl Into<PathBuf>, text: String) -> FileId;
    pub fn origins(&self) -> &Origins;
    pub fn tokens(&self, file: FileId) -> Rc<[Token]>;
    pub fn input(&self, file: FileId) -> Input<'_>;
    pub fn expand(&mut self, file: FileId) -> Vec<ExpandedToken>;
    pub fn expand_span(&mut self, span: TokenSpan, table: MacroTable) -> Vec<ExpandedToken>;
}
```

Each field earns its place:

- `origins` is threaded as `&mut` through every entry point already. Owning
  it drops the caller's duty to keep the text alive beside the store.
- `reader` is reachable today only through `Includes`, so only for headers.
  Hoisting it is what makes the unsaved-buffer case work.
- `lexed` exists, scoped to one `Expander` and discarded after. Hoisting it
  ends the double lex: a file expanded and then parsed lexes once.

`add` lexes rather than leaving it to the first ask. That is what lets
`input` and `origins` both borrow shared — a view that might still have to
lex would take the session exclusively and lock the store out for as long as
it lived, and the callers that want both are the normal ones.

`expand_span` stays an explicit-table call. Expanding a fragment against
definitions assembled elsewhere is its whole purpose, the parser is its only
caller, and it is genuinely lower-level than the two modes.

Raw mode needs no reshaping. `scan(&Input) -> Scan` — every directive and
macro reference, flat and in source order, no text produced — is the right
answer and is already built.

### What this does not do

**`Input` cannot go private.** An earlier draft of this section said it
would. The parser names it in `build.rs`, `mod.rs` and `source.rs`, and
`source.rs` also takes `TokenSpan`, `Item`, `Region`, `Operands` and `scan`;
after §2 those are a separate crate's public surface by definition. What the
session changes is that nothing *builds* one any more — `Input::new` is
`pub(crate)` and `Preprocessor::input` is the only way to get one, which is
what removes the hand-rolled `origins + tokenize + Input::new` harness that
eleven test files each had their own copy of.

**No scan cache.** It was going to have one, to fix `Shapes::of` scanning the
same input twice. But `scan` has exactly two callers and both are those two
lines, twenty-five apart in one function, so the fix is a local variable and
a cache would have been a second speculative door. Kept as a note because the
reasoning generalises: a cache with one caller is a caller with a bug.

**No predefines.** `+define+` seeding is recorded in `limitations.md` as
waiting for the driver, which is what knows a filelist. The field belongs
here when that lands; a field nothing can fill is not a design.

**A held `Input` still blocks expansion.** `input` borrows the store shared
and `expand` needs it exclusively, so a caller that wants both reads what it
needs from the raw side first. `examples/conditionals.rs` does exactly that
now, and it is the same constraint §5 describes: the store hands out `&str`,
so it cannot be written to while anything is reading it. An `Arc<str>` buffer
would lift this as well.

Naming is open. `Preprocessor` in a crate called `svirig-preproc` stutters;
`Session` reads better and says less.

---

## 5. Parallelism

No work now. The section exists because the boundaries above decide how
much of it is available later, and two of them are cheap to keep open and
expensive to reopen.

File granularity is the answer; nothing finer is worth considering. The
largest file in the corpus lexes in 2.8 ms and builds its tree in 15.5 ms,
so there is nothing inside one file to split.

### The formatter shares nothing, by construction

D6 has the formatter never follow an `` `include ``: each file is
formatted alone. Taken for correctness, it also removes every cross-file
dependency, and the types already say so -- `Raw::new` takes an `Input`
and no store, while `Expanded::new` is the constructor that needs
`Origins`. So lex, scan, parse and format over one file touch no shared
state at all, and the formatter is a parallel map with nothing underneath
it. On the corpus figures in [`plan.md`](plan.md#8-open-questions) -- 5626
files, 53 MB, 1.9 s single-threaded -- that is a straight factor of the
core count, bounded by the disk.

### Expanded mode serialises on the compilation unit

Macro definitions hold from their definition to the end of the
*compilation unit* (22.3). Several files in one unit therefore cannot be
parsed independently: the table each file starts from is the one the
previous file left. Each file its own unit, and the dependency is gone.
That is the only real serialisation point, and it is a scheduling
constraint rather than a lock.

### What is worth sharing is the bytes, not the buffers

Buffers are not shareable in the first place: two `` `include ``s of one
header are deliberately two buffers with different `included_from`, so a
shared store buys no deduplication. What deduplicates is one level down --
the bytes read from a path. A `Reader` impl holding `path -> Arc<str>`
behind a mutex is the one genuinely shared structure, and §3 already puts
`Reader` in `svirig-text` where it can live.

That leaves `Origins` per thread, which is also the cheaper answer to a
problem the borrow checker poses and C++ does not: `Origins::text` returns
`&str` borrowed from `&self`, so it cannot be handed out from under a lock
guard. Sharing the store would mean `Arc<str>` buffers or a stable-address
arena. The pressure is already visible single-threaded -- `Expander.lexed`
holds `Rc<[Token]>` precisely because a slice of the map cannot be held
across a write to `origins`.

### Three constraints the restructuring should respect

- **`SyntaxNode` is `!Send`; `GreenNode` is `Send + Sync`.** `rowan`'s
  cursor holds a `NonNull<NodeData>` with non-atomic refcounts. A worker
  therefore returns a `GreenNode` and whoever consumes it calls
  `SyntaxNode::new_root` on its own thread. Not a limitation, but it fixes
  the worker's return type.
- **The reader is shared, so it has to be `Sync`.** A `Reader` impl that
  caches is the obvious thing to want, and written single-threaded it holds
  a `RefCell`, which is not `Sync`. The bound is on the trait, so that shows
  up at the `impl` rather than at a call site three layers away.
- **`Rc` in the session's lex cache makes the session `!Send`.** Fine if
  each worker constructs its own, which is the better shape anyway, but it
  is a choice rather than an accident: a session built centrally and handed
  to a worker needs `Arc`.

### Prior art

`slang` parallelises at file granularity through a thread pool, below a
threshold of four files where the pool does not pay for itself. Files that
are separate compilation units go through one parallel loop; the
single-unit path is sequential, for the 22.3 reason above, and library
files that inherit macros from that unit are deferred until it finishes --
the dependency is expressed in the schedule.

Its source manager takes the opposite trade to the one above: shared
across threads behind a `shared_mutex` covering nearly the whole class,
plus a second one for include directories. That is the C++ answer to the
borrow problem, since it hands out pointers into storage that a concurrent
push does not move.

One detail worth taking outright: results go into a pre-sized vector
indexed by file rather than pushed as they finish, so diagnostic order
does not depend on completion order. It matters little for a formatter and
a great deal for a linter.

---

## 6. Order

The API work comes first, inside the single crate. Splitting first would put
a crate boundary around a surface already known to be wrong, and single-crate
refactors are cheaper than path-dependency juggling. Afterwards the split is
close to `git mv`.

1. ~~Move file reading into `svirig-text` (§3).~~ **Done.** `Reader`, `Disk`
   and `clean` are in `svirig-text`; `Origins` has `load_included` and
   `include_depth`, and owns the check for a file already open. `Includes` is
   search paths and `search`, holds no reader, and has lost its lifetime.
   `expand` and `expand_span` take a `&dyn Reader` until step 2 gives it
   somewhere to live. No `Origins::load` for a file named directly: a driver
   holds that text already, and `add_file` takes it.
2. ~~Introduce the session type (§4); make `Input` and friends private.~~
   **Done, with one change.** `Preprocessor` owns the store, the reader, the
   search paths and the token cache; both modes take a `FileId`. `Input`
   stays public because the parser needs it, but `Input::new` is
   `pub(crate)`, so the session is the only thing that builds one. The double
   scan in `Shapes::of` is gone, by a local rather than by a cache.
3. Split `svirig-parse` out of `svirig-syntax`.
4. Split `svirig-preproc` out of `svirig-syntax`.
5. Update the crate table and the diagnostics line in `plan.md`; delete this
   file.

### Known costs

`kind.rs` carries one intra-doc link pointing forward at
`crate::preproc::DirectiveType`. rustdoc cannot link into a dependent crate
and `[workspace.lints.rustdoc] all = "deny"` makes that fatal, so it becomes
prose. That is the only cross-boundary reference in the would-be base crate.

Compile time is not a motivation and should not be claimed as one: the whole
of `svirig-syntax` checks from clean in 0.66 s.
