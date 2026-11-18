//! Applying the fixes diagnostics carry to the text of their file.

use crate::diagnostic::Edit;

/// `source` with each of `fixes` applied, each a diagnostic's edits, and how
/// many were.
///
/// A fix is applied whole or not at all: one with an edit that overlaps an
/// edit of a fix before it, or inserts where one does, is left for a later
/// pass over the new text, since its spans no longer say what they did. An
/// edit's span is taken to be in `source`, whatever file it names.
pub fn apply(source: &str, fixes: &[&[Edit]]) -> (String, usize) {
    let mut taken: Vec<&Edit> = Vec::new();
    let mut applied = 0;
    for fix in fixes {
        let clashes = |edit: &Edit| {
            (taken.iter()).any(|other| {
                (edit.at.start < other.at.end && other.at.start < edit.at.end)
                    || edit.at.start == other.at.start
            })
        };
        if fix.is_empty() || fix.iter().any(clashes) {
            continue;
        }
        taken.extend(fix.iter());
        applied += 1;
    }
    taken.sort_by_key(|edit| edit.at.start);
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    for edit in taken {
        out.push_str(&source[at..edit.at.start as usize]);
        out.push_str(&edit.text);
        at = edit.at.end as usize;
    }
    out.push_str(&source[at..]);
    (out, applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::{SourceId, Span};

    fn edit(start: u32, end: u32, text: &str) -> Edit {
        Edit {
            at: Span::new(SourceId(0), start, end),
            text: text.to_string(),
        }
    }

    #[test]
    fn edits_replace_insert_and_delete() {
        let fixes = [
            &[edit(0, 3, "logic")][..],
            &[edit(6, 6, " // A"), edit(7, 8, "")],
        ];
        assert_eq!(
            apply("reg x;\n;", &fixes),
            ("logic x; // A\n".to_string(), 2)
        );
    }

    #[test]
    fn a_fix_that_overlaps_one_before_it_waits() {
        let fixes = [&[edit(0, 3, "a")][..], &[edit(2, 4, "b"), edit(5, 6, "c")]];
        assert_eq!(apply("012345", &fixes), ("a345".to_string(), 1));
        let inserts = [&[edit(1, 1, "a")][..], &[edit(1, 1, "b")]];
        assert_eq!(apply("01", &inserts), ("0a1".to_string(), 1));
    }
}
