# astli

SystemVerilog language tooling in Rust. Exploratory; the first product is a
formatter.

## Where things are

- [`docs/plan.md`](docs/plan.md) — objectives, architecture, decisions,
  milestones. Read this before doing design work.
- GitHub milestones and issues — the working queue. The open milestone is the
  one in progress, an issue per step: `gh issue list --milestone "<title>"`.
- [`docs/api.md`](docs/api.md) — the shape of the public API, and why. Read
  this before changing what a crate exposes.
- [`docs/preprocessor.md`](docs/preprocessor.md) — the directive and macro
  design.
- [`docs/formatter.md`](docs/formatter.md) — the style the formatter produces,
  and the printer behind it.
- [`docs/sema.md`](docs/sema.md) — semantic analysis, and the lint and
  check built on it.
- [`docs/packages.md`](docs/packages.md) — discovering a design without a
  filelist, and the package model it grows into.
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
  grammar. `astli-fmt` — the formatter. `astli-index` — the top-level names
  files declare and use, and which files a design needs. `astli-sema` — the
  HIR a tree lowers to, and what it means. `astli-lint` — lint rules over one
  file's tree. `astli-package` — finding a package's root and reading its
  `astli.toml`. `astli-resolve` — fetching dependencies into the shared
  cache, and choosing their versions. `astli` — the umbrella, re-exporting each library crate as a
  module but `astli-resolve`, which, like `astli-package`, serves the driver
  and promises others no API. `astli-cli` — the
  driver, a binary named `astli`, one subcommand per stage.
- `scripts/fetch-corpus.sh` — populates the gitignored `corpus/` and
  `sv-tests/`. `scripts/sv-tests.py` scores astli against the latter.
  `scripts/consts-oracle.py` holds a repository's constant values to
  `slang`'s, through `pyslang`, `scripts/tree-oracle.py` its instance
  trees, and `scripts/check-oracle.py` what `check` reports on a real
  `bender` build.
- `site/` — the user documentation on GitHub Pages, a Zensical project;
  `scripts/site.sh` builds it, generating the command and lint rule
  references from the CLI.

Under `reference/`, gitignored:

- `1800-2023.pdf` — the specification. **Never commit or redistribute it.**
  `1800-2023.txt` is its text, and `1800-2023-annex-a.txt` its formal syntax
  alone (`pdftotext -layout`, page headers stripped): check a grammar rule
  against Annex A rather than memory, and search the full text for the rest.
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

- Issues hold work that is planned or in progress; `docs/` holds what is true
  now, and why. Progress is never committed, and the docs a change touches
  are the ones its code makes stale. An idea worth keeping that no milestone
  plans is an issue without one.
- Work lands on `main` through a pull request from its own branch, never a
  push. The description says `Closes #N` for each issue the PR finishes, so
  merging closes it, and `Refs #N` for one it only advances. PRs are
  rebase-merged, so each commit lands as it is: fold a fixup into the commit
  it fixes rather than stacking one that undoes it.
- Published on crates.io at 0.x, and all crates share one version. A breaking
  change is fine, and goes in the next minor version. Do not add deprecation
  shims, compatibility aliases, or migration paths.
- A commit that changes what `astli fmt` writes for input it already formatted
  is breaking (`feat(fmt)!:` and so on), since it fails `astli fmt --check` in
  CI after an upgrade. A fix is not, if the old output was wrong: not
  transparent, not idempotent, or no longer parsing.
- Breaking is measured against the last release, not the last commit: a
  change to something no release has shipped yet is a plain `feat:` or
  `fix:`, since no user can depend on it.
- Comments: be concise, and explain why rather than what. A comment has to make
  sense to someone who never saw the session that produced it — no answers to
  questions I asked, no "as discussed", no narrating what changed.
