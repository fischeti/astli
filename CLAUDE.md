# svirig

SystemVerilog language tooling in Rust. Exploratory; the first product is a
formatter.

## Where things are

- [`docs/plan.md`](docs/plan.md) — objectives, architecture, decisions,
  milestones. Read this before doing design work.
- [`docs/preprocessor.md`](docs/preprocessor.md) — the directive and macro
  design.
- [`docs/grammar-coverage.md`](docs/grammar-coverage.md) — what the parser
  handles so far.
- `crates/svirig-syntax/` — lexer, preprocessor, parser, CST. Every crate
  carries the `svirig-` prefix.
- `scripts/fetch-corpus.sh` — populates the gitignored `corpus/`.

Under `reference/`, gitignored:

- `1800-2023.pdf` — the specification. **Never commit or redistribute it.**
- `slang/` — the C++ state of the art. Consult it when a design question has a
  non-obvious answer; don't transliterate it, and don't carry over its type
  names.
- `rdlfmt/` — my SystemRDL formatter. The module docs in
  `src/syntax/parser/mod.rs` and `src/formatter.rs` are the design brief for
  the trivia and whitespace models here.

## Conventions

- Nothing is published and nothing depends on this. Break any API, rename
  anything, delete anything. Do not add deprecation shims, compatibility
  aliases, or migration paths.
- Comments: be concise, and explain why rather than what. A comment has to make
  sense to someone who never saw the session that produced it — no answers to
  questions I asked, no "as discussed", no narrating what changed.
