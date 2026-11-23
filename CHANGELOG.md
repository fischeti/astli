# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.7.0](https://github.com/fischeti/astli/compare/v0.6.1...v0.7.0) - 2026-11-23

### ⚠️ Breaking

- **package**: own runs enable the default features, and a top table names its own
- **fmt**: leave it to the printer that nothing breaks inside brackets
- **fmt**: align every broken list as a module's ports are
- **package**: sources take glob patterns
- **package**: feature and dev patterns add to the sources

### ✨ Added

#### cli

- group the subcommands in help by what they are for
- keep each unit's macro calls and skipped branches in the server's snapshot
- hand the server what check and lint report of each unit, and whether the dependencies were read
- keep each unit lowered across a design's loads, while what it read is unchanged
- the loader reads through a `Reader` and hands its problems to its caller
- list each lint rule's options under it
- set a `-G` on the named tops that declare it

#### lint

- `unused-signal` and `unused-parameter` take the names they never report as `ignore`
- drop a fix pass that groups an expression otherwise
- a pattern takes any number of `*`
- add comparison-parentheses
- add mixed-operator-parentheses

#### lsp

- fix all runs every pass of `lint --fix`
- keep the build each unit was expanded with in its snapshot
- keep what a design's tops build in its snapshot, made once
- go to definition, from a lookup of what stands at a position
- publish each open file's diagnostics, from its buffer and from its design
- watch the files a design is found from, and find it again
- load the design each open file is in, as a snapshot swapped in whole
- format a buffer under its package's `[fmt]`
- the language server's loop, behind `astli lsp`
- a code action that waives a lint finding
- inlay hints
- fade the branches the build did not take
- semantic tokens say what each name is
- find references and document highlights
- lint fixes as code actions
- hover lays out an expansion as a type, an expression, a statement or a list
- hover writes a value by its type
- hover, what the name or the macro call under the cursor is
- workspace symbols from each loaded design's index
- document symbols and folding ranges from the buffer's raw tree

#### package

- `dev-features` are on in a package's own runs alone

#### preproc

- list a file's expansions, what each call wrote, and the branches skipped

#### sema

- find what stands at a place of a file

#### text

- list every expansion an origins recorded
- a text's line table, for a column counted in other units

### 🐛 Fixed

#### cli

- start `git` without the repository a hook or `rebase --exec` names
- fail `test --list` for a file it could not read
- build a relative `-I` from where it was given
- tell a problem once in test --list, whatever the tables define
- repeat every flag of a failed test's run in the command that repeats it
- keep a Code Quality record when it is told, not when rendered
- write an include directory named twice once for files named too
- end `files` at a one-file list's diagnostics, with no tally
- write a placed problem's names in backticks, as a diagnostic does
- say one way that the dependencies come from a Bender.yml
- snapshot a package without `[dependencies]` unless a Bender.yml lists them
- warn once of each list `build` and `test` read
- warn in `check` in the order of the list
- leave a stub's headers out of what `check` searches as the list
- report a file's lint findings as one list in line order

#### fmt

- keep the space before a type argument's dimensions

#### lint

- size what a function's or a task's body holds
- leave unsized what a constant condition rules out
- size a parameter's default and an override of it
- size what a function returns against its return type
- size an argument against its subroutine's input
- size a compound assignment's value
- size what a body evaluates wherever it stands
- count a name an opaque type spells as one another file may use

#### package

- mark a mirror's feature mistake as any other in its manifest
- leave unread what only a root reads of a dependency

#### parse

- report a case item without its `:`
- nest brackets and braces in an undefined macro's arguments
- let a block comment spanning lines keep an undefined macro's arguments
- report a second edge written as a pair at its `(`
- one table says what opens and closes what
- read `matches` inside a cross bin's selection

#### preproc

- say why an include that is there cannot be read
- a token the lexer cannot read in an argument is reported
- a name a paste joins nothing to is expanded first

#### resolve

- fail on missing credentials instead of prompting

#### sema

- bound the generate blocks a walk enters, not those built
- check a design as deep as elab shows it
- a named type with dimensions is as wide as both
- an include before a definition stands in the scope around it
- a struct's member through a type parameter is no name
- type a connection by `.name` or `.*`
- type what a body evaluates wherever it stands
- find a handle's or a base's class through what a class inherits
- bind into a definition, not a nested one spelled alike
- hold a `bind`'s definition before `:` to be declared
- give a parameter a run asks for a step budget of its own
- step a genvar as a constant function's statement
- say how an access writes, and report a package's net assigned

### ♻️ Changed

#### cli

- a load writes to any writer, and a build compares and hashes
- build the one top build names without a loop over several
- pass the package a build is shown under to it
- make test's points from the lists that build them
- write test's JUnit report in a module of its own
- declare the flags build, run and test build with once
- read each file once to find the tests it marks
- read test and sweep marks apart from a summary
- split lockfile.rs into the package chosen and the lock's policy
- ask loading whether a description applies in `tree`
- render pickle's left includes against the origins it holds
- write a marked span uncoloured as the library does
- write every format --diag promises besides pretty in one module
- ask whether files were named through Sources::named
- merge the build flags in one place
- edit a manifest through one type
- say in one place which units each command reports
- end a run with one tally, left out of a document in one place
- carry what a file rendered and its errors as one `Report`
- lint each file read on its own one way
- make each file's lint levels once
- write a design's filelist through one Listing
- move what selects a design's files out of `files`
- summarize a unit for a list in one function
- check the tops a build lists through the same loop
- analyse a package's designs through one loop
- set `-G` one way, through `Flags`
- choose a run's tops outside its `Plan`
- model a chosen top once, as a `Top`
- split several designs into its two meanings
- keep what a run shares in a Run it passes
- hand each command the package it acts on

#### diag

- give a problem's header the diagnostics' palette
- a resolved diagnostic carries its place
- resolve diagnostics through `resolve_all` alone

#### fmt

- headers that are not scopes write their own lists
- a scope lays out its header from its parts
- give a trailing comment a column of its own
- write as inside brackets through one scoped helper
- build the aligned-or-indented layout once
- say once that an unlabelled block's end shares its line
- scan a declaration's type and names once
- wrap a node's comments around a rule once
- tell a declarator and an assignment the column of their cell
- break a scope's header where the scope lays it out
- say once how a header lays out each of its lists
- split the rules into a module per area

#### index

- `Summary::new` takes the origins, not the session
- drop the test and sweep marks from `Declaration`

#### lint

- read a file's waivers once, apart from lint
- Linter takes what is built, and runs the built rules too
- a rule's layer is one enum, Layer
- keep a fix's grouping by construction, not a check
- drop `Config::level`, which nothing asks
- the width rules size the conversions the list gives
- the width rules recurse into what the HIR lists
- read the package a member is declared in off its scope
- share what the driver and use rules leave unjudged
- run every kind of rule through one loop, and report one way

#### package

- load what a resolution chose in astli-resolve
- walk what a design reaches in one place
- name what a package requires directly apart from what it needs
- fetch a mirror and read its manifests in one place each
- give a repository's origin the commit chosen of it
- resolve with the roots' pins and patches as one argument
- keep a requirement's path as written, joined where used
- check a manifest once, as a Manifest a Package holds
- split what only a root reads off Package
- keep what a run decides off Package
- keep the patch file's name private
- name the keys of a dependency's entry beside its reader
- say which manifest lists a package's dependencies
- carry a requirement's submodules and LFS as one Takes
- say once what a manifest requires with features enabled

#### parse

- make enveloped one method, from a position
- check the nesting once, in any
- take a name and a label through Parser
- put decline and terminated on Parser
- move the checks of a finished tree to checks.rs
- check a UDP table's symbols as its row is parsed
- read the scope only to choose the rule for an entry

#### preproc

- find an include through one search over a reader
- yield each include with its written name
- export the raw scan the parser and the formatter read, and document it
- remove the conditionals example and Session::expand_span
- validate a directive apart from expanding it
- carry whether a walk is defining on its frame
- split a frame into the scope it resolves in and its placement

#### resolve

- check out through a resolution alone

#### sema

- check what the tops build in one call, check_built
- say in children() whether an instance's definition is certain
- say where an undeclared name was looked for
- say how a parameter was declared in one kind
- export what other crates use, and the HIR by name
- typing checks the conversions the list gives
- list each conversion in a frame once
- typing recurses into what the HIR lists
- access recurses into what the HIR lists
- resolve recurses into what the HIR lists
- the HIR lists what each statement and expression holds
- mark the scope an include not followed stands in opaque
- lowering marks a scope that may declare anything
- give loose names an expression of their own
- give each enum an id its members point at
- file the instances `bind`s add with the elaboration
- look a name up through scopes in one `Design` function
- give a bound instance the scope holding its `bind`
- decide a `bind`'s target once, in the design
- take a literal at a type in one place
- keep the string and bits conversions with the values
- lower a literal to what it means, beside its text
- split what passes to an instance or a top in one place
- take a hierarchical name's head off in one place
- add Cx::new for a context outside any run or walk
- merge Queries into Elaboration
- name a constant function's run Run, not Frame
- type what no top reaches over a Built of its defaults
- keep the frames Built makes in one list, each with its parent
- record the scope a symbol is declared in
- find what encloses a scope in one walk
- store the parameters an instance may override

#### syntax

- export the lexer through `tokenize` alone

#### text

- drop `Severity::Help`, which nothing reports

### 📚 Documentation

- the README and the site say what the release brings

## [0.6.1](https://github.com/fischeti/astli/compare/v0.6.0...v0.6.1) - 2026-11-17

### ✨ Added

#### cli

- pass a test that fails with the text it should
- give every run of a profile arguments of its own
- write the elaborated design as JSON from elab

#### lint

- add async-reset-style and forbid-async-reset
- add always-ff-registers-only
- add one-bit-comparison

#### text

- read files from a map of paths to text

### 🐛 Fixed

#### cli

- let a lint flag win over a [lint.paths] pattern
- bound every thread a run uses by -j
- write a failing test's output with the progress bars hidden
- report pickle's warnings through term and fail on errors

#### parse

- parse every body through one loop and its own scope
- read a conditional branch alone, inside a fence at its end

#### sema

- bind arguments to formals in one place
- compare an interface's drivers within each top

### ♻️ Changed

#### cli

- name a build's inputs and its directory once
- build and run through one simulator backend

#### lint

- size operators as sema does

#### package

- key a profile's simulator flags by simulator

#### syntax

- read what runs before else in one place
- spell an operator from the kind's own text
- find a child token in one place
- iterate a node's significant tokens
- read an identifier as a name in one place

### 📚 Documentation

- say what is true now in the design docs

## [0.6.0](https://github.com/fischeti/astli/compare/v0.5.2...v0.6.0) - 2026-11-15

### ⚠️ Breaking

- **cli**: read a path in a profile's args from the manifest
- **cli**: check and lint every [top.*] table without --top
- **cli**: print tree's directories from the workspace root, or the package's
- **cli**: leave a stub of `extern` declarations out of the list
- **cli**: report `check`'s findings in path dependencies
- **cli**: trace nothing by default, and take `--trace` to turn it on
- **preproc**: an `include is not searched for beside its file
- **package**: set a top's parameters from its [top.NAME] table and -G
- **fmt**: format the files the walk finds, without the header scan
- **package**: stop the walk at a nested `Bender.yml`

### ✨ Added

#### cli

- draw a spinner while a top builds or a test runs
- check and lint --tests together with --top
- pass what follows -- to every run of astli test
- drop astli_plusargs and astli_timeout
- a test's own timeout as astli_timeout, in place of [top.NAME] timeout
- run each test point once per astli_plusargs entry
- a JUnit report of astli test, and each member's builds named
- run each test point under several random seeds
- a test's point as a top, and check --tests over every point
- sweep a test's parameters, each point built and run on its own
- astli test, building and running the tests a package marks
- bender export --workspace writes or checks every package's Bender.yml
- sort what check reports in a package by path
- name the workspace root's manifest in a described dependency's note
- tell a workspace package only the pin warnings of its design
- run `--workspace` outside a workspace on the one package
- read no design from a virtual workspace root, and warn of its tables
- `--offline` never clones or fetches
- choose a workspace package with `-p`, and run on every one
- snapshot into each workspace package that requires it, pinning at the root
- share one lock, its pins, patches and build directory across a workspace

#### index

- read whether a declaration carries astli_test

#### package

- read a test timeout from [test] and [top.NAME]
- find the workspace a package is in, and its members

#### parse

- say that a parameter in a parameter list takes no attribute
- read a sequence or an event passed as an argument
- read a property `case`, and an `if` property in parentheses
- read `default` among a constructor's arguments, from SystemVerilog 2023
- read `ref static` ports, from SystemVerilog 2023
- parse `alias`
- parse a type parameter's restriction
- read a soft union

#### preproc

- test an expression of names in `ifdef, `ifndef and `elsif

#### resolve

- fetch a git requirement's LFS files, and say what a checkout left out
- check out the submodules a git requirement takes

#### sema

- set a top's parameters to values of their types
- an array method may name its index method as well as its iterator

### 🐛 Fixed

#### cli

- let files --top write its list past a file the tops do not reach
- go on past a file that cannot be read
- read a filelist line as a shell splits it
- report a missing nested filelist at the line that names it
- leave a path dependency only a patch names to its authors
- count each package an error told before fails under `check --workspace`
- read a `//` inside a filelist's path as part of the path
- build no stale model when a source changes within a second
- elab reads the package and its tops as check does

#### lint

- proper-parameter-declaration leaves a test's body parameters alone
- size a conditional by its wider arm in width-expand

#### parse

- read `const ref` declaring a subroutine's port in its body
- read a method defined through an interface port, `task a.Read`
- read a non-ANSI port joining signals, and `bus.mp y;`
- take attributes between a call's name and its arguments
- take a direction before a modport's `.name(expr)`
- continue a type parameter in a parameter port list
- read a checker instance named through $unit::
- read a delay on a net of a user-defined nettype
- keep a based literal after a delay out of the delay's value
- count an assignment pattern's '{ when finding a brace group's end

#### preproc

- keep a string escape in stringified text
- end a directive in a macro body at its next line continuation

#### resolve

- name a Windows path's mirror after its last component

#### sema

- report a parameter without a default that is left unset
- find a package's checker through an import and in a `bind`
- instantiate a definition declared inside another
- report a constant index outside an array of constants
- evaluate an enum's methods in a constant
- tell apart the elements of an interface array a module drives
- take a packed struct or union as equivalent to its bits
- give an untyped parameter each instance's own type
- leave an overridden parameter's default unchecked
- look up an object's `randomize` arguments in its class
- declare pattern variables in what their match guards
- refuse a hierarchical name where a constant is needed
- lower `[*]` as an associative array, not a dynamic one
- type a conditional of arrays, and `const'(x)`
- type a literal two-state unless it holds an `x`
- type `shortreal` as its own type
- type a division, a modulus and a power four-state
- type a comparison of two-state operands `bit`
- a cross declares CrossValType and CrossQueueType
- a non-ANSI port list may name one port twice

#### syntax

- lex an encrypted envelope whole, and let it close what is open

### ⚡ Performance

- type and evaluate each expression once per walk of a body

#### cli

- walk and lower a file once for every package that builds it alike
- resolve a workspace once under --workspace
- one analysis pipeline for check and lint
- stream files to threads instead of running them in waves

#### package

- lex only the files that write an `include

#### parse

- walk the finished tree only for what it holds

### ♻️ Changed

- drop the public items nothing calls

#### cli

- check and list several tops apart from building them
- split a build into a check per set of defines and a build per top

### 📚 Documentation

- bring the documents up to what is built

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
