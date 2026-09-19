//! `svirig parse` -- the syntax tree, and what it cost to build.
//!
//! This is the one command that opens a session by hand rather than through
//! `SyntaxTree::read`. Reading a file and parsing it are one call there, on
//! purpose, and separating them is the whole point of the summary: the figures
//! in `docs/plan.md` are a load and a parse, measured apart.
//!
//! A build adds a third. Given an include path or a definition, the file is
//! expanded before it is parsed and only the table that expansion ended with
//! is kept -- arities, which is all raw mode can use. That is not the parser's
//! time and is reported as its own figure, so a run with a build behind it and
//! a run without one can still be compared on the number that matters.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use svirig_parse::parse_seeded;
use svirig_preproc::MacroTable;
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::render::{self, count, duration, tree};
use crate::session;
use crate::sources::{self, Build};

/// Print the syntax tree a file parses to.
#[derive(Args)]
pub struct Parse {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub build: BuildArgs,
}

/// What one file contributed to the run's figures.
pub struct Stats {
    bytes: usize,
    tokens: usize,
    nodes: usize,
    leaves: usize,
    load: Duration,
    /// What the arity pre-pass cost, and zero where there was no build to run
    /// one for.
    seed: Duration,
    parse: Duration,
}
impl RunWith<Ctx<'_>> for Parse {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let resolved = sources::resolve(&self.sources, &self.build)?;

        let quiet = run.quiet;
        let started = Instant::now();
        let outcome = cmd::each(out, &resolved.files, run, "", |sink, file| {
            one(sink, file, &resolved.build, quiet)
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
    // The pre-pass is named only when it ran. It is not the parser's time and
    // folding it into either figure would make one of them a lie, so it stands
    // on its own or not at all.
    let seed = took(|file| file.seed);
    let seeding = match seed.is_zero() {
        true => String::new(),
        false => format!("seed {}, ", duration(seed)),
    };
    writeln!(
        out,
        "load {}, {seeding}parse {}, {} wall ({:.1} Mtok/s parsing, {:.1} overall)",
        duration(load),
        duration(parsing),
        duration(wall),
        rate(parsing),
        rate(wall),
    )?;
    Ok(())
}

fn one(sink: &mut cmd::Sink, path: &Path, build: &Build, quiet: bool) -> Result<Stats> {
    // `open` stores the text and lexes it, so the first figure covers both.
    // The parse then reads those tokens back rather than lexing a second time.
    let loaded = Instant::now();
    let mut opened = session::open(path, build)?;
    let load = loaded.elapsed();

    // Arities, and nothing else. Raw mode still reads the file as written --
    // the expansion's tokens are thrown away and only its table is kept -- but
    // a definition that lives in a header is one this file's own scan could
    // never have found, and it is what says whether the `(` after a name opens
    // an argument list.
    let seeding = Instant::now();
    let (seed, seed_took) = match build.is_empty() {
        // Not merely fast: a run with no build did not do this, and a figure
        // that rounds to zero would say it did.
        true => (MacroTable::new(), Duration::ZERO),
        false => (opened.expand().macros, seeding.elapsed()),
    };

    let tokens = opened.session.tokens(opened.file);
    let source = opened.session.source(opened.file);

    let started = Instant::now();
    let parsed = parse_seeded(&opened.session, opened.file, seed);
    let parsing = started.elapsed();
    let root = &parsed.root;

    if !quiet {
        tree(sink.out, root, 0)?;
    }

    // Two sources, and both are the file's: the grammar's own, and whatever
    // the `-D` seeding pass found on its way through the headers.
    let origins = opened.session.origins();
    sink.errors += render::diagnostics(sink.diagnostics, origins, opened.diagnostics())?;
    sink.errors += render::diagnostics(sink.diagnostics, origins, &parsed.diagnostics)?;

    // The invariant the whole tree exists to keep.
    if root.text() != source {
        return Err(Error::failed("the tree's text is not the input"));
    }

    let (nodes, leaves) = count(root);
    Ok(Stats {
        bytes: source.len(),
        tokens: tokens.len(),
        nodes,
        leaves,
        load,
        seed: seed_took,
        parse: parsing,
    })
}
