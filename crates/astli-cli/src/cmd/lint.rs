//! `astli lint` subcommand.
//!
//! The rules that read a tree read each file on its own, as written, as
//! `fmt` formats it: no include path or `+define+` reaches them. The rules
//! that read a design run only when one is on: then every file is also
//! expanded with the build and lowered, and those rules run over the design
//! all of them make. Levels come from `astli.toml`, then the file's patterns
//! in `astli.toml`, then the flags, so a run can override the project.
//!
//! What the grammar did not cover is an error with a design, as `check`
//! reports it, since expanded it is what a compiler reads. Without one it
//! is a warning: as written, a conditional that splits a construct or a
//! macro standing for syntax keeps valid code from parsing too.
//!
//! With no file named, the files are the package the current directory is
//! in: every file it holds is read on its own, and its dependencies join
//! the design, unreported. `-p` lints another package of the workspace,
//! and `--workspace` every one of them, each with its own `[lint]`.
//!
//! With `--top`, the design rules report only the files the tops reach,
//! as `check` does; the tree rules still read every file. Without it, a
//! package's `[top.NAME]` tables are its tops, and with `--tests` every
//! point of each of its tests too, as `check` takes them: the design rules
//! run once for each set of defines they give, and a finding several of
//! them meet is told once.
//!
//! With `--fix`, the files reported are first rewritten with the fixes the
//! tree rules' findings carry, and what remains is reported.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use astli_lint::{Config, Layer, Level, Linter, RULES, RuleOption, Waivers, lint};
use astli_parse::{NOT_PARSED, SyntaxTree};
use astli_sema::Built;
use astli_text::{Diagnostic, Edit, Origins, apply};
use rayon::prelude::*;
use usage_rs::{Args, RunWith};

use crate::analysis::{self, Analysis, Designs};
use crate::cli::{BuildArgs, FeatureArgs, Members, Sources};
use crate::cmd::{self, Ctx};
use crate::config::{Levels, LintConfig};
use crate::discovery::Plan;
use crate::error::{Error, Result};
use crate::lockfile::Target;
use crate::points;
use crate::render::{self, Report};
use crate::run::Run;
use crate::term;

/// Lint SystemVerilog source files for syntax issues, coding style, and common bugs
#[derive(Args)]
pub struct Lint {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub members: Members,
    #[usage(flatten)]
    pub features: FeatureArgs,
    #[usage(flatten)]
    pub levels: Levels,
    /// Path to a custom configuration file instead of searching for astli.toml
    #[usage(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// List all available lint rules with their rule group and default severity level, and the options each takes with their defaults
    #[usage(long)]
    pub list: bool,
    /// Automatically apply safe fixes for supported lint findings in place
    #[usage(long)]
    pub fix: bool,
    /// Elaborate from the specified root module or [top.NAME] table for hierarchical lint rules (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    /// Also elaborate every point of every test the package marks, for hierarchical lint rules
    #[usage(long)]
    pub tests: bool,
    /// Set a parameter as NAME=VALUE in each top that declares it, over what its [top.NAME] table sets (can be repeated)
    #[usage(short = 'G', long = "param")]
    pub param: Vec<String>,
    /// The build the rules that read a design expand each file with.
    #[usage(flatten)]
    pub build: BuildArgs,
}

impl RunWith<Ctx<'_>> for Lint {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        if self.list {
            return list(out);
        }
        points::asked(self.tests, self.sources.named())?;
        cmd::members(
            run,
            &self.members,
            self.sources.named(),
            out,
            |out, target| self.once(out, target, run),
        )
    }
}

impl Lint {
    /// Lints the files named, or those of the package `target` chose.
    fn once(&self, out: &mut render::Out, target: &Target, run: &Run) -> Result {
        let mut plan = Plan::new(target, &self.sources, &self.features, &self.build, run)?;
        let config = LintConfig::load(run, target, self.config.as_deref(), &self.levels, &plan)?;
        // Each file's levels, which its patterns make.
        let levels: Vec<Config> = (plan.own.iter())
            .map(|path| config.for_file(path))
            .collect();
        let at: HashMap<&Path, usize> = (plan.own.iter().enumerate())
            .map(|(at, path)| (path.as_path(), at))
            .collect();
        let design = levels.iter().any(|it| it.needs(Layer::Design));
        // Only a design rule reads what the tests elaborate.
        let tests = design && self.tests;
        let mut designs = Designs::of(out, &plan, &self.top, tests, &self.param, run)?;
        if self.fix {
            fix(&plan.own, &levels)?;
        }

        // Without a design nothing joins what a file's tree says, so it is
        // told as soon as the file is read; with one, it waits for the
        // design's findings.
        let trees = cmd::each(out, &plan.own, run, None, |sink, path| {
            let mut read = Read::of(path, &levels[at[path]], !self.fix, design)?;
            if !design {
                for (origins, found) in std::mem::take(&mut read.found) {
                    sink.report.render(&origins, &found)?;
                }
            }
            Ok(read)
        })?;
        if !design {
            return trees.finish();
        }
        // A file that could not be read is reported once, and the design
        // is linted without it.
        plan.unread(&trees.failed_of(&plan.own));
        let unread = trees.failed;
        let mut trees = trees.values;
        // The design is every unit, a dependency's too, each expanded with
        // its own package's build. Each file's findings are reported once,
        // what every design found in it together.
        let mut unread_units = 0;
        designs.each(out, plan, run, false, true, |_, plan, analysis, flags| {
            flags.check(analysis)?;
            self.design(&plan, run, analysis, &mut trees)?;
            unread_units = analysis.unread;
            Ok(())
        })?;
        designs.flags.undeclared("no top")?;
        let rendered = render::reports(trees, Read::report)?;
        let mut outcome = analysis::write(out, rendered)?;
        outcome.failed = unread + unread_units;
        outcome.finish()
    }

    /// Lints `analysis`, the design `plan` holds, adding what its rules
    /// find in each file read on its own to what `trees` already found in
    /// it.
    fn design(&self, plan: &Plan, run: &Run, analysis: &Analysis, trees: &mut [Read]) -> Result {
        let tops = analysis.top_names();
        // What a compiler would not read is an error once expanded, where
        // the file as written only warns of it. The rest the expansion and
        // the parser say, the tree read on its own says too.
        let mut unparsed: Vec<Vec<Diagnostic>> = (analysis.parsed.iter())
            .map(|found| {
                (found.iter())
                    .filter(|it| it.code == NOT_PARSED)
                    .cloned()
                    .collect()
            })
            .collect();
        // With tops, the design rules judge only the files they reach: one
        // no top builds is judged at its defaults, which a root's modules
        // rarely build at. The tree rules read every file all the same.
        let reached: Option<HashSet<usize>> = match &analysis.index {
            Some(index) if !tops.is_empty() => {
                let reached =
                    (index.reachable(&tops)).map_err(|err| Error::failed(err.to_string()))?;
                Some(reached.into_iter().collect())
            }
            _ => None,
        };
        let elaboration = &analysis.elaboration;
        let (files, origins) = (&analysis.files, &analysis.origins);
        // Each reported unit, and where it is among the files read on their
        // own.
        let at: HashMap<&Path, usize> = (plan.own.iter().enumerate())
            .map(|(at, path)| (path.as_path(), at))
            .collect();
        let reported: Vec<(usize, usize)> = (plan.units.iter().enumerate())
            .filter(|(unit, it)| it.reported && reached.as_ref().is_none_or(|it| it.contains(unit)))
            .filter_map(|(unit, it)| Some((unit, *at.get(it.path.as_path())?)))
            .collect();
        let built = (reported.iter()).any(|&(_, own)| trees[own].levels.needs(Layer::Built))
            && !tops.is_empty();
        let built = built.then(|| Built::new(elaboration, &analysis.bodies()));
        let linter = Linter::new(elaboration, built.as_ref());
        let mut found: Vec<Vec<Diagnostic>> = (reported.par_iter())
            .map(|&(unit, own)| {
                let tree = &trees[own];
                linter.lint(files[unit], &origins[unit], &tree.waivers, tree.levels)
            })
            .collect();
        for (&(unit, _), found) in reported.iter().zip(&mut found) {
            found.splice(0..0, std::mem::take(&mut unparsed[unit]));
        }

        // Of a file several designs read, each tells only what none before
        // it told.
        if run.tells_once() {
            for (&(unit, _), found) in reported.iter().zip(&mut found) {
                let path = &plan.units[unit].path;
                found.retain(|diagnostic| run.first_found(path, diagnostic));
            }
        }
        for (&(unit, own), found) in reported.iter().zip(found) {
            if !found.is_empty() {
                trees[own].found.push((Arc::clone(&origins[unit]), found));
            }
        }
        Ok(())
    }
}

/// How many times a file is fixed at most: a fix that overlaps another
/// waits for the next pass, and a file that still has one after this many
/// is left with it rather than run on.
const PASSES: usize = 8;

/// Rewrites each of `files`, at the levels `levels` holds for it, with the
/// fixes its tree rules find, pass after
/// pass until none is left, and says how many it applied. A file the parser
/// did not read whole is left alone, since a fix is only known safe in a
/// tree that says what each token is; a pass whose result would not parse
/// whole is dropped.
fn fix(files: &[PathBuf], levels: &[Config]) -> Result {
    let fixed: Vec<usize> = (files.par_iter().zip(levels))
        .map(|(path, config)| {
            let original = crate::session::tree(path)?;
            if !original.diagnostics().is_empty() {
                return Ok(0);
            }
            let (mut tree, mut fixed) = (None, 0);
            for _ in 0..PASSES {
                let current = tree.as_ref().unwrap_or(&original);
                let (waivers, _) = Waivers::read(current);
                let found = lint(current, &waivers, config);
                let fixes: Vec<&[Edit]> = (found.iter())
                    .map(|it| it.fix.as_slice())
                    .filter(|it| !it.is_empty())
                    .collect();
                if fixes.is_empty() {
                    break;
                }
                let (text, applied) = apply(current.source(), &fixes);
                let next = SyntaxTree::parse(path, text);
                if !next.diagnostics().is_empty() {
                    break;
                }
                fixed += applied;
                tree = Some(next);
            }
            if let Some(tree) = tree {
                std::fs::write(path, tree.source()).map_err(|err| Error::io(path, err))?;
            }
            Ok(fixed)
        })
        .collect::<Result<_>>()?;
    let files = fixed.iter().filter(|&&it| it > 0).count();
    if files > 0 {
        let problems = fixed.iter().sum();
        term::status!(
            "Fixed",
            "{} in {}",
            render::plural(problems, "problem"),
            render::plural(files, "file")
        );
    }
    Ok(())
}

/// One file read on its own: what was found in it, each set with what
/// resolves its spans, the tree's first and then each design's; what it
/// waives for the design's rules; and its levels.
struct Read<'a> {
    path: PathBuf,
    found: Vec<(Arc<Origins>, Vec<Diagnostic>)>,
    waivers: Waivers,
    levels: &'a Config,
}

impl<'a> Read<'a> {
    /// What the parser and the tree's rules at `levels` find in `path`, each
    /// finding with a fix saying `--fix` applies it when `offer` asks. A
    /// file the parser did not read whole is not offered it, since `--fix`
    /// leaves that file alone, and neither is a run with `--fix`, whose
    /// findings still holding a fix are those it could not apply.
    ///
    /// What the parser kept as written, which the rules did not read, is a
    /// warning, but with a `design`, whose expansion says it instead.
    fn of(path: &Path, levels: &'a Config, offer: bool, design: bool) -> Result<Read<'a>> {
        let tree = crate::session::tree(path)?;
        let offer = offer && tree.diagnostics().is_empty();
        let mut found = tree.diagnostics().to_vec();
        if !design {
            found.extend(tree.unparsed());
        }
        let (waivers, invalid) = Waivers::read(&tree);
        found.extend(invalid);
        for mut diagnostic in lint(&tree, &waivers, levels) {
            if offer && !diagnostic.fix.is_empty() {
                diagnostic = diagnostic.help("`astli lint --fix` fixes this");
            }
            found.push(diagnostic);
        }
        let found = match found.is_empty() {
            true => Vec::new(),
            false => vec![(Arc::new(tree.into_origins()), found)],
        };
        Ok(Read {
            path: path.to_path_buf(),
            found,
            waivers,
            levels,
        })
    }

    /// What was found, rendered as one list, whichever layer found it.
    fn report(self) -> Result<Report> {
        let sets: Vec<(&Origins, &[Diagnostic])> = (self.found.iter())
            .map(|(origins, found)| (&**origins, found.as_slice()))
            .collect();
        let mut said = Vec::new();
        let errors = render::findings(&mut said, &self.path, &sets)?;
        Ok(Report { said, errors })
    }
}

/// Prints each rule's name, group and default level, and what it finds;
/// under it, each option it takes, set to its default as `astli.toml`
/// writes it, and what the option is.
fn list(out: &mut dyn Write) -> Result {
    let setting = |option: &RuleOption| {
        let values: Vec<String> = (option.default.iter())
            .map(|value| format!("{value:?}"))
            .collect();
        format!("{} = [{}]", option.key, values.join(", "))
    };
    let wide = RULES.iter().map(|rule| rule.name.len()).max().unwrap_or(0);
    let set_wide = (RULES.iter())
        .flat_map(|rule| rule.options)
        .map(|option| setting(option).len())
        .max()
        .unwrap_or(0);
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
        for option in rule.options {
            writeln!(out, "    {:set_wide$}  {}", setting(option), option.summary)?;
        }
    }
    Ok(())
}
