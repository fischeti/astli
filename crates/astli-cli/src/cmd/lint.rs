//! `astli lint` subcommand.
//!
//! The rules that read a tree read each file on its own, as written, as
//! `fmt` formats it: no include path or `+define+` reaches them. The rules
//! that read a design run only when one is on: then every file is also
//! expanded with the build and lowered, and those rules run over the design
//! all of them make. Levels come from `astli.toml`, then the flags, then the
//! file's patterns in `astli.toml`, the most specific last.
//!
//! With no file named, the files are the package the current directory is
//! in: every file it holds is read on its own, and its dependencies join
//! the design, unreported.
//!
//! With `--top`, the design rules report only the files the tops reach,
//! as `check` does; the tree rules still read every file.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use astli_index::{Index, Summary};
use astli_lint::{Level, Linter, RULES, Waivers, lint};
use astli_parse::SyntaxTree;
use astli_parse::parse_expanded;
use astli_sema::{Built, Design, Elaboration, Hir, lower};
use astli_text::{Diagnostic, Origins};
use rayon::prelude::*;
use usage_rs::{Args, RunWith};

use astli_package::mark;

use crate::cli::{BuildArgs, FeatureArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::config::{Levels, LintConfig};
use crate::discovery::{Plan, kept_only};
use crate::error::{Error, Result};
use crate::render;
use crate::session;
use crate::term;

/// Lint SystemVerilog source files for syntax issues, coding style, and common bugs
#[derive(Args)]
pub struct Lint {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub features: FeatureArgs,
    #[usage(flatten)]
    pub levels: Levels,
    /// Path to a custom configuration file instead of searching for astli.toml
    #[usage(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// List all available lint rules with their rule group and default severity level
    #[usage(long)]
    pub list: bool,
    /// Elaborate from the specified root module, or a `[top.NAME]` of astli.toml, for rules requiring hierarchy information (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    #[usage(flatten)]
    pub build: LintBuild,
}

/// The build the rules that read a design expand each file with.
///
/// As `BuildArgs`, but for `-D`, which denies a rule here: a definition is
/// `--define` or `+define+`.
#[derive(Args, Default)]
pub struct LintBuild {
    /// Add directory to the `include` search path (can be repeated)
    #[usage(short = 'I', long = "incdir")]
    pub incdir: Vec<PathBuf>,
    /// Add include search path(s), separated by '+'
    #[usage(arg, sigil = "+incdir+", value_name = "+incdir+DIR+...")]
    pub incdir_plus: Vec<String>,
    /// Define preprocessor macro as NAME or NAME=VALUE (can be repeated)
    #[usage(long = "define")]
    pub define: Vec<String>,
    /// Define preprocessor macro(s), separated by '+'
    #[usage(arg, sigil = "+define+", value_name = "+define+NAME[=VALUE]+...")]
    pub define_plus: Vec<String>,
}

impl LintBuild {
    fn args(&self) -> BuildArgs {
        BuildArgs {
            incdir: self.incdir.clone(),
            incdir_plus: self.incdir_plus.clone(),
            define: self.define.clone(),
            define_plus: self.define_plus.clone(),
        }
    }
}

impl RunWith<Ctx<'_>> for Lint {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        if self.list {
            return list(out);
        }
        let mut plan = Plan::new(
            &self.sources,
            &self.features,
            &self.build.args(),
            Some(&self.top),
            run,
        )?;
        let config = LintConfig::load(self.config.as_deref(), &self.levels, &plan, run.locked)?;
        let top = std::mem::take(&mut plan.tops.modules);
        // Nothing goes to standard output, so a heading per file would stand
        // alone.
        let run = RunArgs {
            quiet: true,
            jobs: run.jobs,
            diagnostics: run.diagnostics,
            color: run.color,
            locked: run.locked,
        };

        let design = (plan.own.iter()).any(|path| config.for_file(path).needs_design());
        if !design {
            let outcome = cmd::each(out, &plan.own, &run, "", |sink, path| {
                let tree = SyntaxTree::read(path).map_err(|err| Error::io(path, err))?;
                sink.errors +=
                    render::diagnostics(sink.diagnostics, tree.origins(), &linted(&tree, &config))?;
                Ok(())
            })?;
            return outcome.finish();
        }

        // Each file's findings wait for the design's, to be printed together.
        let trees = cmd::each(out, &plan.own, &run, "", |_, path| {
            let tree = SyntaxTree::read(path).map_err(|err| Error::io(path, err))?;
            let mut said = Vec::new();
            let errors = render::diagnostics(&mut said, tree.origins(), &linted(&tree, &config))?;
            Ok(Read {
                said,
                errors,
                waivers: Waivers::of(&tree),
            })
        })?;
        if trees.failed > 0 {
            return trees.finish();
        }
        let mut trees = trees.values;
        // The design is every unit, a dependency's too, each expanded with
        // its own package's build.
        // Read off the tree only where an override needs what each declares,
        // or the tops which files they reach.
        let summarized = plan.overrides() || !top.is_empty();
        let lowered = cmd::each(out, &plan.units, &run, "", |_, path| {
            let mut opened = session::open(path, plan.build(plan.unit(path)))?;
            let expanded = opened.expand();
            let parsed = parse_expanded(&opened.session, &expanded.tokens);
            let summary = summarized.then(|| Summary::new(&opened.session, opened.file, &parsed));
            Ok((lower(&parsed), opened.session.into_origins(), summary))
        })?;
        if lowered.failed > 0 {
            return lowered.finish();
        }
        let mut hirs = Vec::with_capacity(lowered.values.len());
        let mut origins = Vec::with_capacity(lowered.values.len());
        let mut summaries = Vec::with_capacity(lowered.values.len());
        for (hir, origin, summary) in lowered.values {
            hirs.push(hir);
            origins.push(origin);
            summaries.extend(summary);
        }
        let kept = plan.replace(&summaries)?;
        let (hirs, origins): (Vec<Hir>, Vec<Origins>) =
            (kept_only(hirs, &kept), kept_only(origins, &kept));
        // With tops, the design rules judge only the files they reach: one
        // no top builds is judged at its defaults, which a root's modules
        // rarely build at. The tree rules read every file all the same.
        let reached: Option<HashSet<usize>> = match top.is_empty() {
            true => None,
            false => {
                let index = Index::new(kept_only(summaries, &kept));
                let reached =
                    (index.reachable(&top)).map_err(|err| Error::failed(err.to_string()))?;
                Some(reached.into_iter().collect())
            }
        };
        let design = Design::new(hirs.into_iter().map(Arc::new).collect());
        let elaboration = match top.is_empty() {
            true => Elaboration::new(Arc::new(design)),
            false => Elaboration::with_tops(Arc::new(design), &top),
        };
        (plan.tops).unambiguous(|name| elaboration.design().definition(name).is_some())?;
        let linter = Linter::new(&elaboration);
        let files: Vec<_> = elaboration.design().files().map(|(file, _)| file).collect();

        // Each reported unit, and where it is among the files read on their
        // own.
        let at: HashMap<&Path, usize> = (plan.own.iter().enumerate())
            .map(|(at, path)| (path.as_path(), at))
            .collect();
        let reported: Vec<(usize, usize)> = (plan.units.iter().enumerate())
            .filter(|(unit, it)| it.reported && reached.as_ref().is_none_or(|it| it.contains(unit)))
            .filter_map(|(unit, it)| Some((unit, *at.get(it.path.as_path())?)))
            .collect();
        let mut found: Vec<Vec<Diagnostic>> = (reported.par_iter())
            .map(|&(unit, own)| {
                let config = config.for_file(&plan.units[unit].path);
                linter.lint(files[unit], &origins[unit], &trees[own].waivers, &config)
            })
            .collect();
        let tops = (reported.iter())
            .any(|&(unit, _)| config.for_file(&plan.units[unit].path).needs_tops());
        if tops && !top.is_empty() {
            let mut bodies = Vec::new();
            for top in &top {
                let Some(body) = elaboration.top(top) else {
                    return Err(Error::failed(format!(
                        "no module, interface or program is called {}",
                        mark::name(top)
                    )));
                };
                bodies.push(body);
            }
            let mut widths = vec![Vec::new(); plan.units.len()];
            for width in astli_lint::widths(&elaboration, &Built::new(&elaboration, &bodies)) {
                widths[width.file.index()].push(width);
            }
            for (&(unit, own), found) in reported.iter().zip(&mut found) {
                let config = config.for_file(&plan.units[unit].path);
                let waivers = &trees[own].waivers;
                found.extend(linter.lint_built(&origins[unit], waivers, &config, &widths[unit]));
                found.sort_by_key(|it| (it.at.src_id, it.at.start, it.code));
            }
        }
        for (&(unit, own), found) in reported.iter().zip(&found) {
            let tree = &mut trees[own];
            tree.errors += render::diagnostics(&mut tree.said, &origins[unit], found)?;
        }

        out.flush()?;
        let mut wrong = 0;
        for tree in &trees {
            wrong += usize::from(tree.errors > 0);
            term::write(&tree.said).map_err(Error::Output)?;
        }
        let total = trees.len();
        match wrong {
            0 => Ok(()),
            _ if total == 1 => Err(Error::Silent),
            wrong => Err(Error::tally(format!(
                "{wrong} of {} have errors",
                render::plural(total, "file")
            ))),
        }
    }
}

/// What the parser and the rules that read `tree` find in it.
fn linted(tree: &SyntaxTree, config: &LintConfig) -> Vec<Diagnostic> {
    let mut found = tree.diagnostics().to_vec();
    found.extend(lint(tree, &config.for_file(tree.path())));
    found
}

/// One file read on its own, with what its tree's rules found already
/// rendered, and what it waives for the design's rules.
struct Read {
    said: Vec<u8>,
    errors: usize,
    waivers: Waivers,
}

/// Prints each rule's name, group and default level, and what it finds.
fn list(out: &mut dyn Write) -> Result {
    let wide = RULES.iter().map(|rule| rule.name.len()).max().unwrap_or(0);
    for rule in RULES {
        let level = match rule.group.level() {
            Level::Allow => "allow",
            Level::Warn => "warn",
            Level::Deny => "deny",
        };
        writeln!(
            out,
            "{:wide$}  {:11}  {:5}  {}",
            rule.name,
            rule.group.name(),
            level,
            rule.summary
        )?;
    }
    Ok(())
}
