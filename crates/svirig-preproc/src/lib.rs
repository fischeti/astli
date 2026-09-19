//! Preprocessing: directives, macros, includes, conditionals.
//!
//! Meant to be usable on its own -- the Rust ecosystem has no good
//! SystemVerilog preprocessor -- so nothing in here reaches for a grammar.
//! It reads tokens and answers questions about them; what the tokens *mean*
//! is `svirig-parse`'s question.
//!
//! [`scan`] is the one pass everything else is built on: it reads a file's
//! tokens once, forwards, splitting every `` `name `` into a
//! [directive] or a [macro reference](macros) and building the
//! [`MacroTable`] as it goes. [`Session::expand`] is the expanded mode built
//! on it, following `` `include ``s and evaluating conditionals as it goes;
//! [conditional] is the region structure raw mode reads the same directives
//! as. See `docs/preprocessor.md`.
//!
//! Both modes hang off [`Session`], which holds what they share: the store,
//! the reader, and each file's tokens.

pub mod conditional;
pub mod diagnostics;
pub mod directive;
pub mod expand;
pub mod include;
pub mod macros;
pub mod session;
pub mod tokens;

pub use conditional::{Branch, Region, Taken, region, regions};
pub use directive::{Directive, DirectiveType, Formal, IncludePath, MacroDef, Operands};
pub use expand::{Expanded, ExpandedToken, render};
pub use include::Includes;
pub use macros::{Arity, Entry, MacroRef, MacroTable};
pub use session::Session;
pub use tokens::{Input, TokenId, TokenSpan};

use svirig_syntax::SyntaxKind::*;

/// One thing the preprocessor recognises in the token stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Directive(Directive),
    /// A `` `name `` that names no directive, so it names a macro.
    Macro(MacroRef),
}

impl Item {
    /// The tokens this item covers.
    pub fn tokens(&self) -> TokenSpan {
        match self {
            Item::Directive(directive) => directive.tokens,
            Item::Macro(reference) => reference.tokens,
        }
    }
}

/// What one pass over a file found.
#[derive(Debug, Clone)]
pub struct Scan {
    /// Every directive and macro reference, in source order and without
    /// overlap.
    pub items: Vec<Item>,
    /// The macro table as it stands at the end of the file.
    ///
    /// No conditional has been evaluated, so this is *every* `` `define `` in
    /// the file rather than the ones that survive some particular set of
    /// definitions. That is what raw mode wants -- it has to shape the calls in
    /// every branch -- and it is why a name defined differently in two branches
    /// comes back as [`Arity::Unknown`].
    pub macros: MacroTable,
}

impl Scan {
    pub fn directives(&self) -> impl Iterator<Item = &Directive> {
        self.items.iter().filter_map(|item| match item {
            Item::Directive(directive) => Some(directive),
            Item::Macro(_) => None,
        })
    }

    pub fn references(&self) -> impl Iterator<Item = &MacroRef> {
        self.items.iter().filter_map(|item| match item {
            Item::Macro(reference) => Some(reference),
            Item::Directive(_) => None,
        })
    }
}

/// Reads every directive and macro reference in `input`, in order.
///
/// The items are flat and never overlap. Nothing *inside* a `` `define `` body
/// is reported, because a body is the macro's text: the directives and
/// references in it are processed where the macro is used, not where it is
/// defined (22.2). Nothing inside a macro *argument* is reported either, for
/// the same reason -- an argument is text too -- and neither are the operands
/// of any other directive, which belong to the directive.
///
/// So a nested call is found by scanning the argument that holds it, not by
/// reading further down this list. Splitting an argument list never depends on
/// what is nested inside it: whatever a nested call's parentheses are for, they
/// balance.
pub fn scan(input: &Input) -> Scan {
    scan_seeded(input, MacroTable::new())
}

/// The same, starting from definitions made somewhere this file does not
/// contain.
///
/// Raw mode's reason for wanting this is arity, not text. It still reads the
/// file as written -- no `` `include `` is followed and no conditional is
/// evaluated -- but a name it has a definition for is a name whose argument
/// list it no longer has to guess at, and 95% of the corpus's references are
/// defined in a header rather than where they are used. What the caller seeds
/// is its business: `-D` on a command line, or the table a prior expansion
/// ended with.
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
            // The table is consulted as it stands *here*, which is the whole of
            // what a reference may depend on: a macro has to be defined before
            // it is used.
            None => Item::Macro(macros::parse(input, at, len, &macros)),
        };

        // The `max` is insurance: an item that somehow covered no tokens would
        // otherwise spin here forever.
        at = item.tokens().end.max(at + 1);
        items.push(item);
    }

    Scan { items, macros }
}
