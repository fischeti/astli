//! A client, and the host the server it starts asks: one that reads a few
//! files of a design back through the reader it is given and tells the test
//! what it read, lowers them into a design if the test asks, and lays a
//! file out as the test says.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use astli_fmt::Options;
use astli_index::{Index, Summary};
use astli_lint::{Config, Level};
use astli_lsp::{Analysed, At, Ended, Host, Memo, Place, Problem, Snapshot, Unit, Watched};
use astli_parse::parse_expanded;
use astli_preproc::{Build, Session};
use astli_sema::{Design, Elaboration, FileId, lower};
use astli_text::{Origins, Reader, Severity, clean};
use crossbeam_channel::{Receiver, Sender};
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response, ResponseError};
use serde_json::{Value, json};

/// How long a test waits for what the server sends before it fails.
const PATIENCE: Duration = Duration::from_secs(10);

/// What a load read, as the host tells it.
#[derive(Debug)]
pub struct Load {
    pub root: PathBuf,
    pub discover: bool,
    /// Each file of the design that exists, by name, and its text.
    pub texts: Vec<(String, String)>,
}

impl Load {
    /// The text it read of `name`, if the file exists.
    pub fn text(&self, name: &str) -> Option<&str> {
        let found = self.texts.iter().find(|(it, _)| it == name);
        found.map(|(_, text)| text.as_str())
    }
}

/// The layout a test's host answers for the file at a path, as
/// [`Host::fmt_options`] does.
pub type Layout = Box<dyn Fn(&Path) -> Result<Option<Options>, String> + Send + Sync>;

/// A host whose design is a file's directory, and which reads the files
/// `names` names in it, and which lays each file out as `layout` says.
///
/// It panics in [`Host::design`] for a path that holds `panic`, and in
/// [`Host::load`] when a text it read holds `panic`. When one holds `wait`,
/// the load waits until an edit overtakes it. When one holds `hold`, the
/// load tells the test it has read the file and reads on once the test
/// releases it. When one holds `scoped`, the load waits for jobs it spawns
/// on the pool, as the driver's does.
///
/// With `designs`, each `.sv` it read is a unit of one design, lowered as
/// [`designed`] lowers it; without, its snapshots hold no design.
struct Files {
    names: Vec<String>,
    designs: bool,
    layout: Layout,
    loads: Sender<Load>,
    held: Sender<()>,
    release: Receiver<()>,
    forgot: Sender<PathBuf>,
}

impl Host for Files {
    fn version(&self) -> String {
        "VERSION".to_string()
    }

    fn fmt_options(&self, path: &Path) -> Result<Option<Options>, String> {
        (self.layout)(path)
    }

    fn design(&self, path: &Path) -> PathBuf {
        if path.to_string_lossy().contains("panic") {
            panic!("the host was asked to panic");
        }
        path.parent().expect("a file in a directory").to_path_buf()
    }

    fn load(
        &self,
        root: &Path,
        reader: &dyn Reader,
        discover: bool,
        overtaken: &(dyn Fn() -> bool + Sync),
    ) -> Option<Snapshot> {
        let texts: Vec<(String, String)> = (self.names.iter())
            .filter_map(|name| {
                let path = clean(&root.join(name));
                let text = reader.exists(&path).then(|| reader.read(&path).unwrap())?;
                if text.contains("hold") {
                    let _ = self.held.send(());
                    (self.release.recv_timeout(PATIENCE)).expect("the test releases the load");
                }
                Some((name.clone(), text))
            })
            .collect();
        let holds = |what: &str| texts.iter().any(|(_, text)| text.contains(what));
        let (panics, waits) = (holds("panic"), holds("wait"));
        if holds("scoped") {
            scoped();
        }
        let snapshot = match self.designs {
            true => designed(root, &texts, reader),
            false => Snapshot {
                root: root.to_path_buf(),
                designs: Vec::new(),
                problems: Vec::new(),
                lowered: 0,
                lacks_dependencies: false,
                levels: HashMap::new(),
            },
        };
        let load = Load {
            root: root.to_path_buf(),
            discover,
            texts,
        };
        let _ = self.loads.send(load);
        if panics {
            panic!("a load was asked to panic");
        }
        if waits {
            let start = Instant::now();
            while !overtaken() {
                assert!(start.elapsed() < PATIENCE, "no edit overtook the load");
                std::thread::sleep(Duration::from_millis(1));
            }
            return None;
        }
        Some(snapshot)
    }

    fn forget(&self, root: &Path) {
        let _ = self.forgot.send(root.to_path_buf());
    }

    fn watched(&self) -> Watched {
        Watched {
            names: vec!["astli.toml".to_string()],
            extensions: vec!["sv".to_string(), "svh".to_string()],
        }
    }
}

/// `texts`, the files a load of the design at `root` read, by name, as one
/// design: each `.sv` a unit, which searches `root` for what it includes
/// through `reader`, every unit reported, and every file read linted at
/// the default levels.
///
/// A text holding `unfetched` makes the design one read without its
/// dependencies, which a problem at its manifest says; one holding
/// `placeless` adds a problem in no file; one holding `twice` adds a
/// problem at the line of its file that holds it, as a name declared twice
/// is placed. A unit whose text holds `unsound` has origins that hold none
/// of its text, so that placing what is found in it panics. A file whose
/// text holds `restriction` is linted with that group's rules warned too.
/// A unit whose text holds `BUILT` is expanded with `BUILT` defined, as a
/// package's build defines a name.
fn designed(root: &Path, texts: &[(String, String)], reader: &dyn Reader) -> Snapshot {
    let (mut hirs, mut summaries, mut lowered) = (Vec::new(), Vec::new(), Vec::new());
    for (name, text) in texts.iter().filter(|(name, _)| name.ends_with(".sv")) {
        let path = clean(&root.join(name));
        let build = match text.contains("BUILT") {
            true => Build::new().include_dir(root).define("BUILT", ""),
            false => Build::new().include_dir(root),
        };
        let build = Arc::new(build);
        let mut session = Session::reading(reader).building((*build).clone());
        let file = session.add(&path, text.clone());
        let expanded = session.expand(file);
        let parsed = parse_expanded(&session, &expanded.tokens);
        let mut found = expanded.diagnostics;
        found.extend(parsed.diagnostics.iter().cloned());
        let recorded = (expanded.calls.into(), expanded.skipped.into(), build);
        hirs.push(Arc::new(lower(&parsed)));
        summaries.push(Summary::new(session.origins(), file, &parsed));
        let origins = match text.contains("unsound") {
            true => Origins::new(),
            false => session.into_origins(),
        };
        lowered.push((path, Arc::new(origins), found, recorded));
    }
    let elaboration = Elaboration::new(Arc::new(Design::new(hirs)));
    let files: Vec<FileId> = elaboration.design().files().map(|(file, _)| file).collect();
    let units: Vec<Unit> = (lowered.into_iter().zip(files))
        .map(
            |((path, origins, parsed, (calls, skipped, build)), file)| Unit {
                path,
                file,
                origins,
                parsed,
                checked: true,
                linted: true,
                calls,
                skipped,
                build,
                memo: Memo::default(),
            },
        )
        .collect();
    let holds = |what: &str| texts.iter().any(|(_, text)| text.contains(what));
    let mut problems = Vec::new();
    if holds("unfetched") {
        problems.push(Problem {
            severity: Severity::Error,
            message: "`ip` is not fetched".to_string(),
            notes: vec!["the package is read without its dependencies".to_string()],
            help: vec!["`astli fetch` fetches it".to_string()],
            at: Some(At {
                code: "unfetched".to_string(),
                path: root.join("astli.toml"),
                place: Place::Line(1),
                label: Some("required here".to_string()),
            }),
        });
    }
    for (name, text) in texts {
        let Some(line) = text.lines().position(|it| it.contains("twice")) else {
            continue;
        };
        problems.push(Problem {
            severity: Severity::Error,
            message: "a name declared twice".to_string(),
            notes: Vec::new(),
            help: Vec::new(),
            at: Some(At {
                code: "declared-twice".to_string(),
                path: clean(&root.join(name)),
                place: Place::Line(line + 1),
                label: None,
            }),
        });
    }
    if holds("placeless") {
        problems.push(Problem {
            severity: Severity::Warning,
            message: "a problem in no file".to_string(),
            notes: Vec::new(),
            help: Vec::new(),
            at: None,
        });
    }
    let levels = Arc::new(Config::default());
    let mut restricted = Config::default();
    restricted.set("restriction", Level::Warn).expect("a group");
    let restricted = Arc::new(restricted);
    Snapshot {
        root: root.to_path_buf(),
        lowered: units.len(),
        designs: vec![Analysed::new(
            elaboration,
            Index::new(summaries),
            units,
            Vec::new(),
        )],
        problems,
        lacks_dependencies: holds("unfetched"),
        levels: (texts.iter())
            .map(|(name, text)| {
                let levels = match text.contains("restriction") {
                    true => restricted.clone(),
                    false => levels.clone(),
                };
                (clean(&root.join(name)), levels)
            })
            .collect(),
    }
}

/// Spawns two jobs on the pool and waits on a channel for both, as the
/// driver's load waits for the files it spreads over the pool.
fn scoped() {
    let (sender, done) = std::sync::mpsc::channel();
    rayon::in_place_scope(|scope| {
        for _ in 0..2 {
            let sender = sender.clone();
            scope.spawn(move |_| sender.send(()).expect("the load waits"));
        }
        for _ in 0..2 {
            done.recv().expect("each job sends");
        }
    });
}

/// What a request is answered with.
pub type Answer = Result<Value, ResponseError>;

/// A client, with the server it started running its loop on a thread.
pub struct Client {
    connection: Connection,
    server: JoinHandle<Ended>,
    loads: Receiver<Load>,
    held: Receiver<()>,
    release: Sender<()>,
    forgot: Receiver<PathBuf>,
    next: i32,
    /// What the server has told the client's log, in order, but what a
    /// test took with [`Client::logged`].
    pub logs: Vec<String>,
    /// What the server has shown the user, in order.
    pub shown: Vec<String>,
    /// Each request the server sent, by method and params, each answered
    /// with `null`.
    pub asked: Vec<(String, Value)>,
    /// What the server published, in order, but what a test took with
    /// [`Client::published`].
    pub published: Vec<Published>,
}

/// What a server published for a file.
#[derive(Debug, Clone)]
pub struct Published {
    pub uri: String,
    pub version: Option<i64>,
    pub diagnostics: Vec<Value>,
}

impl Published {
    /// The codes of its diagnostics, in order.
    pub fn codes(&self) -> Vec<&str> {
        (self.diagnostics.iter())
            .map(|it| it["code"].as_str().unwrap_or_default())
            .collect()
    }

    /// The diagnostic with `code`, which it has.
    pub fn with(&self, code: &str) -> &Value {
        let found = self.diagnostics.iter().find(|it| it["code"] == code);
        found.unwrap_or_else(|| panic!("no `{code}` in {:#?}", self.diagnostics))
    }
}

impl Client {
    /// Starts a server whose host reads `names` of a design, and sends
    /// `initialize` with `capabilities`. Returns the client and what the
    /// server answered.
    pub fn initialize(names: &[&str], capabilities: Value) -> (Client, Value) {
        let layout: Layout = Box::new(|_| Ok(Some(Options::default())));
        Client::initialize_with(names, false, layout, capabilities)
    }

    /// Starts a server whose host lowers the `.sv` files of `names` into a
    /// design, as [`designed`] does, and sends `initialize` with
    /// `capabilities`.
    pub fn designing(names: &[&str], capabilities: Value) -> Client {
        let layout: Layout = Box::new(|_| Ok(Some(Options::default())));
        Client::initialize_with(names, true, layout, capabilities).0
    }

    /// Starts a server whose host reads no file of a design and lays each
    /// file out as `layout` says, and completes the handshake in UTF-8 if
    /// `utf8`, or else in UTF-16.
    pub fn laid_out(layout: Layout, utf8: bool) -> Client {
        Client::initialize_with(&[], false, layout, offered(utf8)).0
    }

    fn initialize_with(
        names: &[&str],
        designs: bool,
        layout: Layout,
        capabilities: Value,
    ) -> (Client, Value) {
        let (client, server) = Connection::memory();
        let (sender, loads) = crossbeam_channel::unbounded();
        let (holding, held) = crossbeam_channel::unbounded();
        let (release, released) = crossbeam_channel::unbounded();
        let (forgetting, forgot) = crossbeam_channel::unbounded();
        let host = Files {
            names: names.iter().map(|it| it.to_string()).collect(),
            designs,
            layout,
            loads: sender,
            held: holding,
            release: released,
            forgot: forgetting,
        };
        let server = std::thread::spawn(move || astli_lsp::serve(&server, Arc::new(host)));
        let mut client = Client {
            connection: client,
            server,
            loads,
            held,
            release,
            forgot,
            next: 0,
            logs: Vec::new(),
            shown: Vec::new(),
            asked: Vec::new(),
            published: Vec::new(),
        };
        let params = json!({ "processId": null, "rootUri": null, "capabilities": capabilities });
        let result = client.request("initialize", params).expect("initialized");
        client.notify("initialized", json!({}));
        (client, result)
    }

    /// Starts a server whose host reads `names` of a design, and completes
    /// the handshake in UTF-8 if `utf8`, or else in UTF-16.
    pub fn start(names: &[&str], utf8: bool) -> Client {
        Client::initialize(names, offered(utf8)).0
    }

    /// Sends a request, returning its id without waiting for the answer.
    pub fn send(&mut self, method: &str, params: Value) -> RequestId {
        self.next += 1;
        let id = RequestId::from(self.next);
        let request = Request::new(id.clone(), method.to_string(), params);
        self.connection.sender.send(request.into()).expect("sent");
        id
    }

    /// Sends a request and waits for its answer.
    pub fn request(&mut self, method: &str, params: Value) -> Answer {
        let id = self.send(method, params);
        self.answer(&id)
    }

    /// Waits for the answer to request `id`, which is the next one the
    /// server sends.
    pub fn answer(&mut self, id: &RequestId) -> Answer {
        loop {
            if let Some(response) = self.receive() {
                assert_eq!(&response.id, id, "answers in the order asked");
                return response.response_result;
            }
        }
    }

    /// Takes the next message from the server: a response, handed back, or
    /// a log, a message shown, a publish or a request, kept.
    fn receive(&mut self) -> Option<Response> {
        let message = (self.connection.receiver)
            .recv_timeout(PATIENCE)
            .expect("a message from the server");
        match message {
            Message::Response(response) => return Some(response),
            Message::Notification(notification)
                if notification.method == "textDocument/publishDiagnostics" =>
            {
                let params = notification.params;
                self.published.push(Published {
                    uri: params["uri"].as_str().expect("a uri").to_string(),
                    version: params["version"].as_i64(),
                    diagnostics: params["diagnostics"].as_array().expect("a list").clone(),
                });
            }
            Message::Notification(notification) => {
                let message = notification.params["message"].as_str().expect("a message");
                let kept = match notification.method.as_str() {
                    "window/logMessage" => &mut self.logs,
                    "window/showMessage" => &mut self.shown,
                    method => panic!("the server sent `{method}`"),
                };
                kept.push(message.to_string());
            }
            Message::Request(request) => {
                let response = Response::new_ok(request.id, Value::Null);
                self.connection.sender.send(response.into()).expect("sent");
                self.asked.push((request.method, request.params));
            }
        }
        None
    }

    /// Waits for the server to log what `said` takes, and takes it.
    pub fn logged(&mut self, said: impl Fn(&str) -> bool) -> String {
        loop {
            if let Some(at) = self.logs.iter().position(|it| said(it)) {
                return self.logs.remove(at);
            }
            if let Some(response) = self.receive() {
                panic!("an answer to nothing asked: {response:?}");
            }
        }
    }

    /// Waits for the server to publish for `uri` what `said` takes, and
    /// takes it and every publish for `uri` before it.
    pub fn published(&mut self, uri: &str, said: impl Fn(&Published) -> bool) -> Published {
        loop {
            let found = (self.published.iter()).position(|it| it.uri == uri && said(it));
            if let Some(at) = found {
                let taken = self.published.remove(at);
                let mut index = 0;
                self.published.retain(|it| {
                    index += 1;
                    index > at || it.uri != uri
                });
                return taken;
            }
            if let Some(response) = self.receive() {
                panic!("an answer to nothing asked: {response:?}");
            }
        }
    }

    /// The last publish for `uri` the server sent before it answers a
    /// request sent now, taking every publish for `uri` so far.
    pub fn latest(&mut self, uri: &str) -> Option<Published> {
        let _ = self.request("astli/unknown", Value::Null);
        let last = (self.published.iter()).rposition(|it| it.uri == uri);
        let last = last.map(|at| self.published[at].clone());
        self.published.retain(|it| it.uri != uri);
        last
    }

    /// Waits for the next load the host starts.
    pub fn load(&self) -> Load {
        self.loads.recv_timeout(PATIENCE).expect("a load")
    }

    /// Waits for a load to read a file that holds `hold`.
    pub fn held(&self) {
        self.held.recv_timeout(PATIENCE).expect("a load held");
    }

    /// Lets a load held read on.
    pub fn release(&self) {
        self.release.send(()).expect("sent");
    }

    /// Waits for the server to tell the host to forget a design, and hands
    /// back its root.
    pub fn forgotten(&self) -> PathBuf {
        self.forgot
            .recv_timeout(PATIENCE)
            .expect("a design forgotten")
    }

    /// Whether no load starts for a while: a few times as long as the
    /// server waits for edits to settle.
    pub fn idle(&self) -> bool {
        self.loads.recv_timeout(Duration::from_millis(500)).is_err()
    }

    /// Waits for loads until one `done` takes, and hands it back.
    pub fn load_until(&self, done: impl Fn(&Load) -> bool) -> Load {
        loop {
            let load = self.load();
            if done(&load) {
                return load;
            }
        }
    }

    pub fn notify(&self, method: &str, params: Value) {
        let notification = Notification::new(method.to_string(), params);
        self.connection
            .sender
            .send(notification.into())
            .expect("sent");
    }

    /// Opens `uri` holding `text`.
    pub fn open(&self, uri: &str, text: &str) {
        let document =
            json!({ "uri": uri, "languageId": "systemverilog", "version": 1, "text": text });
        self.notify("textDocument/didOpen", json!({ "textDocument": document }));
    }

    /// Replaces the whole text of `uri` with `text`.
    pub fn replace(&self, uri: &str, text: &str) {
        let document = json!({ "uri": uri, "version": 2 });
        let params = json!({ "textDocument": document, "contentChanges": [{ "text": text }] });
        self.notify("textDocument/didChange", params);
    }

    /// Applies `changes`, a list of the protocol's change events, to `uri`,
    /// which they make `version`.
    pub fn change(&self, uri: &str, version: i32, changes: Value) {
        let document = json!({ "uri": uri, "version": version });
        let params = json!({ "textDocument": document, "contentChanges": changes });
        self.notify("textDocument/didChange", params);
    }

    pub fn close(&self, uri: &str) {
        let params = json!({ "textDocument": { "uri": uri } });
        self.notify("textDocument/didClose", params);
    }

    /// Sends `exit`, and returns how the server ended.
    pub fn exit(self) -> Ended {
        self.notify("exit", Value::Null);
        self.server.join().expect("the loop does not panic")
    }

    /// Shuts the server down and exits it, cleanly.
    pub fn finish(mut self) {
        assert_eq!(self.request("shutdown", Value::Null).unwrap(), Value::Null);
        assert_eq!(self.exit(), Ended::Exited);
    }

    /// Drops the connection without `exit`, and returns how the server
    /// ended.
    pub fn hang_up(self) -> Ended {
        drop(self.connection);
        self.server.join().expect("the loop does not panic")
    }
}

/// The capabilities of a client that offers UTF-8 positions if `utf8`, and
/// only UTF-16 otherwise.
fn offered(utf8: bool) -> Value {
    let offered: &[&str] = if utf8 { &["utf-8"] } else { &[] };
    json!({ "general": { "positionEncodings": offered } })
}

/// The path of `name` in a directory that exists on every platform: a path
/// with no drive is no file on Windows.
pub fn path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
}

/// The URI of [`path`]`(name)`.
pub fn uri(name: &str) -> String {
    lsp_types::Uri::from_file_path(path(name))
        .expect("absolute")
        .to_string()
}

/// An error's code, which is what a client acts on.
pub fn code(result: Answer) -> i32 {
    result.expect_err("an error").code
}

/// Whether `said` is the log of a snapshot swapped in.
pub fn loaded(said: &str) -> bool {
    said.starts_with("loaded ")
}
