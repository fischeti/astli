//! `textDocument/publishDiagnostics`: what astli finds in each open file,
//! in two steps that read different things.
//!
//! What the buffer says on its own, its raw parse and the tree rules, is
//! found on the loop whenever its text or its levels change. What its
//! design says, each unit's expansion and expanded parse, `check` and the
//! design rules, is found on a load's thread once its snapshot is made,
//! and only for the units that hold an open file, since a design is loaded
//! again on every edit. A load's problems that have a place are published
//! at their file, open or not.
//!
//! What is published for a file is all of these together, a finding with
//! the same range, code and message once, and only when it differs from
//! what was published last.

use std::collections::{HashMap, HashSet};
use std::ops::Range as Bytes;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use astli_diag::resolve_all;
use astli_lint::{Config, Layer, Linter, RULES, Waivers, lint};
use astli_parse::SyntaxTree;
use astli_sema::{check, check_built};
use astli_text::{Diagnostic as Finding, Edit, LineCol, Lines, Origins, Reader, Severity, Span};
use lsp_server::Notification;
use lsp_types::{
    Code, CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location,
    MessageType, Position, PublishDiagnosticsParams, Range, Uri,
};
use rayon::prelude::*;

use crate::buffers::{self, key};
use crate::position::Encoding;
use crate::server::Server;
use crate::tasks::panic_message;
use crate::{Analysed, At, Place, Problem, Snapshot, Unit};

/// The diagnostics of each file, by its path as a buffer is held under.
pub type ByPath = HashMap<PathBuf, Vec<Told>>;

/// What was last published for each file: the URI it was published under,
/// the version of the buffer it was published for, if the file was open,
/// and the diagnostics, each with its fix, which code actions offer.
pub type Published = HashMap<PathBuf, (Uri, Option<i32>, Vec<Told>)>;

/// A diagnostic as the client is told it, with how far what it says
/// reaches in its own file: the end of its range, or of a related place in
/// the same file that ends later. A change keeps a finding of the design
/// only where all of that is before the first character it changed, since
/// a related place is a line of the message for a client that takes none,
/// and could not be dropped from it alone.
#[derive(Debug, Clone, PartialEq)]
pub struct Told {
    pub diagnostic: Diagnostic,
    pub reach: Position,
    /// The finding's fix, as edits of the bytes of the text it was found
    /// in, which is the file's it is told in; empty where it has none, or
    /// one that would edit anything but that file as written.
    pub fix: Vec<Edit>,
}

/// The page of the site that lists the lint rules, a table for each group
/// under a heading named after it.
const RULES_PAGE: &str = "https://astli.dev/reference/lint-rules/";

/// How the client takes a diagnostic.
#[derive(Debug, Clone, Copy)]
pub struct Shape {
    /// What its positions count.
    pub encoding: Encoding,
    /// Whether it takes related information, as it said in `initialize`.
    /// For one that does not, each related place is a line of the message.
    pub related: bool,
}

/// What a load found in the files open when it started, and its problems,
/// as the client is told them.
pub struct Found {
    /// What the design says of each file open, an empty list for one it
    /// says nothing of; nothing at all if the load read the package
    /// without its dependencies.
    pub design: ByPath,
    /// The problems that have a place, by their file.
    pub problems: ByPath,
    /// The problems that have none.
    pub placeless: Vec<Problem>,
    /// How many units were checked or linted, and how long it took.
    pub units: usize,
    pub took: Duration,
    /// What panicked while finding these, each a line for the log. What the
    /// design says is then nothing, and the problems, if placing them
    /// panicked, have no place.
    pub panicked: Vec<String>,
}

/// What `snapshot` finds in the files `open`, each path as a buffer is held
/// under, and its problems, placed in their files as `reader` holds them;
/// `None` once `overtaken` says an edit made the load stale.
///
/// A package read without its dependencies declares none of what they
/// declare, so every use of it would be reported unknown: then the design
/// says nothing, and the problems say why.
///
/// A panic finding what the design says, or placing the problems, is
/// caught apart from the load, which made its snapshot, and told in
/// [`Found::panicked`]: losing the snapshot would lose what every request
/// reads, and every load after would panic the same way while the file is
/// open.
pub fn found(
    snapshot: &Snapshot,
    open: &[PathBuf],
    reader: &dyn Reader,
    shape: Shape,
    overtaken: &(dyn Fn() -> bool + Sync),
) -> Option<Found> {
    let begun = Instant::now();
    let at = snapshot.root.display();
    let mut panicked = Vec::new();
    let open: HashSet<&Path> = open.iter().map(PathBuf::as_path).collect();
    let mut design: ByPath = (open.iter())
        .map(|path| (path.to_path_buf(), Vec::new()))
        .collect();
    let mut units = 0;
    if !snapshot.lacks_dependencies {
        let of_designs = catch_unwind(AssertUnwindSafe(|| {
            (snapshot.designs.iter())
                .map(|analysed| of_design(snapshot, analysed, &open, shape, overtaken))
                .collect::<Option<Vec<_>>>()
        }));
        match of_designs {
            Ok(of_designs) => {
                for found in of_designs? {
                    units += found.len();
                    for (path, told) in found.into_iter().flatten() {
                        design.entry(path).or_default().push(told);
                    }
                }
            }
            Err(payload) => panicked.push(format!(
                "finding what the design at {at} says panicked: {}",
                panic_message(&*payload)
            )),
        }
    }
    let placeless = (snapshot.problems.iter())
        .filter(|it| it.at.is_none())
        .cloned()
        .collect();
    let problems = catch_unwind(AssertUnwindSafe(|| {
        let mut problems = ByPath::new();
        for problem in &snapshot.problems {
            if let Some(at) = &problem.at {
                let (path, told) = placed(problem, at, reader, shape);
                problems.entry(path).or_default().push(told);
            }
        }
        problems
    }));
    let mut problems = problems.unwrap_or_else(|payload| {
        let said = panic_message(&*payload);
        panicked.push(format!("placing the problems of {at} panicked: {said}"));
        ByPath::new()
    });
    for found in design.values_mut().chain(problems.values_mut()) {
        dedup(found);
    }
    Some(Found {
        design,
        problems,
        placeless,
        units,
        took: begun.elapsed(),
        panicked,
    })
}

/// What each unit of `analysed` that `check` or the design rules report,
/// and that holds a file of `open`, is found to have there, in the order
/// of the units; `None` once `overtaken`.
///
/// What is built from the tops, and the linter, are made once for the
/// design, and only if a unit needs them, as `check` and `lint` make them.
fn of_design(
    snapshot: &Snapshot,
    analysed: &Analysed,
    open: &HashSet<&Path>,
    shape: Shape,
    overtaken: &(dyn Fn() -> bool + Sync),
) -> Option<Vec<Vec<(PathBuf, Told)>>> {
    let needed: Vec<&Unit> = (analysed.units.iter())
        .filter(|unit| (unit.checked || unit.linted) && holds(unit, open))
        .collect();
    if needed.is_empty() {
        return Some(Vec::new());
    }
    let elaboration = &analysed.elaboration;
    // What is built, what it gets wrong and the linter each take a while
    // over a large design, so an overtaken load stops before each.
    if overtaken() {
        return None;
    }
    let built = analysed.built();
    // What only what the tops build gets wrong is found over the whole
    // design at once, and told in the file each finding is in.
    let mut of_built: HashMap<usize, Vec<Finding>> = HashMap::new();
    if let Some(built) = built
        && needed.iter().any(|unit| unit.checked)
    {
        if overtaken() {
            return None;
        }
        for (file, found) in check_built(elaboration, built) {
            of_built.entry(file.index()).or_default().push(found);
        }
    }
    let linted: Vec<&Config> = (needed.iter())
        .filter(|unit| unit.linted)
        .filter_map(|unit| snapshot.levels.get(&unit.path))
        .map(|levels| &**levels)
        .collect();
    if overtaken() {
        return None;
    }
    let linter = (!linted.is_empty()).then(|| {
        // The built rules read what the tops build only where a file's
        // levels leave one of them on, as `lint` reads it.
        let built_rules = linted.iter().any(|it| it.needs(Layer::Built));
        Linter::new(elaboration, built.filter(|_| built_rules))
    });
    (needed.par_iter())
        .map(|unit| {
            if overtaken() {
                return None;
            }
            let mut found = Vec::new();
            if unit.checked {
                found.extend(unit.parsed.iter().cloned());
                found.extend(check(elaboration, unit.file, built));
                found.extend(
                    of_built
                        .get(&unit.file.index())
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
            }
            let levels = snapshot.levels.get(&unit.path);
            if let (true, Some(linter), Some(levels)) = (unit.linted, &linter, levels) {
                found.extend(linter.lint(unit.file, &unit.origins, &waivers(unit), levels));
            }
            Some(converted(&unit.origins, &found, shape, |path| {
                open.contains(path)
            }))
        })
        .collect()
}

/// Whether `unit` holds a file of `open`: its own, or a header it includes.
fn holds(unit: &Unit, open: &HashSet<&Path>) -> bool {
    let origins = &unit.origins;
    // Few files are open and a unit reads many, so a name, which compares
    // cheaply, is asked first.
    let named = |path: &Path| open.iter().any(|it| it.file_name() == path.file_name());
    (origins.files())
        .filter_map(|file| origins.path(file))
        .chain([unit.path.as_path()])
        .any(|path| named(path) && open.contains(key(path).as_path()))
}

/// What the file of `unit` waives, as written: the design rules honour the
/// waivers of the unit's own file, as `lint` does, read off the text its
/// load read.
fn waivers(unit: &Unit) -> Waivers {
    let origins = &unit.origins;
    let own = (origins.files()).find(|&file| {
        origins.path(file) == Some(unit.path.as_path()) && origins.included_from(file).is_none()
    });
    let text = own.map_or_else(String::new, |file| origins.text(file).to_string());
    Waivers::read(&SyntaxTree::parse(&unit.path, text)).0
}

/// Each of `found`, whose spans `origins` resolves, as the client is told
/// it, with the path of the file it is in, if `keep` takes the path. One in
/// text no file holds is dropped.
///
/// A finding is placed at the outermost macro call when a macro wrote what
/// it is about, as the CLI places it, and what explains it are related
/// places: its labels, the text a macro call stands for, each call of the
/// macro chain, and each `` `include `` that reached its file.
///
/// Its fix is kept only where every edit is in that file, as written: an
/// edit in a header the file includes, or in the text a macro call placed,
/// is not the file's to make.
pub fn converted(
    origins: &Origins,
    found: &[Finding],
    shape: Shape,
    keep: impl Fn(&Path) -> bool,
) -> Vec<(PathBuf, Told)> {
    let mut converted = Vec::new();
    for resolved in resolve_all(origins, found) {
        let Some(path) = origins.path(resolved.at.src_id).map(key) else {
            continue;
        };
        if !keep(&path) {
            continue;
        }
        let mut related = Vec::new();
        let mut relate = |at: Span, message: String| {
            if let Some(path) = origins.path(at.src_id) {
                related.push(Related {
                    path: key(path),
                    range: range(origins, at, shape.encoding),
                    at: origins.line_col(at.src_id, at.start),
                    message,
                });
            }
        };
        for &(at, message) in &resolved.labels {
            relate(at, message.to_string());
        }
        if let Some(spelled) = resolved.spelled {
            relate(spelled, "this is the text it stands for".to_string());
        }
        let calls = (resolved.through.iter()).filter(|it| it.call != resolved.at);
        for through in calls {
            relate(
                through.call,
                format!("in this expansion of {}", through.name),
            );
        }
        for &site in &resolved.included_from {
            relate(site, "included from here".to_string());
        }
        let finding = resolved.diagnostic;
        let in_file = |edit: &Edit| {
            let source = edit.at.src_id;
            origins.placed_by(source).is_none()
                && origins.path(source).map(key) == Some(path.clone())
        };
        let fix = match finding.fix.iter().all(in_file) {
            true => finding.fix.clone(),
            false => Vec::new(),
        };
        let said = Said {
            severity: finding.severity,
            code: finding.code.as_str(),
            message: &finding.message,
            notes: &finding.notes,
            help: &finding.help,
        };
        let range = range(origins, resolved.at, shape.encoding);
        let reach = (related.iter())
            .filter(|it| it.path == path)
            .map(|it| it.range.end)
            .fold(range.end, Position::max);
        let diagnostic = said.told(range, related, shape);
        converted.push((
            path,
            Told {
                diagnostic,
                reach,
                fix,
            },
        ));
    }
    converted
}

/// `problem`, which is at `at`, as the client is told it, with the path of
/// its file: placed in the file as `reader` holds it now, or at its start if
/// it cannot be read.
fn placed(problem: &Problem, at: &At, reader: &dyn Reader, shape: Shape) -> (PathBuf, Told) {
    let (range, start) = match reader.read(&at.path) {
        Ok(text) => {
            let lines = Lines::new(&text);
            let bytes = bytes(&text, &lines, &at.place);
            let start = line_col(&text, &lines, bytes.start);
            (shape.encoding.range(&text, &lines, bytes), start)
        }
        Err(_) => (Range::default(), LineCol { line: 1, col: 1 }),
    };
    let path = key(&at.path);
    let related = (at.label.iter())
        .map(|label| Related {
            path: path.clone(),
            range,
            at: start,
            message: label.clone(),
        })
        .collect();
    let said = Said {
        severity: problem.severity,
        code: &at.code,
        message: &problem.message,
        notes: &problem.notes,
        help: &problem.help,
    };
    let diagnostic = said.told(range, related, shape);
    let reach = range.end;
    let fix = Vec::new();
    (
        path,
        Told {
            diagnostic,
            reach,
            fix,
        },
    )
}

/// The bytes of `text` that `place` covers, clamped to the text.
fn bytes(text: &str, lines: &Lines, place: &Place) -> Bytes<u32> {
    // The bytes of the line counted from one, without its line break; the
    // end of the text past the last line.
    let line = |number: usize| {
        let start = (number.checked_sub(1))
            .and_then(|line| lines.start(line))
            .map_or(text.len(), |start| start as usize);
        let rest = &text[start..];
        let rest = &rest[..rest.find('\n').unwrap_or(rest.len())];
        start..start + rest.strip_suffix('\r').unwrap_or(rest).len()
    };
    let bytes = match place {
        Place::Name(at, chars) => {
            let line = line(at.line as usize);
            let mut offsets = (text[line.clone()].char_indices())
                .map(|(at, _)| line.start + at)
                .chain([line.end]);
            let start = (offsets.nth((at.col as usize).saturating_sub(1))).unwrap_or(line.end);
            let end = match chars {
                0 => start,
                chars => offsets.nth(chars - 1).unwrap_or(line.end),
            };
            start..end
        }
        Place::Line(number) => line(*number),
        Place::Bytes(bytes) => {
            let start = text.floor_char_boundary(bytes.start);
            start..text.floor_char_boundary(bytes.end).max(start)
        }
    };
    bytes.start as u32..bytes.end as u32
}

/// Where byte `offset` of `text` is, counted from one, the column in
/// characters, as a message names a place.
fn line_col(text: &str, lines: &Lines, offset: u32) -> LineCol {
    let line = lines.line(offset);
    let start = lines.start(line).unwrap_or_default() as usize;
    LineCol {
        line: line as u32 + 1,
        col: text[start..offset as usize].chars().count() as u32 + 1,
    }
}

/// The range `at` covers, in `encoding`.
fn range(origins: &Origins, at: Span, encoding: Encoding) -> Range {
    let file = at.src_id;
    encoding.range(origins.text(file), origins.lines(file), at.start..at.end)
}

/// Keeps the first of each diagnostic with the same range, code and
/// message, in the order of the file.
pub fn dedup(found: &mut Vec<Told>) {
    found.sort_by_key(|it| it.diagnostic.range);
    let mut seen = HashSet::new();
    found.retain(|Told { diagnostic: it, .. }| {
        seen.insert((it.range, it.code.clone(), it.message.clone()))
    });
}

/// A place that explains a finding, and what it says there.
struct Related {
    path: PathBuf,
    range: Range,
    /// Where it starts, as a line of the message names it.
    at: LineCol,
    message: String,
}

/// What a finding or a problem says, before it is placed.
struct Said<'a> {
    severity: Severity,
    code: &'a str,
    message: &'a str,
    notes: &'a [String],
    help: &'a [String],
}

impl Said<'_> {
    /// The diagnostic at `range`, explained by `related`, as `shape` says
    /// the client takes it. Its notes and help are lines of the message, as
    /// they are lines under it in a terminal.
    fn told(self, range: Range, related: Vec<Related>, shape: Shape) -> Diagnostic {
        let mut message = self.message.to_string();
        for note in self.notes {
            message.push_str(&format!("\nnote: {note}"));
        }
        for help in self.help {
            message.push_str(&format!("\nhelp: {help}"));
        }
        let related = match shape.related {
            true => (related.into_iter())
                .filter_map(|it| {
                    let uri = Uri::from_file_path(&it.path).ok()?;
                    let location = Location::new(uri, it.range);
                    Some(DiagnosticRelatedInformation::new(location, it.message))
                })
                .collect(),
            false => {
                for it in related {
                    let path = it.path.display();
                    message.push_str(&format!("\n{path}:{}: {}", it.at, it.message));
                }
                Vec::new()
            }
        };
        Diagnostic {
            range,
            severity: Some(match self.severity {
                Severity::Error => DiagnosticSeverity::Error,
                Severity::Warning => DiagnosticSeverity::Warning,
                Severity::Note => DiagnosticSeverity::Information,
            }),
            code: Some(Code::String(self.code.to_string())),
            code_description: explained(self.code),
            source: Some("astli".to_string()),
            message: message.into(),
            related_information: (!related.is_empty()).then_some(related),
            ..Diagnostic::default()
        }
    }
}

/// Where `code` is explained, if it is a lint rule's: its group's table on
/// the rules page, which lists it.
fn explained(code: &str) -> Option<CodeDescription> {
    let rule = RULES.iter().find(|rule| rule.name == code)?;
    let href = Uri::parse(&format!("{RULES_PAGE}#{}", rule.group.name())).ok()?;
    Some(CodeDescription::new(href))
}

impl Server {
    /// How the client takes a diagnostic.
    pub fn shape(&self) -> Shape {
        let related = (self.client.text_document.as_ref())
            .and_then(|it| it.publish_diagnostics.as_ref())
            .and_then(|it| it.diagnostics_capabilities.related_information);
        Shape {
            encoding: self.encoding,
            related: related.unwrap_or(false),
        }
    }

    /// Finds again what the buffer at `path` says on its own, as `astli
    /// lint` in its directory reads the file: what its raw parse found
    /// malformed, and, at the levels its snapshot has for it, the waivers
    /// that do not read and the tree rules. Until its design is loaded, a
    /// file has no levels, and only its parse is told.
    ///
    /// What the raw parse keeps as written is not told: a macro standing
    /// for syntax is such a region, and the expanded parse says what a
    /// compiler would not read.
    ///
    /// A fix is kept only where the raw parse read the file whole, as `lint
    /// --fix` fixes only such a file: where the parser guessed, a token may
    /// not be what the rule took it for.
    pub fn find_own(&mut self, path: &Path) {
        let levels = self
            .snapshot(path)
            .and_then(|it| it.levels.get(path).cloned());
        let shape = self.shape();
        let Some((_, tree)) = self.tree(path) else {
            self.own.remove(path);
            return;
        };
        let mut found = tree.diagnostics().to_vec();
        if let Some(levels) = levels {
            let (waivers, invalid) = Waivers::read(tree);
            found.extend(invalid);
            found.extend(lint(tree, &waivers, &levels));
        }
        let whole = tree.diagnostics().is_empty();
        let own = converted(tree.origins(), &found, shape, |_| true);
        let own = (own.into_iter())
            .map(|(_, mut it)| {
                if !whole {
                    it.fix.clear();
                }
                it
            })
            .collect();
        self.own.insert(path.to_path_buf(), own);
    }

    /// Takes what a load of the design at `root` found, its snapshot just
    /// swapped in: each open file's design findings, the problems in place
    /// of the last load's, and each open file's own findings again, under
    /// the levels it has now. Publishes every file that changes, and tells
    /// the user of each problem without a place the last load did not
    /// have, since a design is loaded on every edit.
    pub fn found(&mut self, root: &Path, found: Found) {
        let Found {
            design,
            problems,
            placeless,
            ..
        } = found;
        self.design.extend(design);
        let Some(slot) = self.slots.get_mut(root) else {
            return;
        };
        let mut touched: HashSet<PathBuf> = slot.problems.keys().cloned().collect();
        touched.extend(problems.keys().cloned());
        slot.problems = problems;
        let new: Vec<Problem> = (placeless.iter())
            .filter(|it| !slot.placeless.contains(it))
            .cloned()
            .collect();
        slot.placeless = placeless;
        for problem in new {
            self.tell(&problem);
        }
        let open: Vec<PathBuf> = (self.roots.iter())
            .filter(|(_, it)| it.as_path() == root)
            .map(|(path, _)| path.clone())
            .collect();
        for path in &open {
            self.find_own(path);
        }
        touched.extend(open);
        for path in touched {
            self.publish(&path);
        }
    }

    /// Tells the user of a problem that has no place: an error or a warning
    /// shown, a note only in the log.
    fn tell(&self, problem: &Problem) {
        let mut said = problem.message.clone();
        for note in &problem.notes {
            said.push_str(&format!("\nnote: {note}"));
        }
        for help in &problem.help {
            said.push_str(&format!("\nhelp: {help}"));
        }
        match problem.severity {
            Severity::Error => self.show(MessageType::Error, said),
            Severity::Warning => self.show(MessageType::Warning, said),
            Severity::Note => self.log(MessageType::Info, said),
        }
    }

    /// Publishes what is found in the file at `path`: what its buffer says,
    /// what its design says, and every load's problems there, under the
    /// buffer's version if it is open; unless that is what was published
    /// last. A file whose findings are gone is published an empty list.
    ///
    /// An open file is published under the URI the client opened it with,
    /// which may spell its path otherwise than the path does, and a related
    /// place in an open file names it so too: a client matches a URI to its
    /// documents as written. A file not open is published under the URI it
    /// was last published under, so that what a close clears is what the
    /// client holds, or else under the URI its path makes.
    pub fn publish(&mut self, path: &Path) {
        let buffer = self.buffers.get(path);
        let version = buffer.map(|it| it.version);
        let last = self.published.get(path);
        let uri = match (buffer, last) {
            (Some(buffer), _) => buffer.uri.clone(),
            (None, Some((uri, ..))) => uri.clone(),
            (None, None) => match Uri::from_file_path(path) {
                Ok(uri) => uri,
                Err(()) => return,
            },
        };
        let problems = (self.slots.values()).filter_map(|slot| slot.problems.get(path));
        let mut all: Vec<Told> = (self.own.get(path).into_iter())
            .chain(self.design.get(path))
            .chain(problems)
            .flatten()
            .cloned()
            .collect();
        dedup(&mut all);
        let all = (all.into_iter()).map(|mut it| {
            it.diagnostic = self.as_opened(it.diagnostic);
            it
        });
        let told = (uri, version, all.collect::<Vec<_>>());
        let nothing = version.is_none() && told.2.is_empty();
        let same = match last {
            Some(last) => *last == told,
            None => nothing,
        };
        if same {
            return;
        }
        // Reopened under another spelling, the file is cleared under the
        // one the client holds its diagnostics under.
        if let Some((was, ..)) = last
            && *was != told.0
        {
            self.send_published(was.clone(), None, Vec::new());
        }
        let diagnostics = told.2.iter().map(|it| it.diagnostic.clone()).collect();
        self.send_published(told.0.clone(), told.1, diagnostics);
        match nothing {
            true => self.published.remove(path),
            false => self.published.insert(path.to_path_buf(), told),
        };
    }

    /// Sends `diagnostics` as those of `uri` at `version`.
    fn send_published(&self, uri: Uri, version: Option<i32>, diagnostics: Vec<Diagnostic>) {
        let params = PublishDiagnosticsParams::new(uri, version, diagnostics);
        let method = "textDocument/publishDiagnostics".to_string();
        let _ = self.sender.send(Notification::new(method, params).into());
    }

    /// `diagnostic` with each related place in an open file named by the
    /// URI the client opened the file with.
    fn as_opened(&self, mut diagnostic: Diagnostic) -> Diagnostic {
        for related in diagnostic.related_information.iter_mut().flatten() {
            let path = buffers::path(&related.location.uri);
            if let Some(buffer) = path.and_then(|it| self.buffers.get(&it)) {
                related.location.uri = buffer.uri.clone();
            }
        }
        diagnostic
    }
}
