//! Keywords and names that are a finding wherever they stand, even in code
//! the parser kept as written.

use astli_syntax::SyntaxKind::{DEFPARAM_KW, SYSTEM_IDENT};
use astli_syntax::SyntaxToken;

use crate::rule::Cx;

/// `defparam` changes a parameter from anywhere in the hierarchy, so an
/// instance's parameters are no longer where it is instantiated. The
/// standard deprecates it.
pub(crate) fn forbid_defparam(cx: &mut Cx) {
    for keyword in tokens(cx).filter(|it| it.kind() == DEFPARAM_KW) {
        let diagnostic = cx
            .diagnostic(keyword.text_range(), "`defparam`")
            .pointing("override the parameter where the module is instantiated, `#(.Name(value))`");
        cx.report(diagnostic);
    }
}

/// `$random` and `$dist_*` draw from one generator for the whole simulation,
/// so a change anywhere changes every value after it; `$urandom` and
/// randomization are stable per thread. `$psprintf` and `$srandom` are no
/// standard's.
pub(crate) fn invalid_system_task_function(cx: &mut Cx) {
    for name in tokens(cx).filter(|it| it.kind() == SYSTEM_IDENT) {
        let instead = match name.text() {
            "$random" => "use `$urandom`",
            "$psprintf" => "use `$sformatf`",
            "$srandom" => "seed with `process::self().srandom()` or `obj.srandom()`",
            text if text.starts_with("$dist_") => "use `$urandom_range` or randomization",
            _ => continue,
        };
        let message = format!("`{}` is not to be used", name.text());
        cx.report(cx.diagnostic(name.text_range(), message).pointing(instead));
    }
}

fn tokens<'a>(cx: &Cx<'a>) -> impl Iterator<Item = SyntaxToken> + 'a {
    (cx.root().descendants_with_tokens()).filter_map(|element| element.into_token())
}
