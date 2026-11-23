//! `textDocument/codeAction`: the fixes of the findings published for an
//! open file, one finding's as a quick fix, a lint finding's waiver as
//! another, and `source.fixAll.astli`, which fixes the whole file as `lint
//! --fix` does, for an editor to run on save.
//!
//! An action is made from what was last published for the file, and only
//! while that was published for the buffer's version, since a fix is bytes
//! of the text its finding was found in. A quick fix is offered for a
//! diagnostic the client names in the request's context, matched against
//! what was published, so that one the client holds from an older version
//! is offered nothing. Where the client takes versioned edits, each names
//! the version too, and a client whose buffer moved on refuses it.
//!
//! A waiver is an `astli_allow` attribute on the innermost node of the
//! buffer's raw tree that takes one and holds the finding, as `lint` reads
//! it, or the rule added to the one already there. It is checked as `lint`
//! would read it: the text with it parses whole, and its waivers cover the
//! finding. A design unit's body is no place for one, since it would waive
//! the rule in all of the unit for one directive.
//!
//! Fix all lints the buffer again at the levels it was published with,
//! pass after pass, so that a fix overlapping another is applied too: its
//! first pass finds what was published, in the same text at the same
//! levels.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use astli_lint::{Config, RULES, Rule, Waivers};
use astli_parse::SyntaxTree;
use astli_syntax::SyntaxKind::{self, *};
use astli_syntax::ast::{self, AstNode};
use astli_syntax::{SyntaxNode, significant_child_tokens, significant_tokens};
use astli_text::{Edit, Lines, apply};
use lsp_server::{RequestId, ResponseError};
use lsp_types::{
    CodeAction, CodeActionContext, CodeActionKind, CodeActionParams, Diagnostic,
    OptionalVersionedTextDocumentIdentifier, TextDocumentEdit, TextDocumentIdentifier, TextEdit,
    Uri, WorkspaceEdit,
};
use rowan::{NodeOrToken, TextRange};
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

/// An open file's findings, as they were published for its buffer's
/// version, and the text and levels they were found at.
struct Fixable {
    path: PathBuf,
    uri: Uri,
    version: i32,
    text: Arc<str>,
    encoding: Encoding,
    /// Each diagnostic published, with its fix, empty where it has none, in
    /// the order of the file.
    found: Vec<(Diagnostic, Vec<Edit>)>,
    /// The levels the file's own findings were found at, which fix all
    /// lints it at again; `None` if its design has not loaded.
    levels: Option<Arc<Config>>,
    /// Whether the client takes an edit that names the version it is for.
    versioned: bool,
}

impl Server {
    /// Hands the request to the pool, with the file's findings as they were
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

    /// The findings of the file `params` names, unless it is not open, or
    /// what was last published for it was not for the buffer's version.
    fn fixable(&self, params: &Value) -> Option<Fixable> {
        let params = CodeActionParams::deserialize(params).ok()?;
        let path = buffers::path(&params.text_document.uri)?;
        let buffer = self.buffers.get(&path)?;
        let (_, version, told) = self.published.get(&path)?;
        if *version != Some(buffer.version) {
            return None;
        }
        let found = (told.iter())
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
    /// The actions `context` asks for, of the kinds its `only` names: for
    /// each diagnostic it names, its fix and, for a lint rule's, its waiver,
    /// and the whole file fixed, if a finding has a fix.
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
        // Parsed once, for the first waiver asked.
        let mut tree = None;
        if wanted("quickfix") {
            for (diagnostic, fix) in &self.found {
                cancel.check()?;
                let asked = (context.diagnostics.iter()).any(|it| {
                    (it.range, &it.code, &it.message)
                        == (diagnostic.range, &diagnostic.code, &diagnostic.message)
                });
                if !asked {
                    continue;
                }
                let code = match &diagnostic.code {
                    Some(lsp_types::Code::String(code)) => code.clone(),
                    _ => String::new(),
                };
                if !fix.is_empty() && self.parses_with(fix) {
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
                    actions.push(CodeAction {
                        title: format!("Fix `{code}`"),
                        kind: Some(CodeActionKind::QuickFix),
                        diagnostics: Some(vec![diagnostic.clone()]),
                        is_preferred: Some(true),
                        edit: Some(self.edit(edits)),
                        ..CodeAction::default()
                    });
                }
                // A rule's name is its findings' code, and no other finding
                // has one: not `check`'s, a parse error, nor an
                // `invalid-waiver`.
                let Some(rule) = RULES.iter().find(|it| it.name == code) else {
                    continue;
                };
                let tree = tree
                    .get_or_insert_with(|| SyntaxTree::parse(&self.path, self.text.to_string()));
                let at = (self.encoding).bytes(&self.text, &lines, diagnostic.range);
                let Some((offset, text)) = self.waiver(tree, rule, at) else {
                    continue;
                };
                let edit = TextEdit {
                    range: (self.encoding).range(&self.text, &lines, offset..offset),
                    new_text: text,
                };
                actions.push(CodeAction {
                    title: format!("Waive `{code}` here"),
                    kind: Some(CodeActionKind::QuickFix),
                    diagnostics: Some(vec![diagnostic.clone()]),
                    edit: Some(self.edit(vec![edit])),
                    ..CodeAction::default()
                });
            }
        }
        if let Some(levels) = &self.levels
            && wanted(FIX_ALL)
            && self.found.iter().any(|(_, fix)| !fix.is_empty())
        {
            cancel.check()?;
            let tree = tree.unwrap_or_else(|| SyntaxTree::parse(&self.path, self.text.to_string()));
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

    /// The text to insert, and where, that waives `rule` for its finding at
    /// `at`, bytes of `tree`, the buffer's: on the innermost node that takes
    /// the waiver and holds the finding. `None` where the buffer does not
    /// parse whole, the finding is in a directive, the node is a design unit
    /// and the finding in its body, or no node holds it.
    fn waiver(&self, tree: &SyntaxTree, rule: &Rule, at: Range<u32>) -> Option<(u32, String)> {
        if !tree.diagnostics().is_empty() {
            return None;
        }
        let range = TextRange::new(at.start.into(), at.end.into());
        if !tree.root().text_range().contains_range(range) {
            return None;
        }
        let inner = match tree.root().covering_element(range) {
            NodeOrToken::Node(node) => node,
            NodeOrToken::Token(token) => token.parent()?,
        };
        // No attribute reaches a directive, and the node around one is a
        // body, a block or a region, whose waiver would cover far more.
        if inner.ancestors().any(|it| it.kind() == DIRECTIVE) {
            return None;
        }
        // A node `takes` may not, where the parser reads its attributes as
        // another's, or none: the check finds that, and the next node out
        // is the innermost that does.
        for node in inner.ancestors().filter(|it| takes(it.kind())) {
            // A lone `;`, as one a macro call leaves behind, would read
            // `(* ... *) ;`: its finding is the `;` itself, which its fix
            // deletes.
            if significant_tokens(&node).all(|it| it.kind() == SEMICOLON) {
                return None;
            }
            if is_unit(node.kind()) && past_header(&node, range) {
                return None;
            }
            let (offset, text) = waiving(&node, rule)?;
            let mut waived = self.text.to_string();
            waived.insert_str(offset as usize, &text);
            let moved = |it: u32| match it >= offset {
                true => it + text.len() as u32,
                false => it,
            };
            let waived = SyntaxTree::parse(&self.path, waived);
            let (waivers, _) = Waivers::read(&waived);
            if waived.diagnostics().is_empty()
                && waivers.covers(rule, moved(at.start)..moved(at.end))
            {
                return Some((offset, text));
            }
        }
        None
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

/// The attribute that waives, as `astli-lint` reads it.
const ALLOW: &str = "astli_allow";

/// Whether a node of `kind` takes an attribute that waives: an item, a
/// statement, a port, a member, each of which the parser starts by reading
/// attributes. A directive, a macro call and a run no rule claims are none,
/// and nor is an expression, whose attributes are an operator's.
fn takes(kind: SyntaxKind) -> bool {
    let item = ast::Item::can_cast(kind) && !ast::Preproc::can_cast(kind) && kind != VERBATIM;
    item || matches!(kind, PORT | STRUCT_MEMBER | MODPORT_METHOD)
}

/// Whether a node of `kind` is a design unit, whose body holds items.
fn is_unit(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        MODULE_DECL
            | INTERFACE_DECL
            | PROGRAM_DECL
            | PACKAGE_DECL
            | CLASS_DECL
            | CHECKER_DECL
            | UDP_DECL
            | CONFIG_DECL
    )
}

/// Whether `range` starts past the `;` that ends `unit`'s header.
fn past_header(unit: &SyntaxNode, range: TextRange) -> bool {
    let end = significant_child_tokens(unit).find(|it| it.kind() == SEMICOLON);
    end.is_some_and(|it| range.start() >= it.text_range().end())
}

/// The text to insert, and where, that makes `node` waive `rule`: the rule
/// after the names its `astli_allow` lists, or a new one before its first
/// token. `None` where its `astli_allow` is not a string.
fn waiving(node: &SyntaxNode, rule: &Rule) -> Option<(u32, String)> {
    let specs = (node.children())
        .filter(|it| it.kind() == ATTRIBUTES)
        .flat_map(|it| it.children())
        .filter(|it| it.kind() == ATTRIBUTE_SPEC);
    let named = |spec: &SyntaxNode| {
        let name = significant_child_tokens(spec).next();
        name.is_some_and(|it| it.kind() == IDENT && it.text() == ALLOW)
    };
    // Of two on one node the later counts, as for any attribute.
    let Some(spec) = specs.filter(named).last() else {
        let first = significant_tokens(node).next()?;
        let text = format!("(* {ALLOW} = \"{}\" *) ", rule.name);
        return Some((first.text_range().start().into(), text));
    };
    let string = significant_tokens(&spec).find(|it| it.kind() == STRING_LITERAL)?;
    let inner = string.text().get(1..string.text().len() - 1)?;
    let listed = inner.split(|c: char| c == ',' || c.is_whitespace());
    let separator = match listed.into_iter().all(str::is_empty) {
        true => "",
        false => ", ",
    };
    let end = u32::from(string.text_range().end()) - 1;
    Some((end, format!("{separator}{}", rule.name)))
}
