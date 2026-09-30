//! The HIR written out, a line per member or statement and expressions as
//! SystemVerilog with every operation parenthesised.

use std::fmt::{self, Display, Write};

use astli_syntax::SyntaxKind::{self, *};

use crate::hir::*;

impl Display for Hir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut printer = Printer {
            hir: self,
            out: String::new(),
            depth: 0,
        };
        printer.members(self.root());
        f.write_str(&printer.out)
    }
}

struct Printer<'a> {
    hir: &'a Hir,
    out: String,
    depth: usize,
}

impl Printer<'_> {
    fn line(&mut self, text: impl Display) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
        writeln!(self.out, "{text}").unwrap();
    }

    fn nested(&mut self, body: impl FnOnce(&mut Self)) {
        self.depth += 1;
        body(self);
        self.depth -= 1;
    }

    fn members(&mut self, scope: ScopeId) {
        let hir = self.hir;
        for member in &hir[scope].members {
            self.member(member);
        }
    }

    /// A scope a generate construct holds, headed by its label.
    fn generate(&mut self, scope: ScopeId) {
        self.nested(|printer| {
            printer.line(format_args!("generate{}", printer.label(scope)));
            printer.nested(|printer| printer.members(scope));
        });
    }

    fn label(&self, scope: ScopeId) -> String {
        match self.hir[scope].owner {
            Some(owner) => format!(" : {}", self.hir[owner].name.text),
            None => String::new(),
        }
    }

    fn member(&mut self, member: &Member) {
        match member {
            Member::Declare(symbol) => self.symbol(*symbol),
            Member::Import(import) => {
                let keyword = if import.export { "export" } else { "import" };
                let item = import.item.as_ref().map_or("*", |item| &item.text);
                self.line(format_args!("{keyword} {}::{item}", import.package.text));
            }
            Member::Assign(assignments) => {
                let assignments = list(assignments.iter().map(|&it| self.expr(it)));
                self.line(format_args!("assign {assignments}"));
            }
            Member::Process(process) => {
                self.line(keyword(process.kind));
                self.nested(|printer| printer.stmt(process.body));
            }
            Member::GenerateIf(arms) => {
                for (i, arm) in arms.iter().enumerate() {
                    let head = match (i, arm.conditions.first()) {
                        (0, Some(&condition)) => format!("if ({})", self.expr(condition)),
                        (_, Some(&condition)) => format!("else if ({})", self.expr(condition)),
                        (_, None) => "else".to_string(),
                    };
                    self.line(head);
                    self.generate(arm.body);
                }
            }
            Member::GenerateCase { selector, arms } => {
                self.line(format_args!("case ({})", self.expr(*selector)));
                self.nested(|printer| {
                    for arm in arms {
                        let labels = match arm.conditions.as_slice() {
                            [] => "default".to_string(),
                            labels => list(labels.iter().map(|&it| printer.expr(it))),
                        };
                        printer.line(format_args!("{labels}:"));
                        printer.generate(arm.body);
                    }
                });
            }
            Member::GenerateFor { header, body } => {
                self.line(format_args!("for ({})", self.for_header(header)));
                self.generate(*body);
            }
            Member::Generate(scope) => {
                self.line(format_args!("generate{}", self.label(*scope)));
                self.nested(|printer| printer.members(*scope));
            }
            Member::Opaque(opaque) => self.line(opaque_text(opaque)),
        }
    }

    fn symbol(&mut self, id: SymbolId) {
        let hir = self.hir;
        let symbol = &hir[id];
        let name = &symbol.name.text;
        match &symbol.kind {
            SymbolKind::Definition {
                keyword: kw, scope, ..
            } => {
                self.line(format_args!("{} {name}", keyword(*kw)));
                self.nested(|printer| printer.members(*scope));
            }
            SymbolKind::Class(body) => {
                self.line(format_args!("class {name} {}", opaque_text(body)))
            }
            SymbolKind::Port(port) => {
                let direction = port.direction.map_or("?", keyword);
                let net = port
                    .keyword
                    .map(|kw| format!("{} ", keyword(kw)))
                    .unwrap_or_default();
                let default = self.init(port.default);
                self.line(format_args!(
                    "port {direction} {net}{name}: {}{default}",
                    self.ty(&port.ty)
                ));
            }
            SymbolKind::Parameter(parameter) => {
                let kw = if parameter.local {
                    "localparam"
                } else {
                    "parameter"
                };
                let ty = match parameter.is_type {
                    true => "type ".to_string(),
                    false => String::new(),
                };
                let value = self.init(parameter.value);
                self.line(format_args!(
                    "{kw} {ty}{name}: {}{value}",
                    self.ty(&parameter.ty)
                ));
            }
            SymbolKind::Net(data) => {
                let init = self.init(data.init);
                self.line(format_args!("net {name}: {}{init}", self.ty(&data.ty)));
            }
            SymbolKind::Variable(data) => {
                let init = self.init(data.init);
                self.line(format_args!("var {name}: {}{init}", self.ty(&data.ty)));
            }
            SymbolKind::Genvar(init) => {
                let init = self.init(*init);
                self.line(format_args!("genvar {name}{init}"));
            }
            SymbolKind::Typedef(ty) => self.line(format_args!("typedef {name}: {}", self.ty(ty))),
            SymbolKind::EnumMember { value } => {
                let value = self.init(*value);
                self.line(format_args!("enum member {name}{value}"));
            }
            SymbolKind::Subroutine {
                keyword: kw,
                scope,
                returns,
                body,
                ..
            } => {
                let returns = match returns {
                    Some(ty) => format!(": {}", self.ty(ty)),
                    None => String::new(),
                };
                self.line(format_args!("{} {name}{returns}", keyword(*kw)));
                self.nested(|printer| {
                    printer.members(*scope);
                    for &stmt in body {
                        printer.stmt(stmt);
                    }
                });
            }
            SymbolKind::Instance(instance) => {
                let parameters = match instance.parameters.as_slice() {
                    [] => String::new(),
                    args => format!(" #({})", self.args(args)),
                };
                let dims = self.dims(&instance.dims);
                let connections = self.args(&instance.connections);
                let definition = &instance.definition.text;
                self.line(format_args!(
                    "instance {name}{dims}: {definition}{parameters} ({connections})"
                ));
            }
            // Written where the block is.
            SymbolKind::Block(_) => {}
            SymbolKind::Label(_) => self.line(format_args!("label {name}")),
            SymbolKind::Other(body) => {
                self.line(format_args!("other {name} {}", opaque_text(body)))
            }
        }
    }

    fn init(&self, value: Option<ExprId>) -> String {
        match value {
            Some(value) => format!(" = {}", self.expr(value)),
            None => String::new(),
        }
    }

    fn for_header(&self, header: &For) -> String {
        let mut init = Vec::new();
        for member in &self.hir[header.scope].members {
            let Member::Declare(symbol) = member else {
                continue;
            };
            let symbol = &self.hir[*symbol];
            let (keyword, value) = match &symbol.kind {
                SymbolKind::Genvar(value) => ("genvar".to_string(), *value),
                SymbolKind::Variable(data) => (self.ty(&data.ty), data.init),
                _ => continue,
            };
            init.push(format!(
                "{keyword} {}{}",
                symbol.name.text,
                self.init(value)
            ));
        }
        init.extend(header.init.iter().map(|&it| self.expr(it)));
        let condition = header.condition.map(|it| self.expr(it)).unwrap_or_default();
        let step = list(header.step.iter().map(|&it| self.expr(it)));
        format!("{}; {condition}; {step}", init.join(", "))
    }

    fn stmt(&mut self, id: StmtId) {
        let hir = self.hir;
        match &hir[id].kind {
            StmtKind::Empty => self.line(";"),
            StmtKind::Expr(expr) => self.line(self.expr(*expr)),
            StmtKind::Block { scope, join, stmts } => {
                let open = if join.is_some() { "fork" } else { "begin" };
                self.line(format_args!("{open}{}", self.label(*scope)));
                self.nested(|printer| {
                    printer.members(*scope);
                    for &stmt in stmts {
                        printer.stmt(stmt);
                    }
                });
                if let Some(join) = join {
                    self.line(keyword(*join));
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.line(format_args!("if ({})", self.expr(*condition)));
                self.branch(*then_branch);
                if let Some(else_branch) = else_branch {
                    self.line("else");
                    self.branch(Some(*else_branch));
                }
            }
            StmtKind::Case {
                keyword: kw,
                selector,
                items,
            } => {
                let selector = match selector {
                    Some(selector) => format!(" ({})", self.expr(*selector)),
                    None => String::new(),
                };
                self.line(format_args!("{}{selector}", keyword(*kw)));
                self.nested(|printer| {
                    for item in items {
                        let labels = match item.labels.as_slice() {
                            [] => "default".to_string(),
                            labels => list(labels.iter().map(|&it| printer.expr(it))),
                        };
                        printer.line(format_args!("{labels}:"));
                        printer.branch(item.body);
                    }
                });
            }
            StmtKind::For { header, body } => {
                self.line(format_args!("for ({})", self.for_header(header)));
                self.branch(*body);
            }
            StmtKind::Foreach { scope, array, body } => {
                let variables =
                    (self.hir[*scope].members.iter()).filter_map(|member| match member {
                        Member::Declare(symbol) => Some(self.hir[*symbol].name.text.to_string()),
                        _ => None,
                    });
                let variables = list(variables);
                self.line(format_args!("foreach ({}[{variables}])", self.expr(*array)));
                self.branch(*body);
            }
            StmtKind::Loop {
                keyword: kw,
                condition,
                body,
            } => {
                let condition = match condition {
                    Some(condition) => format!(" ({})", self.expr(*condition)),
                    None => String::new(),
                };
                let kw = if *kw == DO_KW {
                    "do while"
                } else {
                    keyword(*kw)
                };
                self.line(format_args!("{kw}{condition}"));
                self.branch(*body);
            }
            StmtKind::Timed { timing, body } => {
                self.line(self.timing(timing));
                self.branch(*body);
            }
            StmtKind::Wait { condition, body } => match condition {
                Some(condition) => {
                    self.line(format_args!("wait ({})", self.expr(*condition)));
                    self.branch(*body);
                }
                None => self.line("wait fork"),
            },
            StmtKind::Disable(target) => match target {
                Some(target) => self.line(format_args!("disable {}", self.expr(*target))),
                None => self.line("disable fork"),
            },
            StmtKind::Return(value) => match value {
                Some(value) => self.line(format_args!("return {}", self.expr(*value))),
                None => self.line("return"),
            },
            StmtKind::Break => self.line("break"),
            StmtKind::Continue => self.line("continue"),
            StmtKind::Trigger(event) => self.line(format_args!("-> {}", self.expr(*event))),
            StmtKind::ProceduralAssign { keyword: kw, expr } => {
                self.line(format_args!("{} {}", keyword(*kw), self.expr(*expr)));
            }
            StmtKind::Assertion {
                keyword: kw,
                condition,
                pass,
                fail,
            } => {
                self.line(format_args!("{} ({})", keyword(*kw), self.expr(*condition)));
                self.branch(*pass);
                if let Some(fail) = fail {
                    self.line("else");
                    self.branch(Some(*fail));
                }
            }
            StmtKind::Opaque(opaque) => self.line(opaque_text(opaque)),
        }
    }

    fn branch(&mut self, stmt: Option<StmtId>) {
        self.nested(|printer| match stmt {
            Some(stmt) => printer.stmt(stmt),
            None => printer.line(";"),
        });
    }

    fn timing(&self, timing: &Timing) -> String {
        match timing {
            Timing::Delay(op, value) => format!("{}{}", punctuation(*op), self.expr(*value)),
            Timing::Events(events) => format!("@({})", self.events(events)),
            Timing::Star => "@*".to_string(),
            Timing::Repeat(count, events) => {
                format!("repeat ({}) @({})", self.expr(*count), self.events(events))
            }
        }
    }

    fn events(&self, events: &[Event]) -> String {
        let events = events.iter().map(|event| {
            let edge = event
                .edge
                .map(|edge| format!("{} ", keyword(edge)))
                .unwrap_or_default();
            let iff = match event.iff {
                Some(iff) => format!(" iff {}", self.expr(iff)),
                None => String::new(),
            };
            format!("{edge}{}{iff}", self.expr(event.expr))
        });
        events.collect::<Vec<_>>().join(" or ")
    }

    fn ty(&self, ty: &Type) -> String {
        let kind = match &ty.kind {
            TypeKind::Implicit { signing } => match signing {
                Some(signing) => format!("implicit {}", keyword(*signing)),
                None => "implicit".to_string(),
            },
            TypeKind::Builtin {
                keyword: kw,
                signing,
            } => match signing {
                Some(signing) => format!("{} {}", keyword(*kw), keyword(*signing)),
                None => keyword(*kw).to_string(),
            },
            TypeKind::Named(name) => self.expr(*name),
            TypeKind::Enum { base, members } => {
                let members = list(
                    members
                        .iter()
                        .map(|&member| self.hir[member].name.text.to_string()),
                );
                format!("enum {} {{{members}}}", self.ty(base))
            }
            TypeKind::Struct {
                keyword: kw,
                fields,
            } => {
                let fields = fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name.text, self.ty(&field.ty)));
                format!(
                    "{} {{{}}}",
                    keyword(*kw),
                    fields.collect::<Vec<_>>().join("; ")
                )
            }
            TypeKind::TypeOf(expr) => format!("type({})", self.expr(*expr)),
            TypeKind::Opaque(opaque) => opaque_text(opaque),
        };
        format!("{kind}{}", self.dims(&ty.dims))
    }

    fn dims(&self, dims: &[Dim]) -> String {
        let mut out = String::new();
        for dim in dims {
            match dim {
                Dim::Range(hi, lo) => write!(out, " [{}:{}]", self.expr(*hi), self.expr(*lo)),
                Dim::Single(size) => write!(out, " [{}]", self.expr(*size)),
                Dim::Empty => write!(out, " []"),
            }
            .unwrap();
        }
        out
    }

    fn args(&self, args: &[Arg]) -> String {
        list(args.iter().map(|arg| match arg {
            Arg::Positional(value) => value.map(|value| self.expr(value)).unwrap_or_default(),
            Arg::Named { name, value } => {
                let value = value.map(|value| self.expr(value)).unwrap_or_default();
                format!(".{}({value})", name.text)
            }
            Arg::Implicit(name) => format!(".{}", name.text),
            Arg::Wildcard(_) => ".*".to_string(),
        }))
    }

    fn expr(&self, id: ExprId) -> String {
        let expr = |id: &ExprId| self.expr(*id);
        match &self.hir[id].kind {
            ExprKind::Name(name) | ExprKind::System(name) | ExprKind::Literal(name) => {
                name.to_string()
            }
            ExprKind::Keyword(kw) => keyword(*kw).to_string(),
            ExprKind::Unary { op, operand } => format!("{}{}", punctuation(*op), expr(operand)),
            ExprKind::Postfix { op, operand } => format!("{}{}", expr(operand), punctuation(*op)),
            ExprKind::Binary { op, lhs, rhs } => {
                format!("({} {} {})", expr(lhs), punctuation(*op), expr(rhs))
            }
            ExprKind::Conditional {
                condition,
                then_value,
                else_value,
            } => format!(
                "({} ? {} : {})",
                expr(condition),
                expr(then_value),
                expr(else_value)
            ),
            ExprKind::Select { base, index, range } => match range {
                Some((op, end)) => format!(
                    "{}[{}{}{}]",
                    expr(base),
                    expr(index),
                    punctuation(*op),
                    expr(end)
                ),
                None => format!("{}[{}]", expr(base), expr(index)),
            },
            ExprKind::Member { base, name } => format!("{}.{}", expr(base), name.text),
            ExprKind::Scoped { base, name } => format!("{}::{}", expr(base), name.text),
            ExprKind::Call { callee, args } => format!("{}({})", expr(callee), self.args(args)),
            ExprKind::Concat(parts) => format!("{{{}}}", list(parts.iter().map(expr))),
            ExprKind::Replication { count, concat } => {
                format!("{{{}{}}}", expr(count), expr(concat))
            }
            ExprKind::Stream { op, slice, concat } => {
                let slice = slice
                    .map(|slice| format!(" {}", self.expr(slice)))
                    .unwrap_or_default();
                format!("{{{}{slice} {}}}", punctuation(*op), expr(concat))
            }
            ExprKind::Pattern { ty, items } => {
                let ty = ty.map(|ty| self.expr(ty)).unwrap_or_default();
                let items = items.iter().map(|item| match item.key {
                    Some(key) => format!("{}: {}", self.expr(key), self.expr(item.value)),
                    None => self.expr(item.value),
                });
                format!("{ty}'{{{}}}", list(items))
            }
            ExprKind::Cast { ty, operand } => format!("{}'({})", expr(ty), expr(operand)),
            ExprKind::Inside { expr: tested, set } => {
                format!(
                    "({} inside {{{}}})",
                    expr(tested),
                    list(set.iter().map(expr))
                )
            }
            ExprKind::Assign {
                op,
                lhs,
                rhs,
                timing,
            } => {
                let timing = timing
                    .as_ref()
                    .map(|timing| format!("{} ", self.timing(timing)))
                    .unwrap_or_default();
                format!("{} {} {timing}{}", expr(lhs), punctuation(*op), expr(rhs))
            }
            ExprKind::Tagged { member, value } => match value {
                Some(value) => format!("tagged {} {}", member.text, expr(value)),
                None => format!("tagged {}", member.text),
            },
            ExprKind::Type(ty) => self.ty(ty),
            ExprKind::Opaque(opaque) => opaque_text(opaque),
        }
    }
}

fn list(items: impl Iterator<Item = impl Display>) -> String {
    items
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn opaque_text(opaque: &Opaque) -> String {
    let names = opaque.names.iter().map(|name| name.text.to_string());
    format!("opaque({})", names.collect::<Vec<_>>().join(" "))
}

fn keyword(kind: SyntaxKind) -> &'static str {
    kind.keyword_text().unwrap_or_else(|| punctuation(kind))
}

/// The operators and punctuation the HIR keeps by kind.
fn punctuation(kind: SyntaxKind) -> &'static str {
    match kind {
        DOLLAR => "$",
        HASH => "#",
        HASH_HASH => "##",
        COLON => ":",
        PLUS_COLON => "+:",
        MINUS_COLON => "-:",
        PLUS => "+",
        PLUS_PLUS => "++",
        PLUS_EQ => "+=",
        MINUS => "-",
        MINUS_MINUS => "--",
        MINUS_EQ => "-=",
        MINUS_GT => "->",
        STAR => "*",
        STAR_STAR => "**",
        STAR_EQ => "*=",
        SLASH => "/",
        SLASH_EQ => "/=",
        PERCENT => "%",
        PERCENT_EQ => "%=",
        EQ => "=",
        EQ_EQ => "==",
        EQ_EQ_EQ => "===",
        EQ_EQ_QUESTION => "==?",
        BANG => "!",
        BANG_EQ => "!=",
        BANG_EQ_EQ => "!==",
        BANG_EQ_QUESTION => "!=?",
        LT => "<",
        LT_EQ => "<=",
        LT_LT => "<<",
        LT_LT_LT => "<<<",
        LT_LT_EQ => "<<=",
        LT_LT_LT_EQ => "<<<=",
        LT_MINUS_GT => "<->",
        GT => ">",
        GT_EQ => ">=",
        GT_GT => ">>",
        GT_GT_GT => ">>>",
        GT_GT_EQ => ">>=",
        GT_GT_GT_EQ => ">>>=",
        AMP => "&",
        AMP_AMP => "&&",
        AMP_AMP_AMP => "&&&",
        AMP_EQ => "&=",
        PIPE => "|",
        PIPE_PIPE => "||",
        PIPE_EQ => "|=",
        CARET => "^",
        CARET_EQ => "^=",
        CARET_TILDE => "^~",
        TILDE => "~",
        TILDE_AMP => "~&",
        TILDE_PIPE => "~|",
        TILDE_CARET => "~^",
        _ => "?",
    }
}
