//! `astli parse` subcommand.
//!
//! Parses SystemVerilog files into syntax trees, raw or expanded, validates
//! that each tree's text is what it was parsed from, and reports performance
//! statistics (load/lex time, the expansion if one ran, and parse time).

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use astli_parse::{parse, parse_expanded};
use astli_preproc::{Build, MacroTable};
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
    /// Parse what a compiler reads: macros expanded, includes followed, and only the branches the build takes
    #[usage(long)]
    pub expand: bool,
}

/// Per-file parsing statistics and performance metrics.
pub struct Stats {
    bytes: usize,
    tokens: usize,
    nodes: usize,
    leaves: usize,
    load: Duration,
    /// Duration of the expansion, which raw mode runs only to seed macro arities from a build.
    expand: Duration,
    parse: Duration,
}

impl RunWith<Ctx<'_>> for Parse {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let resolved = sources::resolve(&self.sources, &self.build)?;

        let quiet = run.quiet;
        let expand = self.expand;
        let started = Instant::now();
        let outcome = cmd::each(out, &resolved.files, run, "", |sink, file| {
            one(sink, file, &resolved.build, expand, quiet)
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

    // Report the expansion only if one ran.
    let expand = took(|file| file.expand);
    let expanding = match expand.is_zero() {
        true => String::new(),
        false => format!("expand {}, ", duration(expand)),
    };
    writeln!(
        out,
        "load {}, {expanding}parse {}, {} wall ({:.1} Mtok/s parsing, {:.1} overall)",
        duration(load),
        duration(parsing),
        duration(wall),
        rate(parsing),
        rate(wall),
    )?;
    Ok(())
}

fn one(
    sink: &mut cmd::Sink,
    path: &Path,
    build: &Build,
    expand: bool,
    quiet: bool,
) -> Result<Stats> {
    // Reading the file and tokenizing it are measured together as load time.
    let loaded = Instant::now();
    let mut opened = session::open(path, build)?;
    let load = loaded.elapsed();

    // Expanded mode parses the expansion. Raw mode runs one only to learn macro
    // arities from the build, when there is a build to learn from.
    let expanding = Instant::now();
    let expanded = (expand || !build.is_empty()).then(|| opened.expand());
    let expand_took = match expanded {
        Some(_) => expanding.elapsed(),
        None => Duration::ZERO,
    };

    let started = Instant::now();
    let (parsed, tokens, text) = match expanded {
        Some(expanded) if expand => {
            let parsed = parse_expanded(&opened.session, &expanded.tokens);
            let text = astli_preproc::render(opened.session.origins(), &expanded.tokens);
            (parsed, expanded.tokens.len(), text)
        }
        expanded => {
            let seed = expanded.map_or_else(MacroTable::new, |expanded| expanded.macros);
            let parsed = parse(&opened.session, opened.file, seed);
            let tokens = opened.session.tokens(opened.file).len();
            (
                parsed,
                tokens,
                opened.session.source(opened.file).to_string(),
            )
        }
    };
    let parsing = started.elapsed();
    let root = &parsed.root;

    if !quiet {
        tree(sink.out, root, 0)?;
    }

    // Collect diagnostics from both the expansion and the syntax parser.
    let origins = opened.session.origins();
    sink.errors += render::diagnostics(sink.diagnostics, origins, opened.diagnostics())?;
    sink.errors += render::diagnostics(sink.diagnostics, origins, &parsed.diagnostics)?;

    // A raw tree's text is the file's, an expanded tree's the expansion's.
    if root.text() != text.as_str() {
        return Err(Error::failed("the tree's text is not the input"));
    }

    let (nodes, leaves) = count(root);
    Ok(Stats {
        bytes: opened.session.source(opened.file).len(),
        tokens,
        nodes,
        leaves,
        load,
        expand: expand_took,
        parse: parsing,
    })
}
