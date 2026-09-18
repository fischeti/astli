//! Reading a `.f` filelist.
//!
//! The one piece of syntax in this workspace that is not SystemVerilog. It
//! lives in the driver for the reason `docs/api.md` gives: what a build passes
//! arrives from a filelist, a manifest or a command line, and none of those is
//! a question about the language. `svirig-preproc` holds the same line about
//! grammar.
//!
//! # What is in one
//!
//! Source paths, `+incdir+` and `+define+`, another filelist, and comments.
//! Anything else -- `-y`, `-v`, `+libext+` -- is rejected by name rather than
//! skipped, so that a filelist which half works says so instead of producing a
//! build that is quietly missing half its inputs.
//!
//! # Where a relative path is relative to
//!
//! Both answers are in use and they disagree, so both are offered and the flag
//! says which: `-f` resolves against the working directory, `-F` against the
//! directory the filelist is in. A nested filelist is found the same way as
//! any other path in the file that names it, and the flag that names it
//! decides how *its* contents resolve in turn.

use std::path::{Path, PathBuf};

use svirig_text::clean;

use crate::error::{Error, Result};

/// How deep filelists may nest. A cycle is caught by name; this is the
/// backstop for a chain that is merely absurd.
const MAX_DEPTH: usize = 16;

/// What a filelist named.
#[derive(Debug, Default)]
pub struct Filelist {
    pub files: Vec<PathBuf>,
    pub incdir: Vec<PathBuf>,
    pub define: Vec<String>,
}

/// What a relative path in a filelist is relative to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// The working directory, which is what `-f` means to most tools.
    Cwd,
    /// The directory the filelist itself is in, which is what `-F` means.
    File,
}

impl Filelist {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.incdir.is_empty() && self.define.is_empty()
    }
}

/// Reads `path`, and every filelist it names.
pub fn read(path: &Path, base: Base) -> Result<Filelist> {
    let mut list = Filelist::default();
    let mut open = Vec::new();
    read_into(path, base, &mut list, &mut open)?;
    Ok(list)
}

fn read_into(path: &Path, base: Base, list: &mut Filelist, open: &mut Vec<PathBuf>) -> Result {
    // Canonicalised only to compare: a filelist reached twice by two spellings
    // of one path is still the same file, and a path that cannot be
    // canonicalised is about to fail to open with a better message.
    let id = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if open.contains(&id) {
        return Err(Error::filelist(path, 0, "this filelist includes itself"));
    }
    if open.len() >= MAX_DEPTH {
        return Err(Error::filelist(
            path,
            0,
            format!("filelists nested more than {MAX_DEPTH} deep"),
        ));
    }
    open.push(id);

    let text = std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?;
    let root = match base {
        Base::Cwd => PathBuf::new(),
        Base::File => path.parent().unwrap_or(Path::new("")).to_path_buf(),
    };

    for (number, line) in uncommented(&text).lines().enumerate() {
        let number = number + 1;
        let mut words = line.split_whitespace();

        while let Some(word) = words.next() {
            let word = substitute(path, number, word)?;

            if let Some(dirs) = word.strip_prefix("+incdir+") {
                list.incdir
                    .extend(plus(dirs).map(|dir| resolve(&root, dir)));
            } else if let Some(defines) = word.strip_prefix("+define+") {
                list.define.extend(plus(defines).map(String::from));
            } else if word == "-f" || word == "-F" {
                let nested = match words.next() {
                    Some(nested) => substitute(path, number, nested)?,
                    None => {
                        return Err(Error::filelist(
                            path,
                            number,
                            format!("{word} names no file"),
                        ));
                    }
                };
                let base = match word.as_str() {
                    "-f" => Base::Cwd,
                    _ => Base::File,
                };
                read_into(&resolve(&root, &nested), base, list, open)?;
            } else if word.starts_with('-') || word.starts_with('+') {
                return Err(Error::filelist(
                    path,
                    number,
                    format!("{word} is not one of the options a filelist may carry here"),
                ));
            } else {
                list.files.push(resolve(&root, &word));
            }
        }
    }

    open.pop();
    Ok(())
}

/// The text with its comments replaced by nothing, newlines kept so that a
/// line number still means what it says.
fn uncommented(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find("//").into_iter().chain(rest.find("/*")).min() {
        out.push_str(&rest[..at]);
        let block = rest[at..].starts_with("/*");
        rest = &rest[at..];

        if block {
            let (skipped, after) = match rest[2..].find("*/") {
                Some(end) => rest.split_at(2 + end + 2),
                // Unterminated, which is a comment to the end of the file.
                None => (rest, ""),
            };
            // The newlines inside it are not comment, as far as counting goes.
            out.extend(skipped.chars().filter(|char| *char == '\n'));
            rest = after;
        } else {
            rest = match rest.find('\n') {
                Some(end) => &rest[end..],
                None => "",
            };
        }
    }

    out.push_str(rest);
    out
}

/// The values of a plus-separated option: `+incdir+a+b+` is `a` and `b`.
fn plus(rest: &str) -> impl Iterator<Item = &str> {
    rest.split('+').filter(|value| !value.is_empty())
}

/// `$NAME` and `${NAME}` from the environment.
///
/// A filelist is usually generated, and what generates it writes the paths it
/// was given -- which in a simulation flow means a variable. An undefined one
/// is an error rather than an empty string, because the path it would build is
/// wrong in a way that only shows up much later.
fn substitute(path: &Path, line: usize, word: &str) -> Result<String> {
    if !word.contains('$') {
        return Ok(word.to_string());
    }

    let mut out = String::with_capacity(word.len());
    let mut rest = word;

    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];

        let (name, next) = match after.strip_prefix('{') {
            Some(braced) => match braced.find('}') {
                Some(end) => (&braced[..end], &braced[end + 1..]),
                None => {
                    return Err(Error::filelist(
                        path,
                        line,
                        format!("{word} has no closing }}"),
                    ));
                }
            },
            None => {
                let end = after
                    .find(|char: char| !char.is_alphanumeric() && char != '_')
                    .unwrap_or(after.len());
                (&after[..end], &after[end..])
            }
        };

        if name.is_empty() {
            return Err(Error::filelist(
                path,
                line,
                format!("{word} names no variable"),
            ));
        }
        match std::env::var(name) {
            Ok(value) => out.push_str(&value),
            Err(_) => {
                return Err(Error::filelist(
                    path,
                    line,
                    format!("${name} is not set, and {word} needs it"),
                ));
            }
        }
        rest = next;
    }

    out.push_str(rest);
    Ok(out)
}

fn resolve(root: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    match path.is_absolute() {
        true => clean(path),
        false => clean(&root.join(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_comment_runs_to_the_end_of_its_line() {
        assert_eq!(uncommented("a.sv // and b.sv\nc.sv\n"), "a.sv \nc.sv\n");
    }

    #[test]
    fn a_block_comment_keeps_the_lines_it_spanned() {
        // Otherwise every message after one points at the wrong line.
        let text = "a.sv /* one\ntwo\nthree */ b.sv\n";
        assert_eq!(uncommented(text), "a.sv \n\n b.sv\n");
    }

    #[test]
    fn an_unterminated_block_comment_swallows_the_rest() {
        assert_eq!(uncommented("a.sv /* b.sv\nc.sv"), "a.sv \n");
    }

    #[test]
    fn a_plus_option_splits_on_its_separators() {
        let values: Vec<_> = plus("a+b+c").collect();
        assert_eq!(values, ["a", "b", "c"]);
        // Trailing and doubled separators are written by real generators.
        let values: Vec<_> = plus("a++b+").collect();
        assert_eq!(values, ["a", "b"]);
    }

    #[test]
    fn a_variable_is_read_from_the_environment_either_way_it_is_written() {
        // SAFETY: the process is single-threaded here, and the name is one
        // nothing else in the suite reads.
        unsafe { std::env::set_var("SVIRIG_TEST_ROOT", "/rtl") };

        let path = Path::new("x.f");
        assert_eq!(
            substitute(path, 1, "$SVIRIG_TEST_ROOT/a.sv").unwrap(),
            "/rtl/a.sv"
        );
        assert_eq!(
            substitute(path, 1, "${SVIRIG_TEST_ROOT}x").unwrap(),
            "/rtlx"
        );
        assert_eq!(substitute(path, 1, "plain.sv").unwrap(), "plain.sv");
    }

    #[test]
    fn a_variable_that_is_not_set_is_an_error_and_not_an_empty_path() {
        let err = substitute(Path::new("x.f"), 7, "$SVIRIG_TEST_UNSET/a.sv").unwrap_err();
        assert!(err.to_string().contains("is not set"), "{err}");
        assert!(err.to_string().contains("x.f:7"), "{err}");
    }

    #[test]
    fn an_absolute_path_is_left_alone_and_a_relative_one_is_rooted() {
        assert_eq!(resolve(Path::new("/rtl"), "a.sv"), Path::new("/rtl/a.sv"));
        assert_eq!(resolve(Path::new("/rtl"), "/b.sv"), Path::new("/b.sv"));
        // `-f`, which roots nothing: the path is the working directory's.
        assert_eq!(resolve(Path::new(""), "a.sv"), Path::new("a.sv"));
    }
}
