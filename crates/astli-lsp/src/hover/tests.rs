//! Every case under `tests/data/hover/`, with the `.svh` beside it that it
//! may include, loaded as a design of one unit, and what hover says at
//! each offset a `// ^` line marks in either, in a `.hover`: of the macro
//! call whose name is there, or of the names there. The definition called
//! `top`, where a case declares one, is the design's top.
//!
//! Adding a case means writing the `.sv` and running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run -p astli-lsp hover::
//! ```

use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

use astli_index::{Index, Summary};
use astli_parse::parse_expanded;
use astli_preproc::{Build, Session};
use astli_sema::{Design, Elaboration, Sites, lower};
use astli_testing::snapshots;

use super::{Hover, joined};
use crate::snapshot::{Analysed, Memo, Unit};

#[test]
fn snapshots() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/hover");
    snapshots::check_with(&root, "hover", &["svh"], |name, text| {
        let build = Build::new().include_dir(&root);
        let mut session = Session::new().building(build.clone());
        let file = session.add(root.join(name), text);
        let expanded = session.expand(file);
        let parsed = parse_expanded(&session, &expanded.tokens);
        // A macro no file defines is reported, and its call left as written.
        let undefined = |it: &astli_text::Diagnostic| it.code.as_str() == "undefined-macro";
        assert!(
            expanded.diagnostics.iter().all(undefined) && parsed.diagnostics.is_empty(),
            "{name}: {:?}",
            (expanded.diagnostics.iter().chain(&parsed.diagnostics)).collect::<Vec<_>>()
        );
        let summary = Summary::new(session.origins(), file, &parsed);
        let elaboration = Elaboration::new(Arc::new(Design::new(vec![Arc::new(lower(&parsed))])));
        let tops = elaboration.top("top").into_iter().collect();
        let (file, _) = elaboration.design().files().next().unwrap();
        let unit = Unit {
            path: root.join(name),
            file,
            origins: Arc::new(session.into_origins()),
            parsed: Vec::new(),
            checked: true,
            linted: true,
            calls: expanded.calls.into(),
            skipped: expanded.skipped.into(),
            build: Arc::new(build),
            memo: Memo::default(),
        };
        let analysed = Analysed::new(elaboration, Index::new(vec![summary]), vec![unit], tops);
        hovered(&analysed)
    });
}

/// What hover says at each offset the case's `// ^` lines mark, in it and
/// in the header it includes: each `^` marks the column below which it
/// stands in the line above, the nearest that is no such line.
fn hovered(analysed: &Analysed) -> String {
    let unit = &analysed.units[0];
    let origins = &*unit.origins;
    let sites = Sites::new(analysed.elaboration.design(), unit.file, origins);
    let mut hover = Hover::new(analysed);
    let mut out = String::new();
    let files = (origins.files())
        .filter(|&it| (origins.path(it)).is_some_and(|path| path.extension().is_some()));
    for src in files {
        let mut above = None;
        let mut start = 0;
        for line in origins.text(src).split_inclusive('\n') {
            let marks = line.trim_start().strip_prefix("//").is_some_and(|rest| {
                rest.contains('^') && rest.trim_end().chars().all(|c| c == ' ' || c == '^')
            });
            let line_start = start;
            start += line.len();
            if !marks {
                above = Some(line_start);
                continue;
            }
            let code = above.expect("a mark has a line above it");
            for (column, _) in line.char_indices().filter(|(_, c)| *c == '^') {
                let offset = (code + column) as u32;
                let path = origins.path(src).unwrap().file_name().unwrap();
                let at = origins.line_col(src, offset);
                let word = word(origins.text(src), offset as usize);
                writeln!(out, "{}:{at} {word}", path.display()).unwrap();
                // A macro call is found by its name, as a cursor finds it.
                let call = (unit.calls.iter().copied()).find(|&id| {
                    let name = origins.expansion(id).name;
                    name.src_id == src && name.start <= offset && offset <= name.end
                });
                let said: Vec<String> = match call {
                    Some(call) => vec![hover.macro_call(unit, call)],
                    None => (sites.at(src, offset))
                        .filter_map(|site| hover.site(unit, site))
                        .fold(Vec::new(), |mut said, it| {
                            if !said.contains(&it) {
                                said.push(it);
                            }
                            said
                        }),
                };
                let said = joined(&said);
                match said {
                    Some(said) => {
                        for line in said.trim_end().lines() {
                            match line.is_empty() {
                                true => out.push('\n'),
                                false => writeln!(out, "    {line}").unwrap(),
                            }
                        }
                    }
                    None => out += "    nothing\n",
                }
                out.push('\n');
            }
        }
    }
    out
}

/// The word of `text` at `offset`, a name or a macro's.
fn word(text: &str, offset: usize) -> &str {
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '`' || c == '$';
    let start = text[..offset].rfind(|c| !is_word(c)).map_or(0, |it| it + 1);
    let end = text[offset..]
        .find(|c| !is_word(c))
        .map_or(text.len(), |it| offset + it);
    &text[start..end]
}
