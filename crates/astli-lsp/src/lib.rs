//! The language server behind `astli lsp`: its loop, the buffers it keeps
//! of the client's open files, and the positions it converts. It serves the
//! driver, which starts it, and promises no API to anyone else.
//!
//! One thread runs the loop: it owns the open buffers, takes each message
//! from the client in turn, and sends every response. It answers a request
//! that reads the buffers itself, and hands one over the design to the pool,
//! waiting on the client and the pool at once. Stdout carries the protocol
//! alone, so the driver sends what it would write to stderr to the client's
//! log, through [`Log`].
//!
//! A request is added as an arm of the server's dispatch, answered on the
//! spot from the buffers, or handed to the pool with what it reads.
//!
//! ```no_run
//! let stdio = astli_lsp::Stdio::open();
//! let log = stdio.log();
//! log.send("written to the client's log");
//! let host = astli_lsp::Host {
//!     version: "0.1.0".to_string(),
//!     fmt_options: std::sync::Arc::new(|_| Ok(Some(astli_fmt::Options::default()))),
//! };
//! assert_eq!(stdio.serve(&host), astli_lsp::Ended::Exited);
//! ```

mod buffers;
mod folding;
mod formatting;
mod position;
mod server;
mod symbols;
mod tasks;
#[cfg(test)]
mod tests;
mod trees;

use std::sync::{Arc, Mutex};

use crossbeam_channel::{Sender, select};
use lsp_server::{Connection, ErrorCode, IoThreads, Message, Response};
use lsp_types::{
    FoldingRangeProvider, InitializeParams, InitializeResult, MessageType, ServerCapabilities,
    ServerInfo, TextDocumentSyncKind, TextDocumentSyncOptions,
};

pub use formatting::FmtOptions;
use position::Encoding;
use server::Server;

/// What the caller hands the server: what it says of itself to the
/// client, and what only the driver can read.
pub struct Host {
    /// The version `initialize` names: the command's, which a user knows.
    pub version: String,
    /// The `[fmt]` of the package a file is in, which formatting reads.
    pub fmt_options: FmtOptions,
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
    pub fn serve(self, host: &Host) -> Ended {
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
pub fn serve(connection: &Connection, host: &Host) -> Ended {
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
    let result = Response::new_ok(id, initialized(encoding, host));
    let _ = connection.sender.send(result.into());

    let mut server = Server::new(
        connection.sender.clone(),
        encoding,
        host.fmt_options.clone(),
    );
    server.client = params.capabilities;
    // Cloned, so that the loop can hand the server to a handler while it
    // waits on the pool.
    let done = server.tasks.receiver.clone();
    loop {
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
        }
    }
}

/// What the server answers `initialize` with: the capabilities this
/// version has, each announced by what implements it.
fn initialized(encoding: Encoding, host: &Host) -> InitializeResult {
    let sync = TextDocumentSyncOptions {
        open_close: Some(true),
        change: Some(TextDocumentSyncKind::Incremental),
        ..TextDocumentSyncOptions::default()
    };
    InitializeResult {
        capabilities: ServerCapabilities {
            position_encoding: Some(encoding.kind()),
            text_document_sync: Some(sync.into()),
            document_formatting_provider: Some(true.into()),
            document_symbol_provider: Some(true.into()),
            folding_range_provider: Some(FoldingRangeProvider::Bool(true)),
            ..ServerCapabilities::default()
        },
        server_info: Some(ServerInfo {
            name: "astli".to_string(),
            version: Some(host.version.clone()),
        }),
    }
}
