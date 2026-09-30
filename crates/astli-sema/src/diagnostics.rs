//! What `check` reports: errors the standard defines, and only where sema
//! is sure of them.

use astli_text::{Code, Diagnostic, Span};

/// A name declared nowhere it could be.
pub const UNDECLARED_NAME: Code = Code("undeclared-name");

/// An instance of a module, interface or program no file declares.
pub const UNKNOWN_DEFINITION: Code = Code("unknown-definition");

/// A package no file declares, imported or named before `::`.
pub const UNKNOWN_PACKAGE: Code = Code("unknown-package");

/// `pkg::name`, or an import of it, where the package lacks `name`.
pub const NOT_IN_PACKAGE: Code = Code("not-in-package");

/// A connection by name to a port the definition lacks.
pub const UNKNOWN_PORT: Code = Code("unknown-port");

/// An override by name of a parameter the definition lacks.
pub const UNKNOWN_PARAMETER: Code = Code("unknown-parameter");

/// More connections by position than the definition has ports.
pub const TOO_MANY_PORTS: Code = Code("too-many-ports");

/// More overrides by position than the definition has parameters.
pub const TOO_MANY_PARAMETERS: Code = Code("too-many-parameters");

/// An override of a local parameter, which cannot be overridden.
pub const LOCAL_PARAMETER: Code = Code("local-parameter");

/// A port connected twice, or a parameter overridden twice.
pub const CONNECTED_TWICE: Code = Code("connected-twice");

pub(crate) fn undeclared_name(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(UNDECLARED_NAME, at, format!("`{name}` is not declared"))
        .pointing("declared nowhere in scope")
}

pub(crate) fn unknown_definition(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        UNKNOWN_DEFINITION,
        at,
        format!("no module, interface or program is called `{name}`"),
    )
    .note("every file the design needs must be on the command line or in the filelist")
}

pub(crate) fn unknown_package(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        UNKNOWN_PACKAGE,
        at,
        format!("no package is called `{name}`"),
    )
    .note("every file the design needs must be on the command line or in the filelist")
}

pub(crate) fn not_in_package(package: &str, name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        NOT_IN_PACKAGE,
        at,
        format!("package `{package}` declares no `{name}`"),
    )
    .note("a package's imports are not its own; only an `export` passes them on")
}

/// `what` is "port" or "parameter".
pub(crate) fn unknown_connection(what: &str, definition: &str, name: &str, at: Span) -> Diagnostic {
    let code = match what {
        "port" => UNKNOWN_PORT,
        _ => UNKNOWN_PARAMETER,
    };
    Diagnostic::error(code, at, format!("`{definition}` has no {what} `{name}`"))
}

pub(crate) fn too_many(
    what: &str,
    definition: &str,
    given: usize,
    has: usize,
    at: Span,
) -> Diagnostic {
    let code = match what {
        "port" => TOO_MANY_PORTS,
        _ => TOO_MANY_PARAMETERS,
    };
    Diagnostic::error(
        code,
        at,
        format!("{given} {what}s given by position, and `{definition}` has {has}"),
    )
}

pub(crate) fn local_parameter(definition: &str, name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        LOCAL_PARAMETER,
        at,
        format!("parameter `{name}` of `{definition}` is local, so cannot be overridden"),
    )
}

pub(crate) fn connected_twice(what: &str, name: &str, at: Span, first: Span) -> Diagnostic {
    Diagnostic::error(
        CONNECTED_TWICE,
        at,
        format!("{what} `{name}` is given twice"),
    )
    .label(first, "first here")
}
