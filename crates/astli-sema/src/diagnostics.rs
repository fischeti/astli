//! What `check` reports: errors the standard defines, and only where sema
//! is sure of them.

use astli_syntax::SyntaxKind::{self, OUTPUT_KW};
use astli_text::{Code, Diagnostic, Span};

use crate::types::{Packed, Shape, Ty};

/// A name declared nowhere it could be.
pub const UNDECLARED_NAME: Code = Code("undeclared-name");

/// A name used above its declaration, where nothing declares it before.
pub const USED_BEFORE_DECLARATION: Code = Code("used-before-declaration");

/// An instance of a module, interface or program no file declares.
pub const UNKNOWN_DEFINITION: Code = Code("unknown-definition");

/// A package no file declares, imported or named before `::`.
pub const UNKNOWN_PACKAGE: Code = Code("unknown-package");

/// `pkg::name`, or an import of it, where the package lacks `name`.
pub const NOT_IN_PACKAGE: Code = Code("not-in-package");

/// `C::name`, `this.name` or `super.name`, where the class and every class
/// it extends are known and none declares `name`.
pub const NOT_IN_CLASS: Code = Code("not-in-class");

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

/// A parameter without a default that an instance, or a top, is built
/// without a value for.
pub const UNSET_PARAMETER: Code = Code("unset-parameter");

/// A port connected twice, or a parameter overridden twice.
pub const CONNECTED_TWICE: Code = Code("connected-twice");

/// A name declared twice in one scope.
pub const DECLARED_TWICE: Code = Code("declared-twice");

/// A net written by a procedure, which only a variable may be.
pub const PROCEDURAL_NET_ASSIGNMENT: Code = Code("procedural-net-assignment");

/// A `return` with a value where the subroutine returns none, without one
/// where it returns one, or inside a `fork`.
pub const INVALID_RETURN: Code = Code("invalid-return");

/// A function that waits: a delay, an event control, a `wait`, or a `fork`
/// that joins.
pub const WAITS_IN_FUNCTION: Code = Code("waits-in-function");

/// A variable two things drive, in what is built, where the standard allows
/// one.
pub const MULTIPLE_DRIVERS: Code = Code("multiple-drivers");

/// An enum member's value the enum cannot take: a sized literal of another
/// width than its base type, an `x` or `z` in a 2-state enum, none after one
/// with an `x` or `z`, or another member's.
pub const INVALID_ENUM_VALUE: Code = Code("invalid-enum-value");

/// A default value on a member of a packed struct, whose bits have none of
/// their own.
pub const PACKED_MEMBER_DEFAULT: Code = Code("packed-member-default");

/// A `specparam` used outside a specify block and the `specparam`s.
pub const SPECPARAM_OUTSIDE_SPECIFY: Code = Code("specparam-outside-specify");

/// An indexed part-select of no bits, or fewer.
pub const EMPTY_SELECT: Code = Code("empty-select");

/// A value given where its type does not convert to the type it is given
/// to: an assignment, a connection, an argument, a `return`.
pub const INCOMPATIBLE_TYPE: Code = Code("incompatible-type");

/// An operator given an operand of a type it does not take.
pub const INVALID_OPERAND: Code = Code("invalid-operand");

/// A hierarchical name where a constant is needed: a parameter's value or
/// override, a generate condition, a generate `case`'s selector or labels,
/// a generate loop's header.
pub const NOT_CONSTANT: Code = Code("not-constant");

/// An elaboration-time `$fatal` or `$error` in what the tops build.
pub const ELABORATION_ERROR: Code = Code("elaboration-error");

/// An elaboration-time `$warning` in what the tops build.
pub const ELABORATION_WARNING: Code = Code("elaboration-warning");

pub(crate) fn undeclared_name(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(UNDECLARED_NAME, at, format!("`{name}` is not declared"))
        .pointing("declared nowhere in scope")
}

pub(crate) fn used_before_declaration(name: &str, at: Span, declared: Span) -> Diagnostic {
    Diagnostic::error(
        USED_BEFORE_DECLARATION,
        at,
        format!("`{name}` is used before its declaration"),
    )
    .label(declared, "declared here")
    .note("a name is declared only from where its declaration stands")
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

/// `class` is the class named before `::`, or `None` for `this` or `super`.
pub(crate) fn not_in_class(class: Option<&str>, name: &str, at: Span) -> Diagnostic {
    let class = match class {
        Some(class) => format!("class `{class}`"),
        None => "the class".to_string(),
    };
    Diagnostic::error(
        NOT_IN_CLASS,
        at,
        format!("{class} has no member `{name}`, declared or inherited"),
    )
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

pub(crate) fn unset_parameter(
    instance: &str,
    parameter: &str,
    at: Span,
    declared: Span,
) -> Diagnostic {
    Diagnostic::error(
        UNSET_PARAMETER,
        at,
        format!("`{instance}` gives no value to `{parameter}`, which has no default"),
    )
    .label(declared, "declared without a default")
}

pub(crate) fn unset_top_parameter(top: &str, parameter: &str, declared: Span) -> Diagnostic {
    Diagnostic::error(
        UNSET_PARAMETER,
        declared,
        format!("top `{top}` is built with no value for `{parameter}`, which has no default"),
    )
    .pointing("declared without a default")
    .note("a top's parameter is set by its `[top.NAME]` table's `params`, or by `-G`")
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

pub(crate) fn declared_twice(name: &str, at: Span, first: Span) -> Diagnostic {
    Diagnostic::error(
        DECLARED_TWICE,
        at,
        format!("`{name}` is declared twice in one scope"),
    )
    .label(first, "first declared here")
}

/// `direction` is a port's, or `None` for a net declared as one.
pub(crate) fn procedural_net_assignment(
    name: &str,
    direction: Option<SyntaxKind>,
    at: Span,
    declared: Span,
) -> Diagnostic {
    let found = Diagnostic::error(
        PROCEDURAL_NET_ASSIGNMENT,
        at,
        format!("`{name}` is a net, which a procedure cannot assign"),
    )
    .label(declared, "declared as a net here")
    .note("only a continuous assignment, a port or a `force` drives a net");
    match direction {
        None => found.help("declare it as a variable, with `logic` rather than a net type"),
        Some(OUTPUT_KW) => found.help(format!(
            "give the port a data type, as `output logic {name}`"
        )),
        // What an input or inout is connected to drives it.
        Some(_) => found,
    }
}

pub(crate) fn invalid_enum_value(message: String, at: Span) -> Diagnostic {
    Diagnostic::error(INVALID_ENUM_VALUE, at, message)
}

pub(crate) fn invalid_return(problem: &str, at: Span) -> Diagnostic {
    Diagnostic::error(INVALID_RETURN, at, problem.to_string())
}

pub(crate) fn waits_in_function(what: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        WAITS_IN_FUNCTION,
        at,
        format!("a function cannot wait, as {what} does"),
    )
    .note("a function runs in no time; a task, or a `fork … join_none`, may wait")
}

pub(crate) fn packed_member_default(member: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        PACKED_MEMBER_DEFAULT,
        at,
        format!("`{member}` is a member of a packed struct, which takes no default"),
    )
    .note("a packed struct is one vector, so only a variable of it starts with a value")
}

pub(crate) fn specparam_outside_specify(name: &str, at: Span, declared: Span) -> Diagnostic {
    Diagnostic::error(
        SPECPARAM_OUTSIDE_SPECIFY,
        at,
        format!(
            "`{name}` is a `specparam`, which only a specify block or another `specparam` may use"
        ),
    )
    .label(declared, "declared here")
}

pub(crate) fn not_constant(name: &str, at: Span) -> Diagnostic {
    Diagnostic::error(
        NOT_CONSTANT,
        at,
        format!("`{name}` is named through the hierarchy, so is not a constant"),
    )
    .pointing("a hierarchical name")
    .note("the hierarchy is built from the constants, so they cannot read it")
}

pub(crate) fn empty_select(width: i64, at: Span) -> Diagnostic {
    Diagnostic::error(
        EMPTY_SELECT,
        at,
        format!("an indexed part-select is at least one bit wide, but this one is {width}"),
    )
}

/// A pattern given to an unpacked array or a struct without one item for
/// each element or member.
pub(crate) fn pattern_count(items: usize, wanted: usize, target: &Ty, at: Span) -> Diagnostic {
    Diagnostic::error(
        INCOMPATIBLE_TYPE,
        at,
        format!("a pattern of {items} items does not convert to `{target}`"),
    )
    .pointing(format!("`{target}` takes {wanted}"))
    .note("a positional pattern has one item for each element or member, each a pattern of its own where that is one too")
}

pub(crate) fn incompatible_type(
    source: &Ty,
    target: &Ty,
    reason: &str,
    pointing: String,
    at: Span,
) -> Diagnostic {
    let found = Diagnostic::error(
        INCOMPATIBLE_TYPE,
        at,
        format!("a `{source}` does not convert to `{target}`"),
    )
    .pointing(pointing)
    .note(reason);
    match target {
        Ty::Packed(Packed {
            shape: Shape::Enum(ty),
            ..
        }) if ty.name.is_some() => found.help(format!("cast it, as `{target}'(…)`")),
        _ => found,
    }
}

pub(crate) fn invalid_operand(op: &str, operand: &Ty, at: Span) -> Diagnostic {
    Diagnostic::error(
        INVALID_OPERAND,
        at,
        format!("`{op}` does not take a `{operand}`"),
    )
}

pub(crate) fn multiple_drivers(message: String, at: Span) -> Diagnostic {
    Diagnostic::error(MULTIPLE_DRIVERS, at, message)
        .note("a variable an `always_comb`, `always_ff`, `always_latch`, continuous assignment or output drives may have no other driver")
}

/// `task` run in the body at `path`, saying `message` if sema knows what it
/// says.
pub(crate) fn elaboration_task(
    task: &str,
    message: Option<String>,
    path: &str,
    at: Span,
) -> Diagnostic {
    let text = (message.filter(|it| !it.is_empty()))
        .unwrap_or_else(|| format!("`{task}` while elaborating"));
    let diagnostic = match task {
        "$warning" => Diagnostic::warning(ELABORATION_WARNING, at, text),
        _ => Diagnostic::error(ELABORATION_ERROR, at, text),
    };
    diagnostic.pointing(format!("`{task}` in `{path}`"))
}
