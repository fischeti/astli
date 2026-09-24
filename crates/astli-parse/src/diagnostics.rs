//! Parser diagnostic codes and error reporting constructors.
//!
//! Syntactic constructs that the parser cannot recognise are collected into
//! [`VERBATIM`](astli_syntax::SyntaxKind::VERBATIM) nodes without aborting.
//! Diagnostics are reserved for structural errors, such as unclosed delimiter
//! blocks that reach the end of the input stream.

use astli_text::{Code, Diagnostic, Span};

/// Diagnostic code emitted when the file ends while a delimiter or block construct is still open.
pub const UNCLOSED_AT_END: Code = Code("unclosed-at-end-of-file");

/// Creates a diagnostic for an unclosed block or delimiter that reaches the end of the file.
pub(crate) fn unclosed_at_end(opener: &str, closer: &str, at: Span) -> Diagnostic {
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
