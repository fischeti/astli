//! Where each line of a text begins.

/// The byte offset each line of a text begins at, the first at 0.
///
/// A line ends after its `\n`, so a `\r` before it is the line's last
/// byte. What a column counts, characters or an editor's code units, is
/// the caller's: the table only finds the line.
///
/// ```
/// use astli_text::Lines;
///
/// let lines = Lines::new("wire a;\nwire é;\n");
/// assert_eq!(lines.len(), 3);
/// assert_eq!(lines.line(9), 1);
/// assert_eq!(lines.start(1), Some(8));
/// assert_eq!(lines.start(3), None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lines(Vec<u32>);

impl Lines {
    /// Finds where each line of `text` begins.
    pub fn new(text: &str) -> Lines {
        let starts = (text.bytes().enumerate())
            .filter(|&(_, byte)| byte == b'\n')
            .map(|(at, _)| at as u32 + 1);
        Lines(std::iter::once(0).chain(starts).collect())
    }

    /// How many lines there are: one more than the `\n`s, since the text
    /// after the last one is a line too, if an empty one.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always `false`: an empty text is one empty line.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// The line, counted from zero, that the byte at `offset` is on. An
    /// offset past the end is on the last line.
    pub fn line(&self, offset: u32) -> usize {
        self.0.partition_point(|&start| start <= offset) - 1
    }

    /// Where line `line`, counted from zero, begins, if the text has it.
    pub fn start(&self, line: usize) -> Option<u32> {
        self.0.get(line).copied()
    }
}
