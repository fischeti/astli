//! A place in a text as the protocol gives it, a line and a character
//! counted in the negotiated encoding, and as astli holds it, a byte offset.
//!
//! Both directions take the text and its line table, so they serve an open
//! buffer and a file a snapshot read alike.

use std::ops::Range as Bytes;

use astli_text::Lines;
use lsp_types::{Position, PositionEncodingKind, Range};

/// What a position's `character` counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// Bytes, which a span already counts.
    Utf8,
    /// UTF-16 code units, the protocol's default: two for a character past
    /// U+FFFF, one for any other.
    Utf16,
}

impl Encoding {
    /// UTF-8 if the client offers it, since a span is bytes and nothing then
    /// has to be counted; UTF-16 otherwise, which every client takes.
    pub fn negotiate(offered: &[PositionEncodingKind]) -> Encoding {
        match offered.contains(&PositionEncodingKind::UTF8) {
            true => Encoding::Utf8,
            false => Encoding::Utf16,
        }
    }

    /// The encoding as the server's capabilities announce it.
    pub fn kind(self) -> PositionEncodingKind {
        match self {
            Encoding::Utf8 => PositionEncodingKind::UTF8,
            Encoding::Utf16 => PositionEncodingKind::UTF16,
        }
    }

    fn width(self, char: char) -> u32 {
        match self {
            Encoding::Utf8 => char.len_utf8() as u32,
            Encoding::Utf16 => char.len_utf16() as u32,
        }
    }

    /// The byte offset in `text` that `position` names.
    ///
    /// A line past the last is the end of the text, and a character past
    /// the end of its line is the line's end, before its `\n` or `\r\n`, as
    /// the protocol has it. One inside a character, which only a client
    /// that miscounted sends, is the start of that character.
    pub fn offset(self, text: &str, lines: &Lines, position: Position) -> u32 {
        let Some(start) = lines.start(position.line as usize) else {
            return text.len() as u32;
        };
        let rest = &text[start as usize..];
        let line = &rest[..rest.find('\n').unwrap_or(rest.len())];
        let line = line.strip_suffix('\r').unwrap_or(line);
        let mut counted = 0;
        for (at, char) in line.char_indices() {
            counted += self.width(char);
            if counted > position.character {
                return start + at as u32;
            }
        }
        start + line.len() as u32
    }

    /// The bytes of `text` that `range` covers, its end clamped as
    /// [`offset`](Self::offset) clamps, and never before its start.
    pub fn bytes(self, text: &str, lines: &Lines, range: Range) -> Bytes<u32> {
        let start = self.offset(text, lines, range.start);
        let end = self.offset(text, lines, range.end);
        start..end.max(start)
    }

    /// The position of byte `offset` in `text`: past the end, the end, and
    /// inside a character, the start of that character.
    pub fn position(self, text: &str, lines: &Lines, offset: u32) -> Position {
        let offset = text.floor_char_boundary(offset as usize);
        let line = lines.line(offset as u32);
        let start = lines.start(line).unwrap_or_default() as usize;
        let character = text[start..offset].chars().map(|it| self.width(it)).sum();
        Position {
            line: line as u32,
            character,
        }
    }

    /// The range of `bytes` in `text`.
    pub fn range(self, text: &str, lines: &Lines, bytes: Bytes<u32>) -> Range {
        Range {
            start: self.position(text, lines, bytes.start),
            end: self.position(text, lines, bytes.end),
        }
    }
}

#[cfg(test)]
mod tests {
    use astli_text::Lines;
    use lsp_types::Position;

    use super::Encoding;

    #[test]
    fn positions_convert_to_offsets_and_back() {
        use Encoding::{Utf8, Utf16};
        let text = "a\n// 𝄞é x\r\n\nend";
        let lines = Lines::new(text);
        // Each row is an offset and its position, which convert both ways.
        let rows: &[(Encoding, u32, [u32; 2])] = &[
            (Utf16, 0, [0, 0]),
            (Utf16, 1, [0, 1]),
            (Utf16, 2, [1, 0]),
            (Utf16, 5, [1, 3]),
            (Utf16, 9, [1, 5]),
            (Utf16, 11, [1, 6]),
            (Utf16, 13, [1, 8]),
            (Utf16, 15, [2, 0]),
            (Utf16, 16, [3, 0]),
            (Utf16, 19, [3, 3]),
            (Utf8, 5, [1, 3]),
            (Utf8, 9, [1, 7]),
            (Utf8, 11, [1, 9]),
            (Utf8, 13, [1, 11]),
            (Utf8, 19, [3, 3]),
        ];
        for &(encoding, offset, [line, character]) in rows {
            let position = Position { line, character };
            let row = format!("{encoding:?} {offset}");
            assert_eq!(encoding.position(text, &lines, offset), position, "{row}");
            assert_eq!(encoding.offset(text, &lines, position), offset, "{row}");
        }

        // A position past its line, or inside a character, comes back clamped.
        let clamped: &[(Encoding, [u32; 2], u32)] = &[
            (Utf16, [0, 9], 1),
            (Utf16, [1, 4], 5),
            (Utf16, [1, 99], 13),
            (Utf8, [1, 5], 5),
            (Utf8, [1, 99], 13),
            (Utf16, [2, 1], 15),
            (Utf16, [3, 9], 19),
            (Utf16, [4, 0], 19),
            (Utf8, [u32::MAX, 0], 19),
        ];
        for &(encoding, [line, character], offset) in clamped {
            let position = Position { line, character };
            let row = format!("{encoding:?} {position:?}");
            assert_eq!(encoding.offset(text, &lines, position), offset, "{row}");
        }

        // So does an offset past the end, or inside a character.
        assert_eq!(
            Utf16.position(text, &lines, 99),
            Position {
                line: 3,
                character: 3
            }
        );
        assert_eq!(
            Utf16.position(text, &lines, 7),
            Position {
                line: 1,
                character: 3
            }
        );
        assert_eq!(
            Utf8.position(text, &lines, 10),
            Position {
                line: 1,
                character: 7
            }
        );

        let range = Utf16.range(text, &lines, 5..11);
        assert_eq!(Utf16.bytes(text, &lines, range), 5..11);
        assert_eq!(
            Utf8.range("", &Lines::new(""), 0..0).end,
            Position::default()
        );
    }
}
