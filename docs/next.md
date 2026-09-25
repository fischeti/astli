# M6 queue

The steps are in [`plan.md`](plan.md#m6-astli-files-and-astli-pickle).

- [x] **Tree builder for expanded mode.** Every corpus file's expanded tree is
  its `render` and maps each token back; 12.1% of grammar tokens are
  verbatim, mostly covergroups, constraints and assertions from headers.
- [x] **`astli-index` and `astli files`.** Found and fixed on the way: a
  macro default naming its own formal recursed forever.
- [ ] **Two attribute instances in a row** before a module leave it
  `VERBATIM`; `attributes` reads one. The index finds the declaration anyway,
  but the formatter leaves the module as written.
- [ ] **Raw pickle.**
- [ ] **Expanded pickle.** Expansion drops every directive, `` `timescale ``
  and `` `pragma `` included, so it has to write back the ones that change
  meaning downstream.
- [ ] **Encrypted files.** Expansion drops `` `pragma protect `` too, so it is
  found in the raw scan.
