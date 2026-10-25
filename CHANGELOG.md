# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/fischeti/astli/compare/v0.3.0...v0.4.0) - 2026-10-25

### ⚠️ Breaking

- **cli**: fmt writes in place
- **fmt**: parse defparam, gates and UDPs
- **fmt**: parse checker, config, extern modules and wait_order
- **fmt**: parse specify blocks
- **fmt**: a macro call may write an inside list
- **fmt**: a directive among struct members is the struct's
- **fmt**: parse min:typ:max parameter values
- **fmt**: parse drive and charge strengths
- **syntax**: reserve the keywords `begin_keywords` names
- **parse**: not-parsed is an error in expanded mode
- **fmt**: parse a match in a second pair of parentheses
- **sema**: gates, unnamed instances and bind in the tree
- **index**: a name declared twice and used is an error
- **sema**: an elaboration is owned and shared, and types know their declarations
- **sema**: `multiple-drivers` is `check`'s, and width policy is the linter's
- **sema**: hierarchical names resolve, and selects are compared
- **package**: dev-dependencies and optional dependencies, resolved with features
- **package**: dependencies from the resolver, not bender
- **package**: overrides replace a dependency's files
- **package**: features, selected once for a design
- **package**: dev files a dependent never sees
- **cli**: fmt, lint, check and files read the package with no file named
- **cli**: `--diagnostics` is `--diag`

### ✨ Added

- describe a dependency without a manifest

#### check

- `--top` checks what the tops build

#### cli

- group the commands in help by what they work on
- diagnostics as GitHub Actions annotations
- names, versions and paths styled in messages
- a problem at a line is a diagnostic
- astli init
- astli patch
- files orders a discovered package
- transfer counts from gix
- progress while fetching
- problems with notes and help
- status lines
- --color, and one writer for stderr
- astli tree
- astli snapshot
- `astli cache clean` and `astli cache dir`
- `astli fetch`
- group subcommands under help headings

#### lint

- `width-expand` and `width-trunc`, from what `--top` builds

#### package

- find a package's root and read its `astli.toml`
- dependencies from either manifest
- locate dependencies through bender
- tell headers from units
- walk a package's sources

#### parse

- parse an import in a block or a subroutine

#### preproc

- `__LINE__` and `__FILE__` follow `line
- predefine the coverage constants
- find an include by unique path suffix

#### resolve

- choose versions with PubGrub
- fetch dependencies into a shared cache
- sealed checkouts, and `astli path`
- locked repositories fetched in parallel
- a lockfile, and `astli update`

#### sema

- the instance tree, and `astli elab`
- types
- evaluate constants
- values

#### syntax

- lex triple-quoted strings

### 🐛 Fixed

#### parse

- a bare forward typedef names its type in a declarator
- a `?` after a spaced base is a digit

#### preproc

- a pasted /* starts a comment
- an `include` name a macro call makes takes its arguments

#### sema

- know the built-in std package

#### syntax

- the end of the text ends an escaped identifier

### 📚 Documentation

- fix markdown list formatting and polish snapshot docstrings
- restructure the site around what astli is
- update README
- a pattern's key is one keyword or a type's name
- improve wording of documentation
- reword the CLI help messages

## [0.3.0](https://github.com/fischeti/astli/compare/v0.2.0...v0.3.0) - 2026-10-07

### ⚠️ Breaking

- **fmt**: set the width, indent and max padding in astli.toml
- **fmt**: keep an item as written by attribute, not comment
- **fmt**: parse new obj, a shallow copy
- **fmt**: parse type() of a type, and as an operand
- **fmt**: parse an interface class extending several
- **fmt**: parse a for header declaring variables of several types
- **fmt**: parse assignments in parentheses
- **fmt**: parse and lay out pattern matching
- **fmt**: parse and lay out tagged union expressions
- **index**: keep encrypted files and what they need whatever the tops
- **preproc**: keep the directives a compiler still needs in an expansion
- **fmt**: parse a generate block named before its begin
- **fmt**: parse a macro call standing for whole case items
- **fmt**: parse a type as an assignment pattern key
- **fmt**: break a long for header at its semicolons
- **index**: the name tokens of a tree, and its text with them renamed

### ✨ Added

#### cli

- astli pickle
- --diagnostics short, one line per diagnostic
- astli check
- read -j from ASTLI_JOBS
- astli.toml, lint levels for a project and per path

#### diag

- colour the labels and show one snippet per file

#### lint

- astli lint, with always-ff-non-blocking and always-comb-blocking
- the rest of verible's default rules for lowRISC
- multiple-drivers
- undriven-signal
- disable-statement
- unused-signal, unused-parameter, unused-import
- variable-initializer
- report a waiver a later one on the same construct replaces
- lowRISC's own rules, and verible's signal and port naming
- verible's rules for instances and generate blocks
- verible's restriction rules, and forbid-defparam
- verible's naming rules, in the lowrisc group
- duplicate-case-item, case-missing-default and always-comb
- waive rules with (* astli_allow *)

#### parse

- parse with [range] in a stream
- parse let declarations
- parse specparam outside specify
- parse nettype and interconnect
- parse randsequence
- report a number's base with no digits

#### preproc

- hand a session's origins over
- keep an include not found as trivia
- report malformed directives and unreadable text

#### sema

- lower a tree to a HIR
- what each name use reads and writes
- check a design's names and instances
- resolve names across a design

### 🐛 Fixed

#### cli

- define a macro for lint with --define, since -D denies

#### fmt

- a hanging line of a verbatim run goes no further right than its start

#### parse

- read a delay's value as a primary, not an expression
- read repeat before an assignment's event control
- report what a header's rule leaves unparsed
- an import item is one package and one name
- read a function's or a task's ports declared in its body
- read min:typ:max delays
- keep a cross bin's `matches` count out of its selection

#### preproc

- drop a paste at the edge of a macro argument
- look a pasted macro name up once it is whole
- an argument may call the macro it is passed to

#### sema

- read a net's delay

### ♻️ Changed

#### cli

- derive the preprocessor's Build from the resolved lists
- declare the lint level flags on Levels
- make Out an alias for the buffered stdout

#### syntax

- move is_net_type onto SyntaxKind

### 📚 Documentation

- astli lint, its waivers and astli.toml
- a documentation site on GitHub Pages

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
