# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.2](https://github.com/fischeti/astli/compare/v0.5.1...v0.5.2) - 2026-11-09

### 🐛 Fixed

#### resolve

- lock a mirror only while writing to it

## [0.5.1](https://github.com/fischeti/astli/compare/v0.5.0...v0.5.1) - 2026-11-08

### ✨ Added

#### cli

- warn where a file list finds another header than check did
- note a bind a dependency without a manifest brings in

#### index

- a summary's includes say what each searched for

#### lint

- `explicit-static`, an initialized static variable in a procedural block
- `always-ff`, an edge-triggered `always` in place of `always_ff`

#### sema

- `check --top` reports elaboration-time `$error` and `$fatal`

### 🐛 Fixed

#### cli

- with a top, an include a dependency cannot resolve is an error
- an elaboration-time `$error` or `$fatal` fails the build
- a `#` word comments out the rest of a filelist line
- a tally of one file says "is" and "has"

#### fmt

- preserve whitespace after token pasting ([#306](https://github.com/fischeti/astli/pull/306))

#### lint

- what does not parse is an error with a design
- the lifetime rules skip DPI imports and exports

#### parse

- accept parameters on a class scope in a type
- accept macro package imports ([#305](https://github.com/fischeti/astli/pull/305))
- fall back on an assignment with no right-hand side

#### preproc

- report a branch after a region's `else`

### ⚡ Performance

#### cli

- search the list's include directories once per header, in parallel

#### lint

- tree rules share one walk of the file

#### package

- walk a package's directories on every core

#### sema

- find a node's span from its ends, not every token under it
- elaborate what the tops build on every core first
- type the frames the tops build in parallel
- compare a variable's drivers without a search per pair
- share an elaboration across threads without one lock

### ♻️ Changed

#### astli

- stop re-exporting astli-package

#### fmt

- name what `movable` tracks of a `define`

#### syntax

- lex each line as code, a directive's or a define's

## [0.5.0](https://github.com/fischeti/astli/compare/v0.4.0...v0.5.0) - 2026-11-05

### ⚠️ Breaking

- **lint**: --fix applies the fixes findings carry
- **parse**: read a weight's `:/` as a `:` and a `/` that touch
- **sema**: lower constraints, `let` and deferred assertions
- **sema**: lower covergroups and `with` clauses
- **fmt**: parse a modport's imported and exported subroutines
- **sema**: lower concurrent assertions, properties and sequences
- **fmt**: indent a statement after a comment ending the line before it
- **fmt**: a trailing comment does not count towards the width
- **sema**: report patterns, streams, selects and defaults a type refuses
- **sema**: follow `defparam` and chosen `bind`s into the tree
- **sema**: see through generate blocks, instances and interfaces
- **sema**: see classes through forward typedefs and typedefs
- **sema**: lower `unique`, `solve … before …`, rise and fall delays, and `new obj`
- **sema**: resolve members through a class handle
- **sema**: lower and resolve class bodies
- **sema**: see through interface ports
- **sema**: evaluate `$sformatf`
- **sema**: give dynamic arrays and queues a value
- **sema**: evaluate `inside` and `case … inside`
- **sema**: a dimension's bound is an `int`
- **sema**: evaluate time literals
- **sema**: lower modports and clocking blocks
- **cli**: `files` fails when a file has errors
- **cli**: `-D` defines a macro in `lint` too

### ✨ Added

- a Claude Code plugin with a skill for astli

#### cli

- `astli add` and `astli remove`
- `astli build` with Verilator
- `astli bender import` and `astli bender export`
- say what a top needs defined in `[top.NAME]`
- `astli run`
- skip a fresh build, and `astli clean`
- lint --top runs its design rules only on the files the tops reach
- check --top reports only the files the tops reach
- compare the dependencies of `Bender.yml` and `astli.toml` with `init --bender --check`
- leave the files `[fmt] exclude` names unformatted
- write a GitLab Code Quality report with `--diag gitlab`
- write diagnostics as JSON lines with `--diag json`

#### lint

- naming rules take the style a project names
- a project extends a style a team shares

#### package

- build `release` on four threads, and drop `opt`
- read `[build]` and `[profile]`

#### preproc

- report a malformed `timescale` or `unconnected_drive`

#### sema

- an `extern module` declares a definition
- report a misplaced `return`, and a function that waits
- refuse a select of a real, by one, and its edge
- hold enums to their type, and their members' values
- report a net a procedure assigns, and a name declared twice
- report a name used before its declaration
- check reports what typing rejects
- type the expressions in classes
- type the expressions outside classes

### 🐛 Fixed

#### cli

- list each package's files together in `files`
- note described dependencies only in `files` and `tree`
- check --top refuses a name declared twice in a dependency

#### index

- accept a class two files declare

#### package

- an exclude naming a directory matches the files under it

#### parse

- leave a statement's body unparsed at a branch's end
- sampled value functions take a clocking event
- read a cast in a dimension as an expression

#### preproc

- join what a run of `` `` `` pastes stands between
- an empty macro argument takes its formal's default
- a conditional in a macro body tests the name its arguments make

#### resolve

- fetch each mirror once a run
- clean the cache directory the environment names

#### sema

- declare the nets a connection's expression reads
- give a pattern a condition chooses the type it is assigned to
- declare the nets a concatenation in a connection names

### 📚 Documentation

- serve the site and the installer from astli.dev
- polish documentation guides and CLI messages for recent features
- acknowledge Cargo and uv
- polish documentation guides and CLI messages for recent features

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
