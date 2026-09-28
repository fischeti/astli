# M6 queue

The steps are in [`plan.md`](plan.md#m6-astli-files-and-astli-pickle).

- [x] **Tree builder for expanded mode.** Every corpus file's expanded tree is
  its `render` and maps each token back; 12.1% of grammar tokens are
  verbatim, mostly covergroups, constraints and assertions from headers.
- [x] **`astli-index` and `astli files`.** Found and fixed on the way: a
  macro default naming its own formal recursed forever.
- [x] **Two attribute instances in a row** before a module left it
  `VERBATIM`. Each instance is now its own node, and one on a design unit
  gets its own line.
- [x] **Directives as trivia in expansion** (D20). A macro in the operands
  has to expand, since its definition is not kept.
- [x] **Name tokens in `astli-index`**, with `Summary` built on them and a
  tree written with names replaced. Renaming needs two sites the index never
  read: end labels, and a `bind`'s target, now a reference.
- [x] **Expanded pickle.** slang reports the same for `cheshire` with
  `DefaultCfg` pickled as from its filelist, but for two cva6 trace headers
  its manifest lists as sources, which pickling writes twice.
- [x] **Raw pickle.** Renaming was dropped from it: 238 names in `cheshire`
  are written by macros, most `pkg::` in typedef macros.
- [ ] **Encrypted files.** With D20, `` `pragma protect `` survives
  expansion, but the envelope's body still lexes as code.
