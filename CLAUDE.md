# astli

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
- [`docs/formatter.md`](docs/formatter.md) — the style the formatter produces,
  and the printer behind it.
- [`docs/grammar-coverage.md`](docs/grammar-coverage.md) — what the parser
  handles so far.
- [`docs/limitations.md`](docs/limitations.md) — deliberate gaps and shortcuts,
  and what would justify closing each one. Add to it rather than leaving a
  `TODO` in the code.
- The crates, bottom up. `astli-text` — spans, the file store, reading a
  file. `astli-diag` — rendering a diagnostic, with the macro and include
  chain that explains it. `astli-syntax` — `SyntaxKind`, the lexer, the
  `rowan` tree types, and `ast`, the typed views generated from `astli.ungram`.
  `astli-preproc` — directives, macros, includes. `astli-parse` — the
  grammar. `astli-fmt` — the formatter. `astli` — the umbrella, re-exporting
  each library crate as a module. `astli-cli` — the driver, a binary named
  `astli`, one subcommand per stage.
- `scripts/fetch-corpus.sh` — populates the gitignored `corpus/`.

Under `reference/`, gitignored:

- `1800-2023.pdf` — the specification. **Never commit or redistribute it.**
- `lowrisc-verilog-style.md` — the style the formatter targets, from
  `VerilogCodingStyle.md` in `github.com/lowRISC/style-guides`.
- `slang/` — the C++ state of the art. Consult it when a design question has a
  non-obvious answer; don't transliterate it, and don't carry over its type
  names.

**Nothing under `reference/` may be named in committed code**, in comments or
otherwise. It is gitignored, so to anyone reading the crate those names point
at nothing. Give the reason instead of the citation — "SystemVerilog reuses its
punctuation" rather than "slang does it this way". The `docs/` files are the
exception: prior art belongs in a design document.

## Conventions

- Published on crates.io at 0.x, and all crates share one version. A breaking
  change is fine, and goes in the next minor version. Do not add deprecation
  shims, compatibility aliases, or migration paths.
- Comments: be concise, and explain why rather than what. A comment has to make
  sense to someone who never saw the session that produced it — no answers to
  questions I asked, no "as discussed", no narrating what changed.
