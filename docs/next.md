# M5 — Crate APIs, config, polish

The working queue. Delete this file when M5 closes. The formatter's style and
design are in [`formatter.md`](formatter.md); what the API settles on goes in
[`api.md`](api.md).

The crate APIs are revisited; [`api.md`](api.md) has where they landed.

## Next

- **Configuration**, the knobs [D7](plan.md#4-decisions) names. Width and
  indent are a constant in `svirig-fmt/src/lib.rs`, alignment is always on,
  and `format` takes no options.
- **The shapes rules fall back on.** `PAREN_EXPR` 0.4%, `ARG_LIST` 0.3% and
  `CALL_EXPR` 0.1% are left unformatted where their rules give up: look at
  which shapes before writing more.
