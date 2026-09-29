//! How an instance connects its ports and parameters.

use astli_syntax::SyntaxKind::{MACRO_CALL, VERBATIM};
use astli_syntax::ast::{Arg, ArgList, AstNode, Instantiation};

use crate::rule::Cx;

/// With more than one port, a connection by position depends on the order
/// the module declares its ports in, and a reordering there silently swaps
/// signals here. A name says which port it is.
pub(crate) fn module_port(cx: &mut Cx) {
    let instances = cx.root().descendants().filter_map(Instantiation::cast);
    for list in instances
        .flat_map(|it| it.instances())
        .filter_map(|it| it.arg_list())
    {
        positional(
            cx,
            &list,
            "a port connected by position",
            "connect it by name, `.port(signal)`",
        );
    }
}

/// With more than one parameter, a value by position depends on the order
/// the module declares its parameters in. A name says which it sets.
pub(crate) fn module_parameter(cx: &mut Cx) {
    let instances = cx.root().descendants().filter_map(Instantiation::cast);
    for list in instances.filter_map(|it| it.arg_list()) {
        positional(
            cx,
            &list,
            "a parameter set by position",
            "set it by name, `.Name(value)`",
        );
    }
}

/// Reports the first positional entry of `list`, if it has more than one.
/// A macro call or an unparsed run among the entries may write names, so a
/// list holding one is left alone.
fn positional(cx: &mut Cx, list: &ArgList, message: &str, pointing: &str) {
    let hidden = (list.syntax().children()).any(|it| matches!(it.kind(), MACRO_CALL | VERBATIM));
    let args: Vec<Arg> = list.args().collect();
    if hidden || args.len() < 2 {
        return;
    }
    let first = args.iter().find(|arg| {
        arg.dot_token().is_none() && !(arg.syntax().descendants()).any(|it| it.kind() == MACRO_CALL)
    });
    if let Some(first) = first {
        let range = Cx::range(first.syntax());
        cx.report(cx.diagnostic(range, message).pointing(pointing));
    }
}
