//! The document IR, and the printer that decides where its lines break.
//!
//! A rule describes a construct as a [`Doc`]: text, the places a line may
//! break, and groups whose lines break together. The printer puts a group on
//! one line if all of it fits in the width, and breaks every line of it
//! otherwise. This is Wadler's "prettier printer", as Prettier uses it.
//!
//! Separation is requested rather than written. A space or a line break waits
//! for the next text, so two requests merge into the larger one, and nothing is
//! ever written at the end of a line: no trailing whitespace, and no indent on
//! an empty line.

/// A document to lay out.
#[derive(Debug, Clone)]
#[cfg_attr(not(test), expect(dead_code, reason = "no rule breaks lines yet"))]
pub(crate) enum Doc {
    /// Text on one line.
    Text(String),
    /// At least one space, unless a line breaks here.
    Space,
    /// A space if the enclosing group is flat, a line break if it is broken.
    Line,
    /// Nothing if the enclosing group is flat, a line break if it is broken.
    SoftLine,
    /// A line break, which no enclosing group can be flat around.
    HardLine,
    /// A line break with one empty line after it. Two never make two empty
    /// lines: they merge like any other separation.
    BlankLine,
    /// Flat if all of it fits on the line, broken if not.
    Group(Box<Doc>),
    /// Lines broken inside start one level further in.
    Indent(Box<Doc>),
    /// Lines broken inside start at column 0, however far in the rest is.
    Margin(Box<Doc>),
    Concat(Vec<Doc>),
    Verbatim(Verbatim),
}

impl Doc {
    pub(crate) fn text(text: impl Into<String>) -> Doc {
        Doc::Text(text.into())
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "no rule breaks lines yet"))]
    pub(crate) fn group(doc: Doc) -> Doc {
        Doc::Group(Box::new(doc))
    }

    pub(crate) fn indent(doc: Doc) -> Doc {
        Doc::Indent(Box::new(doc))
    }

    pub(crate) fn margin(doc: Doc) -> Doc {
        Doc::Margin(Box::new(doc))
    }

    pub(crate) fn concat(docs: impl IntoIterator<Item = Doc>) -> Doc {
        Doc::Concat(docs.into_iter().collect())
    }

    /// Nothing at all.
    pub(crate) fn nil() -> Doc {
        Doc::Concat(Vec::new())
    }

    /// The text of one token, which may run over lines: a block comment, or a
    /// string continued with `\`. Its later lines are inside the token, so they
    /// are written as they were.
    pub(crate) fn token(text: &str) -> Doc {
        match text.split_once('\n') {
            None => Doc::text(text),
            Some((first, rest)) => Doc::Verbatim(Verbatim {
                // Only moved lines read it, and none is.
                column: 0,
                first: first.to_owned(),
                rest: rest
                    .split('\n')
                    .map(|line| VerbatimLine::Kept(line.to_owned()))
                    .collect(),
            }),
        }
    }
}

/// Text no rule laid out, kept as written but moved as a block: every line
/// shifts by as much as the first one did.
#[derive(Debug, Clone)]
pub(crate) struct Verbatim {
    /// The column the first line started at in the input.
    pub column: u32,
    /// The first line, from where it started.
    pub first: String,
    pub rest: Vec<VerbatimLine>,
}

#[derive(Debug, Clone)]
pub(crate) enum VerbatimLine {
    /// A line whose text starts at column `indent` in the input, `text` being
    /// what follows its leading whitespace.
    Moved { indent: u32, text: String },
    /// A line that starts inside a token or a `` `define ``, where leading
    /// whitespace is content. Written exactly as it was.
    Kept(String),
}

/// What the printer is told: the knobs D7 in `docs/plan.md` allows, less
/// alignment, which is a pass after it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Layout {
    pub width: usize,
    pub indent: usize,
}

/// Lays `doc` out, ending in exactly one newline unless it is empty.
pub(crate) fn print(doc: &Doc, layout: Layout) -> String {
    let mut printer = Printer {
        layout,
        out: String::new(),
        column: 0,
        gap: Gap::None,
    };
    printer.run(doc);
    if !printer.out.is_empty() {
        printer.out.push('\n');
    }
    printer.out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

/// The separation requested since the last text.
#[derive(Debug, Clone, Copy)]
enum Gap {
    None,
    Space,
    /// `count` newlines, then `indent` columns, as the latest request asked.
    Lines {
        count: usize,
        indent: usize,
    },
}

/// A document still to print: its indentation, and the mode its lines are in.
type Command<'a> = (usize, Mode, &'a Doc);

struct Printer {
    layout: Layout,
    out: String,
    column: usize,
    gap: Gap,
}

impl Printer {
    fn run(&mut self, doc: &Doc) {
        let mut stack: Vec<Command> = vec![(0, Mode::Break, doc)];
        while let Some((indent, mode, doc)) = stack.pop() {
            match doc {
                Doc::Text(text) => self.text(text),
                Doc::Space => self.space(),
                Doc::Line if mode == Mode::Flat => self.space(),
                Doc::Line | Doc::HardLine => self.lines(1, indent),
                Doc::SoftLine if mode == Mode::Flat => {}
                Doc::SoftLine => self.lines(1, indent),
                Doc::BlankLine => self.lines(2, indent),
                Doc::Group(inner) => {
                    let flat = mode == Mode::Flat || self.fits(inner, &stack);
                    let mode = if flat { Mode::Flat } else { Mode::Break };
                    stack.push((indent, mode, inner));
                }
                Doc::Indent(inner) => stack.push((indent + self.layout.indent, mode, inner)),
                Doc::Margin(inner) => stack.push((0, mode, inner)),
                Doc::Concat(docs) => stack.extend(docs.iter().rev().map(|doc| (indent, mode, doc))),
                Doc::Verbatim(verbatim) => self.verbatim(verbatim),
            }
        }
    }

    /// Whether `group`, flat, fits in what is left of the line, along with
    /// whatever follows it up to the next line break.
    fn fits(&self, group: &Doc, rest: &[Command]) -> bool {
        // The separation still to come merges as the printer would merge it.
        let mut gap = self.gap;
        let start = match gap {
            Gap::Lines { indent, .. } => indent,
            Gap::None | Gap::Space => self.column,
        };
        let mut left = self.layout.width as isize - start as isize;
        let mut todo = vec![(Mode::Flat, group)];
        let mut rest = rest.iter().rev();
        loop {
            let Some((mode, doc)) = todo
                .pop()
                .or_else(|| rest.next().map(|&(_, mode, doc)| (mode, doc)))
            else {
                return true;
            };
            let text = match doc {
                Doc::Text(text) => text,
                Doc::Line | Doc::SoftLine if mode == Mode::Break => return true,
                Doc::Space | Doc::Line => {
                    if let Gap::None = gap {
                        gap = Gap::Space;
                    }
                    continue;
                }
                Doc::SoftLine => continue,
                Doc::HardLine | Doc::BlankLine => return mode == Mode::Break,
                Doc::Group(inner) | Doc::Indent(inner) | Doc::Margin(inner) => {
                    todo.push((mode, inner));
                    continue;
                }
                Doc::Concat(docs) => {
                    todo.extend(docs.iter().rev().map(|doc| (mode, doc)));
                    continue;
                }
                Doc::Verbatim(verbatim) if !verbatim.rest.is_empty() => {
                    left -= width(&verbatim.first) as isize + spaced(gap);
                    return left >= 0 && mode == Mode::Break;
                }
                Doc::Verbatim(verbatim) => &verbatim.first,
            };
            if !text.is_empty() {
                left -= width(text) as isize + spaced(gap);
                gap = Gap::None;
            }
            if left < 0 {
                return false;
            }
        }
    }

    fn space(&mut self) {
        if let Gap::None = self.gap {
            self.gap = Gap::Space;
        }
    }

    fn lines(&mut self, count: usize, indent: usize) {
        let count = match self.gap {
            Gap::Lines { count: before, .. } => before.max(count),
            Gap::None | Gap::Space => count,
        };
        self.gap = Gap::Lines { count, indent };
    }

    fn text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.flush();
        self.out.push_str(text);
        self.column += width(text);
    }

    /// Writes the requested separation. None goes before the first text.
    fn flush(&mut self) {
        match std::mem::replace(&mut self.gap, Gap::None) {
            Gap::None => {}
            Gap::Space if self.out.is_empty() => {}
            Gap::Space => {
                self.out.push(' ');
                self.column += 1;
            }
            Gap::Lines { count, indent } => {
                if !self.out.is_empty() {
                    self.out.extend(std::iter::repeat_n('\n', count));
                }
                self.out.extend(std::iter::repeat_n(' ', indent));
                self.column = indent;
            }
        }
    }

    fn verbatim(&mut self, verbatim: &Verbatim) {
        self.flush();
        let shift = self.column as i64 - i64::from(verbatim.column);
        self.text(&verbatim.first);
        for line in &verbatim.rest {
            self.out.push('\n');
            self.column = 0;
            match line {
                VerbatimLine::Kept(text) => self.text(text),
                VerbatimLine::Moved { text, .. } if text.is_empty() => {}
                VerbatimLine::Moved { indent, text } => {
                    let indent = (i64::from(*indent) + shift).max(0) as usize;
                    self.out.extend(std::iter::repeat_n(' ', indent));
                    self.column = indent;
                    self.text(text);
                }
            }
        }
    }
}

/// The columns a pending gap adds in front of text on the same line.
fn spaced(gap: Gap) -> isize {
    isize::from(matches!(gap, Gap::Space))
}

/// Columns, counting a character as one.
fn width(text: &str) -> usize {
    text.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print_in(width: usize, docs: impl IntoIterator<Item = Doc>) -> String {
        print(&Doc::concat(docs), Layout { width, indent: 2 })
    }

    fn text(text: &str) -> Doc {
        Doc::text(text)
    }

    /// `name (a, b, …);`, broken one per line when it does not fit.
    fn list(name: &str, items: &[&str]) -> Doc {
        let mut inner = vec![Doc::SoftLine];
        for (at, item) in items.iter().enumerate() {
            if at > 0 {
                inner.extend([text(","), Doc::Line]);
            }
            inner.push(text(item));
        }
        Doc::group(Doc::concat([
            text(name),
            Doc::Space,
            text("("),
            Doc::indent(Doc::concat(inner)),
            Doc::SoftLine,
            text(");"),
        ]))
    }

    #[test]
    fn a_group_that_fits_stays_on_one_line() {
        assert_eq!(print_in(20, [list("m", &["a", "b"])]), "m (a, b);\n");
    }

    #[test]
    fn a_group_that_does_not_fit_breaks_every_line() {
        let printed = print_in(12, [list("m", &["alpha", "beta"])]);
        assert_eq!(printed, "m (\n  alpha,\n  beta\n);\n");
    }

    #[test]
    fn a_line_exactly_the_width_fits() {
        assert_eq!(print_in(9, [list("m", &["a", "b"])]), "m (a, b);\n");
        assert_eq!(
            print_in(8, [list("m", &["a", "b"])]),
            "m (\n  a,\n  b\n);\n"
        );
    }

    #[test]
    fn what_follows_a_group_on_its_line_counts_against_it() {
        let docs = [list("m", &["a", "b"]), text("// note")];
        assert_eq!(print_in(12, docs), "m (\n  a,\n  b\n);// note\n");
    }

    #[test]
    fn a_space_before_a_group_and_one_opening_it_count_once() {
        let group = Doc::group(Doc::concat([
            Doc::Space,
            text("c"),
            Doc::SoftLine,
            text("d"),
        ]));
        assert_eq!(print_in(5, [text("ab"), Doc::Space, group]), "ab cd\n");
    }

    #[test]
    fn an_inner_group_that_fits_stays_flat_inside_a_broken_one() {
        let inner = list("u", &["x", "y"]);
        let outer = Doc::group(Doc::concat([
            text("begin"),
            Doc::indent(Doc::concat([Doc::Line, inner])),
            Doc::Line,
            text("end"),
        ]));
        let printed = print_in(14, [outer]);
        assert_eq!(printed, "begin\n  u (x, y);\nend\n");
    }

    #[test]
    fn a_line_at_the_margin_ignores_the_indentation() {
        let docs = [
            text("module m;"),
            Doc::indent(Doc::concat([
                Doc::margin(Doc::concat([Doc::HardLine, text("`ifdef X")])),
                Doc::HardLine,
                text("a;"),
            ])),
        ];
        assert_eq!(print_in(80, docs), "module m;\n`ifdef X\n  a;\n");
    }

    #[test]
    fn a_hard_line_breaks_its_group() {
        let group = Doc::group(Doc::concat([
            text("a"),
            Doc::HardLine,
            text("b"),
            Doc::Line,
            text("c"),
        ]));
        assert_eq!(print_in(80, [group]), "a\nb\nc\n");
    }

    #[test]
    fn nothing_is_written_at_the_end_of_a_line() {
        let docs = [
            Doc::indent(Doc::concat([
                text("a"),
                Doc::Space,
                Doc::HardLine,
                Doc::BlankLine,
            ])),
            text("b"),
            Doc::Space,
        ];
        // The break was requested inside the indent, so `b` takes it.
        assert_eq!(print_in(80, docs), "a\n\n  b\n");
    }

    #[test]
    fn requested_lines_merge_into_the_largest() {
        let docs = [
            text("a"),
            Doc::BlankLine,
            Doc::BlankLine,
            Doc::HardLine,
            text("b"),
            Doc::HardLine,
            Doc::Space,
            text("c"),
        ];
        assert_eq!(print_in(80, docs), "a\n\nb\nc\n");
    }

    #[test]
    fn spaces_merge_and_the_file_starts_with_text() {
        let docs = [
            Doc::BlankLine,
            Doc::Space,
            text("a"),
            Doc::Space,
            Doc::Space,
            text("b"),
        ];
        assert_eq!(print_in(80, docs), "a b\n");
    }

    #[test]
    fn an_empty_document_prints_nothing() {
        assert_eq!(print_in(80, [Doc::HardLine]), "");
    }

    #[test]
    fn a_verbatim_run_moves_as_a_block() {
        let verbatim = Doc::Verbatim(Verbatim {
            column: 4,
            first: "covergroup cg;".into(),
            rest: vec![
                VerbatimLine::Moved {
                    indent: 8,
                    text: "coverpoint a;".into(),
                },
                VerbatimLine::Moved {
                    indent: 0,
                    text: String::new(),
                },
                VerbatimLine::Moved {
                    indent: 1,
                    text: "// far left".into(),
                },
                VerbatimLine::Kept("    in a string  ".into()),
                VerbatimLine::Moved {
                    indent: 4,
                    text: "endgroup".into(),
                },
            ],
        });
        let docs = [
            text("module m;"),
            Doc::indent(Doc::concat([Doc::HardLine, verbatim])),
            Doc::HardLine,
            text("endmodule"),
        ];
        assert_eq!(
            print_in(80, docs),
            "module m;\n  covergroup cg;\n      coverpoint a;\n\n// far left\n    in a string  \n  endgroup\nendmodule\n"
        );
    }

    #[test]
    fn a_verbatim_run_of_several_lines_breaks_its_group() {
        let verbatim = Doc::Verbatim(Verbatim {
            column: 0,
            first: "x".into(),
            rest: vec![VerbatimLine::Kept("y".into())],
        });
        let group = Doc::group(Doc::concat([text("a"), Doc::Line, verbatim]));
        assert_eq!(print_in(80, [group]), "a\nx\ny\n");
    }
}
