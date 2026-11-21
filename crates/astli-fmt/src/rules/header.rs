//! Headers, each laid out from its parts: its attributes, its words, and
//! the imports and lists among them. Words are tokens and the types among
//! them, as many as run together, spaced as [`separation`](super::separation)
//! says: the keywords and the name, or a base class. The header says what
//! goes between its parts, and how each of its lists breaks.

use astli_syntax::{SyntaxElement, SyntaxKind, SyntaxKind::*};

use super::{Writer, is_wildcard, separation, significant_children};
use crate::doc::Doc;

impl Writer<'_> {
    /// A design unit's header: its keywords and name, each import on a line
    /// of its own one level in, as the guide has it, and its parameters and
    /// ports, each broken an entry per line. Each attribute is followed by
    /// `line`.
    pub(super) fn unit_header(&mut self, header: &[SyntaxElement], line: &Doc) -> Option<Doc> {
        let mut parts = Parts(header);
        let attributes = parts.all(ATTRIBUTES);
        let name = parts.words();
        let imports = parts.all(IMPORT_DECL);
        let lists = [parts.one(PARAM_PORT_LIST), parts.one(PORT_LIST)];
        parts.end()?;

        let mut docs = vec![self.attributes(attributes, line), self.spaced(name)];
        for import in imports {
            docs.extend([Doc::indent(Doc::HardLine), self.element(import)]);
        }
        // What follows the imports starts on the next line.
        if !imports.is_empty() {
            docs.push(Doc::HardLine);
        }
        for list in lists.into_iter().flatten() {
            docs.extend([Doc::Space, self.header_list(list, true)]);
        }
        Some(Doc::concat(docs))
    }

    /// A class's header, each attribute on a line of its own: its keywords
    /// and name, its parameters, the classes it extends, the first with the
    /// arguments to its constructor, and the interfaces it implements.
    pub(super) fn class_header(&mut self, header: &[SyntaxElement]) -> Option<Doc> {
        let mut parts = Parts(header);
        let attributes = parts.all(ATTRIBUTES);
        let head = parts.words();
        let params = parts.one(PARAM_PORT_LIST);
        let base = parts.words();
        let args = parts.one(ARG_LIST);
        // The other classes it extends, and those it implements.
        let others = parts.words();
        parts.end()?;

        let mut docs = vec![
            self.attributes(attributes, &Doc::HardLine),
            self.spaced(head),
        ];
        if let Some(params) = params {
            docs.extend([Doc::Space, self.header_list(params, false)]);
        }
        if !base.is_empty() {
            docs.push(Doc::Space);
        }
        docs.push(self.called(base, args));
        if let (Some(args), Some(first)) = (args, others.first()) {
            docs.extend([separation(Some(args), first), self.spaced(others)]);
        }
        Some(Doc::concat(docs))
    }

    /// A function's, a task's or a checker's header: its qualifiers, its
    /// keyword and lifetime, its return type, and its name, `class::name` or
    /// `port.name`, with its ports after it.
    pub(super) fn subroutine_header(&mut self, header: &[SyntaxElement]) -> Option<Doc> {
        let mut parts = Parts(header);
        let attributes = parts.all(ATTRIBUTES);
        let name = parts.words();
        let ports = parts.one(PORT_LIST);
        parts.end()?;
        Some(Doc::concat([
            self.attributes(attributes, &Doc::Space),
            self.called(name, ports),
        ]))
    }

    /// A covergroup's header: its keyword and name with its formals after
    /// it, then what samples it, a clock or a `sample` function with its
    /// ports after it.
    pub(super) fn covergroup_header(&mut self, header: &[SyntaxElement]) -> Option<Doc> {
        let mut parts = Parts(header);
        let attributes = parts.all(ATTRIBUTES);
        let name = parts.words();
        let formals = parts.one(PORT_LIST);
        let clock = parts.one(EVENT_CONTROL);
        let sample = parts.words();
        let ports = parts.one(PORT_LIST);
        parts.end()?;

        let mut docs = vec![
            self.attributes(attributes, &Doc::Space),
            self.called(name, formals),
        ];
        if let Some(clock) = clock {
            docs.extend([Doc::Space, self.element(clock)]);
        }
        if !sample.is_empty() {
            docs.extend([Doc::Space, self.called(sample, ports)]);
        }
        Some(Doc::concat(docs))
    }

    /// A clocking block's header: its keywords and name, and its clock. A
    /// configuration's is its keyword and name alone.
    pub(super) fn clocking_header(&mut self, header: &[SyntaxElement]) -> Option<Doc> {
        let mut parts = Parts(header);
        let attributes = parts.all(ATTRIBUTES);
        let name = parts.words();
        let clock = parts.one(EVENT_CONTROL);
        parts.end()?;

        let mut docs = vec![self.attributes(attributes, &Doc::Space), self.spaced(name)];
        if let Some(clock) = clock {
            docs.extend([Doc::Space, self.element(clock)]);
        }
        Some(Doc::concat(docs))
    }

    /// Each of `attributes`, followed by `line`.
    fn attributes(&mut self, attributes: &[SyntaxElement], line: &Doc) -> Doc {
        let mut docs = Vec::new();
        for attribute in attributes {
            docs.extend([self.element(attribute), line.clone()]);
        }
        Doc::concat(docs)
    }

    /// `words`, then `list` right after them on one line, as a call's
    /// arguments follow its callee: a name and its ports, or a base class
    /// and the arguments to its constructor.
    pub(super) fn called(&mut self, words: &[SyntaxElement], list: Option<&SyntaxElement>) -> Doc {
        let words = self.spaced(words);
        match list {
            Some(list) => Doc::concat([words, self.header_list(list, false)]),
            None => words,
        }
    }

    /// A header's list of parameters, ports or arguments, with an entry per
    /// line if `broken`, or else as [`Writer::list`] says. `(.*)` takes the
    /// ports an `extern` prototype declared, and is written as it is.
    pub(super) fn header_list(&mut self, list: &SyntaxElement, broken: bool) -> Doc {
        let list = list.as_node().expect("a list is a node");
        self.with_comments(list, |this, list| {
            match list.kind() == PORT_LIST && is_wildcard(list) {
                true => this.unspaced(&significant_children(list)),
                false => this.list(list, broken),
            }
        })
    }
}

/// What is left of a header, taken from the front part by part.
struct Parts<'a>(&'a [SyntaxElement]);

impl<'a> Parts<'a> {
    /// The words at the front: tokens, and the types among them.
    fn words(&mut self) -> &'a [SyntaxElement] {
        self.take_while(|it| it.as_token().is_some() || it.kind() == TYPE_REF)
    }

    /// The nodes of `kind` at the front.
    fn all(&mut self, kind: SyntaxKind) -> &'a [SyntaxElement] {
        self.take_while(|it| it.kind() == kind)
    }

    /// The node at the front, if it is of `kind`.
    fn one(&mut self, kind: SyntaxKind) -> Option<&'a SyntaxElement> {
        let [first, rest @ ..] = self.0 else {
            return None;
        };
        (first.kind() == kind).then(|| {
            self.0 = rest;
            first
        })
    }

    fn take_while(&mut self, take: impl Fn(&SyntaxElement) -> bool) -> &'a [SyntaxElement] {
        let at = self.0.iter().take_while(|it| take(it)).count();
        let (taken, rest) = self.0.split_at(at);
        self.0 = rest;
        taken
    }

    /// `Some` if nothing is left, a shape the header knows.
    fn end(&self) -> Option<()> {
        self.0.is_empty().then_some(())
    }
}
