# M6 queue

The steps are in [`plan.md`](plan.md#m6-astli-files-and-astli-pickle).

- [x] **Tree builder for expanded mode.** Every corpus file's expanded tree is
  its `render` and maps each token back; 12.1% of grammar tokens are
  verbatim, mostly covergroups, constraints and assertions from headers.
- [ ] **`astli-index` and `astli files`.**
- [ ] **Raw pickle.**
- [ ] **Expanded pickle.** Expansion drops every directive, `` `timescale ``
  and `` `pragma `` included, so it has to write back the ones that change
  meaning downstream.
- [ ] **Encrypted files.** Expansion drops `` `pragma protect `` too, so it is
  found in the raw scan.
