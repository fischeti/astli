//! What the main loop does with each message: the state it owns, and a
//! handler for each request and notification it knows.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::ops::ControlFlow;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{Receiver, Sender};
use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response, ResponseError};
use lsp_types::{
    CancelParams, ClientCapabilities, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidChangeWatchedFilesRegistrationOptions, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, FileChangeType, FileSystemWatcher, Id, LogMessageParams,
    MessageType, Registration, RegistrationParams, ShowMessageParams,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::buffers::{self, Buffers};
use crate::position::Encoding;
use crate::slots::{Loaded, Slot, drop_later};
use crate::tasks::{Cancel, Done, Outcome, Tasks, panic_message};
use crate::trees::Trees;
use crate::{Ended, Host, Snapshot, Watched};

/// The id of the one request the server sends, which registers the files
/// it watches.
const WATCHING: &str = "astli/watching";

/// The server's state, owned by the main loop.
pub struct Server {
    sender: Sender<Message>,
    pub encoding: Encoding,
    pub buffers: Buffers,
    pub trees: Trees,
    pub tasks: Tasks,
    pub host: Arc<dyn Host>,
    /// The design root of each open buffer, asked of the host when it was
    /// opened: a file moves to another design only when discovery runs
    /// again.
    roots: HashMap<PathBuf, PathBuf>,
    /// Each design an open buffer is in, by its root.
    slots: HashMap<PathBuf, Slot>,
    /// What the loads send back.
    loads: Sender<Loaded>,
    /// What the main loop waits on beside the client and the requests.
    pub loaded: Receiver<Loaded>,
    /// What the client said it can take in `initialize`.
    pub client: ClientCapabilities,
    /// Whether the client watches the files on disk for the server once
    /// asked to. A client that does not tells it of no change on disk: a
    /// design is found again only when it is made again, after its files
    /// are closed and one is opened.
    watches: bool,
    /// What it watches.
    watched: Watched,
    /// Whether the client has sent `shutdown`, after which every request is
    /// refused and `exit` ends the server cleanly.
    shut_down: bool,
}

impl Server {
    pub fn new(
        sender: Sender<Message>,
        encoding: Encoding,
        host: Arc<dyn Host>,
        watches: bool,
    ) -> Server {
        let (loads, loaded) = crossbeam_channel::unbounded();
        let watched = host.watched();
        Server {
            sender,
            encoding,
            buffers: Buffers::default(),
            trees: Trees::default(),
            tasks: Tasks::new(),
            host,
            roots: HashMap::new(),
            slots: HashMap::new(),
            loads,
            loaded,
            client: ClientCapabilities::default(),
            watches,
            watched,
            shut_down: false,
        }
    }

    /// Handles one message from the client, and breaks with how the server
    /// ends once it is `exit`.
    ///
    /// A panic in a handler is caught here, told to the client, and for a
    /// request answered with an error, so that one bug does not take every
    /// open file's state with it.
    pub fn handle(&mut self, message: Message) -> ControlFlow<Ended> {
        match message {
            Message::Request(request) => {
                let (id, method) = (request.id.clone(), request.method.clone());
                let handled = catch_unwind(AssertUnwindSafe(|| self.request(request)));
                if let Err(payload) = handled {
                    self.answer_outcome(id, &method, Err(payload));
                }
            }
            Message::Notification(notification) if notification.method == "exit" => {
                return ControlFlow::Break(match self.shut_down {
                    true => Ended::Exited,
                    false => Ended::ExitedWithoutShutdown,
                });
            }
            Message::Notification(notification) => {
                let method = notification.method.clone();
                let handled = catch_unwind(AssertUnwindSafe(|| self.notification(notification)));
                if let Err(payload) = handled {
                    let said = panic_message(&*payload);
                    self.log(MessageType::Error, format!("`{method}` panicked: {said}"));
                }
            }
            // The client cannot watch for the server: it watches nothing.
            Message::Response(response) => {
                if let Err(error) = response.response_result {
                    let said = format!("the client watches no file: {}", error.message);
                    self.log(MessageType::Warning, said);
                }
            }
        }
        ControlFlow::Continue(())
    }

    /// Starts a load of each design whose edits have settled, and says when
    /// the next one is due.
    pub fn load_due(&mut self, now: Instant) -> Option<Instant> {
        for (root, slot) in &mut self.slots {
            if slot.due.is_some_and(|due| due <= now) {
                let texts = self.buffers.texts();
                slot.load(root, texts, &self.host, &self.loads);
            }
        }
        self.slots.values().filter_map(|slot| slot.due).min()
    }

    /// Swaps in the snapshot a load made, if no edit has overtaken it, and
    /// tells the client's log; a load that panicked is told too. What is
    /// dropped of a design is dropped off the loop.
    pub fn swap(&mut self, loaded: Loaded) {
        let root = loaded.root.clone();
        let Some(slot) = self.slots.get_mut(&root) else {
            return drop_later(loaded);
        };
        match slot.swap(loaded, &self.buffers) {
            Ok(Some(snapshot)) => {
                let (units, lowered) = (snapshot.units(), snapshot.lowered);
                let took = slot.took.as_millis();
                let root = root.display();
                let said = format!("loaded {root}: {units} units, {lowered} lowered, in {took} ms");
                self.log(MessageType::Log, said);
            }
            Ok(None) => {}
            Err(panicked) => {
                let said = format!("loading {} panicked: {panicked}", root.display());
                self.log(MessageType::Error, said);
            }
        }
    }

    /// The snapshot current for the file at `path`, if it is open and its
    /// design has loaded.
    #[expect(dead_code, reason = "the requests over the design call it")]
    pub fn snapshot(&self, path: &Path) -> Option<Arc<Snapshot>> {
        let root = self.roots.get(&buffers::key(path))?;
        self.slots.get(root)?.current.clone()
    }

    /// The snapshot current for each design an open file is in, of those
    /// that have loaded.
    pub fn snapshots(&self) -> Vec<Arc<Snapshot>> {
        (self.slots.values())
            .filter_map(|slot| slot.current.clone())
            .collect()
    }

    /// Marks stale every design that the file at `path` is in, by `root`
    /// or by what its load read, or that a load running reads, so that a
    /// load running stops and another is made once the edits settle.
    fn touch(&mut self, path: &Path, root: Option<&Path>) {
        let now = Instant::now();
        for (at, slot) in &mut self.slots {
            if Some(at.as_path()) == root || slot.reads(path) {
                slot.stale(now);
            }
        }
    }

    /// Drops each design no open buffer is in, off the loop: overtakes its
    /// load running, and tells the host to forget what it kept of it.
    fn drop_unused(&mut self) {
        let unused: Vec<PathBuf> = (self.slots.keys())
            .filter(|root| !self.roots.values().any(|it| it == *root))
            .cloned()
            .collect();
        for root in unused {
            if let Some(slot) = self.slots.remove(&root) {
                slot.overtake();
                self.host.forget(&root);
                drop_later(slot);
            }
        }
    }

    /// Answers the request whose work on the pool is `done`.
    pub fn finish(&mut self, done: Done) {
        let (id, method) = (done.id.clone(), done.method.clone());
        let outcome = self.tasks.finish(done);
        self.answer_outcome(id, &method, outcome);
    }

    /// Answers a request now, or hands it to the pool.
    ///
    /// A request a handler answers here reads the buffers and their trees,
    /// which stay on this thread. One over the design goes to
    /// [`spawn`](Self::spawn) with what it reads, so that the loop goes on
    /// taking edits and cancellations while it runs.
    fn request(&mut self, request: Request) {
        let Request { id, method, params } = request;
        if self.shut_down {
            let error = error(ErrorCode::InvalidRequest, "the server is shut down");
            return self.answer::<()>(id, Err(error));
        }
        match method.as_str() {
            "shutdown" => {
                self.shut_down = true;
                self.answer(id, Ok(()));
            }
            "textDocument/documentSymbol" => {
                self.reply(id, &method, params, Server::document_symbols);
            }
            "textDocument/foldingRange" => self.reply(id, &method, params, Server::folding_ranges),
            "textDocument/formatting" => self.formatting(id, params),
            "workspace/symbol" => self.workspace_symbols(id, params),
            _ => {
                let _ = params;
                let said = format!("`{method}` is not a request astli answers");
                self.answer::<()>(id, Err(error(ErrorCode::MethodNotFound, said)));
            }
        }
    }

    /// Handles a notification. One the server does not know is dropped, as
    /// the protocol has it, and so is every notification after `shutdown`.
    fn notification(&mut self, notification: Notification) {
        if self.shut_down {
            return;
        }
        let Notification { method, params } = notification;
        match method.as_str() {
            "$/cancelRequest" => self.with(&method, params, |server, it: CancelParams| {
                let id = match it.id {
                    Id::Int(id) => RequestId::from(id),
                    Id::String(id) => RequestId::from(id),
                };
                server.tasks.cancel(&id);
            }),
            "textDocument/didOpen" => self.with(&method, params, Server::open),
            "textDocument/didChange" => self.with(&method, params, Server::change),
            "textDocument/didClose" => self.with(&method, params, Server::close),
            "initialized" => self.watch(),
            "workspace/didChangeWatchedFiles" => self.with(&method, params, Server::on_disk),
            _ => {}
        }
    }

    fn open(&mut self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        let Some(path) = buffers::path(&document.uri) else {
            return;
        };
        // Asked first, so that a host that panics leaves no buffer without
        // a design.
        let root = self.host.design(&path);
        self.buffers.open(&path, document.text);
        self.slots.entry(root.clone()).or_insert_with(Slot::new);
        self.roots.insert(path.clone(), root.clone());
        self.touch(&path, Some(&root));
        // Opened again, a file may be in another design than before.
        self.drop_unused();
    }

    fn change(&mut self, params: DidChangeTextDocumentParams) {
        let document = params.text_document;
        let uri = document.text_document_identifier.uri;
        let Some(path) = buffers::path(&uri) else {
            return;
        };
        let changes = params.content_changes;
        if !(self.buffers).change(&path, changes, self.encoding) {
            return self.log(
                MessageType::Warning,
                format!("{uri} changed, and is not open"),
            );
        }
        let root = self.roots.get(&path).cloned();
        self.touch(&path, root.as_deref());
    }

    fn close(&mut self, params: DidCloseTextDocumentParams) {
        let Some(path) = buffers::path(&params.text_document.uri) else {
            return;
        };
        if self.buffers.close(&path) {
            self.roots.remove(&path);
            // What the disk holds may not be what the buffer did.
            self.touch(&path, None);
            self.drop_unused();
        }
    }

    /// Asks the client to watch the files a design is found from and read
    /// from, if it can.
    fn watch(&mut self) {
        if !self.watches {
            return;
        }
        let names = self.watched.names.iter().map(|name| format!("**/{name}"));
        let sources = format!("**/*.{{{}}}", self.watched.extensions.join(","));
        let watchers = (names.chain([sources]))
            .map(|glob| FileSystemWatcher::new(glob.into(), None))
            .collect();
        let options = DidChangeWatchedFilesRegistrationOptions::new(watchers);
        let registration = Registration::new(
            WATCHING.to_string(),
            "workspace/didChangeWatchedFiles".to_string(),
            Some(serde_json::to_value(options).expect("options serialize")),
        );
        let params = RegistrationParams::new(vec![registration]);
        let method = "client/registerCapability".to_string();
        let request = Request::new(RequestId::from(WATCHING.to_string()), method, params);
        let _ = self.sender.send(request.into());
    }

    /// Takes what changed on disk. A file that says what a package is, or a
    /// source created or deleted, finds every design again: what a walk finds
    /// changes, and a new manifest can move a file to another package. A
    /// source changed loads again each design that read it, unless it is
    /// open, when its buffer is what is read.
    fn on_disk(&mut self, params: DidChangeWatchedFilesParams) {
        let among = |part: Option<&OsStr>, names: &[String]| {
            let part = part.and_then(|it| it.to_str());
            part.is_some_and(|it| names.iter().any(|name| name == it))
        };
        let mut found = false;
        for change in params.changes {
            let Some(path) = buffers::path(&change.uri) else {
                continue;
            };
            let named = among(path.file_name(), &self.watched.names);
            let source = among(path.extension(), &self.watched.extensions);
            if named || (source && change.kind != FileChangeType::Changed) {
                found = true;
            } else if source && self.buffers.get(&path).is_none() {
                self.touch(&path, None);
            }
        }
        if found {
            self.rediscover();
        }
    }

    /// Finds every design again: each open buffer's, which may be another
    /// than before, and each one's package.
    fn rediscover(&mut self) {
        let host = self.host.clone();
        for (path, root) in &mut self.roots {
            *root = host.design(path);
            self.slots.entry(root.clone()).or_insert_with(Slot::new);
        }
        self.drop_unused();
        let now = Instant::now();
        for slot in self.slots.values_mut() {
            slot.rediscover();
            slot.stale(now);
        }
    }

    /// Runs `handle` with a notification's params, or tells the client
    /// they do not read as `method`'s.
    fn with<P: DeserializeOwned>(
        &mut self,
        method: &str,
        params: Value,
        handle: impl FnOnce(&mut Server, P),
    ) {
        match serde_json::from_value(params) {
            Ok(params) => handle(self, params),
            Err(err) => self.log(MessageType::Error, format!("`{method}`: {err}")),
        }
    }

    /// Answers a request on the spot with what `handle` makes of its
    /// params, or tells the client they do not read as `method`'s.
    fn reply<P: DeserializeOwned, R: Serialize>(
        &mut self,
        id: RequestId,
        method: &str,
        params: Value,
        handle: impl FnOnce(&mut Server, P) -> Result<R, ResponseError>,
    ) {
        let result = match serde_json::from_value(params) {
            Ok(params) => handle(self, params),
            Err(err) => Err(error(
                ErrorCode::InvalidParams,
                format!("`{method}`: {err}"),
            )),
        };
        self.answer(id, result);
    }

    /// Hands `work` to the pool with the request's params, answering it
    /// when it is done. `work` takes what it reads by value; whether the
    /// client has cancelled the request it asks of the [`Cancel`].
    pub fn spawn<P, R>(
        &mut self,
        id: RequestId,
        method: &str,
        params: Value,
        work: impl FnOnce(P, &Cancel) -> Result<R, ResponseError> + Send + 'static,
    ) where
        P: DeserializeOwned + Send + 'static,
        R: Serialize,
    {
        let params = match serde_json::from_value::<P>(params) {
            Ok(params) => params,
            Err(err) => {
                let said = format!("`{method}`: {err}");
                return self.answer::<()>(id, Err(error(ErrorCode::InvalidParams, said)));
            }
        };
        (self.tasks).spawn(id, method.to_string(), move |cancel| {
            let result = work(params, cancel)?;
            Ok(serde_json::to_value(result).expect("a result serializes"))
        });
    }

    /// Answers request `id` with `result`.
    pub fn answer<R: Serialize>(&self, id: RequestId, result: Result<R, ResponseError>) {
        let response_result =
            result.map(|it| serde_json::to_value(it).expect("a result serializes"));
        let response = Response {
            id,
            response_result,
        };
        let _ = self.sender.send(response.into());
    }

    /// Answers request `id` with what its handler made, or with an internal
    /// error if it panicked, telling the client the panic too.
    fn answer_outcome(&self, id: RequestId, method: &str, outcome: Outcome) {
        let result = outcome.unwrap_or_else(|payload| {
            let said = format!("`{method}` panicked: {}", panic_message(&*payload));
            self.log(MessageType::Error, said.clone());
            Err(error(ErrorCode::InternalError, said))
        });
        self.answer(id, result);
    }

    /// Tells the client `message`, which it shows in its log.
    pub fn log(&self, kind: MessageType, message: String) {
        let _ = self.sender.send(log(kind, message));
    }

    /// Tells the client `message`, which it shows the user.
    pub fn show(&self, kind: MessageType, message: String) {
        let params = ShowMessageParams { kind, message };
        let message = Notification::new("window/showMessage".to_string(), params);
        let _ = self.sender.send(message.into());
    }
}

/// A `window/logMessage` notification of `message`.
pub fn log(kind: MessageType, message: String) -> Message {
    let params = LogMessageParams { kind, message };
    Notification::new("window/logMessage".to_string(), params).into()
}

/// An error response's error.
pub fn error(code: ErrorCode, message: impl Into<String>) -> ResponseError {
    ResponseError {
        code: code as i32,
        message: message.into(),
        data: None,
    }
}
