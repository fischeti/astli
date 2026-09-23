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
/// comment loses its alignment before the names do. A cell with nothing after
/// it on its line is left alone, since padding would only give trailing
/// whitespace. `continuations` are in the order of the lines they are placed
/// by.
pub(crate) fn align(
    out: String,
    mut cells: Vec<Cell>,
    continuations: &[Continuation],
    width: usize,
) -> String {
    // Stable, so each table's rows stay in the order they were printed.
    cells.sort_by_key(|cell| (cell.table, cell.block));
    cells.retain(|cell| !out[cell.offset..].starts_with('\n'));

    // The columns from `offset` to the end of its line.
    let line_width_at = |offset: usize| {
        let rest = out[offset..].split('\n').next().unwrap_or("");
        rest.chars().count()
    };
    let mut pads: Vec<(usize, usize)> = Vec::new();
    for run in cells.chunk_by(|a, b| (a.table, a.block) == (b.table, b.block)) {
        let rows: Vec<&[Cell]> = run.chunk_by(|a, b| a.line == b.line).collect();
        let mut indices: Vec<usize> = run.iter().map(|cell| cell.index).collect();
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

        for (row, row_pads) in rows.iter().zip(row_pads) {
            let first = row[0];
            let line_width = first.column + line_width_at(first.offset);
            let later = continuations.partition_point(|it| it.line < first.line);
            let later: Vec<&Continuation> = continuations[later..]
                .iter()
                .take_while(|it| it.line == first.line)
                .collect();
            // How far text that starts at `offset` moves, padding the first
            // `cells` of the row.
            let moved = |cells: usize, offset: usize| -> usize {
                (row.iter().zip(&row_pads).take(cells))
                    .take_while(|(cell, _)| cell.offset <= offset)
                    .map(|(_, pad)| pad)
                    .sum()
            };
            let fits = |cells: usize| {
                line_width + row_pads[..cells].iter().sum::<usize>() <= width
                    && later
                        .iter()
                        .all(|it| line_width_at(it.offset) + moved(cells, it.start) <= width)
            };
            // A row already too wide is left as it is.
            let cells = (0..=row.len())
                .rev()
                .find(|&cells| fits(cells))
                .unwrap_or(0);
            pads.extend(later.iter().map(|it| (it.offset, moved(cells, it.start))));
            pads.extend(
                (row.iter().map(|cell| cell.offset))
                    .zip(row_pads)
                    .take(cells),
            );
        }
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
