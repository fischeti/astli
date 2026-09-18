# svirig

SystemVerilog language tooling in Rust. Exploratory; the first product is a
formatter.

## Where things are

- [`docs/plan.md`](docs/plan.md) — objectives, architecture, decisions,
  milestones. Read this before doing design work.
- `docs/next.md` — the working queue for the milestone in progress, when there
  is one. Transient: it is written when a milestone opens and deleted when it
  closes, so its absence means no milestone is half-finished.
- [`docs/api.md`](docs/api.md) — the shape of the public API, and why. Read
  this before changing what a crate exposes.
- [`docs/preprocessor.md`](docs/preprocessor.md) — the directive and macro
  design.
- [`docs/grammar-coverage.md`](docs/grammar-coverage.md) — what the parser
  handles so far.
- [`docs/limitations.md`](docs/limitations.md) — deliberate gaps and shortcuts,
  and what would justify closing each one. Add to it rather than leaving a
  `TODO` in the code.
- The crates, bottom up. `svirig-text` — spans, the file store, reading a
  file. `svirig-syntax` — `SyntaxKind`, the lexer, the `rowan` tree types.
  `svirig-preproc` — directives, macros, includes. `svirig-parse` — the
  grammar. `svirig` — the driver binary, one subcommand per stage. Every
  library crate carries the `svirig-` prefix; the driver is the bare name.
- `scripts/fetch-corpus.sh` — populates the gitignored `corpus/`.

Under `reference/`, gitignored:

- `1800-2023.pdf` — the specification. **Never commit or redistribute it.**
- `slang/` — the C++ state of the art. Consult it when a design question has a
  non-obvious answer; don't transliterate it, and don't carry over its type
  names.

**Nothing under `reference/` may be named in committed code**, in comments or
otherwise. It is gitignored, so to anyone reading the crate those names point
at nothing. Give the reason instead of the citation — "SystemVerilog reuses its
punctuation" rather than "slang does it this way". The `docs/` files are the
exception: prior art belongs in a design document.

## Conventions

- Nothing is published and nothing depends on this. Break any API, rename
  anything, delete anything. Do not add deprecation shims, compatibility
  aliases, or migration paths.
- Comments: be concise, and explain why rather than what. A comment has to make
  sense to someone who never saw the session that produced it — no answers to
  questions I asked, no "as discussed", no narrating what changed.
