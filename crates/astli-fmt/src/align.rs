//! Tabular alignment, a pass over the printed text.
//!
//! A rule marks the end of each cell of a row with [`Doc::Cell`], and the rows
//! that line up with each other with [`Doc::Table`]. Line breaking has already
//! happened, so this pass only pads: what follows the cells of one column
//! starts at the same place on every row, columns taken left to right. A row
//! may leave out a column. An empty line ends a table and starts another; any
//! other line between two rows, such as a comment, leaves it whole.
//!
//! A verbatim run moves as a block, and an aligned group's later lines stand
//! under something on its first, so when padding moves the first line of
//! either, its later lines move by as much.
//!
//! [`Doc::Cell`]: crate::doc::Doc::Cell
//! [`Doc::Table`]: crate::doc::Doc::Table

/// The column of a comment at the end of a line, after every other.
pub(crate) const COMMENT: usize = usize::MAX;

/// Where the printer wrote the end of a cell.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cell {
    /// The table the cell is in, unique within one printing.
    pub table: usize,
    /// The column of the table the cell ends.
    pub index: usize,
    /// How many empty lines were printed before the cell.
    pub block: usize,
    /// The line it is on.
    pub line: usize,
    /// Where the padding goes, in bytes.
    pub offset: usize,
    /// The column of text it ends at.
    pub column: usize,
}

/// A line the printer placed relative to text on an earlier line: a later
/// line of a verbatim run it moved, or of an aligned group.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Continuation {
    /// The line the text it is placed by is on.
    pub line: usize,
    /// Where that text starts, in bytes.
    pub start: usize,
    /// Where this line starts, in bytes.
    pub offset: usize,
}

/// `out` with its cells padded to line up. A row is padded cell by cell from
/// the left for as long as none of its lines goes past `width`, so a long
/// comment loses its alignment before the names do. Rows that cannot take
/// their padding before the comment split their table, as [`table`] says. A
/// cell with nothing after it on its line is left alone, since padding would
/// only give trailing whitespace. `continuations` are in the order of the
/// lines they are placed by.
pub(crate) fn align(
    out: String,
    mut cells: Vec<Cell>,
    continuations: &[Continuation],
    width: usize,
) -> String {
    // Stable, so each table's rows stay in the order they were printed.
    cells.sort_by_key(|cell| (cell.table, cell.block));
    cells.retain(|cell| !out[cell.offset..].starts_with('\n'));

    let mut pads: Vec<(usize, usize)> = Vec::new();
    for run in cells.chunk_by(|a, b| (a.table, a.block) == (b.table, b.block)) {
        let rows: Vec<&[Cell]> = run.chunk_by(|a, b| a.line == b.line).collect();
        let text = Text {
            out: &out,
            continuations,
            width,
        };
        table(&rows, &text, &mut pads);
    }
    pads.retain(|&(_, pad)| pad > 0);
    if pads.is_empty() {
        return out;
    }

    pads.sort_unstable();
    let mut padded =
        String::with_capacity(out.len() + pads.iter().map(|(_, pad)| pad).sum::<usize>());
    let mut from = 0;
    for (offset, pad) in pads {
        padded.push_str(&out[from..offset]);
        padded.extend(std::iter::repeat_n(' ', pad));
        from = offset;
    }
    padded.push_str(&out[from..]);
    padded
}

/// What the rows of a table are padded within.
struct Text<'a> {
    out: &'a str,
    continuations: &'a [Continuation],
    width: usize,
}

impl Text<'_> {
    /// The columns from `offset` to the end of its line.
    fn width_at(&self, offset: usize) -> usize {
        let rest = self.out[offset..].split('\n').next().unwrap_or("");
        rest.chars().count()
    }

    /// The later lines placed by the line `row` is on.
    fn later(&self, row: &[Cell]) -> &[Continuation] {
        let line = row[0].line;
        let from = self.continuations.partition_point(|it| it.line < line);
        let to = self.continuations.partition_point(|it| it.line <= line);
        &self.continuations[from..to]
    }
}

/// Adds to `pads` what lines `rows` up. A row whose cells before its trailing
/// comment cannot all be padded within the width splits the table: the rows
/// on either side of it line up among themselves, and so do consecutive rows
/// that cannot, so no row is left out of the columns around it.
fn table(rows: &[&[Cell]], text: &Text, pads: &mut Vec<(usize, usize)>) {
    let row_pads = targets(rows);
    let cells: Vec<usize> = (rows.iter().zip(&row_pads))
        .map(|(row, row_pads)| fitting(row, row_pads, text))
        .collect();
    let whole: Vec<bool> = (rows.iter().zip(&cells))
        .map(|(row, &cells)| cells >= row.iter().filter(|cell| cell.index != COMMENT).count())
        .collect();
    if whole.iter().any(|&it| it) && whole.iter().any(|&it| !it) {
        let mut at = 0;
        for group in whole.chunk_by(|a, b| a == b) {
            table(&rows[at..at + group.len()], text, pads);
            at += group.len();
        }
        return;
    }
    for ((row, row_pads), cells) in rows.iter().zip(&row_pads).zip(cells) {
        let later = text.later(row);
        pads.extend(
            later
                .iter()
                .map(|it| (it.offset, moved(row, row_pads, cells, it.start))),
        );
        pads.extend(
            (row.iter().map(|cell| cell.offset))
                .zip(row_pads.iter().copied())
                .take(cells),
        );
    }
}

/// The padding of each cell of each row that lines its columns up, taken
/// left to right.
fn targets(rows: &[&[Cell]]) -> Vec<Vec<usize>> {
    let mut indices: Vec<usize> = rows
        .iter()
        .flat_map(|row| row.iter().map(|cell| cell.index))
        .collect();
    indices.sort_unstable();
    indices.dedup();

    // How far each row's text has moved right, and each cell's padding.
    let mut shifts = vec![0; rows.len()];
    let mut row_pads: Vec<Vec<usize>> = rows.iter().map(|row| vec![0; row.len()]).collect();
    for index in indices {
        let at = |row: &[Cell]| row.iter().position(|cell| cell.index == index);
        let target = (rows.iter().zip(&shifts))
            .filter_map(|(row, shift)| at(row).map(|at| row[at].column + shift))
            .max()
            .unwrap_or(0);
        for ((row, shift), pads) in rows.iter().zip(&mut shifts).zip(&mut row_pads) {
            if let Some(at) = at(row) {
                pads[at] = target - (row[at].column + *shift);
                *shift += pads[at];
            }
        }
    }
    row_pads
}

/// How many of `row`'s cells, from the left, can take their padding with
/// none of its lines going past the width. A row already too wide takes
/// none.
fn fitting(row: &[Cell], row_pads: &[usize], text: &Text) -> usize {
    let first = row[0];
    let line_width = first.column + text.width_at(first.offset);
    let later = text.later(row);
    let fits = |cells: usize| {
        line_width + row_pads[..cells].iter().sum::<usize>() <= text.width
            && later.iter().all(|it| {
                text.width_at(it.offset) + moved(row, row_pads, cells, it.start) <= text.width
            })
    };
    (0..=row.len())
        .rev()
        .find(|&cells| fits(cells))
        .unwrap_or(0)
}

/// How far text that starts at `offset` moves when the first `cells` of `row`
/// are padded.
fn moved(row: &[Cell], row_pads: &[usize], cells: usize, offset: usize) -> usize {
    (row.iter().zip(row_pads).take(cells))
        .take_while(|(cell, _)| cell.offset <= offset)
        .map(|(_, pad)| pad)
        .sum()
}
