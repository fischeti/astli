//! A struct or enum type knows where it is declared, so two of one shape
//! declared apart are told apart, and a typedef that only renames one is
//! the same type.

use std::sync::Arc;

use astli_parse::parse_expanded;
use astli_preproc::Session;
use astli_sema::{Design, Elaboration, Shape, SymbolKind, SymbolRef, Ty, lower};

const SOURCE: &str = "
package a_pkg;
  typedef struct packed { logic [3:0] hi; logic [3:0] lo; } pair_t;
  typedef enum logic [1:0] { Idle, Busy } state_e;
  typedef pair_t alias_t;
endpackage

package b_pkg;
  typedef struct packed { logic [3:0] hi; logic [3:0] lo; } pair_t;
endpackage
";

/// The type of each typedef the source declares, by name and package.
fn typedefs() -> Vec<(String, SymbolRef, Ty)> {
    let mut session = Session::new();
    let file = session.add("identity.sv", SOURCE.to_string());
    let expanded = session.expand(file);
    let parsed = parse_expanded(&session, &expanded.tokens);
    let design = Design::new(vec![Arc::new(lower(&parsed))]);
    let elaboration = Elaboration::new(Arc::new(design));
    let (file, hir) = elaboration.design().files().next().unwrap();
    let mut found = Vec::new();
    for (id, symbol) in hir.symbols() {
        let SymbolKind::Definition { scope, .. } = symbol.kind else {
            continue;
        };
        let at = SymbolRef { file, symbol: id };
        let body = elaboration.body(at).unwrap();
        for (member, declared) in hir.symbols() {
            let SymbolKind::Typedef(_) = declared.kind else {
                continue;
            };
            let at = SymbolRef {
                file,
                symbol: member,
            };
            if elaboration.design().home(at) != scope {
                continue;
            }
            let name = format!("{}::{}", symbol.name.text, declared.name.text);
            found.push((name, at, elaboration.type_of(body, member)));
        }
    }
    found
}

fn find<'a>(typedefs: &'a [(String, SymbolRef, Ty)], name: &str) -> &'a (String, SymbolRef, Ty) {
    typedefs.iter().find(|(it, ..)| it == name).unwrap()
}

#[test]
fn types_carry_their_declarations() {
    let typedefs = typedefs();
    let (_, a_at, a) = find(&typedefs, "a_pkg::pair_t");
    let (_, _, b) = find(&typedefs, "b_pkg::pair_t");
    let (_, _, alias) = find(&typedefs, "a_pkg::alias_t");

    // One shape and one name, declared in two packages.
    assert_ne!(a, b);
    // A typedef of a typedef names the same type.
    assert_eq!(a, alias);
    let Ty::Packed(packed) = a else {
        panic!("a packed struct: {a:?}");
    };
    let Shape::Struct(st) = &packed.shape else {
        panic!("a packed struct: {a:?}");
    };
    assert_eq!(st.typedef, Some(*a_at));
    assert!(st.members.iter().all(|member| member.span.start > 0));

    let (_, state_at, state) = find(&typedefs, "a_pkg::state_e");
    let Ty::Packed(packed) = state else {
        panic!("an enum: {state:?}");
    };
    let Shape::Enum(en) = &packed.shape else {
        panic!("an enum: {state:?}");
    };
    assert_eq!(en.typedef, Some(*state_at));
    assert_eq!(en.members.len(), 2);
}
