//! Widths that differ where the standard extends or truncates without a
//! word, in what the tops build: an assignment's sides, a port and its
//! connection, a comparison's operands. Verilator's `WIDTHEXPAND` and
//! `WIDTHTRUNC`.

use astli_sema::{Width, WidthKind};

use crate::rule::BuiltCx;

/// A value extended to a wider target, port or operand.
pub(crate) fn width_expand(cx: &mut BuiltCx) {
    report(cx, false);
}

/// A value truncated to a narrower target or port.
pub(crate) fn width_trunc(cx: &mut BuiltCx) {
    report(cx, true);
}

fn report(cx: &mut BuiltCx, truncates: bool) {
    for width in cx.widths {
        // A macro's user can change it only in the macro.
        if width.truncates() != truncates || cx.origins.placed_by(width.source.src_id).is_some() {
            continue;
        }
        let message = message(cx, width);
        let path = &width.path;
        cx.report(width.source, message)
            .notes
            .push(format!("as built in `{path}`"));
    }
}

fn message(cx: &BuiltCx, width: &Width) -> String {
    let done = if width.truncates() {
        "truncated"
    } else {
        "extended"
    };
    let (source, target) = (width.source_width, width.target_width);
    let text = |span| quoted(cx.origins.slice(span));
    match &width.kind {
        WidthKind::Assign => format!(
            "{} is {source} bits wide, and is {done} to the {target} of {}",
            text(width.source),
            text(width.target)
        ),
        WidthKind::Compare => format!(
            "{} is {source} bits wide, and is {done} to the {target} of {}, which it is compared with",
            text(width.source),
            text(width.target)
        ),
        WidthKind::Connection {
            port,
            definition,
            output,
        } => {
            // `.*` connects the name the port has.
            let connected = match cx.origins.slice(width.source).trim_start_matches('.') {
                "*" => format!("`{port}`, connected by `.*`,"),
                text => quoted(text),
            };
            match output {
                false => format!(
                    "{connected} is {source} bits wide, and is {done} to the {target} of port `{port}` of `{definition}`"
                ),
                true => format!(
                    "port `{port}` of `{definition}` is {source} bits wide, and is {done} to the {target} of {connected}"
                ),
            }
        }
    }
}

/// `text` in backquotes, or a phrase standing for it if it is long.
fn quoted(text: &str) -> String {
    if text.len() > 40 || text.contains('\n') {
        return "the expression".to_string();
    }
    format!("`{text}`")
}
