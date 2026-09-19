//! Everything this crate can say is wrong, in one place.
//!
//! Each of these is a recovery that already existed and used to happen in
//! silence; `docs/limitations.md` tabulated them for exactly as long as there
//! was nowhere to report them to. **None of them changes what expansion does.**
//! The recovery is still the recovery -- the reference still stands as written,
//! the extra arguments are still dropped -- and what is added is only that it
//! says so.
//!
//! # Why a module of constructors
//!
//! So that every message the crate can produce is readable in one file, and so
//! that a [`Code`] sits beside the wording it goes with and the two cannot
//! drift apart. An `enum` of problems would do the same and then be converted
//! to a [`Diagnostic`] at every call site, which is a second representation
//! with a one-way trip.
//!
//! It is also what keeps `svirig-text` free of this crate's vocabulary: a
//! message naming a macro or a formal is formatted here, where those words
//! mean something.
//!
//! The [`Code`]s are public and the constructors are not, which is the right
//! way round: a code is what a caller matches on and what a `--deny` would
//! name, and the wording behind it is this crate's business.
//!
//! # Only the expanded path reports
//!
//! [`Session::expand`](super::Session::expand) takes `&mut self` and
//! [`scan`](super::scan) does not, so raw mode cannot reach a sink even in
//! principle. That is deliberate rather than incidental: a reference with no
//! definition in its own file is the ordinary case when a file is read alone
//! and a mistake only once a whole compilation is in view, and the two modes
//! disagreeing about it is the reason severity is not a setting.

use svirig_text::{Code, Diagnostic, TokenOrigin};

// A macro's name arrives as it was written, backtick included, because that is
// what the token covers and what the author will be looking for. So nothing
// here quotes one: `` `FOO `` already reads as a name. A formal has no tick and
// is quoted like any other identifier.

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

/// A reference to a name nothing has defined at the point it is used.
///
/// The one users most want told about, and the one that is *not* a mistake in
/// raw mode -- see the module docs.
pub(crate) fn undefined_macro(name: &str, at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(UNDEFINED_MACRO, at, format!("{name} is not defined"))
        .pointing("not defined here")
        .note("the reference stands as written")
}

/// A macro that takes formals, used without the parentheses.
pub(crate) fn missing_argument_list(name: &str, at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        MISSING_ARGUMENT_LIST,
        at,
        format!("{name} takes an argument list"),
    )
    .pointing("no argument list")
    .note("a default cannot stand in: the list is what makes it a call")
}

/// More actuals than the macro has formals.
pub(crate) fn too_many_arguments(
    name: &str,
    formals: usize,
    given: usize,
    at: TokenOrigin,
) -> Diagnostic {
    Diagnostic::error(
        TOO_MANY_ARGUMENTS,
        at,
        format!("{name} takes {formals} argument(s), and is given {given}"),
    )
    .pointing(format!("{given} given, {formals} taken"))
    .note("the extra arguments are dropped")
}

/// A formal with neither an argument nor a default.
pub(crate) fn missing_argument(name: &str, formal: &str, at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        MISSING_ARGUMENT,
        at,
        format!("{name} is not given an argument for `{formal}`"),
    )
    .pointing(format!("`{formal}` has no argument and no default"))
    .note("it expands to nothing")
}

/// A macro that reaches itself, directly or through others.
pub(crate) fn recursive_macro(name: &str, at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        RECURSIVE_MACRO,
        at,
        format!("{name} is already being expanded"),
    )
    .pointing("reaches itself")
    .note("the reference stands as written, because substituting again cannot terminate")
}

/// A `` `" `` that no second one closes.
pub(crate) fn unclosed_stringification(at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        UNCLOSED_STRINGIFICATION,
        at,
        "this `\" is never closed".to_string(),
    )
    .pointing("never closed")
    .note("the text to the end of the body is quoted")
}

/// ``` `` ``` with nothing on one side to fuse.
pub(crate) fn paste_without_operand(at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        PASTE_WITHOUT_OPERAND,
        at,
        "this `` has nothing on one side of it".to_string(),
    )
    .pointing("nothing to fuse")
    .note("the operator is dropped, and what is on the other side stands")
}

/// An `` `include `` whose name is empty once it has been expanded.
pub(crate) fn include_without_name(at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        INCLUDE_WITHOUT_NAME,
        at,
        "this `include names no file".to_string(),
    )
    .pointing("the name is empty")
    .note("it expands to nothing")
}

/// Nothing on the include path holds the file.
pub(crate) fn include_not_found(name: &str, at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(INCLUDE_NOT_FOUND, at, format!("cannot find `{name}`"))
        .pointing("nothing on the include path holds it")
        .note("the directive expands to nothing")
}

/// The file reads, and is already open above this include.
pub(crate) fn include_cycle(name: &str, at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        INCLUDE_CYCLE,
        at,
        format!("`{name}` is already open above this point"),
    )
    .pointing("includes itself")
    .note("following it cannot terminate, so the directive expands to nothing")
}

/// An include chain deeper than the implementation follows.
pub(crate) fn include_too_deep(name: &str, limit: usize, at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        INCLUDE_TOO_DEEP,
        at,
        format!("`{name}` is more than {limit} includes deep"),
    )
    .pointing(format!("more than {limit} deep"))
    .note("the directive expands to nothing")
}

/// `` `ifdef `` or `` `elsif `` with no name after it.
pub(crate) fn conditional_without_name(at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        CONDITIONAL_WITHOUT_NAME,
        at,
        "this conditional tests no name".to_string(),
    )
    .pointing("no name to test")
    .note("the name is the whole of the condition, so the branch is never taken")
}

/// A region that reaches the end of its text with no `` `endif ``.
pub(crate) fn unclosed_conditional(at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(
        UNCLOSED_CONDITIONAL,
        at,
        "this conditional is never closed".to_string(),
    )
    .pointing("opened here, never closed")
    .note("the region runs to the end of the text it is in")
}

/// An `` `endif ``, `` `else `` or `` `elsif `` with no region above it.
pub(crate) fn stray_conditional(directive: &str, at: TokenOrigin) -> Diagnostic {
    Diagnostic::error(STRAY_CONDITIONAL, at, format!("{directive} closes nothing"))
        .pointing("no region above it")
        .note("it is consumed, like any other directive")
}

#[cfg(test)]
mod tests {
    /// Every code this crate can emit. Kept by hand so that adding one is a
    /// deliberate act, and checked for collisions below.
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
