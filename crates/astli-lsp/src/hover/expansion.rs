//! What a macro call expands to, as hover shows it.
//!
//! The expansion is laid out by the formatter where it parses as what a
//! macro commonly stands for. Neither the parser nor the formatter takes a
//! fragment, so the expansion is put in a wrapper that makes it a file,
//! that file is formatted, and the wrapper, as the formatter writes it, is
//! taken off again.

use astli_parse::SyntaxTree;

use super::cut;

/// At most this many lines of what a macro call expands to.
const LINES: usize = 20;
/// At most this many characters of what a macro call expands to, when it
/// lays out as none of the [`FORMS`].
const CHARACTERS: usize = 400;

/// A wrapper an expansion may parse in, and the text it is formatted to
/// around the expansion.
struct Form {
    /// What is written before and after the expansion.
    around: (&'static str, &'static str),
    /// What the formatter writes before and after it.
    laid: (&'static str, &'static str),
    /// How far the formatter indents the expansion's lines.
    indent: usize,
    /// Another wrapper the expansion must parse in too, where the parser
    /// is lenient in `around`.
    strict: Option<(&'static str, &'static str)>,
}

/// The forms an expansion is tried as, in order. Each puts the expansion
/// at the start of a line, indented by the wrapper alone: the formatter
/// aligns a broken expression or list under its start and indents a body
/// from its line's indentation, so only there do both come off as one
/// indentation. Each also holds it inside a module, so that an expansion
/// that closes the wrapper and opens another leaves a line short of the
/// indentation, and is refused. The formatter's text around it is matched
/// whole, so a layout the wrapper does not predict is refused too, rather
/// than cut in the wrong place.
///
/// What parses in two forms lays out alike in both, as a name does, an
/// expression and a type, or a declaration, a module item and a statement;
/// but for a name with a select, `a[3:0]`, which as a type takes a space
/// before its `[`. A select of a signal is the likelier, so expressions go
/// before types.
const FORMS: &[Form] = &[
    // Module items, a struct's members among them.
    Form {
        around: ("module m;\n", "\nendmodule\n"),
        laid: ("module m;\n", "\nendmodule\n"),
        indent: 2,
        strict: None,
    },
    Form {
        around: ("module m;\ninitial begin\n", "\nend\nendmodule\n"),
        laid: ("module m;\n  initial begin\n", "\n  end\nendmodule\n"),
        indent: 4,
        strict: None,
    },
    // An expression starts a line as a case item's. The parser takes a
    // case item with no `:`, so `a b` would pass for one there.
    Form {
        around: ("module m;\ninitial case (x)\n", ": ;\nendcase\nendmodule\n"),
        laid: (
            "module m;\n  initial case (x)\n",
            ": ;\n  endcase\nendmodule\n",
        ),
        indent: 4,
        strict: Some(("module m;\ninitial x = ", ";\nendmodule\n")),
    },
    // A data type, as a declaration's.
    Form {
        around: ("module m;\n", " x;\nendmodule\n"),
        laid: ("module m;\n", " x;\nendmodule\n"),
        indent: 2,
        strict: None,
    },
    // Ports and parameters, which a module's header breaks one to a line.
    Form {
        around: ("module m (\n", "\n);\nendmodule\n"),
        laid: ("module m (\n", "\n);\nendmodule\n"),
        indent: 2,
        strict: None,
    },
    Form {
        around: ("module m #(\n", "\n);\nendmodule\n"),
        laid: ("module m #(\n", "\n);\nendmodule\n"),
        indent: 2,
        strict: None,
    },
];

/// What a macro call expands to, laid out as the first of the [`FORMS`]
/// it parses as, or else as written, tidied and cut short.
pub(super) fn text(wrote: &str) -> String {
    match FORMS.iter().find_map(|form| laid_out(form, wrote)) {
        Some(it) => cut(&it, LINES),
        None => tidied(wrote),
    }
}

/// `text` formatted in `form`, where it parses there with no error.
fn laid_out(form: &Form, text: &str) -> Option<String> {
    let parsed = |(before, after): (&str, &str)| {
        let tree = SyntaxTree::parse("", format!("{before}{text}{after}"));
        (tree.diagnostics().is_empty() && tree.unparsed().is_empty()).then_some(tree)
    };
    let tree = parsed(form.around)?;
    if let Some(strict) = form.strict {
        parsed(strict)?;
    }
    let formatted = astli_fmt::format(&tree, &astli_fmt::Options::default()).ok()?;
    let (before, after) = form.laid;
    let inside = formatted.strip_prefix(before)?.strip_suffix(after)?;
    let indent = " ".repeat(form.indent);
    let mut lines = Vec::new();
    for line in inside.split('\n') {
        match line.strip_prefix(&indent) {
            Some(it) => lines.push(it),
            None if line.is_empty() => lines.push(""),
            None => return None,
        }
    }
    let out = lines.join("\n");
    let whole = !out.trim().is_empty() && !out.ends_with(char::is_whitespace);
    whole.then_some(out)
}

/// `text` as written, where nothing lays it out: each line without the
/// spaces a `\` left at its end, the lines after the first without the
/// indentation they share, as the first starts where the body did, and cut
/// at the end of the last line that fits.
fn tidied(text: &str) -> String {
    let lines: Vec<&str> = text.trim().lines().map(str::trim_end).collect();
    let indent = (lines.iter().skip(1))
        .filter(|it| !it.is_empty())
        .map(|it| it.len() - it.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out = String::new();
    for (at, line) in lines.iter().enumerate() {
        let line = if at == 0 {
            line
        } else {
            line.get(indent..).unwrap_or("")
        };
        let next = out.chars().count() + line.chars().count() + 1;
        if at == LINES || (at > 0 && next > CHARACTERS) {
            return out + "\n…";
        }
        if at > 0 {
            out.push('\n');
        }
        // A first line too long alone is cut at the end of a word.
        match line.char_indices().nth(CHARACTERS) {
            Some((end, _)) => {
                let end = line[..end].rfind(char::is_whitespace).unwrap_or(end);
                return out + line[..end].trim_end() + " …";
            }
            None => out += line,
        }
    }
    out
}
