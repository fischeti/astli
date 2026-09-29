//! `astli lint` subcommand.
//!
//! Lints each file on its own, as written, as `fmt` formats it: no include
//! path or `+define+` reaches a rule, and a filelist only names the files.
//! Levels come from `astli.toml`, then the flags, then the file's patterns
//! in `astli.toml`, the most specific last.

use std::io::Write;
use std::path::PathBuf;

use astli_lint::{Level, RULES, lint};
use astli_parse::SyntaxTree;
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::config::{Levels, LintConfig};
use crate::error::{Error, Result};
use crate::render;
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
        };

        let resolved = sources::resolve(&self.sources, &BuildArgs::default())?;
        let outcome = cmd::each(out, &resolved.files, &run, "", |sink, path| {
            let tree = SyntaxTree::read(path).map_err(|err| Error::io(path, err))?;
            let mut found = tree.diagnostics().to_vec();
            found.extend(lint(&tree, &config.for_file(path)));
            sink.errors += render::diagnostics(sink.diagnostics, tree.origins(), &found)?;
            Ok(())
        })?;
        outcome.finish()
    }
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
