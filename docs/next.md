# M5 — Crate APIs, config, polish

The working queue. Delete this file when M5 closes. The formatter's style and
design are in [`formatter.md`](formatter.md); what the API settles on goes in
[`api.md`](api.md).

## Now: revisit the crate APIs

The formatter is the first real caller. Each library crate stays usable on
its own, as `svirig preprocess` uses `svirig-preproc` without the grammar.

0. **Fold `TokenOrigin` into `Span`.** A token's origin is a span plus the
   expansion that placed it. Interning each (buffer, expansion) pair as its
   own `FileId` makes it a plain `Span` with the buffer's own offsets, so
   `Diagnostic`, `Label`, `ExpandedToken` and the parser take one location
   type and `ExpansionId` goes behind `Origins`. `FileId` then names a view
   rather than a file, and may want renaming.
1. **Move `Build` onto `Session`.** The driver keeps include directories and
   `+define+`s in its own `Build` and seeds definitions by lexing a
   `<command-line>` buffer (`svirig/src/session.rs`), a step every caller of
   expanded mode would repeat. `api.md` already names this shape.
2. **Let transparency reuse the tree's session.** `transparency::check` opens
   a second `Session` and copies and lexes the input again, though the
   `SyntaxTree` holds it lexed. It could take the tree and add only the
   output.
3. **Trim `SyntaxTree`.** `session()`, `file()` and `into_session()` have no
   caller but the `metrics` example, and `origins()` none but a snapshot test.
   Keep what a single-file tool needs; send the rest through tier 2.
4. **Trim `svirig-parse`'s exports.** `Raw`, `Tokens`, `Position`,
   `Expanded` and the `*Shape` types are used only by the `metrics` example,
   and `parse` only inside the crate.
5. **Decide what `ast` is for.** No crate calls the typed views: `rules.rs`
   dispatches on `SyntaxKind` as it walks elements in order, 114 times. Find
   the places a rule looks up a child by kind or position, and either move
   them onto typed views or record that `ast` exists for the shape gate
   ([D16](plan.md#4-decisions)) alone.

## Later

- **Configuration**, the knobs [D7](plan.md#4-decisions) names. Width and
  indent are a constant in `svirig-fmt/src/lib.rs`, alignment is always on,
  and `format` takes no options.
- **The shapes rules fall back on.** `PAREN_EXPR` 0.4%, `ARG_LIST` 0.3% and
  `CALL_EXPR` 0.1% are left unformatted where their rules give up: look at
  which shapes before writing more.
