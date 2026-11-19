//! Widths that differ where the standard extends or truncates without a
//! word, in what the tops build: an assignment's sides, a port and its
//! connection, a comparison's operands. Verilator's `WIDTHEXPAND` and
//! `WIDTHTRUNC`.

use crate::rule::DesignCx;
use crate::widths::{Width, WidthKind};

/// A value extended to a wider target, port or operand.
pub(crate) fn width_expand(cx: &mut DesignCx) {
    report(cx, false);
}

/// A value truncated to a narrower target or port.
pub(crate) fn width_trunc(cx: &mut DesignCx) {
    report(cx, true);
}

fn report(cx: &mut DesignCx, truncates: bool) {
    for width in cx.widths {
        // A macro's user can change it only in the macro.
        if width.truncates() != truncates || cx.origins.placed_by(width.source.src_id).is_some() {
            continue;
        }
        let message = message(cx, width);
        let path = &width.path;
        let diagnostic =
            (cx.diagnostic(width.source, message)).note(format!("as built in `{path}`"));
        cx.report(diagnostic);
    }
}

fn message(cx: &DesignCx, width: &Width) -> String {
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
