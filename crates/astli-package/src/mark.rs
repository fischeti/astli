//! Spans of a message marked by what they are, so that a driver can style a
//! package's name apart from a version or a path. A message is a `String`
//! all the way up, so a span stays marked whatever builds it into a longer
//! one, and [`plain`] writes each span in backticks, as text that cannot be
//! styled reads.
//!
//! ```
//! use astli_package::mark::{self, Kind};
//!
//! let message = format!("{} is at {}", mark::name("axi"), mark::version("v0.39.4"));
//! assert_eq!(mark::plain(&message), "`axi` is at `v0.39.4`");
//! let kinds: Vec<_> = mark::pieces(&message).map(|(kind, _)| kind).collect();
//! assert_eq!(kinds, [Some(Kind::Name), None, Some(Kind::Version)]);
//! ```

use std::fmt::Display;

/// What a span of a message is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A package's, a feature's or a design unit's name.
    Name,
    /// A version, a branch, a tag or a commit.
    Version,
    /// A path or a URL.
    Path,
    /// What a person types: a command, a flag, a manifest's key or table.
    Code,
}

/// Around each span, with the kind's letter after the start: control
/// characters, which no name, version or path holds.
const START: char = '\u{2}';
const END: char = '\u{3}';

/// `text` marked as a name.
pub fn name(text: impl Display) -> String {
    marked('n', text)
}

/// `text` marked as a version, a branch, a tag or a commit.
pub fn version(text: impl Display) -> String {
    marked('v', text)
}

/// `text` marked as a path or a URL.
pub fn path(text: impl Display) -> String {
    marked('p', text)
}

/// `text` marked as what a person types.
pub fn code(text: impl Display) -> String {
    marked('c', text)
}

fn marked(letter: char, text: impl Display) -> String {
    format!("{START}{letter}{text}{END}")
}

/// The pieces of `message` in order, each a span of its kind or the plain
/// text between spans. A span cut short is read as plain text.
pub fn pieces(message: &str) -> impl Iterator<Item = (Option<Kind>, &str)> {
    let mut rest = message;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let Some(start) = rest.find(START) else {
            return Some((None, std::mem::take(&mut rest)));
        };
        if start > 0 {
            let (text, after) = rest.split_at(start);
            rest = after;
            return Some((None, text));
        }
        let mut chars = rest[START.len_utf8()..].chars();
        let kind = match chars.next() {
            Some('n') => Kind::Name,
            Some('v') => Kind::Version,
            Some('p') => Kind::Path,
            Some('c') => Kind::Code,
            _ => return Some((None, std::mem::take(&mut rest))),
        };
        let span = chars.as_str();
        let Some(end) = span.find(END) else {
            return Some((None, std::mem::take(&mut rest)));
        };
        rest = &span[end + END.len_utf8()..];
        Some((Some(kind), &span[..end]))
    })
}

/// `message` with each span in backticks.
pub fn plain(message: &str) -> String {
    pieces(message)
        .map(|(kind, text)| match kind {
            Some(_) => format!("`{text}`"),
            None => text.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_built_into_a_longer_message_stays_marked() {
        let asker = format!("{} {}", name("axi"), version("0.39.4"));
        let message = format!("{asker} depends on {} at {}", name("cells"), path("vendor"));
        assert_eq!(
            plain(&message),
            "`axi` `0.39.4` depends on `cells` at `vendor`"
        );
    }

    #[test]
    fn a_span_cut_short_is_plain_text() {
        let cut = format!("a {START}nname");
        assert_eq!(
            pieces(&cut).collect::<Vec<_>>(),
            [(None, "a "), (None, &cut[2..])]
        );
    }
}
