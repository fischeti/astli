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
- [ ] **Directives as trivia in expansion** (D20).
- [ ] **Name tokens in `astli-index`**, with `Summary` built on them and a
  tree written with names replaced.
- [ ] **Expanded pickle.**
- [ ] **Raw pickle.**
- [ ] **Encrypted files.** With D20, `` `pragma protect `` survives
  expansion, but the envelope's body still lexes as code.
