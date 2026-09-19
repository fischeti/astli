//! Parser for EDA `.f` command and filelist files.
//!
//! Handles source files, `+incdir+`, `+define+`, nested filelists (`-f` and `-F`),
//! line/block comments, and environment variable substitution (`$VAR` and `${VAR}`).
//!
//! Relative paths are resolved either against the current working directory (`-f`)
//! or against the directory containing the filelist (`-F`).

use std::path::{Path, PathBuf};

use svirig_text::clean;

use crate::error::{Error, Result};

/// Maximum recursion depth for nested filelists to prevent circular inclusion.
const MAX_DEPTH: usize = 16;

/// Contents extracted from a `.f` filelist.
#[derive(Debug, Default)]
pub struct Filelist {
    /// Source file paths to compile.
    pub files: Vec<PathBuf>,
    /// Include search directories (`+incdir+`).
    pub incdir: Vec<PathBuf>,
    /// Macro definitions (`+define+`).
    pub define: Vec<String>,
}

/// Base directory for resolving relative paths within a filelist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// Current working directory (`-f`).
    Cwd,
    /// Directory containing the filelist file (`-F`).
    File,
}

impl Filelist {
    /// Returns `true` if the filelist contains no files, include directories, or macro definitions.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.incdir.is_empty() && self.define.is_empty()
    }
}

/// Reads a `.f` filelist at `path` and recursively resolves all nested filelists.
pub fn read(path: &Path, base: Base) -> Result<Filelist> {
    let mut list = Filelist::default();
    let mut open = Vec::new();
    read_into(path, base, &mut list, &mut open)?;
    Ok(list)
}

/// Recursively reads `path` into `list`, tracking open file paths to prevent recursion cycles.
fn read_into(path: &Path, base: Base, list: &mut Filelist, open: &mut Vec<PathBuf>) -> Result {
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

/// Strips line and block comments from `text` while preserving newline characters for line numbering.
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
                None => (rest, ""),
            };
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

/// Splits a plus-separated option argument (e.g. `+incdir+dir1+dir2+`) into individual values.
pub fn plus(rest: &str) -> impl Iterator<Item = &str> {
    rest.split('+').filter(|value| !value.is_empty())
}

/// Expands `$VAR` and `${VAR}` environment variable references within `word`.
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

/// Resolves `path` relative to `root`, or returns it cleaned if absolute.
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
        let values: Vec<_> = plus("a++b+").collect();
        assert_eq!(values, ["a", "b"]);
    }

    #[test]
    fn a_variable_is_read_from_the_environment_either_way_it_is_written() {
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
        assert_eq!(resolve(Path::new(""), "a.sv"), Path::new("a.sv"));
    }
}
