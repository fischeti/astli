//! Rendering a [`Diagnostic`](astli_text::Diagnostic) for a person to read,
//! with the macro calls and includes that explain it.
//!
//! A diagnostic from `astli-preproc` or `astli-parse` points where a token is
//! *placed*, which inside a macro is text the user never wrote. Showing it
//! takes two steps:
//!
//! 1. [`resolve`] (or [`resolve_all`] for many) maps each span to where it is
//!    written, against the [`Origins`](astli_text::Origins) the diagnostic
//!    came with. The result, a [`Resolved`], points at the outermost macro
//!    call, holds the text the call stands for as
//!    [`spelled`](Resolved::spelled), and lists the calls it went
//!    [`through`](Resolved::through) and the includes it was reached from.
//!    It is plain data, for a tool that renders diagnostics its own way, such
//!    as a language server.
//! 2. [`write()`] renders one to a terminal, with the source under it, using
//!    a [`Sources`] cache of the text and a [`Style`].
//!
//! # Example
//!
//! ```
//! use astli_diag::{Sources, Style, resolve_all, write};
//! use astli_preproc::Session;
//!
//! let mut session = Session::new();
//! let file = session.add("top.sv", "`define BAD `NOPE\nassign y = `BAD;\n".into());
//! let expanded = session.expand(file);
//!
//! let resolved = resolve_all(session.origins(), &expanded.diagnostics);
//! let [one] = resolved.as_slice() else { panic!() };
//! assert_eq!(one.diagnostic.code.as_str(), "undefined-macro");
//! assert_eq!(session.origins().slice(one.at), "`BAD"); // where the user wrote it
//! assert_eq!(session.origins().slice(one.spelled.unwrap()), "`NOPE"); // what it stands for
//!
//! let mut out = Vec::new();
//! let mut sources = Sources::new(session.origins());
//! write(&mut out, &mut sources, one, Style::plain()).unwrap();
//! let text = String::from_utf8(out).unwrap();
//! assert!(text.contains("[undefined-macro]"));
//! assert!(text.contains("top.sv:2:12"));
//! ```
//!
//! which renders as:
//!
//! ```text
//! [undefined-macro] Error: `NOPE is not defined
//!    ,-[ top.sv:2:12 ]
//!    |
//!  2 | assign y = `BAD;
//!    |            ^^|^
//!    |              `--- not defined here
//!    |
//!    |-[ top.sv:2:12 ]
//!    |
//!  1 | `define BAD `NOPE
//!    |             ^^|^^
//!    |               `---- this is the text it stands for
//!    |
//!    | Note: the reference stands as written
//! ---'
//! ```
//!
//! `cargo run -p astli-diag --example show` renders a few more, such as an
//! error reached through two macros.
//!
//! [`resolve_all`] also sorts by position and drops a report made twice, as
//! when the same problem is reached twice through one call. A tree from
//! `astli-parse` works the same way: pass its `origins()` and
//! `diagnostics()`.
//!
//! [`Style::default`] uses color and Unicode box drawing; a tool writing to
//! something other than a terminal wants [`Style::plain`].

mod resolve;
mod sources;
mod terminal;

pub use resolve::{Resolved, Through, resolve, resolve_all};
pub use sources::Sources;
pub use terminal::{Style, write};
