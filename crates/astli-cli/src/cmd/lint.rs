//! `astli lint` subcommand.
//!
//! The rules that read a tree read each file on its own, as written, as
//! `fmt` formats it: no include path or `+define+` reaches them. The rules
//! that read a design run only when one is on: then every file is also
//! expanded with the build and lowered, and those rules run over the design
//! all of them make. Levels come from `astli.toml`, then the flags, then the
//! file's patterns in `astli.toml`, the most specific last.

use std::io::Write;
use std::path::PathBuf;

use astli_lint::{Level, Linter, RULES, Waivers, lint};
use astli_parse::SyntaxTree;
use astli_parse::parse_expanded;
use astli_sema::{Design, Hir, lower};
use astli_text::{Diagnostic, Origins};
use rayon::prelude::*;
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::config::{Levels, LintConfig};
use crate::error::{Error, Result};
use crate::render;
use crate::session;
use crate::sources;

/// Check SystemVerilog source files against lint rules; an error fails the run
#[derive(Args)]
pub struct Lint {
    #[usage(flatten)]
    pub sources: Sources,
    #[usage(flatten)]
    pub levels: Levels,
    /// Read levels from this file rather than the `astli.toml` found from the current directory up
    #[usage(long, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// List the rules, with their group and default level, and lint nothing
    #[usage(long)]
    pub list: bool,
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
        let config = LintConfig::load(self.config.as_deref(), &self.levels)?;
        // Nothing goes to standard output, so a heading per file would stand
        // alone.
        let run = RunArgs {
            quiet: true,
            jobs: run.jobs,
            diagnostics: run.diagnostics,
        };

        let resolved = sources::resolve(&self.sources, &self.build.args())?;
        let design = resolved
            .files
            .iter()
            .any(|path| config.for_file(path).needs_design());
        if !design {
            let outcome = cmd::each(out, &resolved.files, &run, "", |sink, path| {
                let tree = SyntaxTree::read(path).map_err(|err| Error::io(path, err))?;
                sink.errors +=
                    render::diagnostics(sink.diagnostics, tree.origins(), &linted(&tree, &config))?;
                Ok(())
            })?;
            return outcome.finish();
        }

        // Each file's findings wait for the design's, to be printed together.
        let build = resolved.build();
        let outcome = cmd::each(out, &resolved.files, &run, "", |_, path| {
            let tree = SyntaxTree::read(path).map_err(|err| Error::io(path, err))?;
            let mut said = Vec::new();
            let errors = render::diagnostics(&mut said, tree.origins(), &linted(&tree, &config))?;
            let mut opened = session::open(path, &build)?;
            let expanded = opened.expand();
            let hir = lower(&parse_expanded(&opened.session, &expanded.tokens));
            let lowered = Lowered {
                path: path.to_path_buf(),
                said,
                errors,
                waivers: Waivers::of(&tree),
                origins: opened.session.into_origins(),
            };
            Ok((hir, lowered))
        })?;
        if outcome.failed > 0 {
            return outcome.finish();
        }
        let total = outcome.values.len();
        let (hirs, lowered): (Vec<Hir>, Vec<Lowered>) = outcome.values.into_iter().unzip();
        let design = Design::new(hirs);
        let linter = Linter::new(&design);
        let files: Vec<_> = design.files().map(|(file, _)| file).collect();
        let found: Vec<_> = (files.par_iter().zip(&lowered))
            .map(|(&file, one)| {
                let config = config.for_file(&one.path);
                linter.lint(file, &one.origins, &one.waivers, &config)
            })
            .collect();

        out.flush()?;
        let mut stderr = std::io::stderr().lock();
        let mut wrong = 0;
        for (one, found) in lowered.iter().zip(&found) {
            let mut said = one.said.clone();
            let errors = one.errors + render::diagnostics(&mut said, &one.origins, found)?;
            wrong += usize::from(errors > 0);
            stderr.write_all(&said).map_err(Error::Output)?;
        }
        match wrong {
            0 => Ok(()),
            _ if total == 1 => Err(Error::Silent),
            wrong => Err(Error::failed(format!(
                "{wrong} of {total} file(s) have errors"
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

/// One file, lowered for the design, with what its tree's rules found
/// already rendered.
struct Lowered {
    path: PathBuf,
    said: Vec<u8>,
    errors: usize,
    waivers: Waivers,
    origins: Origins,
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
