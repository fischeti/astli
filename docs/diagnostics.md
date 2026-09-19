# Diagnostics

> **Status:** WIP plan, with §6 step 1 landed -- the `svirig-text` types exist
> and nothing produces one yet. The rest is decided-on-paper and should be read
> as such: it is written down because the design questions were settled and the
> answers are worth more than the conversation that settled them.

Every stage below the driver currently recovers from bad input in silence. The
lexer turns a byte no rule matches into a `LEX_ERROR` token, the parser drops
what it cannot read into a `VERBATIM` node, and the preprocessor has thirteen
separate recoveries that [`limitations.md`](limitations.md#the-expanded-path-reports-no-errors-only-recoveries)
lists one by one. None of them says anything. This is the layer that lets them
speak.

[`plan.md`](plan.md#diagnostics) already decided the shape at the top level —
the data is a `svirig-text` type, the rendering is `svirig-diag` — and [the
crate table](plan.md#crates) has `svirig-diag` arriving "with the first thing
that reports one". What follows is the rest of it.

---

## 1. What a diagnostic is

One concrete struct, in `svirig-text`, which keeps its empty `[dependencies]`.

```rust
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Code,
    pub message: String,
    /// Where the message points.
    pub at: TokenOrigin,
    /// Further places that explain it. Not the expansion chain -- see §4.
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
}

pub struct Label { pub at: TokenOrigin, pub message: String }

pub enum Severity { Error, Warning, Note, Help }

pub struct Code(pub &'static str);
```

### It carries a `TokenOrigin`, not a `Span`

This is the one decision that is specific to this project rather than to
diagnostics in general. `Origins::trace` and `Origins::reported_at` exist so
that a message about a token produced by a macro points at the `` `FOO `` the
author wrote rather than at the inside of a body they cannot see. A `Diagnostic`
holding a bare `Span` forces every producer to resolve that itself, and throws
the chain away before anything can render it.

`TokenOrigin::written(span)` is the raw-mode case, so one field serves both
modes and raw mode pays nothing for a facility it does not use.

### The message is a `String`, and that is forced

`svirig-text` is the bottom crate and knows nothing of `SyntaxKind` — by
[D12](plan.md#4-decisions) the vocabulary lives in `svirig-syntax`, which sits
*beside* it rather than under it. So a diagnostic cannot hold `expected:
SyntaxKind` without inverting the graph, and the message has to arrive
pre-rendered.

That costs nothing in practice because of where messages are built (§2), and
`keyword::text` already turns a kind into the text to quote.

### Codes

`Code` is a newtype over `&'static str` rather than an enum, because an enum
would have to live in `svirig-text` and enumerate every problem every crate
above it can have — the layering inverted again, this time for a string. The
constants are declared beside the constructors that use them, so a code and its
wording cannot drift apart, and a unit test per crate asserts they are unique.

What a code is *for*: an LSP that wants a stable identifier, and a future
`--deny` that names one. Neither exists, which is why this is one field and not
a lint-level system.

---

## 2. Where a diagnostic comes from

### One currency type, not one per crate

The driver has to merge what the preprocessor and the parser produced into one
list, order it, cap it and render it the same way. Two types means boxing them
behind a trait or a third enum wrapping both, and `svirig-diag` becoming generic
over a distinction it has no reason to care about.

### A catalogue per producing crate

What each crate gets instead is one module listing everything it can say:

```rust
// crates/svirig-preproc/src/diagnostics.rs
const UNDEFINED: Code = Code("undefined-macro");

pub(crate) fn undefined_macro(name: &str, at: TokenOrigin) -> Diagnostic { … }
pub(crate) fn include_not_found(path: &Path, at: TokenOrigin) -> Diagnostic { … }
```

Thirteen of those map one-to-one onto the recovery table in
[`limitations.md`](limitations.md#the-expanded-path-reports-no-errors-only-recoveries),
which is the point: the table stops being a list of things that are not done and
becomes a list of functions.

This is also what makes §1's "the message is a `String`" free. The constructor
lives in the crate that *can* see the vocabulary, so `svirig-parse` formats a
`SyntaxKind` into its own message and `svirig-text` never learns what one is.

**Rejected: a per-crate `enum PreprocError` behind a trait in `svirig-text`.**
It is a second representation that is converted to the first immediately, and
the exhaustiveness it buys is a test either way. Worth revisiting only if a
consumer needs to match structurally on which problem occurred, which `Code`
covers for now.

### Three producers, three mechanisms

The mechanism differs because what each producer leaves behind differs.

| Producer | Mechanism | Why |
| --- | --- | --- |
| Lexer | none | `LEX_ERROR` is already a token kind and reaches the tree. The diagnostic is *derived* by whoever holds the store. |
| Parser | a `Vec<Diagnostic>` on `Events`, truncated by `Snapshot` | Rollback. |
| Preprocessor | a sink on `&mut Session` | A recovery leaves no trace anywhere else. |

**The lexer cannot report and should not.** `svirig-syntax` takes `svirig-text`
as a dev-dependency only, deliberately: "a `Token` carries bare offsets into the
text it was lexed from". So it cannot construct a `Diagnostic` even if it wanted
to. The existing boundary gives the right answer for free — a `LEX_ERROR` in the
tree is the durable record, and turning one into a message is a tree walk.
`VERBATIM` is the same, except that it is a successful recovery rather than a
problem and should stay one.

**The parser's constraint is rollback.** [D2](plan.md#4-decisions) buys
speculative parsing with a snapshot-and-truncate over the event list, and a
diagnostic emitted down a path that is then undone is a message about something
that did not happen. The event list already has exactly the machinery:
`precedes` is a side `Vec` whose length is a field of `Snapshot` and which
`rollback` truncates. Diagnostics are the same shape — a fourth field, truncated
alongside. Rollback correctness costs no new invariant, and a rule that
speculates, fails, and hands over to `verbatim` drops its complaint on the way,
which is what you want.

**Rejected: an `Event::Diagnostic` variant.** It would put a `String` and two
`Vec`s into an enum that is pushed once per token, and a diagnostic carries its
own location so it needs no slot in the list.

**The preprocessor is the only one that needs new plumbing**, which is the
argument for doing it first (§6). Its recoveries are invisible by construction —
that is the complaint `limitations.md` records. `Session::expand` is already
`&mut self`, so the sink needs no interior mutability.

### Severity and the two modes

The same recovery is a mistake in one mode and ordinary in the other: an
undefined macro is an error when expanding and the common case in raw mode,
where a file is read alone and most of its macros are defined elsewhere.

This falls out rather than needing a policy layer. `scan` is `&self` and
therefore cannot report; `expand` is `&mut self` and can. Raw mode's silence
stops being an accident and becomes something the signatures state.

**Revisit when** something raw mode genuinely should report turns up and is not
already expressible as a token kind in the tree.

---

## 3. The dependency graph

```
                  svirig-text          ← Diagnostic lives here, no dependencies
                 ↑     ↑     ↑
    svirig-preproc     │     svirig-diag   (+ ariadne)
           ↑           │          ↑
      svirig-parse ────┘          │
           ↑                      │
        svirig  ──────────────────┘
```

`svirig-diag` sits **beside** the producers, not above or below them: it is a
sink for a vocabulary type, not a stage in the pipeline. The mermaid diagram in
[`plan.md`](plan.md#data-flow) should hang it off `svirig-text` for the same
reason, or it reads as though parsing feeds a renderer.

### The producers do not depend on `svirig-diag`, for free

`svirig-parse` and `svirig-preproc` both already depend on `svirig-text`, so
putting `Diagnostic` there adds **no new edge to the graph**. A consumer who
wants a tree and nothing else gets a `Vec` it can ignore: no crate to compile,
three words of stack, and no allocation until something is actually wrong.

### There is no sink trait

Parameterising the producers over a `trait DiagnosticSink` would buy nothing,
because the dependency worth avoiding is *ariadne* and the split above already
avoids it. It would cost: a second type parameter on `Parser<T>`, which is
already generic over its token source, and therefore on every rule signature
across `expr.rs`, `decl.rs`, `item.rs` and `stmt.rs` — to abstract over a
`Vec::push`. It also models a push-based pipeline that does not exist; the
parser accumulates and returns, it does not call out.

The pattern earns its keep when a producer would otherwise take a heavy
dependency, or when the sink must intercept — an early abort after N errors, a
stream to an editor. If the first of those is ever wanted it is a `max_errors`
field on `Session`, not a trait.

---

## 4. Rendering

`ariadne`, at 0.6, two dependencies (`yansi`, `unicode-width`).

`annotate-snippets` and `codespan-reporting` were the alternatives and either
would do; `codespan-reporting`'s `Files` trait is the closest fit to `Origins`,
having exactly the four methods it already answers. `miette` was rejected on two
grounds: with `fancy` it pulls 101 transitive crates including ICU, against a
workspace with five direct dependencies; and it is a *protocol* first, built
around returning one error through `Result`, where this accumulates many
alongside a successful parse.

The choice is cheap to revisit precisely because §1 put the data in
`svirig-text`. Swapping renderers touches one crate.

### The `Cache` impl is where `Origins` plugs in

`Cache::fetch` returns `&Source<Self::Storage>`, so the cache owns the
`Source`s. Two consequences:

- `Source<I: AsRef<str>>` is generic over its storage and `&str` satisfies it,
  so `Source<&'a str>` borrows out of `Origins` and copies no text.
- `Source` builds its own line table, and its `Line` is four `usize`s against
  the four *bytes* a line costs in `Origins`. **Build them lazily, inside
  `fetch`**, so that only a file which actually carries a diagnostic pays.
  `ariadne`'s own `From<I> for Source` warns that it "can be expensive for long
  strings. Use an implementor of `Cache` where possible."

`(FileId, Range<usize>)` satisfies `ariadne`'s `Span` through its blanket impl
for tuples; `FileId` already derives everything that asks for.

### Byte offsets are not character offsets

`ariadne`'s `Span::start`/`end` are **character** offsets from the start of the
buffer, and `get_line_range` binary-searches the char-offset table. `Span` here
is bytes. Handing bytes over directly does not merely shift a caret sideways:
because the offset is file-wide, one multi-byte character anywhere earlier puts
every later diagnostic on the wrong *line*. A UTF-8 copyright header is enough,
and real files have them.

The conversion belongs in `svirig-diag`, which is another reason `Diagnostic`
holds its native byte spans and stays innocent of the renderer. It is O(line)
rather than O(file), using `ariadne`'s own tables:

```rust
let (line, _, byte_col) = source.get_byte_line(span.start as usize)?;
let char_col = source.get_line_text(line)?[..byte_col].chars().count();
let start = line.offset() + char_col;
```

Worth a test with a non-ASCII comment above the error, because getting it wrong
is silent.

### The chain is derived, not stored

The expansion chain is a pure function of `at` and the store, so `svirig-diag`
walks `trace()` at render time. Storing it in the `Diagnostic` would duplicate
derivable state and make every producer responsible for remembering to fill it
in. `labels` is only for locations the producer knows that the chain does not —
"macro defined here", "opened here, never closed".

The presentation-independent half of `svirig-diag` — resolving through
`reported_at`, ordering, deduplicating, capping per file, flattening the chain —
is the bulk of it and is what a future LSP backend reuses; that one wants
`relatedInformation`, not a terminal. `ariadne` is the last stretch of the
crate, which is the argument for it being a crate at all rather than a module of
the driver.

**Not feature-gated yet.** Two dependencies against an `--all-features` matrix
is a poor trade for a second consumer that does not exist. **Revisit when**
`svirig-lsp` exists and minds.

---

## 5. The line table stays eager

[D10](plan.md#4-decisions) built the line table eagerly and deferred the
question, saying "the query pattern that settles the design — a diagnostics
layer, or an editor — does not exist yet". This is that layer, so the question
is due.

The answer is that it stays as it is, and in particular that it should **not**
move into a `logos` callback.

Measured on 1.3 MB of synthetic RTL, the size of the largest file in the corpus
(45.8k lines, 445k tokens), best of 40 runs, release:

| Building the table | |
| --- | --- |
| Current, `bytes().enumerate().filter()` | 0.44 ms |
| `memchr_iter` over the buffer | 0.28 ms |
| Per-token, every token | 2.01 ms |
| Per-token, only tokens that can hold a newline | 0.30 ms |

So a *well-written* callback — firing only on `WHITESPACE` and `BLOCK_COMMENT`,
the two rules whose regexes admit a newline — lands about where `memchr` does,
and saves 0.14 ms against a file that takes 19.7 ms to parse. A naive one,
firing on every token, is four and a half times slower than what is there now.
The figure is synthetic and on one machine; it wants re-measuring against the
real file before anyone acts on it.

The performance case is therefore thin, and three structural ones point the
other way:

- **It inverts the graph.** The table lives in `Origins`, `logos` lives in
  `svirig-syntax`, and `svirig-syntax` deliberately does not depend on
  `svirig-text`. Either the bottom crate takes a lexer dependency — which is
  the premise §3 rests on — or the table moves up and `Origins` can no longer
  answer `line_col` for a buffer it holds.
- **It makes `line_col` partial.** A buffer can be added and not yet lexed, and
  a diagnostic *about* lexing needs a line before the lex finishes. An editor
  asking where byte 4000 is should not have to trigger a parse.
- **A file is lexed more than once.** Raw and expanded mode share a cache, but a
  callback-built table has to be built exactly once and neither rebuilt nor
  appended to on a second pass — state to get wrong for no gain.

There is a fourth argument from the other direction. `` `line `` is a
*preprocessor* directive whose effect on reported numbers is known only once
directives are read, and [`limitations.md`](limitations.md#the-line-directive-does-not-move-the-line-numbers-we-report)
already names this layer as a reason to honour it. Whatever eventually remaps
those numbers has to sit where the directives are, above the lexer — so binding
the table to lexing would put it below the one thing that needs to adjust it.
`ariadne` has `Source::with_display_line_offset`, which covers a single
directive at the top of a generated file and not several through the middle of
one; the real fix is the sorted per-buffer list `limitations.md` describes.

Newlines here are not tokens of their own: `WHITESPACE` is `[ \t\r\n]+`, so one
token spans many, and a block comment spans more. A callback would be counting
newlines inside slices, which is the same scan fragmented, not a scan avoided.

**If the 0.16 ms `memchr` saves is ever wanted**, it is the honest way to take
it: one dependency, one line, no layering change. At 2% of a parse it is not
currently worth the first dependency in a crate that has none.

---

## 6. Sequencing

Not the formatter. It degrades to `VERBATIM` and has almost nothing to report,
so building this for M4 ships a crate whose only caller prints nothing. `svirig
preprocess` is the first consumer that has something to say, and it says the
thing users most want to hear.

1. ~~`Diagnostic`, `Severity`, `Label`, `Code` in `svirig-text`.~~ *Done.*
   Builders are by value (`error(..).label(..).note(..)`), matching
   `Session::searching`. `Severity` is declared in increasing order so that
   `Ord` means "more severe" and the worst of a run is a `max`.
2. The sink on `Session`, and `crates/svirig-preproc/src/diagnostics.rs`. Each
   of the thirteen recoveries gains an emit and **keeps its recovery** — no
   behaviour changes, which is what `limitations.md` already predicts.
3. `svirig-diag`: resolution and ordering, then the `ariadne` backend.
4. The driver: print after the file's output, fold into the exit code, cap per
   file and say how many were suppressed.
5. The parser's side vec, when a rule first genuinely cannot proceed. `expr.rs`
   already carries the comment marking the spot.

Closing this out also closes the entries in [`limitations.md`](limitations.md)
whose **Revisit when** is this layer: the unreported expanded path, the escaped
identifier at end of file, the unpaired conditional directive, and the
`` `line `` directive that does not move the numbers we report.

## 7. The gate

The corpus is 5626 files of well-kept code, which makes a false-positive gate
nearly free and unusually strong:

- **Raw mode over the corpus produces zero diagnostics.** Anything else is a bug
  in this layer, not in the corpus.
- **Expanded mode produces exactly the undefined macros that a missing include
  path explains**, a number that must fall as `-I` handling improves. That is a
  figure of the kind [`preprocessor.md`](preprocessor.md#the-oracle) already
  keeps, and it is falsifiable.

Snapshot the `Diagnostic` **values**, and keep rendered-output snapshots to a
handful of smoke tests — one of them a two-level macro chain, which is the case
nothing else gets right. `ariadne` is a third party with its own release
cadence, and corpus fixtures should not churn on someone else's glyphs.

## 8. Open questions

- Does anything want a diagnostic that raw mode alone can see and that a token
  kind cannot already express? If not, §2's `&self`/`&mut self` split is the
  whole severity story and no policy layer is needed.
- Where does the per-file cap belong — `svirig-diag`, which knows how much is
  worth printing, or the driver, which owns opinions about a terminal? The
  second matches [`plan.md`](plan.md#which-parts-the-driver-owns), but the cap
  has to happen before rendering to be worth anything.
- Is `Code` enough for a future `--deny`, or does that want a hierarchy
  (`preproc::undefined-macro`) so a whole crate's worth can be named at once?
