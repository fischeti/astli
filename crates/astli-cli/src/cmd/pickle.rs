//! `astli pickle` subcommand.
//!
//! Writes the files a design needs as one source. Each file is written as it
//! is, its headers inlined, or with `--expand` as a compiler reads it, when
//! the names it declares can also be renamed so that two designs can share a
//! compilation without their names clashing.
//!
//! Either way the files are chosen from their expansions, since only an
//! expansion sees what a macro instantiates.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use astli_index::{Index, Summary, renamed};
use astli_parse::parse_expanded;
use astli_preproc::{Build, IncludePath, Operands, Session};
use astli_syntax::SyntaxNode;
use rowan::GreenNode;
use usage::{Args, RunWith};

use crate::cli::{BuildArgs, RunArgs, Sources};
use crate::cmd::{self, Ctx, files};
use crate::error::{Error, Result};
use crate::render;
use crate::session;
use crate::sources;

/// Write the files a design needs as one source, its names optionally renamed
#[derive(Args)]
pub struct Pickle {
    #[usage(flatten)]
    pub sources: Sources,
    /// Keep only the files this top needs (can be repeated)
    #[usage(long)]
    pub top: Vec<String>,
    /// Put each file after the files it depends on
    #[usage(long)]
    pub order: bool,
    /// Write what a compiler reads: macros expanded, includes followed, and
    /// only the branches the build takes
    #[usage(long)]
    pub expand: bool,
    /// Prefix each module, interface, program, package and class the files
    /// declare, wherever it is named
    #[usage(long, requires("--expand"))]
    pub prefix: Option<String>,
    /// Suffix each module, interface, program, package and class the files
    /// declare, wherever it is named
    #[usage(long, requires("--expand"))]
    pub suffix: Option<String>,
    /// Leave this name as it is (can be repeated)
    #[usage(long, value_name = "NAME", requires("--expand"))]
    pub exclude_rename: Vec<String>,
    #[usage(flatten)]
    pub build: BuildArgs,
}

impl RunWith<Ctx<'_>> for Pickle {
    type Output = Result;

    fn run_with(self, ctx: Ctx<'_>) -> Result {
        let Ctx { out, run } = ctx;
        let resolved = sources::resolve(&self.sources, &self.build)?;

        // Nothing is printed per file: the output is all of them together.
        let silent = RunArgs {
            quiet: true,
            jobs: run.jobs,
        };
        let outcome = cmd::each(out, &resolved.files, &silent, "", |sink, file| {
            one(sink, file, &resolved.build, self.expand)
        })?;
        if outcome.failed > 0 {
            return outcome.finish();
        }
        let (summaries, trees): (Vec<Summary>, Vec<Option<GreenNode>>) =
            outcome.values.into_iter().unzip();
        let index = Index::new(summaries);

        let mut kept = files::kept(&index, &self.top)?;
        if self.order {
            kept = index.ordered(&kept);
        }

        if self.expand {
            let declared: HashSet<&str> = kept
                .iter()
                .flat_map(|&file| &index.summaries()[file].declarations)
                .map(|declaration| declaration.name.as_str())
                .filter(|name| !self.exclude_rename.iter().any(|excluded| excluded == name))
                .collect();
            let (prefix, suffix) = (
                self.prefix.as_deref().unwrap_or_default(),
                self.suffix.as_deref().unwrap_or_default(),
            );
            let renaming = !(prefix.is_empty() && suffix.is_empty());
            let rename = |name: &str| {
                (renaming && declared.contains(name)).then(|| format!("{prefix}{name}{suffix}"))
            };

            for &file in &kept {
                let tree = trees[file].clone().expect("kept when expanding");
                let text = renamed(&SyntaxNode::new_root(tree), rename);
                write!(out, "{text}")?;
                if !text.is_empty() && !text.ends_with('\n') {
                    writeln!(out)?;
                }
            }
            return Ok(());
        }

        // Each file starts from the definitions alone, as it would as its own
        // compilation unit.
        let mut defines = String::new();
        for define in &resolved.define {
            let (name, body) = define.split_once('=').unwrap_or((define, "1"));
            defines.push_str(&format!("`define {name} {body}\n"));
        }
        let paths: Vec<&Path> = kept
            .iter()
            .map(|&file| resolved.files[file].as_path())
            .collect();
        let outcome = cmd::each(out, &paths, &silent, "", |sink, file| {
            write!(sink.out, "{defines}")?;
            let text = inlined(sink, file, &resolved.build, &mut vec![file.to_path_buf()])?;
            write!(sink.out, "{text}")?;
            if !text.is_empty() && !text.ends_with('\n') {
                writeln!(sink.out)?;
            }
            writeln!(sink.out, "`undefineall")?;
            Ok(())
        })?;
        outcome.finish()
    }
}

/// Expands, parses and summarizes one file, printing what the expansion and
/// the parser reported. The tree goes back when it is what gets written, as
/// its green node, which unlike the tree can cross threads.
fn one(
    sink: &mut cmd::Sink,
    path: &Path,
    build: &Build,
    expand: bool,
) -> Result<(Summary, Option<GreenNode>)> {
    let mut opened = session::open(path, build)?;
    let expanded = opened.expand();
    let parsed = parse_expanded(&opened.session, &expanded.tokens);
    let summary = Summary::new(&opened.session, opened.file, &parsed);
    let origins = opened.session.origins();
    render::diagnostics(sink.diagnostics, origins, opened.diagnostics())?;
    render::diagnostics(sink.diagnostics, origins, &parsed.diagnostics)?;
    Ok((summary, expand.then(|| parsed.root.green().to_owned())))
}

/// The text of `path` with each `` `include `` it names replaced by the
/// header's own text, inlined in turn, whichever branch the directive is in.
///
/// A header that cannot be found, that includes itself, or that a macro
/// names is left as a directive.
fn inlined(
    sink: &mut cmd::Sink,
    path: &Path,
    build: &Build,
    open: &mut Vec<PathBuf>,
) -> Result<String> {
    let text = std::fs::read_to_string(path).map_err(|err| Error::io(path, err))?;
    let mut session = Session::new();
    let file = session.add(path, text.clone());
    let tokens = session.tokens(file);
    let input = session.input(file);

    let mut out = String::with_capacity(text.len());
    let mut written = 0;
    for directive in astli_preproc::scan(&input).directives() {
        let Operands::Include(named) = &directive.operands else {
            continue;
        };
        let start = tokens[directive.tokens.start as usize].start as usize;
        let at = session.origins().line_col(file, start as u32);
        let (name, angle) = match named {
            IncludePath::Quoted(at) => (input.text(at.index).trim_matches('"').to_string(), false),
            IncludePath::Angle(span) => {
                let bytes = span.bytes(&tokens);
                (
                    text[bytes.start as usize..bytes.end as usize]
                        .trim()
                        .to_string(),
                    true,
                )
            }
            IncludePath::Expanded(_) => {
                writeln!(
                    sink.diagnostics,
                    "astli: {}:{at}: a macro names this header, so it is left as a directive",
                    path.display()
                )?;
                continue;
            }
        };
        let found = build
            .search(&name, Some(path), angle)
            .into_iter()
            .find(|candidate| candidate.is_file());
        // One in a branch the build takes was reported by its expansion, and
        // one in another branch may never be needed.
        let Some(header) = found else {
            continue;
        };
        if open.contains(&header) {
            writeln!(
                sink.diagnostics,
                "astli: {}:{at}: `{name}` includes itself, so it is left as a directive",
                path.display()
            )?;
            continue;
        }

        open.push(header.clone());
        let header = inlined(sink, &header, build, open)?;
        open.pop();

        let end = (directive.tokens.start..directive.tokens.end)
            .rev()
            .map(|at| tokens[at as usize])
            .find(|token| !token.kind.is_trivia())
            .map_or(start, |token| token.end as usize);
        out.push_str(&text[written..start]);
        out.push_str(&header);
        written = end;
    }
    out.push_str(&text[written..]);
    Ok(out)
}
