//! The language server behind `astli lsp`: its loop, the buffers it keeps
//! of the client's open files, the snapshots of the designs they are in, and
//! the positions it converts. It serves the driver, which starts it, and
//! promises no API to anyone else.
//!
//! One thread runs the loop: it owns the open buffers, takes each message
//! from the client in turn, and sends every response. It answers a request
//! that reads the buffers itself, and hands one over the design to the pool,
//! waiting on the client and the pool at once. Stdout carries the protocol
//! alone, so the driver sends what it would write to stderr to the client's
//! log, through [`Log`].
//!
//! What a design is, and how it is loaded, is the driver's: it implements
//! [`Host`], and the server asks it which design a file is in and to load
//! one through the buffers. Each design an open file is in is loaded again
//! on a thread of its own after the client's edits settle, and its
//! [`Snapshot`] is swapped in whole.
//!
//! A request is added as an arm of the server's dispatch, answered on the
//! spot from the buffers, or handed to the pool with what it reads.
//!
//! ```no_run
//! # use std::collections::HashMap;
//! # use std::path::{Path, PathBuf};
//! # use std::sync::Arc;
//! # use astli_text::Reader;
//! struct Files;
//!
//! impl astli_lsp::Host for Files {
//!     fn version(&self) -> String {
//!         "0.1.0".to_string()
//!     }
//!
//!     fn fmt_options(&self, _: &Path) -> Result<Option<astli_fmt::Options>, String> {
//!         Ok(Some(astli_fmt::Options::default()))
//!     }
//!
//!     fn design(&self, path: &Path) -> PathBuf {
//!         path.to_path_buf()
//!     }
//!
//!     fn load(
//!         &self,
//!         root: &Path,
//!         _: &dyn Reader,
//!         _: bool,
//!         _: &(dyn Fn() -> bool + Sync),
//!     ) -> Option<astli_lsp::Snapshot> {
//!         Some(astli_lsp::Snapshot {
//!             root: root.to_path_buf(),
//!             designs: Vec::new(),
//!             problems: Vec::new(),
//!             lowered: 0,
//!             lacks_dependencies: false,
//!             levels: HashMap::new(),
//!         })
//!     }
//!
//!     fn forget(&self, _: &Path) {}
//!
//!     fn watched(&self) -> astli_lsp::Watched {
//!         let (names, extensions) = (Vec::new(), vec!["sv".to_string()]);
//!         astli_lsp::Watched { names, extensions }
//!     }
//! }
//!
//! let stdio = astli_lsp::Stdio::open();
//! let log = stdio.log();
//! log.send("written to the client's log");
//! assert_eq!(stdio.serve(Arc::new(Files)), astli_lsp::Ended::Exited);
//! ```

mod actions;
mod buffers;
mod cursor;
mod definition;
mod diagnostics;
mod folding;
mod formatting;
mod hover;
mod position;
mod references;
mod server;
mod slots;
mod snapshot;
mod symbols;
mod tasks;
mod tokens;
mod trees;
mod workspace_symbols;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use astli_fmt::Options;
use astli_text::Reader;
use crossbeam_channel::{Sender, after, never, select};
use lsp_server::{Connection, ErrorCode, IoThreads, Message, Response};
use lsp_types::{
    CodeActionKind, CodeActionOptions, FoldingRangeProvider, InitializeParams, InitializeResult,
    MessageType, ServerCapabilities, ServerInfo, TextDocumentSyncKind, TextDocumentSyncOptions,
};

use position::Encoding;
use server::Server;

pub use snapshot::{Analysed, At, Memo, Place, Problem, Snapshot, Unit};

/// What the server needs of the driver: what it says of itself, what only
/// the driver can read, and what a design is and how one is loaded. Shared
/// with the threads that load on it.
pub trait Host: Send + Sync + 'static {
    /// The version `initialize` names: the command's, which a user knows.
    fn version(&self) -> String;

    /// The `[fmt]` of the package the file at `path` is in, which
    /// formatting reads: `None` if the package's `[fmt] exclude` leaves it
    /// as it is, and an error, said as `astli fmt` says it, if the
    /// configuration cannot be read.
    fn fmt_options(&self, path: &Path) -> Result<Option<Options>, String>;

    /// The root of the design the file at `path` is in, as [`Snapshot`]
    /// says what that is. Every file of one design gives the same root.
    fn design(&self, path: &Path) -> PathBuf;

    /// Loads the design at `root`, reading every source file through
    /// `reader`. With `discover`, the package is found and read again;
    /// without, what the last load of `root` found of it may be kept.
    ///
    /// `overtaken` says whether an edit has made the load stale, and is
    /// asked between units: `None` if the load stopped for it. What went
    /// wrong otherwise is in the snapshot's problems, a snapshot with no
    /// design if the load could not analyse one.
    fn load(
        &self,
        root: &Path,
        reader: &dyn Reader,
        discover: bool,
        overtaken: &(dyn Fn() -> bool + Sync),
    ) -> Option<Snapshot>;

    /// Forgets what the loads of the design at `root` kept for the next,
    /// once no open file is in it: a load of it started before keeps
    /// nothing either. Asked on the main loop, so what it drops is dropped
    /// elsewhere.
    fn forget(&self, root: &Path);

    /// The files on disk whose change the server asks the client to tell
    /// it of.
    fn watched(&self) -> Watched;
}

/// The files on disk a design is found from and read from.
pub struct Watched {
    /// The names of the files that say what a package is, such as its
    /// manifest: a change to one finds every design again.
    pub names: Vec<String>,
    /// The extensions of the source files a package's walk finds: one
    /// created or deleted finds every design again, and one changed loads
    /// again each design that read it.
    pub extensions: Vec<String>,
}

/// How the server ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// The client sent `exit` after `shutdown`.
    Exited,
    /// The client sent `exit` with no `shutdown` before it, which the
    /// protocol ends with an exit code of 1.
    ExitedWithoutShutdown,
    /// The connection failed, or the client broke the protocol, as said.
    Failed(String),
}

/// A handle that sends text to the client's log, from any thread, while
/// the server runs; after it ends, what it is sent goes nowhere.
#[derive(Clone)]
pub struct Log(Arc<Mutex<Option<Sender<Message>>>>);

impl Log {
    /// Sends `text` as a `window/logMessage`, its trailing newline dropped.
    pub fn send(&self, text: &str) {
        let message = text.trim_end().to_string();
        if let Some(sender) = self.0.lock().unwrap_or_else(|it| it.into_inner()).as_ref() {
            let _ = sender.send(server::log(MessageType::Log, message));
        }
    }
}

/// A connection to the client over stdin and stdout.
pub struct Stdio {
    connection: Connection,
    threads: IoThreads,
    log: Log,
}

impl Stdio {
    /// Opens the connection. Nothing is read until [`serve`](Self::serve).
    pub fn open() -> Stdio {
        let (connection, threads) = Connection::stdio();
        let log = Log(Arc::new(Mutex::new(Some(connection.sender.clone()))));
        Stdio {
            connection,
            threads,
            log,
        }
    }

    /// A handle to the client's log.
    pub fn log(&self) -> Log {
        self.log.clone()
    }

    /// Serves the client until it says `exit`, then cuts every [`Log`] off,
    /// since the connection closes only once nothing can write to it.
    pub fn serve(self, host: Arc<dyn Host>) -> Ended {
        let Stdio {
            connection,
            threads,
            log,
        } = self;
        let ended = serve(&connection, host);
        *log.0.lock().unwrap_or_else(|it| it.into_inner()) = None;
        // A server that ends in an error may have a client still writing,
        // so only one the client told to exit waits for its threads.
        if let Ended::Failed(_) = ended {
            return ended;
        }
        drop(connection);
        match threads.join() {
            Ok(()) => ended,
            Err(err) => Ended::Failed(format!("the connection to the client failed: {err}")),
        }
    }
}

/// Runs the server over `connection`: the handshake, then each message
/// until `exit`.
pub fn serve(connection: &Connection, host: Arc<dyn Host>) -> Ended {
    let (id, params) = match connection.initialize_start() {
        Ok(started) => started,
        Err(err) => return Ended::Failed(err.to_string()),
    };
    let params: InitializeParams = match serde_json::from_value(params) {
        Ok(params) => params,
        Err(err) => {
            let said = format!("`initialize`: {err}");
            let code = ErrorCode::InvalidParams as i32;
            let _ = (connection.sender).send(Response::new_err(id, code, said.clone()).into());
            return Ended::Failed(said);
        }
    };
    let general = params.capabilities.general.clone().unwrap_or_default();
    let encoding = Encoding::negotiate(&general.position_encodings.unwrap_or_default());
    let watches = (params.capabilities.workspace.as_ref())
        .and_then(|it| it.did_change_watched_files)
        .and_then(|it| it.dynamic_registration)
        .unwrap_or(false);
    let result = Response::new_ok(id, initialized(encoding, &*host));
    let _ = connection.sender.send(result.into());

    let mut server = Server::new(connection.sender.clone(), encoding, host, watches);
    server.client = params.capabilities;
    // Cloned, so that the loop can hand the server to a handler while it
    // waits on the pool.
    let done = server.tasks.receiver.clone();
    let loaded = server.loaded.clone();
    loop {
        // Starts each load that is due, and wakes when the next one is.
        let next = server.load_due(Instant::now());
        let wake = next.map_or_else(never, |it| {
            after(it.saturating_duration_since(Instant::now()))
        });
        select! {
            recv(connection.receiver) -> message => {
                let Ok(message) = message else {
                    let said = "the client closed the connection without `exit`";
                    return Ended::Failed(said.to_string());
                };
                if let std::ops::ControlFlow::Break(ended) = server.handle(message) {
                    return ended;
                }
            }
            recv(done) -> finished => {
                server.finish(finished.expect("the server holds a sender"));
            }
            recv(loaded) -> finished => {
                server.swap(finished.expect("the server holds a sender"));
            }
            recv(wake) -> _ => {}
        }
    }
}

/// What the server answers `initialize` with: the capabilities this
/// version has, each announced by what implements it.
fn initialized(encoding: Encoding, host: &dyn Host) -> InitializeResult {
    let sync = TextDocumentSyncOptions {
        open_close: Some(true),
        change: Some(TextDocumentSyncKind::Incremental),
        ..TextDocumentSyncOptions::default()
    };
    let actions = CodeActionOptions {
        code_action_kinds: Some(vec![
            CodeActionKind::QuickFix,
            CodeActionKind::new(actions::FIX_ALL),
        ]),
        ..CodeActionOptions::default()
    };
    InitializeResult {
        capabilities: ServerCapabilities {
            position_encoding: Some(encoding.kind()),
            text_document_sync: Some(sync.into()),
            code_action_provider: Some(actions.into()),
            definition_provider: Some(true.into()),
            document_highlight_provider: Some(true.into()),
            document_formatting_provider: Some(true.into()),
            document_symbol_provider: Some(true.into()),
            folding_range_provider: Some(FoldingRangeProvider::Bool(true)),
            hover_provider: Some(true.into()),
            references_provider: Some(true.into()),
            semantic_tokens_provider: Some(tokens::options().into()),
            workspace_symbol_provider: Some(true.into()),
            ..ServerCapabilities::default()
        },
        server_info: Some(ServerInfo {
            name: "astli".to_string(),
            version: Some(host.version()),
        }),
    }
}
