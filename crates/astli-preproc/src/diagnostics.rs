//! Diagnostic codes and constructor helpers for preprocessor errors.
//!
//! This module centralizes diagnostic definitions emitted during preprocessing,
//! such as unresolved macros, invalid argument lists, and recursion cycles.

use astli_text::{Code, Diagnostic, Span};

pub const UNDEFINED_MACRO: Code = Code("undefined-macro");
pub const MISSING_ARGUMENT_LIST: Code = Code("missing-argument-list");
pub const TOO_MANY_ARGUMENTS: Code = Code("too-many-arguments");
pub const MISSING_ARGUMENT: Code = Code("missing-argument");
pub const RECURSIVE_MACRO: Code = Code("recursive-macro");
pub const UNCLOSED_STRINGIFICATION: Code = Code("unclosed-stringification");
pub const PASTE_WITHOUT_OPERAND: Code = Code("paste-without-operand");
pub const INCLUDE_WITHOUT_NAME: Code = Code("include-without-name");
pub const INCLUDE_NOT_FOUND: Code = Code("include-not-found");
pub const INCLUDE_CYCLE: Code = Code("include-cycle");
pub const INCLUDE_TOO_DEEP: Code = Code("include-too-deep");
pub const CONDITIONAL_WITHOUT_NAME: Code = Code("conditional-without-name");
pub const UNCLOSED_CONDITIONAL: Code = Code("unclosed-conditional");
pub const STRAY_CONDITIONAL: Code = Code("stray-conditional");

/// Emitted when an undefined macro reference is encountered during expansion.
pub(crate) fn undefined_macro(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(UNDEFINED_MACRO, at, format!("{name} is not defined"))
        .pointing("not defined here")
        .note("the reference stands as written")
}

/// Emitted when a macro defined with formals is invoked without an argument list.
pub(crate) fn missing_argument_list(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        MISSING_ARGUMENT_LIST,
        at,
        format!("{name} takes an argument list"),
    )
    .pointing("no argument list")
    .note("a default cannot stand in: the list is what makes it a call")
}

/// Emitted when a macro call provides more actual arguments than formal parameters.
pub(crate) fn too_many_arguments(name: &str, formals: usize, given: usize, at: Span) -> Diagnostic {
    Diagnostic::error(
        TOO_MANY_ARGUMENTS,
        at,
        format!("{name} takes {formals} argument(s), and is given {given}"),
    )
    .pointing(format!("{given} given, {formals} taken"))
    .note("the extra arguments are dropped")
}

/// Emitted when a required formal parameter is omitted and has no default value.
pub(crate) fn missing_argument(name: &str, formal: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        MISSING_ARGUMENT,
        at,
        format!("{name} is not given an argument for `{formal}`"),
    )
    .pointing(format!("`{formal}` has no argument and no default"))
    .note("it expands to nothing")
}

/// Emitted when macro expansion encounters a recursive self-reference.
pub(crate) fn recursive_macro(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        RECURSIVE_MACRO,
        at,
        format!("{name} is already being expanded"),
    )
    .pointing("reaches itself")
    .note("the reference stands as written, because substituting again cannot terminate")
}

/// Emitted when a stringification quote (`` `\" ``) is not closed before the end of the body.
pub(crate) fn unclosed_stringification(at: Span) -> Diagnostic {
    Diagnostic::error(
        UNCLOSED_STRINGIFICATION,
        at,
        "this `\" is never closed".to_string(),
    )
    .pointing("never closed")
    .note("the text to the end of the body is quoted")
}

/// Emitted when the token-pasting operator (``` `` ```) lacks an operand on one side.
pub(crate) fn paste_without_operand(at: Span) -> Diagnostic {
    Diagnostic::error(
        PASTE_WITHOUT_OPERAND,
        at,
        "this `` has nothing on one side of it".to_string(),
    )
    .pointing("nothing to fuse")
    .note("the operator is dropped, and what is on the other side stands")
}

/// Emitted when an `` `include `` directive contains an empty target filename.
pub(crate) fn include_without_name(at: Span) -> Diagnostic {
    Diagnostic::error(
        INCLUDE_WITHOUT_NAME,
        at,
        "this `include names no file".to_string(),
    )
    .pointing("the name is empty")
    .note("it expands to nothing")
}

/// Emitted when an included file cannot be located on the search paths.
pub(crate) fn include_not_found(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(INCLUDE_NOT_FOUND, at, format!("cannot find `{name}`"))
        .pointing("nothing on the include path holds it")
        .note("the directive expands to nothing")
}

/// Emitted when an `` `include `` directive attempts to include an already active file.
pub(crate) fn include_cycle(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        INCLUDE_CYCLE,
        at,
        format!("`{name}` is already open above this point"),
    )
    .pointing("includes itself")
    .note("following it cannot terminate, so the directive expands to nothing")
}

/// Emitted when `` `include `` nesting exceeds the maximum allowed depth.
pub(crate) fn include_too_deep(name: &str, limit: usize, at: Span) -> Diagnostic {
    Diagnostic::error(
        INCLUDE_TOO_DEEP,
        at,
        format!("`{name}` is more than {limit} includes deep"),
    )
    .pointing(format!("more than {limit} deep"))
    .note("the directive expands to nothing")
}

/// Emitted when an `` `ifdef `` or `` `elsif `` directive has no identifier argument.
pub(crate) fn conditional_without_name(at: Span) -> Diagnostic {
    Diagnostic::error(
        CONDITIONAL_WITHOUT_NAME,
        at,
        "this conditional tests no name".to_string(),
    )
    .pointing("no name to test")
    .note("the name is the whole of the condition, so the branch is never taken")
}

/// Emitted when a conditional region lacks a closing `` `endif ``.
pub(crate) fn unclosed_conditional(at: Span) -> Diagnostic {
    Diagnostic::error(
        UNCLOSED_CONDITIONAL,
        at,
        "this conditional is never closed".to_string(),
    )
    .pointing("opened here, never closed")
    .note("the region runs to the end of the text it is in")
}

/// Emitted when an `` `endif ``, `` `else ``, or `` `elsif `` appears without a matching opening directive.
pub(crate) fn stray_conditional(directive: &str, at: Span) -> Diagnostic {
    Diagnostic::error(STRAY_CONDITIONAL, at, format!("{directive} closes nothing"))
        .pointing("no region above it")
        .note("it is consumed, like any other directive")
}

#[cfg(test)]
mod tests {
    const ALL: &[super::Code] = &[
        super::UNDEFINED_MACRO,
        super::MISSING_ARGUMENT_LIST,
        super::TOO_MANY_ARGUMENTS,
        super::MISSING_ARGUMENT,
        super::RECURSIVE_MACRO,
        super::UNCLOSED_STRINGIFICATION,
        super::PASTE_WITHOUT_OPERAND,
        super::INCLUDE_WITHOUT_NAME,
        super::INCLUDE_NOT_FOUND,
        super::INCLUDE_CYCLE,
        super::INCLUDE_TOO_DEEP,
        super::CONDITIONAL_WITHOUT_NAME,
        super::UNCLOSED_CONDITIONAL,
        super::STRAY_CONDITIONAL,
    ];

    #[test]
    fn every_code_is_distinct() {
        let mut seen: Vec<&str> = ALL.iter().map(|code| code.as_str()).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), count, "two diagnostics share a code");
    }

    #[test]
    fn a_code_is_a_lowercase_dashed_word() {
        for code in ALL {
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
}
