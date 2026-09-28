//! Parser diagnostic codes and error reporting constructors.
//!
//! Syntactic constructs that the parser cannot recognise are collected into
//! [`VERBATIM`](astli_syntax::SyntaxKind::VERBATIM) nodes without aborting.
//! Parsing reports only structural errors, such as unclosed delimiter blocks
//! that reach the end of the input stream. The `VERBATIM` nodes themselves are
//! reported on request, read off the finished tree, since rollback withdraws
//! whatever a speculative rule reports.

use astli_text::{Code, Diagnostic, Span};

/// Diagnostic code emitted when the file ends while a delimiter or block construct is still open.
pub const UNCLOSED_AT_END: Code = Code("unclosed-at-end-of-file");

/// Diagnostic code emitted where constructs nest deeper than the parser goes.
pub const NESTED_TOO_DEEP: Code = Code("nested-too-deep");

/// Diagnostic code emitted for a run of tokens the parser kept as written.
pub const NOT_PARSED: Code = Code("not-parsed");

/// Diagnostic code emitted for a `` `resetall `` inside a design element.
pub const MISPLACED_RESETALL: Code = Code("misplaced-resetall");

/// Diagnostic code emitted for a number's base with no digits after it.
pub const BASE_WITHOUT_DIGITS: Code = Code("base-without-digits");

/// Creates a diagnostic for a run of tokens from `first` to `last` that was
/// not parsed.
///
/// It is a warning because the parser cannot tell code it does not cover yet
/// from code that is malformed, or from code a conditional cuts in two.
pub(crate) fn not_parsed(first: Span, last: Span) -> Diagnostic {
    let diagnostic = Diagnostic::warning(NOT_PARSED, first, "not parsed, so kept as written")
        .pointing("from here")
        .note("the grammar does not cover this yet, it is malformed, or an `ifdef splits it");
    match first == last {
        true => diagnostic,
        false => diagnostic.label(last, "to here"),
    }
}

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

/// Creates a diagnostic for a `` `resetall `` inside a module, interface,
/// program or package, where the standard forbids it.
pub(crate) fn misplaced_resetall(at: Span) -> Diagnostic {
    Diagnostic::error(
        MISPLACED_RESETALL,
        at,
        "`resetall is only allowed between design elements",
    )
    .pointing("inside one")
    .note("it is kept as written")
}

/// Creates a diagnostic for a base, such as `'d`, that no digits follow.
pub(crate) fn base_without_digits(at: Span) -> Diagnostic {
    Diagnostic::error(BASE_WITHOUT_DIGITS, at, "a number's base takes its digits")
        .pointing("no digits after it")
        .note("a sign goes before the whole number, as in `-8'd6`")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_code_is_written_the_way_the_others_are() {
        for code in [
            super::UNCLOSED_AT_END,
            super::NESTED_TOO_DEEP,
            super::NOT_PARSED,
            super::MISPLACED_RESETALL,
            super::BASE_WITHOUT_DIGITS,
        ] {
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

    /// Where each `not-parsed` warning for `text` points, as `line:col`.
    fn unparsed(text: &str) -> Vec<String> {
        let tree = crate::SyntaxTree::parse("unparsed.sv", text.to_string());
        (tree.unparsed().iter())
            .map(|it| tree.line_col(it.at.start).to_string())
            .collect()
    }

    #[test]
    fn a_run_kept_as_written_is_reported_once_at_its_first_token() {
        // `specify` is left to the fallback, and the `$setup` inside it is
        // part of the same run rather than one of its own.
        let text =
            "module m;\n  specify\n    $setup(d, posedge clk, 1);\n  endspecify\nendmodule\n";
        assert_eq!(unparsed(text), ["2:3"]);
    }

    #[test]
    fn a_file_the_parser_understood_has_nothing_to_report() {
        assert!(unparsed("module m;\n  logic q;\nendmodule\n").is_empty());
    }

    #[test]
    fn a_run_another_diagnostic_explains_is_not_reported_again() {
        let tree = crate::SyntaxTree::parse("open.sv", "module m;\n  logic q;\n".into());
        assert_eq!(tree.diagnostics().len(), 1);
        assert!(tree.unparsed().is_empty());

        let deep = crate::MAX_NESTING as usize + 1;
        let text = format!(
            "module m; assign a = {}b{}; endmodule",
            "(".repeat(deep),
            ")".repeat(deep)
        );
        assert!(unparsed(&text).is_empty());
    }

    #[test]
    fn a_tree_past_the_limit_is_flattened_with_a_warning() {
        let links = crate::build::MAX_DEPTH as usize;
        let text = format!("module m; assign a = {}b; endmodule", "b + ".repeat(links));
        assert_eq!(too_deep(text), ["nested more than 2048 deep"]);
    }
}
