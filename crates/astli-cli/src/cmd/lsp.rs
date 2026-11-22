//! `astli lsp` subcommand.
//!
//! Runs the language server over stdin and stdout, for an editor or an
//! agent's client to start. Stdout is the protocol's, so the command writes
//! nothing of its own there, and what it would write to stderr goes to the
//! client's log.
//!
//! What it answers is a preview, and may change in any release.

use std::sync::Arc;

use astli_lsp::{Ended, Stdio};
use usage_rs::{Args, RunWith};

use crate::cmd::Ctx;
use crate::error::{Error, Result};
use crate::host::Host;
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
        let ended = stdio.serve(Arc::new(Host::default()));
        term::redirect(None);
        match ended {
            Ended::Exited => Ok(()),
            // The protocol's exit code for an exit without a shutdown.
            Ended::ExitedWithoutShutdown => Err(Error::Exited(1)),
            Ended::Failed(why) => Err(Error::failed(why)),
        }
    }
}
