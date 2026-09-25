//! The SystemVerilog preprocessor: directives, macros, includes, and
//! conditionals.
//!
//! It reads a file in one of two modes. **Expanded** mode is what a compiler
//! sees: macros substituted, `` `include `` followed, `` `ifdef `` evaluated.
//! **Raw** mode is what a formatter sees: the file as written, with its
//! directives and macro calls found but nothing substituted, every branch of
//! a conditional kept, and no other file read. `astli-parse` builds its tree
//! in raw mode.
//!
//! Everything works on tokens, not text. A file is lexed once when it joins a
//! [`Session`], and from then on is addressed by token index through
//! [`TokenId`] and [`TokenSpan`], and by byte through
//! [`Span`](astli_text::Span).
//!
//! # Expanding a file
//!
//! A [`Session`] holds the files, and a [`Build`] the include directories and
//! `+define+`s they are expanded against.
//!
//! ```
//! use astli_preproc::{Build, Session, render};
//!
//! let build = Build::new().define("WIDTH", "8");
//! let mut session = Session::new().building(build);
//! let file = session.add(
//!     "top.sv",
//!     "`define MAX(a, b) ((a) > (b) ? (a) : (b))\n\
//!      `ifdef WIDTH\n\
//!      logic [`MAX(`WIDTH, 4)-1:0] q;\n\
//!      `endif\n"
//!         .into(),
//! );
//!
//! let expanded = session.expand(file);
//! let text = render(session.origins(), &expanded.tokens);
//! assert_eq!(text.trim(), "logic [((8) > (4) ? (8) : (4))-1:0] q;");
//! assert!(expanded.diagnostics.is_empty());
//! ```
//!
//! [`Session::add`] takes text in hand; [`Session::open`] reads a path. The
//! expansion is [`Expanded::tokens`], each with a kind and a span. [`render`]
//! writes them out with a space or newline added where two would otherwise
//! paste, which is also the text of the tree `astli-parse` builds from them.
//!
//! [`Expanded::macros`] is what is defined at the end of the file, headers
//! included, which is what `astli-parse` learns macro arities from. Each
//! [`Session::expand`] starts again from the build's definitions.
//!
//! # Includes
//!
//! `` `include "name" `` is searched for beside the including file, then in
//! each [`Build::include_dir`]; `` `include <name> `` only in each
//! [`Build::system_include_dir`]. Files are read through a
//! [`Reader`](astli_text::Reader), which is the disk for [`Session::new`] and
//! anything else for [`Session::reading`], such as an editor's unsaved
//! buffers:
//!
//! ```
//! use std::collections::HashMap;
//! use std::path::{Path, PathBuf};
//!
//! use astli_preproc::{Build, Session, render};
//! use astli_text::Reader;
//!
//! struct Buffers(HashMap<PathBuf, String>);
//!
//! impl Reader for Buffers {
//!     fn read(&self, path: &Path) -> Option<String> {
//!         self.0.get(path).cloned()
//!     }
//! }
//!
//! let buffers = Buffers(HashMap::from([(
//!     PathBuf::from("inc/defs.svh"),
//!     "localparam int W = 4;\n".to_string(),
//! )]));
//! let mut session = Session::reading(&buffers).building(Build::new().include_dir("inc"));
//! let file = session.add("top.sv", "`include \"defs.svh\"\n".into());
//!
//! let expanded = session.expand(file);
//! assert_eq!(render(session.origins(), &expanded.tokens).trim(), "localparam int W = 4;");
//! ```
//!
//! # Where a token came from
//!
//! A token's span is where it is *placed*: a macro's body is placed once for
//! each call, and each placement is a file of its own in
//! [`Origins`](astli_text::Origins), with the same text as the one it is
//! written in. That is how a diagnostic inside a macro can say which call it
//! came through. [`Origins::spelled`](astli_text::Origins::spelled) gives the
//! bytes as written, and [`Origins::trace`](astli_text::Origins::trace) the
//! calls that placed them, innermost first.
//!
//! ```
//! use astli_preproc::Session;
//!
//! let mut session = Session::new();
//! let file = session.add("top.sv", "`define W 8\nlogic [`W:0] q;\n".into());
//! let expanded = session.expand(file);
//!
//! let origins = session.origins();
//! let eight = expanded.tokens.iter().find(|token| origins.slice(token.span) == "8").unwrap();
//! assert_ne!(eight.span.src_id, file);
//! assert_eq!(origins.spelled(eight.span).src_id, file);
//!
//! let call = origins.trace(eight.span.src_id).next().unwrap().call;
//! assert_eq!(origins.slice(call), "`W");
//! ```
//!
//! # Diagnostics
//!
//! Expansion never fails. Each problem, such as an undefined macro or an
//! include that is not found, keeps the tokens around it, and is reported in
//! [`Expanded::diagnostics`]. To render one, hand
//! [`Session::origins`] to `astli-diag`, which follows the trace back to
//! what the user wrote.
//!
//! ```
//! use astli_preproc::Session;
//!
//! let mut session = Session::new();
//! let file = session.add("top.sv", "assign y = `UNDEFINED;\n".into());
//! let expanded = session.expand(file);
//! assert_eq!(expanded.diagnostics[0].code.as_str(), "undefined-macro");
//! ```
//!
//! # Raw mode
//!
//! [`Session::scan`] (or [`scan`], given an [`Input`]) finds each
//! [`Directive`] and [`MacroRef`] in a file, in order, and the [`MacroTable`]
//! of what the file defines.
//!
//! ```
//! use astli_preproc::{Arity, DirectiveType, Session};
//!
//! let mut session = Session::new();
//! let file = session.add(
//!     "top.sv",
//!     "`define LOG(msg) $display(msg)\ninitial `LOG(\"hi\");\n`uvm_info(\"T\", \"m\", LOW)\n".into(),
//! );
//! let scan = session.scan(file);
//!
//! let directive = scan.directives().next().unwrap();
//! assert_eq!(directive.ty, DirectiveType::Define);
//! assert_eq!(scan.macros.arity("LOG"), Arity::Formals(1));
//!
//! let calls: Vec<_> = scan.references().collect();
//! assert_eq!(calls.len(), 2);
//! assert_eq!(calls[1].args.as_ref().unwrap().len(), 3);
//! ```
//!
//! A reference's arguments depend on its macro's arity, which raw mode rarely
//! knows, since most definitions are in headers it does not read. For a macro
//! it has no definition of, a `(` on the same line opens an argument list:
//! that is how `` `uvm_info `` above got three. [`scan_seeded`] starts from a
//! table, such as [`Expanded::macros`], to know better.
//!
//! [`regions`] finds each `` `ifdef `` .. `` `endif `` in a stretch of tokens,
//! as a [`Region`] of [`Branch`]es, each with the condition it is taken
//! under.
//!
//! ```
//! use astli_preproc::{Session, Taken, regions};
//!
//! let mut session = Session::new();
//! let file = session.add("top.sv", "`ifdef SIM\na;\n`else\nb;\n`endif\n".into());
//! let input = session.input(file);
//!
//! let found = regions(&input, input.span(0..input.len()));
//! let [region] = found.as_slice() else { panic!() };
//! assert!(region.closed && region.has_else());
//! assert!(matches!(region.branches[0].taken, Taken::Defined(_)));
//! assert_eq!(region.branches[1].taken, Taken::Otherwise);
//! ```

mod build;
mod conditional;
mod diagnostics;
mod directive;
mod expand;
mod include;
mod macros;
mod session;
mod tokens;

pub use build::{Build, COMMAND_LINE};
pub use conditional::{Branch, Region, Taken, region, regions};
pub use directive::{Directive, DirectiveType, Formal, IncludePath, MacroDef, Operands};
pub use expand::{Expanded, ExpandedToken, render, spaced};
pub use macros::{Arity, Entry, MacroRef, MacroTable};
pub use session::Session;
pub use tokens::{Input, TokenId, TokenSpan};

use astli_syntax::SyntaxKind::*;

/// Preprocessor item recognized in a token stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A compiler directive (e.g. `` `define ``, `` `include ``).
    Directive(Directive),
    /// A macro reference or invocation.
    Macro(MacroRef),
}

impl Item {
    /// Returns the token span covered by this item.
    pub fn tokens(&self) -> TokenSpan {
        match self {
            Item::Directive(directive) => directive.tokens,
            Item::Macro(reference) => reference.tokens,
        }
    }
}

/// Results of a preprocessor scan over a file's tokens.
#[derive(Debug, Clone)]
pub struct Scan {
    /// Discovered directives and macro references in source order.
    pub items: Vec<Item>,
    /// Macro table accumulated across the scanned tokens.
    pub macros: MacroTable,
}

impl Scan {
    /// Iterates over all compiler directives found in the scan.
    pub fn directives(&self) -> impl Iterator<Item = &Directive> {
        self.items.iter().filter_map(|item| match item {
            Item::Directive(directive) => Some(directive),
            Item::Macro(_) => None,
        })
    }

    /// Iterates over all macro references found in the scan.
    pub fn references(&self) -> impl Iterator<Item = &MacroRef> {
        self.items.iter().filter_map(|item| match item {
            Item::Macro(reference) => Some(reference),
            Item::Directive(_) => None,
        })
    }
}

/// Scans `input` for compiler directives and macro references using an empty initial macro table.
pub fn scan(input: &Input) -> Scan {
    scan_seeded(input, MacroTable::new())
}

/// Scans `input` starting with pre-defined macros from `seed`.
pub fn scan_seeded(input: &Input, seed: MacroTable) -> Scan {
    let mut items = Vec::new();
    let mut macros = seed;
    let len = input.len();
    let mut at = 0u32;

    while at < len {
        if input.kind(at) != TICK_IDENT {
            at += 1;
            continue;
        }

        let item = match DirectiveType::lookup(input.text(at)) {
            Some(name) => {
                let directive = directive::parse(name, input, at);
                macros.apply(input, &directive);
                Item::Directive(directive)
            }
            // Consult the macro table at the current position to resolve arity.
            None => Item::Macro(macros::parse(input, at, len, &macros)),
        };

        // Advance cursor past the processed item, guaranteeing forward progress.
        at = item.tokens().end.max(at + 1);
        items.push(item);
    }

    Scan { items, macros }
}
