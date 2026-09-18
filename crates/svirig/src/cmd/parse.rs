//! `svirig parse` -- the syntax tree, and what it cost to build.
//!
//! This is the one command that opens a session by hand rather than through
//! `SyntaxTree::read`. Reading a file and parsing it are one call there, on
//! purpose, and separating them is the whole point of the summary: the figures
//! in `docs/plan.md` are a load and a parse, measured apart.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use svirig_parse::parse;
use usage::RunWith;

use crate::cli::{BuildArgs, Parse};
use crate::cmd;
use crate::error::{Error, Result};
use crate::render::{Out, count, duration, tree};
use crate::session;
use crate::sources::{self, Build};

/// What one file contributed to the run's figures.
pub struct Stats {
    bytes: usize,
    tokens: usize,
    nodes: usize,
    leaves: usize,
    load: Duration,
    parse: Duration,
}

impl RunWith<&mut Out> for Parse {
    type Output = Result;

    fn run_with(self, out: &mut Out) -> Result {
        let resolved = sources::resolve(&self.sources, &BuildArgs::default())?;
        resolved.warn_unused_build("parse");

        let quiet = self.run.quiet;
        let started = Instant::now();
        let outcome = cmd::each(out, &resolved.files, &self.run, "", |out, file| {
            one(out, file, quiet)
        })?;
        let wall = started.elapsed();

        if !outcome.values.is_empty() {
            if !quiet {
                writeln!(out)?;
            }
            summary(out, &outcome.files(), &outcome.values, wall)?;
        }

        outcome.finish()
    }
}

/// The run's figures, summed.
///
/// Two rates rather than one, because the two questions are different. The
/// parse rate is the parser's own and is what `docs/plan.md` quotes; the
/// overall rate includes reading the files, lexing them and printing the
/// result, and is what the run actually took. They are the same number only
/// when nothing else happened, which is never.
fn summary(out: &mut dyn Write, read: &str, files: &[Stats], wall: Duration) -> Result {
    let sum = |of: fn(&Stats) -> usize| files.iter().map(of).sum::<usize>();
    let took = |of: fn(&Stats) -> Duration| files.iter().map(of).sum::<Duration>();

    let tokens = sum(|file| file.tokens);
    let load = took(|file| file.load);
    let parsing = took(|file| file.parse);
    let rate = |over: Duration| tokens as f64 / over.as_secs_f64() / 1e6;

    writeln!(
        out,
        "{read}, {} bytes, {tokens} tokens, {} nodes, {} leaves, round-trips: true",
        sum(|file| file.bytes),
        sum(|file| file.nodes),
        sum(|file| file.leaves),
    )?;
    writeln!(
        out,
        "load {}, parse {}, {} wall ({:.1} Mtok/s parsing, {:.1} overall)",
        duration(load),
        duration(parsing),
        duration(wall),
        rate(parsing),
        rate(wall),
    )?;
    Ok(())
}

fn one(out: &mut dyn Write, path: &Path, quiet: bool) -> Result<Stats> {
    // `open` stores the text and lexes it, so the first figure covers both.
    // The parse then reads those tokens back rather than lexing a second time.
    let loaded = Instant::now();
    let opened = session::open(path, &Build::default())?;
    let load = loaded.elapsed();

    let tokens = opened.session.tokens(opened.file);
    let source = opened.session.source(opened.file);

    let started = Instant::now();
    let root = parse(&opened.session, opened.file);
    let parsing = started.elapsed();

    if !quiet {
        tree(out, &root, 0)?;
    }

    // The invariant the whole tree exists to keep.
    if root.text() != source {
        return Err(Error::failed("the tree's text is not the input"));
    }

    let (nodes, leaves) = count(&root);
    Ok(Stats {
        bytes: source.len(),
        tokens: tokens.len(),
        nodes,
        leaves,
        load,
        parse: parsing,
    })
}
