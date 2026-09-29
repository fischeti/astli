//! A file named for what it declares, and routines with their lifetime
//! written out.

use std::path::Path;

use astli_syntax::SyntaxKind;
use astli_syntax::SyntaxKind::{AUTOMATIC_KW, CLASS_DECL, FUNCTION_DECL, STATIC_KW, TASK_DECL};
use astli_syntax::ast::{AstNode, ModuleDecl, PackageDecl};

use crate::rule::Cx;

/// A module is found by its file's name, so one of the modules a file
/// declares is named as the file is, up to its first dot.
pub(crate) fn module_filename(cx: &mut Cx) {
    let stem = stem(cx.tree.path());
    let mut names =
        (cx.root().descendants().filter_map(ModuleDecl::cast)).filter_map(|it| it.name());
    let Some(first) = names.next() else {
        return;
    };
    if first.text() == stem || names.any(|name| name.text() == stem) {
        return;
    }
    let message = format!("no module is named `{stem}`, as the file is");
    cx.report(
        cx.diagnostic(first.text_range(), message)
            .pointing("rename the file or the module"),
    );
}

/// A package is found by its file's name, so it is named as the file is, up
/// to its first dot.
pub(crate) fn package_filename(cx: &mut Cx) {
    let stem = stem(cx.tree.path());
    let names = (cx.root().descendants().filter_map(PackageDecl::cast)).filter_map(|it| it.name());
    for name in names.filter(|name| name.text() != stem) {
        let message = format!("package `{name}` is not named `{stem}`, as the file is");
        cx.report(
            cx.diagnostic(name.text_range(), message)
                .pointing("rename the file or the package"),
        );
    }
}

/// A function or task outside a class is `static` unless it says otherwise,
/// so a call that recurses, or two that overlap in time, share one set of
/// variables. Writing the lifetime out says which was meant.
fn explicit_lifetime(cx: &mut Cx, routine: SyntaxKind, what: &str) {
    let routines = cx.root().descendants().filter(|it| it.kind() == routine);
    for node in routines.filter(|it| !it.ancestors().any(|it| it.kind() == CLASS_DECL)) {
        let mut tokens = (node.children_with_tokens())
            .filter_map(|element| element.into_token())
            .filter(|token| !token.kind().is_trivia());
        let Some(keyword) = tokens.next() else {
            continue;
        };
        if tokens
            .next()
            .is_some_and(|it| matches!(it.kind(), AUTOMATIC_KW | STATIC_KW))
        {
            continue;
        }
        let message = format!("a {what} without `automatic` or `static`");
        cx.report(
            cx.diagnostic(keyword.text_range(), message)
                .pointing(format!("write `{what} automatic`")),
        );
    }
}

pub(crate) fn explicit_function_lifetime(cx: &mut Cx) {
    explicit_lifetime(cx, FUNCTION_DECL, "function");
}

pub(crate) fn explicit_task_lifetime(cx: &mut Cx) {
    explicit_lifetime(cx, TASK_DECL, "task");
}

/// The file's name up to its first dot, as `foo` for `foo.sv` or `foo.pkg.sv`.
fn stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|it| it.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.split('.').next().unwrap_or_default().to_string()
}
