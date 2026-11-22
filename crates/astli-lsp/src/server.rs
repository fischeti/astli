//! What the main loop does with each message: the state it owns, and a
//! handler for each request and notification it knows.

use std::ops::ControlFlow;
use std::panic::{AssertUnwindSafe, catch_unwind};

use crossbeam_channel::Sender;
use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response, ResponseError};
use lsp_types::{
    CancelParams, ClientCapabilities, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, Id, LogMessageParams, MessageType, ShowMessageParams,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::Ended;
use crate::buffers::{self, Buffers};
use crate::formatting::FmtOptions;
use crate::position::Encoding;
use crate::tasks::{Cancel, Done, Outcome, Tasks, panic_message};
use crate::trees::Trees;

/// The server's state, owned by the main loop.
pub struct Server {
    sender: Sender<Message>,
    pub encoding: Encoding,
    pub buffers: Buffers,
    pub trees: Trees,
    pub tasks: Tasks,
    /// The host's answer to what layout a file takes.
    pub fmt_options: FmtOptions,
    /// What the client said it can take in `initialize`.
    pub client: ClientCapabilities,
    /// Whether the client has sent `shutdown`, after which every request is
    /// refused and `exit` ends the server cleanly.
    shut_down: bool,
}

impl Server {
    pub fn new(sender: Sender<Message>, encoding: Encoding, fmt_options: FmtOptions) -> Server {
        Server {
            sender,
            encoding,
            buffers: Buffers::default(),
            trees: Trees::default(),
            tasks: Tasks::new(),
            fmt_options,
            client: ClientCapabilities::default(),
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
            // The server sends no request yet, so a response answers none.
            Message::Response(_) => {}
        }
        ControlFlow::Continue(())
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
            #[cfg(test)]
            method if method.starts_with("test/") => self.test_request(id, method, params),
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
            #[cfg(test)]
            "test/panic" => panic!("a notification asked to panic"),
            _ => {}
        }
    }

    fn open(&mut self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        if let Some(path) = buffers::path(&document.uri) {
            self.buffers.open(&path, document.text, document.version);
        }
    }

    fn change(&mut self, params: DidChangeTextDocumentParams) {
        let document = params.text_document;
        let uri = document.text_document_identifier.uri;
        let Some(path) = buffers::path(&uri) else {
            return;
        };
        let changes = params.content_changes;
        if !(self.buffers).change(&path, document.version, changes, self.encoding) {
            self.log(
                MessageType::Warning,
                format!("{uri} changed, and is not open"),
            );
        }
    }

    fn close(&mut self, params: DidCloseTextDocumentParams) {
        if let Some(path) = buffers::path(&params.text_document.uri) {
            self.buffers.close(&path);
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
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the requests over the design call it")
    )]
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

/// Requests only the tests send, to reach what no request of the protocol
/// does yet: a buffer as the server holds it, a panic, and work on the pool
/// that waits to be cancelled.
#[cfg(test)]
impl Server {
    fn test_request(&mut self, id: RequestId, method: &str, params: Value) {
        match method {
            "test/buffer" => {
                let uri: lsp_types::Uri = serde_json::from_value(params).expect("a URI");
                let path = buffers::path(&uri);
                let buffer = path.as_deref().and_then(|it| self.buffers.get(it));
                let buffer = buffer.map(|it| (it.version, it.text.clone()));
                self.answer(id, Ok(buffer));
            }
            "test/panic" => panic!("a request asked to panic"),
            "test/panicOnPool" => self.spawn(
                id,
                method,
                params,
                |(): (), _| -> Result<(), ResponseError> {
                    panic!("work on the pool asked to panic")
                },
            ),
            // Waits for its cancellation, or answers after ten seconds,
            // which fails the test that waited.
            "test/wait" => self.spawn(id, method, params, |(): (), cancel| {
                let start = std::time::Instant::now();
                while start.elapsed().as_secs() < 10 {
                    cancel.check()?;
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Ok("not cancelled")
            }),
            _ => self.answer::<()>(id, Err(error(ErrorCode::MethodNotFound, method))),
        }
    }
}
