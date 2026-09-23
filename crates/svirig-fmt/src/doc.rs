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
//!
//! A [`Doc::Fill`] packs its parts instead, breaking only where the next one
//! would not fit.
//!
//! Alignment comes after: the printer notes where each [`Doc::Cell`] ended
//! up, and [`align`] pads the text it wrote.

use crate::align::{Cell, Continuation, align};

/// A document to lay out.
#[derive(Debug, Clone)]
pub(crate) enum Doc {
    /// Text on one line.
    Text(String),
    /// At least one space, unless a line breaks here. Text that ends in one,
    /// an escaped identifier with the space that ends it, needs no other.
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
    /// Lines broken inside start at the column its first text does, under
    /// it.
    Align(Box<Doc>),
    /// Parts and the separators between them, alternating, each separator a
    /// line break only if the part after it does not fit on the line.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "no rule breaks an expression yet")
    )]
    Fill(Vec<Doc>),
    /// Rows whose cells line up, a row being the cells on one line.
    Table(Box<Doc>),
    /// The end of a cell in the given column of the innermost enclosing
    /// table. Outside one, it is nothing.
    Cell(usize),
    Concat(Vec<Doc>),
    Verbatim(Verbatim),
}

impl Doc {
    pub(crate) fn text(text: impl Into<String>) -> Doc {
        Doc::Text(text.into())
    }

    pub(crate) fn group(doc: Doc) -> Doc {
        Doc::Group(Box::new(doc))
    }

    pub(crate) fn indent(doc: Doc) -> Doc {
        Doc::Indent(Box::new(doc))
    }

    pub(crate) fn margin(doc: Doc) -> Doc {
        Doc::Margin(Box::new(doc))
    }

    pub(crate) fn align(doc: Doc) -> Doc {
        Doc::Align(Box::new(doc))
    }

    pub(crate) fn table(doc: Doc) -> Doc {
        Doc::Table(Box::new(doc))
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
                // Only moved lines read them, and none is.
                column: 0,
                indent: 0,
                first: first.to_owned(),
                rest: rest
                    .split('\n')
                    .map(|line| VerbatimLine::Kept(line.to_owned()))
                    .collect(),
            }),
        }
    }
}

/// Text no rule laid out, kept as written but moved as a block. A later line
/// that starts left of the first hangs off the indentation of the line the
/// first is on, so every later line shifts by as much as that indentation
/// did; otherwise each is aligned under something on the first line, and
/// shifts by as much as the first line's start.
#[derive(Debug, Clone)]
pub(crate) struct Verbatim {
    /// The column the first line started at in the input.
    pub column: u32,
    /// The indentation of the line the first line is on, in the input.
    pub indent: u32,
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

/// What the printer is told: the knobs D7 in `docs/plan.md` allows.
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
        line: 0,
        indent: 0,
        block: 0,
        tables: 0,
        anchor: None,
        cells: Vec::new(),
        continuations: Vec::new(),
    };
    printer.run(doc);
    if !printer.out.is_empty() {
        printer.out.push('\n');
    }
    // Stable, so the lines placed by one line stay in order.
    printer.continuations.sort_by_key(|it| it.line);
    align(
        printer.out,
        printer.cells,
        &printer.continuations,
        layout.width,
    )
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
        anchor: Option<Anchor>,
    },
}

/// The text an aligned line is placed under: the line it is on, and where it
/// starts in bytes. Padding that moves it moves the aligned line too.
#[derive(Debug, Clone, Copy)]
struct Anchor {
    line: usize,
    start: usize,
}

/// A document still to print, and what it inherits from around it.
#[derive(Clone, Copy)]
struct Command<'a> {
    indent: usize,
    /// What the lines it breaks are aligned under, if anything.
    anchor: Option<Anchor>,
    mode: Mode,
    /// The innermost table it is in.
    table: Option<usize>,
    doc: Work<'a>,
}

/// What a command prints.
#[derive(Clone, Copy)]
enum Work<'a> {
    Doc(&'a Doc),
    /// The parts of a [`Doc::Fill`] still to print, from a part on.
    Fill(&'a [Doc]),
}

impl<'a> Command<'a> {
    /// `doc`, inheriting what this command does.
    fn inner(self, doc: &'a Doc) -> Command<'a> {
        Command {
            doc: Work::Doc(doc),
            ..self
        }
    }
}

struct Printer {
    layout: Layout,
    out: String,
    column: usize,
    gap: Gap,
    /// The line being written, counting from 0.
    line: usize,
    /// The indentation of the line being written.
    indent: usize,
    /// The empty lines written so far, since each ends every table.
    block: usize,
    /// The tables entered so far.
    tables: usize,
    /// What the line being written is aligned under, if anything.
    anchor: Option<Anchor>,
    cells: Vec<Cell>,
    continuations: Vec<Continuation>,
}

impl Printer {
    fn run(&mut self, doc: &Doc) {
        let mut stack = vec![Command {
            indent: 0,
            anchor: None,
            mode: Mode::Break,
            table: None,
            doc: Work::Doc(doc),
        }];
        while let Some(command) = stack.pop() {
            let Command {
                indent,
                anchor,
                mode,
                ..
            } = command;
            let doc = match command.doc {
                Work::Doc(doc) => doc,
                Work::Fill(parts) => {
                    self.fill(command, parts, &mut stack);
                    continue;
                }
            };
            match doc {
                Doc::Text(text) => self.text(text),
                Doc::Space => self.space(),
                Doc::Line if mode == Mode::Flat => self.space(),
                Doc::Line | Doc::HardLine => self.lines(1, indent, anchor),
                Doc::SoftLine if mode == Mode::Flat => {}
                Doc::SoftLine => self.lines(1, indent, anchor),
                Doc::BlankLine => self.lines(2, indent, anchor),
                Doc::Group(group) => {
                    let flat = mode == Mode::Flat || self.fits(std::slice::from_ref(group), &stack);
                    let mode = if flat { Mode::Flat } else { Mode::Break };
                    stack.push(Command {
                        mode,
                        ..command.inner(group)
                    });
                }
                Doc::Indent(doc) => stack.push(Command {
                    indent: indent + self.layout.indent,
                    ..command.inner(doc)
                }),
                Doc::Margin(doc) => stack.push(Command {
                    indent: 0,
                    anchor: None,
                    ..command.inner(doc)
                }),
                Doc::Align(doc) => {
                    let (indent, anchor) = self.alignment();
                    stack.push(Command {
                        indent,
                        anchor,
                        ..command.inner(doc)
                    });
                }
                Doc::Fill(parts) => self.fill(command, parts, &mut stack),
                Doc::Table(doc) => {
                    self.tables += 1;
                    stack.push(Command {
                        table: Some(self.tables),
                        ..command.inner(doc)
                    });
                }
                Doc::Cell(index) => self.cell(command.table, *index),
                Doc::Concat(docs) => stack.extend(docs.iter().rev().map(|doc| command.inner(doc))),
                Doc::Verbatim(verbatim) => self.verbatim(verbatim),
            }
        }
    }

    /// Queues the first of a fill's `parts`, flat if it fits, and the
    /// separator after it, broken unless the part after that fits on the line
    /// too; then the rest of the fill, to be decided when it is reached.
    fn fill<'a>(&self, command: Command<'a>, parts: &'a [Doc], stack: &mut Vec<Command<'a>>) {
        let queue = |mode, doc| Command {
            mode,
            ..command.inner(doc)
        };
        if command.mode == Mode::Flat {
            stack.extend(parts.iter().rev().map(|doc| queue(Mode::Flat, doc)));
            return;
        }
        let mode = |flat| if flat { Mode::Flat } else { Mode::Break };
        // Only the last part has what follows the fill after it on its line.
        let after = |last: bool| if last { &stack[..] } else { &[] };
        match parts {
            [] => {}
            [part] => {
                let flat = self.fits(&parts[..1], after(true));
                stack.push(queue(mode(flat), part));
            }
            [part, separator, rest @ ..] => {
                let flat = self.fits(&parts[..1], after(false));
                let both = match rest {
                    [] => flat,
                    [_, more @ ..] => self.fits(&parts[..3], after(more.is_empty())),
                };
                if !rest.is_empty() {
                    stack.push(Command {
                        doc: Work::Fill(rest),
                        ..command
                    });
                }
                stack.push(queue(mode(both), separator));
                stack.push(queue(mode(flat), part));
            }
        }
    }

    /// Where lines broken in an aligned group start: the column the next text
    /// does. Padding moves them with that text, or, if its line is aligned
    /// itself, with what that line is aligned under.
    fn alignment(&self) -> (usize, Option<Anchor>) {
        match self.gap {
            Gap::Lines { indent, anchor, .. } => (indent, anchor),
            Gap::None | Gap::Space => {
                let space = usize::from(matches!(self.gap, Gap::Space) && !self.out.is_empty());
                let anchor = self.anchor.unwrap_or(Anchor {
                    line: self.line,
                    start: self.out.len() + space,
                });
                (self.column + space, Some(anchor))
            }
        }
    }

    /// Whether `docs`, flat, fit in what is left of the line, along with
    /// whatever follows them up to the next line break.
    fn fits(&self, docs: &[Doc], rest: &[Command]) -> bool {
        // The separation still to come merges as the printer would merge it.
        let mut gap = self.gap;
        let start = match gap {
            Gap::Lines { indent, .. } => indent,
            Gap::None | Gap::Space => self.column,
        };
        let mut left = self.layout.width as isize - start as isize;
        let mut todo: Vec<(Mode, &Doc)> = docs.iter().rev().map(|doc| (Mode::Flat, doc)).collect();
        let mut rest = rest.iter().rev();
        loop {
            let Some((mode, doc)) = todo.pop() else {
                let Some(command) = rest.next() else {
                    return true;
                };
                match command.doc {
                    Work::Doc(doc) => todo.push((command.mode, doc)),
                    Work::Fill(parts) => {
                        todo.extend(parts.iter().rev().map(|doc| (command.mode, doc)))
                    }
                }
                continue;
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
                Doc::SoftLine | Doc::Cell(_) => continue,
                Doc::HardLine | Doc::BlankLine => return mode == Mode::Break,
                Doc::Group(inner)
                | Doc::Indent(inner)
                | Doc::Margin(inner)
                | Doc::Align(inner)
                | Doc::Table(inner) => {
                    todo.push((mode, inner));
                    continue;
                }
                Doc::Concat(docs) | Doc::Fill(docs) => {
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

    /// Notes where a cell of `table` ends: after the last text, and before
    /// any separation still pending. A cell at the start of a line has nothing
    /// before it to align.
    fn cell(&mut self, table: Option<usize>, index: usize) {
        let Some(table) = table else {
            return;
        };
        if let Gap::Lines { .. } = self.gap {
            return;
        }
        self.cells.push(Cell {
            table,
            index,
            block: self.block,
            line: self.line,
            offset: self.out.len(),
            column: self.column,
        });
    }

    fn space(&mut self) {
        if let Gap::None = self.gap {
            self.gap = Gap::Space;
        }
    }

    fn lines(&mut self, count: usize, indent: usize, anchor: Option<Anchor>) {
        let count = match self.gap {
            Gap::Lines { count: before, .. } => before.max(count),
            Gap::None | Gap::Space => count,
        };
        self.gap = Gap::Lines {
            count,
            indent,
            anchor,
        };
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
            Gap::Space if self.out.is_empty() || self.out.ends_with(' ') => {}
            Gap::Space => {
                self.out.push(' ');
                self.column += 1;
            }
            Gap::Lines {
                count,
                indent,
                anchor,
            } => {
                if !self.out.is_empty() {
                    self.out.extend(std::iter::repeat_n('\n', count));
                    self.line += count;
                    self.block += usize::from(count > 1);
                    if let Some(anchor) = anchor {
                        self.continuations.push(Continuation {
                            line: anchor.line,
                            start: anchor.start,
                            offset: self.out.len(),
                        });
                    }
                }
                self.anchor = anchor;
                self.out.extend(std::iter::repeat_n(' ', indent));
                self.column = indent;
                self.indent = indent;
            }
        }
    }

    fn verbatim(&mut self, verbatim: &Verbatim) {
        self.flush();
        let hanging = verbatim.rest.iter().any(|line| {
            matches!(line, VerbatimLine::Moved { indent, text } if !text.is_empty() && *indent < verbatim.column)
        });
        let shift = match hanging {
            true => self.indent as i64 - i64::from(verbatim.indent),
            false => self.column as i64 - i64::from(verbatim.column),
        };
        // A hanging line stays put when padding moves the first, unless the
        // line it hangs off moves.
        let anchor = match hanging {
            true => self.anchor,
            false => Some(self.anchor.unwrap_or(Anchor {
                line: self.line,
                start: self.out.len(),
            })),
        };
        self.text(&verbatim.first);
        for line in &verbatim.rest {
            self.out.push('\n');
            self.column = 0;
            self.indent = 0;
            self.line += 1;
            self.anchor = None;
            match line {
                VerbatimLine::Kept(text) => self.text(text),
                VerbatimLine::Moved { text, .. } if text.is_empty() => self.block += 1,
                VerbatimLine::Moved { indent, text } => {
                    let indent = (i64::from(*indent) + shift).max(0) as usize;
                    if let Some(anchor) = anchor {
                        self.continuations.push(Continuation {
                            line: anchor.line,
                            start: anchor.start,
                            offset: self.out.len(),
                        });
                    }
                    self.anchor = anchor;
                    self.out.extend(std::iter::repeat_n(' ', indent));
                    self.column = indent;
                    self.indent = indent;
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
    fn text_that_ends_in_a_space_takes_no_other() {
        let docs = [text("\\a+b "), Doc::Space, text("=")];
        assert_eq!(print_in(80, docs), "\\a+b =\n");
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

    /// `rows`, each on a line of its own, its cells split at `|`.
    fn table(rows: &[&str]) -> Doc {
        let mut docs = Vec::new();
        for row in rows {
            docs.push(Doc::HardLine);
            for (at, cell) in row.split('|').enumerate() {
                if at > 0 {
                    docs.extend([Doc::Cell(at - 1), Doc::Space]);
                }
                docs.push(text(cell));
            }
        }
        Doc::table(Doc::concat(docs))
    }

    #[test]
    fn cells_line_up_column_by_column() {
        let rows = table(&["logic|a|= 0;", "logic [7:0]|bb|= 1;", "int|c;"]);
        assert_eq!(
            print_in(80, [rows]),
            "logic       a  = 0;\nlogic [7:0] bb = 1;\nint         c;\n"
        );
    }

    #[test]
    fn a_cell_outside_a_table_is_nothing() {
        let docs = [
            text("a"),
            Doc::Cell(0),
            Doc::HardLine,
            text("bbb"),
            Doc::Cell(0),
            text("c"),
        ];
        assert_eq!(print_in(80, docs), "a\nbbbc\n");
    }

    #[test]
    fn a_cell_at_the_end_of_its_line_is_not_padded() {
        let rows = table(&["a|", "bbb|c"]);
        assert_eq!(print_in(80, [rows]), "a\nbbb c\n");
    }

    #[test]
    fn tables_align_apart() {
        let docs = [table(&["a|x", "bbb|y"]), table(&["cc|z"])];
        assert_eq!(print_in(80, docs), "a   x\nbbb y\ncc z\n");
    }

    #[test]
    fn broken_lines_align_under_the_first_text() {
        let call = Doc::group(Doc::concat([
            text("x ="),
            Doc::Space,
            Doc::align(Doc::concat([
                text("alpha &&"),
                Doc::Line,
                text("f("),
                Doc::align(Doc::concat([text("beta,"), Doc::Line, text("gamma")])),
                text(")"),
            ])),
            text(";"),
        ]));
        assert_eq!(
            print_in(16, [Doc::indent(call)]),
            "x = alpha &&\n    f(beta,\n      gamma);\n"
        );
    }

    #[test]
    fn aligned_lines_move_with_what_they_align_under() {
        // `g(` starts on a line aligned under `p`, so `s` moves with `p`.
        let aligned = Doc::align(Doc::concat([
            text("p,"),
            Doc::HardLine,
            text("g("),
            Doc::align(Doc::concat([text("r,"), Doc::HardLine, text("s)")])),
            text(");"),
        ]));
        let rows = Doc::table(Doc::concat([
            text("int"),
            Doc::Cell(0),
            Doc::Space,
            text("a = f("),
            aligned,
            Doc::HardLine,
            text("logic"),
            Doc::Cell(0),
            Doc::Space,
            text("b;"),
        ]));
        assert_eq!(
            print_in(80, [rows]),
            "int   a = f(p,\n            g(r,\n              s));\nlogic b;\n"
        );
    }

    /// `f(` and `parts`, packed under the first, then `);`.
    fn packed(parts: &[&str]) -> Doc {
        let mut fill = Vec::new();
        for (at, part) in parts.iter().enumerate() {
            if at > 0 {
                fill.push(Doc::Line);
            }
            fill.push(text(part));
        }
        Doc::concat([text("f("), Doc::align(Doc::Fill(fill)), text(");")])
    }

    #[test]
    fn a_fill_breaks_only_before_a_part_that_does_not_fit() {
        let parts = ["alpha,", "beta,", "gamma,", "delta"];
        assert_eq!(
            print_in(16, [packed(&parts)]),
            "f(alpha, beta,\n  gamma, delta);\n"
        );
        // What follows the last part counts against it.
        assert_eq!(
            print_in(15, [packed(&parts)]),
            "f(alpha, beta,\n  gamma,\n  delta);\n"
        );
    }

    #[test]
    fn a_part_too_long_for_a_line_starts_one_and_breaks_inside() {
        let part = Doc::group(Doc::concat([
            text("bbbb("),
            Doc::indent(Doc::concat([Doc::SoftLine, text("cccc")])),
            Doc::SoftLine,
            text(")"),
        ]));
        let fill = Doc::Fill(vec![text("a,"), Doc::Line, part]);
        assert_eq!(print_in(8, [fill]), "a,\nbbbb(\n  cccc\n)\n");
    }

    #[test]
    fn a_verbatim_run_moves_as_a_block() {
        let verbatim = Doc::Verbatim(Verbatim {
            column: 4,
            indent: 4,
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
    fn later_lines_hang_off_the_indentation_or_align_under_the_first() {
        // `x = {` at column 10 of a line indented 4; its later lines were
        // written at 6 and 4, or under the `{` at 11.
        let run = |rest: &[(u32, &str)]| {
            Doc::Verbatim(Verbatim {
                column: 10,
                indent: 4,
                first: "{".into(),
                rest: (rest.iter())
                    .map(|&(indent, text)| VerbatimLine::Moved {
                        indent,
                        text: text.into(),
                    })
                    .collect(),
            })
        };
        let line = |verbatim| Doc::indent(Doc::concat([Doc::HardLine, text("a = "), verbatim]));
        assert_eq!(
            print_in(80, [line(run(&[(6, "b,"), (4, "}")]))]),
            "  a = {\n    b,\n  }\n"
        );
        assert_eq!(
            print_in(80, [line(run(&[(11, "b}")]))]),
            "  a = {\n       b}\n"
        );
    }

    #[test]
    fn a_verbatim_run_of_several_lines_breaks_its_group() {
        let verbatim = Doc::Verbatim(Verbatim {
            column: 0,
            indent: 0,
            first: "x".into(),
            rest: vec![VerbatimLine::Kept("y".into())],
        });
        let group = Doc::group(Doc::concat([text("a"), Doc::Line, verbatim]));
        assert_eq!(print_in(80, [group]), "a\nx\ny\n");
    }
}
