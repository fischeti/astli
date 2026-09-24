//! The check that formatting left the preprocessed meaning alone.
//!
//! Preprocessing reads the token sequence and which tokens a directive's line
//! holds, and nothing else. So three properties, each checkable without any
//! definition, make the output preprocess like the input under every set of
//! definitions at once:
//!
//! - the tokens other than whitespace are the same, comments included;
//! - every directive covers the same tokens, so nothing was joined onto a
//!   line that ends a directive, or split off it;
//! - every `` `define `` is byte for byte what it was, because `` `" `` makes
//!   whitespace in a body observable, and a body's lines are its `\`
//!   continuations.

use std::fmt;
use std::ops::Range;

use svirig_preproc::{Directive, DirectiveType, Input, Session};
use svirig_syntax::SyntaxKind::*;
use svirig_text::SourceId;

/// Why [`format()`](crate::format()) returned no text.
///
/// Each one is a bug in the formatter: it produced text that would preprocess
/// differently, and caught itself before handing it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// Byte offset in the input of the first place the output departs from it.
    pub offset: u32,
    pub reason: Reason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// A token other than whitespace was changed, added or dropped.
    Tokens,
    /// A directive would cover different tokens.
    Directive,
    /// The text of a `` `define `` was changed.
    Define,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.reason {
            Reason::Tokens => "formatting would change the tokens here",
            Reason::Directive => "formatting would change what this directive covers",
            Reason::Define => "formatting would rewrite this `define",
        })
    }
}

impl std::error::Error for Refusal {}

/// Whether `after` preprocesses like `before` under any definitions.
pub(crate) fn check(before: &str, after: &str) -> Result<(), Refusal> {
    let mut session = Session::new();
    let before = session.add("before", before.to_owned());
    let after = session.add("after", after.to_owned());
    let before = Side::new(&session, before);
    let after = Side::new(&session, after);

    tokens(&before, &after)?;
    directives(&before, &after)
}

/// One of the two texts, lexed and scanned on its own.
struct Side<'a> {
    input: Input<'a>,
    directives: Vec<Directive>,
    /// Indices of the tokens that are not whitespace, `EOF` last.
    significant: Vec<u32>,
    /// For each token index, and one past the end, how many of `significant`
    /// come before it: the index both texts agree on.
    rank: Vec<u32>,
}

impl<'a> Side<'a> {
    fn new(session: &'a Session, file: SourceId) -> Side<'a> {
        let input = session.input(file);
        let mut significant = Vec::new();
        let mut rank = Vec::with_capacity(input.tokens.len() + 1);
        for at in 0..input.len() {
            rank.push(significant.len() as u32);
            if input.kind(at) != WHITESPACE {
                significant.push(at);
            }
        }
        rank.push(significant.len() as u32);
        Side {
            input,
            directives: session.scan(file).directives().cloned().collect(),
            significant,
            rank,
        }
    }

    /// The significant tokens a directive covers, by rank.
    fn covers(&self, directive: &Directive) -> Range<u32> {
        let tokens = directive.tokens.range();
        self.rank[tokens.start as usize]..self.rank[tokens.end as usize]
    }

    fn text(&self, directive: &Directive) -> &'a str {
        let span = directive.tokens.bytes(self.input.tokens);
        &self.input.source[span.start as usize..span.end as usize]
    }

    /// The byte offset of the significant token of rank `rank`.
    fn offset(&self, rank: u32) -> u32 {
        self.input.token(self.significant[rank as usize]).start
    }
}

fn tokens(before: &Side, after: &Side) -> Result<(), Refusal> {
    let (old, new) = (&before.input, &after.input);
    // Both end in `EOF` and nothing else is one, so a length difference is
    // found as a mismatch before either runs out.
    let differs = before
        .significant
        .iter()
        .zip(&after.significant)
        .position(|(&x, &y)| old.kind(x) != new.kind(y) || old.text(x) != new.text(y));
    match differs {
        None => Ok(()),
        Some(rank) => Err(Refusal {
            offset: before.offset(rank as u32),
            reason: Reason::Tokens,
        }),
    }
}

/// Assumes the significant tokens already agree, so ranks name the same token
/// on both sides.
fn directives(before: &Side, after: &Side) -> Result<(), Refusal> {
    let mut old = before.directives.iter();
    let mut new = after.directives.iter();
    loop {
        let (x, y) = match (old.next(), new.next()) {
            (None, None) => return Ok(()),
            (Some(x), Some(y)) => (x, y),
            (Some(x), None) => return Err(unmatched(before, before.covers(x).start)),
            (None, Some(y)) => return Err(unmatched(before, after.covers(y).start)),
        };
        let (covered, covers) = (before.covers(x), after.covers(y));
        if covered != covers {
            return Err(unmatched(before, covered.start.min(covers.start)));
        }
        if x.ty == DirectiveType::Define && before.text(x) != after.text(y) {
            return Err(Refusal {
                offset: before.offset(covered.start),
                reason: Reason::Define,
            });
        }
    }
}

/// A directive, starting at significant token `rank`, that has no counterpart
/// covering the same tokens.
fn unmatched(before: &Side, rank: u32) -> Refusal {
    Refusal {
        offset: before.offset(rank),
        reason: Reason::Directive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(before: &str, after: &str) -> Option<Reason> {
        check(before, after).err().map(|refusal| refusal.reason)
    }

    #[test]
    fn whitespace_between_tokens_is_the_formatters_to_change() {
        let before = "module m;\n    logic a;\n\n\nendmodule\n";
        let after = "module m;\n  logic a;\n\nendmodule\n";
        assert_eq!(refused(before, after), None);
    }

    #[test]
    fn a_changed_token_is_refused_where_it_starts() {
        let refusal = check("assign a = b;", "assign a = c;").unwrap_err();
        assert_eq!(refusal.reason, Reason::Tokens);
        assert_eq!(refusal.offset, 11);
    }

    #[test]
    fn a_dropped_comment_is_refused() {
        assert_eq!(refused("a; // why\n", "a;\n"), Some(Reason::Tokens));
    }

    #[test]
    fn an_added_token_is_refused() {
        assert_eq!(refused("a", "a;"), Some(Reason::Tokens));
    }

    #[test]
    fn an_escaped_identifier_keeps_the_space_that_ends_it() {
        assert_eq!(refused("\\a+b ;", "\\a+b;"), Some(Reason::Tokens));
    }

    #[test]
    fn a_line_joined_onto_a_define_is_refused() {
        let before = "`define X 1\nfoo;\n";
        let after = "`define X 1 foo;\n";
        assert_eq!(refused(before, after), Some(Reason::Directive));
    }

    #[test]
    fn a_define_body_is_not_reindented() {
        let before = "`define X(a) \\\n    a\n";
        let after = "`define X(a) \\\n  a\n";
        assert_eq!(refused(before, after), Some(Reason::Define));
    }

    #[test]
    fn a_define_may_be_indented() {
        let before = "module m;\n`define X 1\nendmodule\n";
        let after = "module m;\n  `define X 1\nendmodule\n";
        assert_eq!(refused(before, after), None);
    }

    #[test]
    fn a_line_directive_keeps_what_follows_it_on_the_line() {
        let before = "`timescale 1ns/1ps\nmodule m; endmodule\n";
        let after = "`timescale 1ns/1ps module m; endmodule\n";
        assert_eq!(refused(before, after), Some(Reason::Directive));
    }

    #[test]
    fn a_conditional_may_be_split_onto_lines_of_its_own() {
        let before = "`ifdef X a; `else b; `endif\n";
        let after = "`ifdef X\n  a;\n`else\n  b;\n`endif\n";
        assert_eq!(refused(before, after), None);
    }
}
