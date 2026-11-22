//! `workspace/symbol`: the design elements and packages the loaded designs
//! declare, matched against the client's query as a fuzzy finder matches.
//!
//! It reads each design's index, so it lists what a build takes, from the
//! package and its dependencies alike, and nothing of an open file until
//! its design has loaded.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use astli_index::{Declaration, Declares};
use astli_text::Origins;
use lsp_server::{RequestId, ResponseError};
use lsp_types::{
    BaseSymbolInformation, Location, Position, Range, SymbolKind, Uri, WorkspaceSymbol,
    WorkspaceSymbolParams,
};
use serde_json::Value;

use crate::Snapshot;
use crate::position::Encoding;
use crate::server::Server;
use crate::tasks::Cancel;

/// How many symbols an answer holds at most: a fuzzy finder shows a screen
/// of them, and an empty or one-letter query over a large design matches
/// thousands, which the client would take long to read and the user never
/// scroll through.
pub const LIMIT: usize = 128;

impl Server {
    /// Hands the request to the pool, with the snapshots current now.
    pub fn workspace_symbols(&mut self, id: RequestId, params: Value) {
        let (snapshots, encoding) = (self.snapshots(), self.encoding);
        self.spawn(id, "workspace/symbol", params, move |params, cancel| {
            symbols(&snapshots, &params, encoding, cancel)
        });
    }
}

/// How well a name matches a query; the better, the lower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Exact,
    Prefix,
    /// The query, whole, inside the name.
    Within,
    /// The query's characters in order, apart.
    Scattered,
}

/// How well `name` matches `query`, ignoring case, or `None` if the query's
/// characters are not all in it in order. An empty query matches every
/// name as a prefix.
fn rank(name: &str, query: &str) -> Option<Rank> {
    let (name, query) = (name.to_lowercase(), query.to_lowercase());
    if name == query {
        return Some(Rank::Exact);
    }
    if name.starts_with(&query) {
        return Some(Rank::Prefix);
    }
    if name.contains(&query) {
        return Some(Rank::Within);
    }
    let mut rest = name.chars();
    (query.chars())
        .all(|wanted| rest.any(|it| it == wanted))
        .then_some(Rank::Scattered)
}

/// A declaration that matched, with the unit whose origins hold its text.
struct Found<'a> {
    rank: Rank,
    declaration: &'a Declaration,
    origins: &'a Origins,
}

fn symbols(
    snapshots: &[Arc<Snapshot>],
    params: &WorkspaceSymbolParams,
    encoding: Encoding,
    cancel: &Cancel,
) -> Result<Vec<WorkspaceSymbol>, ResponseError> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for design in snapshots.iter().flat_map(|it| &it.designs) {
        cancel.check()?;
        // A design's summaries are its units, in the same order.
        let summaries = design.index.summaries().iter().zip(&design.units);
        for (summary, unit) in summaries {
            for declaration in &summary.declarations {
                // A declaration with its body stands in for an `extern` one.
                if declaration.prototype {
                    continue;
                }
                let Some(rank) = rank(&declaration.name, &params.query) else {
                    continue;
                };
                let at = &declaration.location;
                // A header several units include, or a file several designs
                // hold, declares it once.
                if seen.insert((&declaration.name, &at.path, at.at)) {
                    let origins = &unit.origins;
                    found.push(Found {
                        rank,
                        declaration,
                        origins,
                    });
                }
            }
        }
    }
    found.sort_by(|a, b| order(a).cmp(&order(b)));
    found.truncate(LIMIT);
    let answer = (found.iter())
        .filter_map(|it| symbol(it.declaration, it.origins, encoding))
        .collect();
    Ok(answer)
}

/// What sorts a match: how well it matched, then shorter names first, so
/// that what the query nearly spells comes before what merely holds it;
/// the rest only makes the order the same on every request.
fn order<'a>(found: &Found<'a>) -> (Rank, usize, &'a str, &'a Path, u32, u32) {
    let declaration = found.declaration;
    let (name, at) = (declaration.name.as_str(), &declaration.location);
    (
        found.rank,
        name.len(),
        name,
        &at.path,
        at.at.line,
        at.at.col,
    )
}

/// The symbol of `declaration`, its range read off the text `origins` holds
/// of its file; `None` for a path that is no `file:` URI.
fn symbol(
    declaration: &Declaration,
    origins: &Origins,
    encoding: Encoding,
) -> Option<WorkspaceSymbol> {
    let at = &declaration.location;
    let uri = Uri::from_file_path(&at.path).ok()?;
    let range = range(declaration, origins, encoding);
    let kind = match declaration.declares {
        Declares::Module | Declares::Program | Declares::Checker | Declares::Primitive => {
            SymbolKind::Module
        }
        Declares::Interface => SymbolKind::Interface,
        Declares::Package => SymbolKind::Package,
        Declares::Class => SymbolKind::Class,
    };
    Some(WorkspaceSymbol {
        location: Location { uri, range }.into(),
        data: None,
        base_symbol_information: BaseSymbolInformation {
            name: declaration.name.clone(),
            kind,
            tags: None,
            container_name: None,
        },
    })
}

/// The range of the declaration's name, as the text of its file counts it
/// in `encoding`: the location is a line and a column in characters, so
/// the line's characters are counted in the text to find its offset. A
/// name a macro wrote is not spelled where its outermost call is, so it is
/// the empty range there. A file the origins do not hold, which a summary
/// never names, is taken to count as the location does.
pub fn range(declaration: &Declaration, origins: &Origins, encoding: Encoding) -> Range {
    let at = &declaration.location;
    let (line, col) = (at.at.line.saturating_sub(1), at.at.col.saturating_sub(1));
    let file = (origins.files()).find(|&it| origins.path(it) == Some(Path::new(&at.path)));
    let Some(file) = file else {
        let start = Position::new(line, col);
        return Range::new(start, start);
    };
    let (text, lines) = (origins.text(file), origins.lines(file));
    let start = lines.start(line as usize).unwrap_or(text.len() as u32) as usize;
    let start = (text[start..].char_indices())
        .nth(col as usize)
        .map_or(text.len(), |(it, _)| start + it);
    // An escaped name is spelled after its backslash, which it covers.
    let rest = &text[start..];
    let escaped = rest.strip_prefix('\\').unwrap_or(rest);
    let end = match escaped.starts_with(&declaration.name) {
        true => text.len() - escaped.len() + declaration.name.len(),
        false => start,
    };
    encoding.range(text, lines, start as u32..end as u32)
}

#[cfg(test)]
mod tests {
    use super::{Rank, rank};

    #[test]
    fn names_rank_by_how_they_match() {
        use Rank::*;
        // Each row is a name, a query, and how well it matches.
        let rows: &[(&str, &str, Option<Rank>)] = &[
            ("uart", "uart", Some(Exact)),
            ("UART", "uart", Some(Exact)),
            ("uart_tx", "uart", Some(Prefix)),
            ("uart_tx", "UART_T", Some(Prefix)),
            ("my_uart", "uart", Some(Within)),
            ("u_a_r_t", "uart", Some(Scattered)),
            ("ultra_fast", "uft", Some(Scattered)),
            ("uart", "", Some(Prefix)),
            ("Ärger_pkg", "ärg", Some(Prefix)),
            ("tx_uart", "trau", None),
            ("uar", "uart", None),
            ("spi", "uart", None),
        ];
        for &(name, query, expected) in rows {
            assert_eq!(rank(name, query), expected, "{name:?} {query:?}");
        }
        // Better matches sort first.
        assert!(Exact < Prefix && Prefix < Within && Within < Scattered);
    }
}
