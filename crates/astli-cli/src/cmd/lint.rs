//! `astli lint` subcommand.
//!
//! Lints each file on its own, as written, as `fmt` formats it: no include
//! path or `+define+` reaches a rule, and a filelist only names the files.

use std::io::Write;

use astli_lint::{Config, Group, Level, RULES, lint};
use astli_parse::SyntaxTree;
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx};
use crate::error::{Error, Result};
use crate::render;
use crate::sources;

/// Check SystemVerilog source files against lint rules; an error fails the run
#[derive(Args)]
pub struct Lint {
    #[usage(flatten)]
    pub sources: Sources,
    /// Turn off a rule, or every rule of a group (can be repeated)
    #[usage(short = 'A', long, value_name = "RULE")]
    pub allow: Vec<String>,
    /// Report a rule, or every rule of a group, as a warning (can be repeated)
    #[usage(short = 'W', long, value_name = "RULE")]
    pub warn: Vec<String>,
    /// Report a rule, or every rule of a group, as an error (can be repeated)
    #[usage(short = 'D', long, value_name = "RULE")]
    pub deny: Vec<String>,
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
        let config = self.config()?;
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
            found.extend(lint(&tree, &config));
            sink.errors += render::diagnostics(sink.diagnostics, tree.origins(), &found)?;
            Ok(())
        })?;
        outcome.finish()
    }
}

impl Lint {
    /// The levels the flags ask for. Groups are set before rules, so a rule
    /// named on its own wins over its group whichever flag names it; among
    /// flags of one kind, the stricter wins.
    fn config(&self) -> Result<Config> {
        let mut config = Config::default();
        let flags = [
            (&self.allow, Level::Allow),
            (&self.warn, Level::Warn),
            (&self.deny, Level::Deny),
        ];
        for groups in [true, false] {
            for (names, level) in flags {
                let named = names
                    .iter()
                    .filter(|name| Group::named(name).is_some() == groups);
                for name in named {
                    config
                        .set(name, level)
                        .map_err(|err| Error::failed(format!("{err}; see --list")))?;
                }
            }
        }
        Ok(config)
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
