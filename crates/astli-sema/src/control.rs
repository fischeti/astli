//! Where a statement may stand: a `return` with a value only in a function
//! that returns one, and without only elsewhere, and never inside a `fork`;
//! nothing that waits in a function, but in a `fork` it does not join.

use astli_syntax::SyntaxKind::*;
use astli_text::{Diagnostic, Span};

use crate::access::Driver;
use crate::diagnostics;
use crate::hir::*;

/// What the statements of `driver`, a process or a subroutine of `hir`,
/// do where they may not.
pub(crate) fn control(hir: &Hir, driver: Driver) -> Vec<Diagnostic> {
    let mut walk = Walk {
        hir,
        returns: Returns::Nothing,
        found: Vec::new(),
    };
    match driver {
        Driver::Process(body) => walk.stmt(body, Place::default()),
        Driver::Subroutine(symbol) => {
            let SymbolKind::Subroutine {
                keyword,
                returns,
                body,
                ..
            } = &hir[symbol].kind
            else {
                return Vec::new();
            };
            let function = *keyword == FUNCTION_KW;
            walk.returns = match returns {
                _ if !function => Returns::Task,
                Some(Type {
                    kind:
                        TypeKind::Builtin {
                            keyword: VOID_KW, ..
                        },
                    ..
                }) => Returns::Void,
                // A constructor returns the object it makes.
                _ if &*hir[symbol].name.text == "new" => Returns::Nothing,
                _ => Returns::Value(hir[symbol].name.text.clone()),
            };
            let place = Place {
                in_function: function,
                in_fork: false,
            };
            for &stmt in body {
                walk.stmt(stmt, place);
            }
        }
        _ => {}
    }
    walk.found
}

/// What a `return` in the subroutine being walked gives back.
enum Returns {
    /// Nothing is required: a process, which has no `return`, or a
    /// constructor.
    Nothing,
    Void,
    Task,
    /// A value, of the function named.
    Value(Box<str>),
}

/// Where a statement stands.
#[derive(Clone, Copy, Default)]
struct Place {
    /// In a function, outside a `fork … join_none`, where nothing may wait.
    in_function: bool,
    in_fork: bool,
}

struct Walk<'a> {
    hir: &'a Hir,
    returns: Returns,
    found: Vec<Diagnostic>,
}

impl Walk<'_> {
    fn stmt(&mut self, stmt: StmtId, place: Place) {
        let hir = self.hir;
        let at = hir[stmt].span;
        let each = |walk: &mut Self, stmt: &Option<StmtId>, place| {
            if let Some(stmt) = stmt {
                walk.stmt(*stmt, place);
            }
        };
        match &hir[stmt].kind {
            StmtKind::Block { stmts, join, .. } => {
                let mut inner = place;
                if let Some(join) = join {
                    if place.in_function && *join != JOIN_NONE_KW {
                        self.found
                            .push(diagnostics::waits_in_function("a `fork` that joins", at));
                    }
                    // A process `join_none` starts runs apart from the
                    // function.
                    inner = Place {
                        in_function: place.in_function && *join != JOIN_NONE_KW,
                        in_fork: true,
                    };
                }
                for &stmt in stmts {
                    self.stmt(stmt, inner);
                }
            }
            StmtKind::Timed { timing, body } => {
                if place.in_function {
                    let what = match timing {
                        Timing::Delay(..) => "a delay",
                        _ => "an event control",
                    };
                    self.found.push(diagnostics::waits_in_function(what, at));
                }
                each(self, body, place);
            }
            StmtKind::Wait { body, .. } => {
                if place.in_function {
                    self.found
                        .push(diagnostics::waits_in_function("a `wait`", at));
                }
                each(self, body, place);
            }
            StmtKind::Return(value) => self.returned(*value, place, at),
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                each(self, then_branch, place);
                each(self, else_branch, place);
            }
            StmtKind::Case { items, .. } => {
                for item in items {
                    each(self, &item.body, place);
                }
            }
            StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. }
            | StmtKind::Loop { body, .. } => each(self, body, place),
            StmtKind::Assertion { pass, fail, .. }
            | StmtKind::ConcurrentAssertion { pass, fail, .. } => {
                each(self, pass, place);
                each(self, fail, place);
            }
            _ => {}
        }
    }

    fn returned(&mut self, value: Option<ExprId>, place: Place, at: Span) {
        if place.in_fork {
            let found = diagnostics::invalid_return("`return` cannot leave a `fork`", at);
            self.found.push(found);
            return;
        }
        let problem = match (&self.returns, value) {
            (Returns::Void, Some(_)) => "a `void` function returns no value".to_string(),
            (Returns::Task, Some(_)) => "a task returns no value".to_string(),
            (Returns::Value(name), None) => {
                format!("`{name}` returns a value, so its `return` needs one")
            }
            _ => return,
        };
        self.found.push(diagnostics::invalid_return(&problem, at));
    }
}
