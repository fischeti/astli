//! Preprocessor for SystemVerilog directives, macros, includes, and conditionals.
//!
//! This crate operates directly on token streams to recognize directives, build
//! macro definition tables, and perform macro substitution:
//! - [`scan`]: Single-pass scan over a token stream recognizing directives and macro references.
//! - [`Session`]: Compilation context holding file buffers, tokens, and the [`Build`].
//! - [`Session::expand`]: Fully expanded preprocessing mode following `` `include `` files and evaluating conditionals.
//! - [`region`] / [`regions`]: Conditional compilation regions (`` `ifdef `` .. `` `endif ``).
//! - [`MacroTable`]: Macro definitions and call site argument resolution.

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
pub use expand::{Expanded, ExpandedToken, render};
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
