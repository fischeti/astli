# M5 — Crate APIs, config, polish

The working queue. Delete this file when M5 closes. The formatter's style and
design are in [`formatter.md`](formatter.md); what the API settles on goes in
[`api.md`](api.md).

## Now: revisit the crate APIs

The formatter is the first real caller. Each library crate stays usable on
its own, as `svirig preprocess` uses `svirig-preproc` without the grammar.

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
