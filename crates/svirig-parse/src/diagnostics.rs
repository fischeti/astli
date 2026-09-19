//! Everything the grammar can say is wrong.
//!
//! Far fewer than the preprocessor's, and that is the design rather than a gap.
//! What the rules cannot make sense of becomes a [`VERBATIM`] node holding a
//! balanced run of its own tokens, and that is a **successful recovery**: the
//! formatter leaves those bytes alone and the file still round-trips. Ninety
//! per cent of a grammar this size is unwritten at any given time, so treating
//! every unparsed construct as a fault would report the parser's progress
//! rather than the file's problems.
//!
//! So a diagnostic here means something narrower: a rule that recognised what
//! it was reading, got part way, and then found the text could not be what it
//! had already decided it was.
//!
//! [`VERBATIM`]: svirig_syntax::SyntaxKind::VERBATIM

use svirig_text::{Code, Diagnostic, TokenOrigin};

pub const UNCLOSED_AT_END: Code = Code("unclosed-at-end-of-file");

/// The text ran out with something still open.
///
/// The one thing the grammar can say today, and it earns it by being about the
/// *file* rather than about the grammar: a construct no rule claims still
/// balances, so its run ends with nothing on the stack. A run that reaches the
/// end of the text with `begin` or `module` still open means the brackets do
/// not match, which is true whatever Annex A the parser has got to.
///
/// It also survives, which nothing else does yet -- see
/// [`Parser::report`](crate::Parser::report).
pub(crate) fn unclosed_at_end(opener: &str, closer: &str, at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        UNCLOSED_AT_END,
        at,
        format!("the file ends with `{opener}` still open"),
    )
    .pointing(format!("expected `{closer}`"))
    .note("the run of tokens it opened reaches the end of the text")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_code_is_written_the_way_the_others_are() {
        let text = super::UNCLOSED_AT_END.as_str();
        assert!(
            !text.is_empty()
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-'),
            "{text:?} is not written the way the others are"
        );
    }
}
