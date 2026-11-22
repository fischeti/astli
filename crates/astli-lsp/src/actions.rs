//! `textDocument/codeAction`: the fixes of the findings published for an
//! open file, one finding's as a quick fix, and `source.fixAll.astli`,
//! which fixes the whole file as `lint --fix` does, for an editor to run on
//! save.
//!
//! An action is made from what was last published for the file, and only
//! while that was published for the buffer's version, since a fix is bytes
//! of the text its finding was found in. A quick fix is offered for a
//! diagnostic the client names in the request's context, matched against
//! what was published, so that one the client holds from an older version
//! is offered nothing. Where the client takes versioned edits, each names
//! the version too, and a client whose buffer moved on refuses it.
//!
//! Fix all lints the buffer again at the levels it was published with,
//! pass after pass, so that a fix overlapping another is applied too: its
//! first pass finds what was published, in the same text at the same
//! levels.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use astli_lint::Config;
use astli_parse::SyntaxTree;
use astli_text::{Edit, Lines, apply};
use lsp_server::{RequestId, ResponseError};
use lsp_types::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionParams, Diagnostic,
    OptionalVersionedTextDocumentIdentifier, TextDocumentEdit, TextDocumentIdentifier, TextEdit,
    Uri, WorkspaceEdit,
};
use serde::Deserialize;
use serde_json::Value;

use crate::buffers;
use crate::formatting;
use crate::position::Encoding;
use crate::server::Server;
use crate::tasks::Cancel;

/// The kind of the action that applies every fix in a file: `source.fixAll`
/// under the server's name, as the protocol asks of a server's own kinds.
pub const FIX_ALL: &str = "source.fixAll.astli";

/// An open file's findings that have a fix, as they were published for its
/// buffer's version, and the text and levels they were found at.
struct Fixable {
    path: PathBuf,
    uri: Uri,
    version: i32,
    text: Arc<str>,
    encoding: Encoding,
    /// Each diagnostic published with a fix, with it, in the order of the
    /// file.
    found: Vec<(Diagnostic, Vec<Edit>)>,
    /// The levels the file's own findings were found at, which fix all
    /// lints it at again; `None` if its design has not loaded.
    levels: Option<Arc<Config>>,
    /// Whether the client takes an edit that names the version it is for.
    versioned: bool,
}

impl Server {
    /// Hands the request to the pool, with the file's fixes as they were
    /// last published.
    pub fn code_actions(&mut self, id: RequestId, params: Value) {
        let fixable = self.fixable(&params);
        let method = "textDocument/codeAction";
        self.spawn(
            id,
            method,
            params,
            move |params: CodeActionParams, cancel| {
                let Some(fixable) = fixable else {
                    return Ok(Vec::new());
                };
                fixable.actions(&params.context, cancel)
            },
        );
    }

    /// The fixes of the file `params` names, unless it is not open, or what
    /// was last published for it was not for the buffer's version.
    fn fixable(&self, params: &Value) -> Option<Fixable> {
        let params = CodeActionParams::deserialize(params).ok()?;
        let path = buffers::path(&params.text_document.uri)?;
        let buffer = self.buffers.get(&path)?;
        let (_, version, told) = self.published.get(&path)?;
        if *version != Some(buffer.version) {
            return None;
        }
        let found = (told.iter())
            .filter(|it| !it.fix.is_empty())
            .map(|it| (it.diagnostic.clone(), it.fix.clone()))
            .collect();
        let versioned = (self.client.workspace.as_ref())
            .and_then(|it| it.workspace_edit.as_ref())
            .and_then(|it| it.document_changes)
            .unwrap_or(false);
        // The snapshot whose levels the published findings were found at:
        // the buffer's findings are found again whenever it is swapped.
        let levels = (self.snapshot(&path)).and_then(|it| it.levels.get(&path).cloned());
        Some(Fixable {
            path,
            uri: buffer.uri.clone(),
            version: buffer.version,
            text: buffer.text.clone(),
            encoding: self.encoding,
            found,
            levels,
            versioned,
        })
    }
}

impl Fixable {
    /// The actions `context` asks for, of the kinds its `only` names: a
    /// quick fix for each diagnostic it names that was published with a
    /// fix, and the whole file fixed, if one was.
    fn actions(
        &self,
        context: &CodeActionContext,
        cancel: &Cancel,
    ) -> Result<Vec<CodeAction>, ResponseError> {
        // A kind is wanted where `only` names it or a kind it is under.
        let wanted = |kind: &str| match &context.only {
            None => true,
            Some(only) => (only.iter()).any(|it| {
                let it = it.to_string();
                kind == it || kind.starts_with(&format!("{it}."))
            }),
        };
        let lines = Lines::new(&self.text);
        let mut actions = Vec::new();
        if wanted("quickfix") {
            for (diagnostic, fix) in &self.found {
                cancel.check()?;
                let asked = (context.diagnostics.iter()).any(|it| {
                    (it.range, &it.code, &it.message)
                        == (diagnostic.range, &diagnostic.code, &diagnostic.message)
                });
                if !asked || !self.parses_with(fix) {
                    continue;
                }
                let edits = (fix.iter())
                    .map(|edit| TextEdit {
                        range: (self.encoding).range(
                            &self.text,
                            &lines,
                            edit.at.start..edit.at.end,
                        ),
                        new_text: edit.text.clone(),
                    })
                    .collect();
                let code = match &diagnostic.code {
                    Some(lsp_types::Code::String(code)) => code.clone(),
                    _ => String::new(),
                };
                actions.push(CodeAction {
                    title: format!("Fix `{code}`"),
                    kind: Some(CodeActionKind::QuickFix),
                    diagnostics: Some(vec![diagnostic.clone()]),
                    is_preferred: Some(true),
                    edit: Some(self.edit(edits)),
                    ..CodeAction::default()
                });
            }
        }
        if let Some(levels) = &self.levels
            && wanted(FIX_ALL)
            && !self.found.is_empty()
        {
            cancel.check()?;
            let tree = SyntaxTree::parse(&self.path, self.text.to_string());
            if let Some((fixed, _)) = astli_lint::fix(&tree, levels) {
                let edits = formatting::edits(&self.text, &lines, fixed.source(), self.encoding);
                actions.push(CodeAction {
                    title: "Fix all lint findings".to_string(),
                    kind: Some(CodeActionKind::new(FIX_ALL)),
                    edit: Some(self.edit(edits)),
                    ..CodeAction::default()
                });
            }
        }
        Ok(actions)
    }

    /// Whether the text with `fix` applied parses whole, as `lint --fix`
    /// drops a pass that would not.
    fn parses_with(&self, fix: &[Edit]) -> bool {
        let (text, _) = apply(&self.text, &[fix]);
        SyntaxTree::parse(&self.path, text).diagnostics().is_empty()
    }

    /// `edits` of the file as the client takes them: naming the version
    /// they are for where it can, so that it refuses them once its buffer
    /// has moved on.
    fn edit(&self, edits: Vec<TextEdit>) -> WorkspaceEdit {
        if !self.versioned {
            let changes = HashMap::from([(self.uri.clone(), edits)]);
            return WorkspaceEdit {
                changes: Some(changes),
                ..WorkspaceEdit::default()
            };
        }
        let document = OptionalVersionedTextDocumentIdentifier::new(
            Some(self.version),
            TextDocumentIdentifier::new(self.uri.clone()),
        );
        let edits = edits.into_iter().map(Into::into).collect();
        WorkspaceEdit {
            document_changes: Some(vec![TextDocumentEdit::new(document, edits).into()]),
            ..WorkspaceEdit::default()
        }
    }
}
