//! Parser diagnostic codes and error reporting constructors.
//!
//! Syntactic constructs that the parser cannot recognise are collected into
//! [`VERBATIM`](astli_syntax::SyntaxKind::VERBATIM) nodes without aborting.
//! Diagnostics are reserved for structural errors, such as unclosed delimiter
//! blocks that reach the end of the input stream.

use astli_text::{Code, Diagnostic, Span};

/// Diagnostic code emitted when the file ends while a delimiter or block construct is still open.
pub const UNCLOSED_AT_END: Code = Code("unclosed-at-end-of-file");

/// Diagnostic code emitted where constructs nest deeper than the parser goes.
pub const NESTED_TOO_DEEP: Code = Code("nested-too-deep");

/// Creates a diagnostic for a construct nested `limit` deep, which is left
/// as written.
pub(crate) fn nested_too_deep(at: Span, limit: u32) -> Diagnostic {
    Diagnostic::warning(
        NESTED_TOO_DEEP,
        at,
        format!("nested more than {limit} deep"),
    )
    .pointing("left as written from here")
    .note("a limit on nesting keeps a tree shallow enough to walk recursively")
}

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
        for code in [super::UNCLOSED_AT_END, super::NESTED_TOO_DEEP] {
            let text = code.as_str();
            assert!(
                !text.is_empty()
                    && text
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte == b'-'),
                "{text:?} is not written the way the others are"
            );
        }
    }

    /// The messages of the depth warnings for `text`, which must round-trip.
    fn too_deep(text: String) -> Vec<String> {
        let tree = crate::SyntaxTree::parse("deep.sv", text.clone());
        assert_eq!(tree.root().text().to_string(), text);
        (tree.diagnostics().iter())
            .filter(|it| it.code == super::NESTED_TOO_DEEP)
            .map(|it| it.message.clone())
            .collect()
    }

    #[test]
    fn nesting_past_the_limit_is_left_as_written_with_a_warning() {
        let deep = crate::MAX_NESTING as usize + 1;
        let text = format!(
            "module m; assign a = {}b{}; endmodule",
            "(".repeat(deep),
            ")".repeat(deep)
        );
        assert_eq!(too_deep(text), ["nested more than 256 deep"]);
    }

    #[test]
    fn a_tree_past_the_limit_is_flattened_with_a_warning() {
        let links = crate::build::MAX_DEPTH as usize;
        let text = format!("module m; assign a = {}b; endmodule", "b + ".repeat(links));
        assert_eq!(too_deep(text), ["nested more than 2048 deep"]);
    }
}
