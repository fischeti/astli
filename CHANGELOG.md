# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/fischeti/astli/compare/v0.1.1...v0.2.0) - 2026-09-28

### Added

- *(index)* trim and order filelists by the names files declare and use
- *(fmt)* [**breaking**] parse and lay out `timeunit` and `timeprecision`
- [**breaking**] parse clocking blocks
- [**breaking**] parse randcase
- [**breaking**] parse concurrent assertions
- [**breaking**] parse covergroups
- [**breaking**] parse bind directives
- [**breaking**] fix the misparses left in RTL
- [**breaking**] parse immediate and deferred assertions
- [**breaking**] parse constraint bodies
- *(parse)* build a tree from the expanded stream
- warn where the parser kept code as written
- *(fmt)* [**breaking**] line up an assignment pattern's values
- *(fmt)* [**breaking**] line up the `\`s of a `define
- *(fmt)* keep an item after `// astli-fmt: skip` as written
- *(fmt)* [**breaking**] align assignments, and cap the padding a cell takes

### Changed

- *(cli)* make sigil `+define+` and `+incdir+` visible in CLI help message

### Documentation

- the constructs 0.2 parses, and `// astli-fmt: skip`, in the README
- the installer script, uv and cargo-binstall in the README

### Fixed

- *(parse)* close the gaps Annex A shows in the new constructs
- *(parse)* read every attribute instance in a row
- *(fmt)* keep define comments, CRLF comments and one-line conditionals
- *(parse)* find conditional regions in one pass
- *(preproc)* expand a macro default outside the macro's formals
- *(parse)* bound how deep the parser recurses and the tree nests
- *(fmt)* line up a `define's `\`s past a line over the width
- *(fmt)* walk an operator chain in a loop, not once per operator
- *(fmt)* [**breaking**] stop deeply nested lists taking exponential time
- build on the declared MSRV, and pass the index tests on Windows
- *(cli)* describe the command as its package does

## [0.1.1](https://github.com/fischeti/astli/compare/v0.1.0...v0.1.1) - 2026-09-25

### Documentation

- *(astli)* inline each crate as a module page
- *(astli)* a tour and a map of the modules in the crate docs
- *(text)* a usage guide in the crate docs
- *(diag)* a usage guide in the crate docs
- *(syntax)* a usage guide in the crate docs
- *(preproc)* a usage guide in the crate docs
- *(parse)* a usage guide in the crate docs
- *(fmt)* a usage guide in the crate docs

### Performance

- *(fmt)* place comments only where a gap holds some
- *(cli)* allocate with mimalloc

## [0.1.0](https://github.com/fischeti/astli/releases/tag/v0.1.0) - 2026-09-24

The first release: a preprocessor, a lossless syntax tree, and a formatter in
the lowRISC style.
