# Restructuring

> **Status:** proposal. Nothing here is built. It supersedes the crate table
> in [`plan.md`](plan.md#crates), which sketches a `svirig-lexer` and folds
> the parser into `svirig-syntax`; §2 argues that split runs the wrong way.

Two questions, asked together because their answers depend on each other:
where the crate boundaries go, and what the preprocessor's entry points look
like. The boundary question is cheap — nothing is published, so any API may
break at any time — and the entry-point question is the one with content.

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
| `` `include `` reached while expanding | preproc: `resolve` → `Files::read` → `add_included` | preproc |

The root file and an included file take different roads into the same store.
An editor can install a `Files` impl serving unsaved buffers and have it
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

- the `Files` trait and `Disk`
- `clean()`, textual path normalisation, which exists to serve the cycle check
- a new `Origins::load(&dyn Files, path, included_from) -> Option<FileId>`,
  so that text has exactly one door in
- `reenters` and `depth`, today methods on `Expander` but written entirely in
  terms of `include_trace` and `path`

Staying in `svirig-preproc`:

- `Includes { quoted, angle }` and `search()` — the candidate list and the
  order it is tried in, which is the part the standard specifies. It produces
  paths and no longer reads anything or mutates the store.
- `MAX_DEPTH`, since the fifteen-level floor is 22.4 as well, though the check
  that consults it becomes a store query.

Not to be changed while in there: two `` `include ``s of one header correctly
produce two buffers with different `included_from`. `load` must not dedup by
path.

---

## 4. One entry point per mode

The state that has to outlive a single call — and be shared between the two
modes — gets owned rather than re-threaded:

```rust
pub struct Preprocessor<'a> {
    origins: Origins,
    files: &'a dyn Files,
    includes: Includes,       // search paths only, after §3
    predefined: MacroTable,   // +define+FOO=bar
    lexed: FxHashMap<FileId, Rc<[Token]>>,
}

impl Preprocessor<'_> {
    pub fn load(&mut self, path: &Path) -> Option<FileId>;
    pub fn add(&mut self, path: &Path, text: String) -> FileId;
    pub fn define(&mut self, text: &str);
    pub fn tokens(&mut self, file: FileId) -> Rc<[Token]>;
    pub fn scan(&mut self, file: FileId) -> &Scan;
    pub fn expand(&mut self, file: FileId) -> Vec<ExpandedToken>;
    pub fn origins(&self) -> &Origins;
}
```

Each field earns its place:

- `origins` is threaded as `&mut` through every entry point already. Owning
  it drops the caller's duty to pre-register the root file, and lets `load`
  treat the root exactly as it treats a header.
- `files` is reachable today only through `Includes`, so only for headers.
  Hoisting it is what makes the unsaved-buffer case work.
- `lexed` exists, scoped to one `Expander` and discarded after. Hoisting it
  ends the double lex.
- `predefined` has no home at all: `expand` hardcodes `MacroTable::new()`.
  Any driver reading a filelist needs `+define+`.

Both modes then take a `FileId` and nothing else, which is the symmetry §1 is
missing. `Input`, `TokenId` and `TokenSpan` stop being public surface and
become how the crate talks to itself.

`expand_span` stays an explicit-table call. Expanding a fragment against
definitions assembled elsewhere is its whole purpose, the parser is its only
caller, and it is genuinely lower-level than the two modes.

Raw mode needs no reshaping. `scan(&Input) -> Scan` — every directive and
macro reference, flat and in source order, no text produced — is the right
answer and is already built. The session only gives its result somewhere to
live.

Naming is open. `Preprocessor` in a crate called `svirig-preproc` stutters;
`Session` reads better and says less.

---

## 5. Order

The API work comes first, inside the single crate. Splitting first would put
a crate boundary around a surface already known to be wrong, and single-crate
refactors are cheaper than path-dependency juggling. Afterwards the split is
close to `git mv`.

1. Move file reading into `svirig-text` (§3) and give `Origins` a `load`.
2. Introduce the session type (§4); make `Input` and friends private.
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
