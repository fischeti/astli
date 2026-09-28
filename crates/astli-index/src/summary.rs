//! What one file declares and uses at the top level, read off its tree.

use std::fmt;
use std::path::{Path, PathBuf};

use astli_parse::Parsed;
use astli_preproc::Session;
use astli_syntax::SyntaxToken;
use astli_text::{LineCol, SourceId};

use crate::names::{Role, names};

/// What a top-level declaration declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Declares {
    Module,
    Interface,
    Program,
    Package,
    Class,
}

/// How a reference uses the name it names, which says what it may resolve to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Uses {
    /// The type of an instantiation: a module, interface or program.
    Instance,
    /// The package of an `import` or `export`.
    Import,
    /// The name left of `::`: a package, or a class that may be local.
    Scope,
    /// The head of a type: an interface, a class, or a typedef that may be
    /// local.
    Type,
    /// The target of a `bind`, or the head of its path: a module, an
    /// interface, or an instance.
    Bind,
    /// A name in text the grammar left `VERBATIM`, shaped like an
    /// instantiation's type.
    Unparsed,
}

impl Uses {
    /// Whether the name must be declared at the top level of some file, so
    /// that finding it nowhere is a problem.
    pub fn is_global(self) -> bool {
        matches!(self, Uses::Instance | Uses::Import)
    }
}

/// Where a name is written: the file, and the line and column in it. A name
/// a macro wrote is where the outermost call was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub at: LineCol,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.path.display(), self.at)
    }
}

/// A module, interface, program, package or class declared at the top level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    pub name: String,
    pub declares: Declares,
    pub location: Location,
}

/// A name used where it can only mean something declared at the top level,
/// or might.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub name: String,
    pub uses: Uses,
    pub location: Location,
}

/// The top-level names one file declares and uses, and the headers it read.
///
/// It holds no span, so it outlives the session it was read from and can be
/// sent across threads: an [`Index`](crate::Index) is built from summaries
/// made in parallel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub path: PathBuf,
    /// In the order they are written, headers included where they are.
    pub declarations: Vec<Declaration>,
    /// Every use, in the order written; one name may be used many times.
    pub references: Vec<Reference>,
    /// Every file an `` `include `` read, directly or through another, in the
    /// order first read.
    pub includes: Vec<PathBuf>,
}

impl Summary {
    /// Reads the summary of `file` off `parsed`, its tree in either mode.
    ///
    /// In expanded mode what the macros write and the headers declare is part
    /// of it, and only the branches the build takes. In raw mode neither is,
    /// and every branch is.
    pub fn new(session: &Session, file: SourceId, parsed: &Parsed) -> Summary {
        let origins = session.origins();
        let locate = |token: &SyntaxToken| {
            let span = parsed.span(token)?;
            let at = origins.spelled(origins.reported_at(span));
            Some(Location {
                path: origins.path(at.src_id)?.to_path_buf(),
                at: origins.line_col(at.src_id, at.start),
            })
        };

        let mut declarations = Vec::new();
        let mut references = Vec::new();
        for name in names(&parsed.root) {
            let Some(location) = locate(&name.token) else {
                continue;
            };
            match name.role {
                Role::Declares(declares) => declarations.push(Declaration {
                    name: name.text().to_string(),
                    declares,
                    location,
                }),
                Role::Uses(uses) => references.push(Reference {
                    name: name.text().to_string(),
                    uses,
                    location,
                }),
                Role::Closes(_) => {}
            }
        }

        let path = origins
            .path(file)
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let includes = origins
            .files()
            .filter(|&other| {
                let root = origins.include_trace(other).last();
                root.is_some_and(|site| origins.spelled(site).src_id == file)
            })
            .filter_map(|other| origins.path(other).map(Path::to_path_buf))
            .collect();

        Summary {
            path,
            declarations,
            references,
            includes,
        }
    }
}
