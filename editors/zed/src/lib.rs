//! Tells Zed how to start astli's language server.

use zed_extension_api::settings::LspSettings;
use zed_extension_api::{self as zed, Command, LanguageServerId, Result, Worktree};

struct Astli;

impl zed::Extension for Astli {
    fn new() -> Self {
        Astli
    }

    /// `astli lsp`: the binary `lsp.astli.binary.path` of the settings
    /// names, or the `astli` on `PATH`. A build from a checkout is tried
    /// through the first without replacing the one installed.
    fn language_server_command(
        &mut self,
        server: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let binary = LspSettings::for_worktree(server.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.binary);
        // Zed hands over an empty list where the settings give no arguments.
        let args = (binary.as_ref())
            .and_then(|it| it.arguments.clone())
            .filter(|it| !it.is_empty());
        let command = match binary.and_then(|it| it.path) {
            Some(path) => path,
            None => worktree.which("astli").ok_or_else(|| {
                "no `astli` on PATH; install it, or set `lsp.astli.binary.path`".to_string()
            })?,
        };
        Ok(Command {
            command,
            args: args.unwrap_or_else(|| vec!["lsp".to_string()]),
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(Astli);
