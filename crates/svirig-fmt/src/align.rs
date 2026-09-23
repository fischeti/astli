//! Tabular alignment, a pass over the printed text.
//!
//! A rule marks the end of each cell of a row with [`Doc::Cell`], and the rows
//! that line up with each other with [`Doc::Table`]. Line breaking has already
//! happened, so this pass only pads: what follows the `n`th cell of each row
//! starts at the same column. An empty line ends a table and starts another;
//! any other line between two rows, such as a comment, leaves it whole.
//!
//! A verbatim run moves as a block, so when padding moves the first line of
//! one, its later lines move by as much.
//!
//! [`Doc::Cell`]: crate::doc::Doc::Cell
//! [`Doc::Table`]: crate::doc::Doc::Table

/// Where the printer wrote the end of a cell.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cell {
    /// The table the cell is in, unique within one printing.
    pub table: usize,
    /// How many empty lines were printed before the cell.
    pub block: usize,
    /// The line it is on.
    pub line: usize,
    /// Where the padding goes, in bytes.
    pub offset: usize,
    /// The column it ends at.
    pub column: usize,
}

/// A later line of a verbatim run the printer moved, which it placed relative
/// to where the run's first line started.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Continuation {
    /// The line the run started on.
    pub line: usize,
    /// Where the run's first line starts, in bytes.
    pub start: usize,
    /// Where this line starts, in bytes.
    pub offset: usize,
    /// The columns this line takes.
    pub width: usize,
}

/// `out` with its cells padded to line up. A row that padding would take past
/// `width`, on any of its lines, is left as it is, and so is a cell with
/// nothing after it on its line, which padding would only give trailing
/// whitespace. `continuations` are in the order they were printed.
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
        let columns = rows.iter().map(|row| row.len()).max().unwrap_or(0);
        // How far each row's text has moved right, and by what, cell by cell.
        let mut shifts = vec![0; rows.len()];
        let mut row_pads: Vec<Vec<usize>> = vec![Vec::new(); rows.len()];
        for at in 0..columns {
            let target = rows
                .iter()
                .zip(&shifts)
                .filter_map(|(row, shift)| row.get(at).map(|cell| cell.column + shift))
                .max()
                .unwrap_or(0);
            for (row, (shift, pads)) in rows.iter().zip(shifts.iter_mut().zip(&mut row_pads)) {
                if let Some(cell) = row.get(at) {
                    let pad = target - (cell.column + *shift);
                    pads.push(pad);
                    *shift += pad;
                }
            }
        }
        for ((row, shift), row_pads) in rows.iter().zip(shifts).zip(row_pads) {
            let first = row[0];
            // How far text that starts at `offset` on the row's line moves.
            let moved = |offset: usize| -> usize {
                (row.iter().zip(&row_pads))
                    .take_while(|(cell, _)| cell.offset <= offset)
                    .map(|(_, pad)| pad)
                    .sum()
            };
            let later = continuations.partition_point(|it| it.line < first.line);
            let later = continuations[later..]
                .iter()
                .take_while(|it| it.line == first.line);
            let rest = out[first.offset..].split('\n').next().unwrap_or("");
            let overflows = first.column + rest.chars().count() + shift > width
                || later.clone().any(|it| it.width + moved(it.start) > width);
            if overflows {
                continue;
            }
            pads.extend(later.map(|it| (it.offset, moved(it.start))));
            pads.extend(row.iter().map(|cell| cell.offset).zip(row_pads));
        }
    }
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
