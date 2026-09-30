# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/fischeti/astli/compare/v0.2.0...v0.3.0) - 2026-09-30

### Added

- *(fmt)* [**breaking**] keep an item as written by attribute, not comment
- *(lint)* astli lint, with always-ff-non-blocking and always-comb-blocking
- *(cli)* astli pickle
- *(cli)* --diagnostics short, one line per diagnostic
- *(diag)* colour the labels and show one snippet per file
- *(fmt)* [**breaking**] parse new obj, a shallow copy
- *(fmt)* [**breaking**] parse type() of a type, and as an operand
- *(parse)* parse with [range] in a stream
- *(parse)* parse let declarations
- *(parse)* parse specparam outside specify
- *(parse)* parse nettype and interconnect
- *(fmt)* [**breaking**] parse an interface class extending several
- *(parse)* parse randsequence
- *(fmt)* [**breaking**] parse a for header declaring variables of several types
- *(fmt)* [**breaking**] parse assignments in parentheses
- *(fmt)* [**breaking**] parse and lay out pattern matching
- *(fmt)* [**breaking**] parse and lay out tagged union expressions
- *(index)* [**breaking**] keep encrypted files and what they need whatever the tops
- *(preproc)* [**breaking**] keep the directives a compiler still needs in an expansion
- *(preproc)* report malformed directives and unreadable text
- *(fmt)* [**breaking**] parse a generate block named before its begin
- *(fmt)* [**breaking**] parse a macro call standing for whole case items
- *(lint)* the rest of verible's default rules for lowRISC
- *(parse)* report a number's base with no digits
- *(fmt)* [**breaking**] parse a type as an assignment pattern key
- *(fmt)* [**breaking**] break a long for header at its semicolons
- *(index)* [**breaking**] the name tokens of a tree, and its text with them renamed
- *(lint)* variable-initializer
- *(lint)* report a waiver a later one on the same construct replaces
- *(lint)* lowRISC's own rules, and verible's signal and port naming
- *(lint)* verible's rules for instances and generate blocks
- *(lint)* verible's restriction rules, and forbid-defparam
- *(lint)* verible's naming rules, in the lowrisc group
- *(lint)* duplicate-case-item, case-missing-default and always-comb
- *(lint)* waive rules with (* astli_allow *)
- *(cli)* read -j from ASTLI_JOBS
- *(cli)* astli.toml, lint levels for a project and per path

### Changed

- *(syntax)* move is_net_type onto SyntaxKind
- *(cli)* derive the preprocessor's Build from the resolved lists
- *(cli)* declare the lint level flags on Levels
- *(cli)* make Out an alias for the buffered stdout

### Documentation

- astli lint, its waivers and astli.toml
- a documentation site on GitHub Pages

### Fixed

- *(parse)* read repeat before an assignment's event control
- *(parse)* report what a header's rule leaves unparsed
- *(preproc)* look a pasted macro name up once it is whole
- *(preproc)* an argument may call the macro it is passed to
- *(parse)* read a function's or a task's ports declared in its body
- *(parse)* read min:typ:max delays
- *(parse)* keep a cross bin's `matches` count out of its selection

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
