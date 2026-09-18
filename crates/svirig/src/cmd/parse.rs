//! `svirig parse` -- the syntax tree, and what it cost to build.
//!
//! This is the one command that opens a session by hand rather than through
//! `SyntaxTree::read`. Reading a file and parsing it are one call there, on
//! purpose, and separating them is the whole point of the summary line: the
//! figures in `docs/plan.md` are a load and a parse, measured apart.

use std::io::Write;
use std::time::Instant;

use svirig_parse::parse;

use crate::cli::{BuildArgs, Parse};
use crate::error::{Error, Result};
use crate::render::{Out, count, tree};
use crate::session;

pub fn run(out: &mut Out, args: &Parse) -> Result {
    // `open` stores the text and lexes it, so the first figure covers both.
    // The parse then reads those tokens back rather than lexing a second time.
    let loaded = Instant::now();
    let opened = session::open(&args.file, &BuildArgs::default())?;
    let loading = loaded.elapsed();

    let tokens = opened.session.tokens(opened.file);
    let source = opened.session.source(opened.file);

    let started = Instant::now();
    let root = parse(&opened.session, opened.file);
    let parsing = started.elapsed();

    if !args.stats {
        tree(out, &root, 0)?;
    }

    // The invariant the whole tree exists to keep.
    let round_trips = root.text() == source;
    let (nodes, leaves) = count(&root);
    writeln!(out)?;
    writeln!(
        out,
        "{} bytes, {} tokens, {nodes} nodes, {leaves} leaves, round-trips: {round_trips}",
        source.len(),
        tokens.len()
    )?;
    writeln!(
        out,
        "load {:?}, parse {:?} ({:.1} Mtok/s)",
        loading,
        parsing,
        tokens.len() as f64 / parsing.as_secs_f64() / 1e6
    )?;

    match round_trips {
        true => Ok(()),
        false => Err(Error::failed("the tree's text is not the input")),
    }
}
