//! `astli lsp` subcommand.
//!
//! Runs the language server over stdin and stdout, for an editor or an
//! agent's client to start. Stdout is the protocol's, so the command writes
//! nothing of its own there, and what it would write to stderr goes to the
//! client's log.
//!
//! What it answers is a preview, and may change in any release.

use std::path::Path;
use std::sync::Arc;

use astli_fmt::Options;
use astli_lsp::{Ended, Host, Stdio};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::config::FmtConfig;
use crate::error::{Error, Result};
use crate::target::Target;
use crate::term;

/// Run the language server over stdin and stdout (preview)
#[derive(Args)]
#[usage(
    after_help = "A preview: what it answers, and the capabilities it announces, may change in any release."
)]
pub struct Lsp {}

impl RunWith<Ctx<'_>> for Lsp {
    type Output = Result;

    fn run_with(self, _: Ctx<'_>) -> Result {
        let stdio = Stdio::open();
        let log = stdio.log();
        term::redirect(Some(Box::new(move |text| log.send(text))));
        let host = Host {
            version: env!("CARGO_PKG_VERSION").to_string(),
            fmt_options: Arc::new(fmt_options),
        };
        let ended = stdio.serve(&host);
        term::redirect(None);
        match ended {
            Ended::Exited => Ok(()),
            // The protocol's exit code for an exit without a shutdown.
            Ended::ExitedWithoutShutdown => Err(Error::Exited(1)),
            Ended::Failed(why) => Err(Error::failed(why)),
        }
    }
}

/// The layout of the file at `path`: the `[fmt]` of the package it is in,
/// as `astli fmt` run beside it reads it, or `None` if its `exclude` names
/// the file. The manifest is read on each request, so that an edit to it
/// counts from the next format on.
fn fmt_options(path: &Path) -> std::result::Result<Option<Options>, String> {
    let dir = path.parent().unwrap_or(path);
    let config = FmtConfig::load(&Target::new(dir.to_path_buf()), None);
    let config = config.map_err(|err| err.to_string())?;
    Ok((!config.excludes(path)).then_some(config.options))
}
