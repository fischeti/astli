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
