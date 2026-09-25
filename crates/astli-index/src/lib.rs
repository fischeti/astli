//! The top-level names SystemVerilog files declare and use, and which files a
//! design needs.
//!
//! A [`Summary`] is what one file declares at the top level (modules,
//! interfaces, programs, packages, classes) and every name it uses that could
//! mean one. An [`Index`] over the summaries of many files resolves each name
//! to the file that declares it, which answers what a filelist needs: the
//! files a top reaches, an order to compile them in, and the names no file
//! declares. An editor's definitions and references across files are the same
//! question.
//!
//! This is not name resolution. A reference is a name in a place that could
//! mean a top-level declaration, and nothing checks what else is in scope
//! there, so the index errs towards keeping a file.
//!
//! # A design, trimmed
//!
//! ```
//! use astli_index::{Index, summarize};
//! use astli_preproc::Session;
//!
//! let files = [
//!     ("top.sv", "module top;\n  core u_core ();\nendmodule\n"),
//!     ("core.sv", "module core;\n  import cfg_pkg::*;\nendmodule\n"),
//!     ("cfg_pkg.sv", "package cfg_pkg;\nendpackage\n"),
//!     ("unused.sv", "module unused;\nendmodule\n"),
//! ];
//! let summaries = files.map(|(path, text)| {
//!     let mut session = Session::new();
//!     let file = session.add(path, text.into());
//!     summarize(&mut session, file).0
//! });
//!
//! let index = Index::new(summaries.into());
//! let needed = index.reachable(&["top"]).unwrap();
//! assert_eq!(needed, [0, 1, 2]);
//! // A package before the module that imports it.
//! assert_eq!(index.ordered(&needed), [2, 1, 0]);
//! ```
//!
//! Each file is its own session here, as each is its own compilation unit;
//! sessions made in parallel work as well, since a summary holds no span.

mod index;
mod summary;

pub use index::{Index, Step, UnknownTop};
pub use summary::{Declaration, Declares, Location, Reference, Summary, Uses};

use astli_parse::parse_expanded;
use astli_preproc::Session;
use astli_text::{Diagnostic, SourceId};

/// Expands `file`, parses the expansion and reads its summary, with what the
/// expansion and the parser reported.
pub fn summarize(session: &mut Session, file: SourceId) -> (Summary, Vec<Diagnostic>) {
    let mut expanded = session.expand(file);
    let mut parsed = parse_expanded(session, &expanded.tokens);
    let summary = Summary::new(session, file, &parsed);
    expanded.diagnostics.append(&mut parsed.diagnostics);
    (summary, expanded.diagnostics)
}
