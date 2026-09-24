//! `svirig parse` subcommand.
//!
//! Parses SystemVerilog files into syntax trees, validates the round-trip invariant,
//! and reports performance statistics (load/lex time, optional macro-seeding pass,
//! and parse time).

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use svirig_parse::parse_seeded;
use svirig_preproc::{Build, MacroTable};
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::render::{self, count, duration, tree};
use crate::session;
use crate::sources;

/// Parse SystemVerilog source files and print syntax trees.
#[derive(Args)]
pub struct Parse {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub build: BuildArgs,
}

/// Per-file parsing statistics and performance metrics.
pub struct Stats {
    bytes: usize,
    tokens: usize,
    nodes: usize,
    leaves: usize,
    load: Duration,
    /// Duration of macro-seeding pre-pass (zero if no build flags or include paths were provided).
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

/// Prints aggregated performance and throughput statistics across all processed files.
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

    // Report macro seeding duration only if a build pre-pass was executed.
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
    // Reading the file and tokenizing it are measured together as load time.
    let loaded = Instant::now();
    let mut opened = session::open(path, build)?;
    let load = loaded.elapsed();

    // If build options (defines / includes) were provided, run a quick preprocessor
    // expansion pass to collect macro names and arities for parsing disambiguation.
    let seeding = Instant::now();
    let (seed, seed_took) = match build.is_empty() {
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

    // Collect diagnostics from both preprocessor seeding and the syntax parser.
    let origins = opened.session.origins();
    sink.errors += render::diagnostics(sink.diagnostics, origins, opened.diagnostics())?;
    sink.errors += render::diagnostics(sink.diagnostics, origins, &parsed.diagnostics)?;

    // Validate lossless round-trip invariant.
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
