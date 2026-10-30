//! Each parameter and enum member of a repository, and its value with every
//! definition at its defaults: one `path:line:col name = value` a line, in
//! order, `unknown` where sema cannot evaluate it.
//!
//!     cargo run --release -p astli-sema --example consts -- corpus/axi
//!     cargo run --release -p astli-sema --example consts -- --files corpus/axi
//!     cargo run --release -p astli-sema --example consts -- -F design.f
//!
//! The repository is one design: every file in it but those another
//! includes, each expanded with the directories its headers are in. Given a
//! filelist instead, its files are, with its `+incdir+` and `+define+`, its
//! paths relative to it. With `--files`, it prints that selection instead,
//! as a filelist, which is what `scripts/consts-oracle.py` hands `slang` to
//! compare the values with.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use astli_parse::parse_expanded;
use astli_preproc::{Build, Session};
use astli_sema::{Design, Elaboration, Member, ScopeId, SymbolKind, SymbolRef, lower};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let files_only = args.first().is_some_and(|it| it == "--files");
    if files_only {
        args.remove(0);
    }
    let (dirs, defines, units) = match args.as_slice() {
        [flag, filelist] if flag == "-F" => read(Path::new(filelist)),
        [repo] => {
            let (dirs, units) = select(Path::new(repo));
            (dirs, Vec::new(), units)
        }
        _ => {
            eprintln!("usage: consts [--files] (<repository> | -F <filelist>)");
            std::process::exit(1);
        }
    };
    if files_only {
        for dir in &dirs {
            println!("+incdir+{}", dir.display());
        }
        for define in &defines {
            println!("+define+{define}");
        }
        for unit in &units {
            println!("{}", unit.display());
        }
        return;
    }

    let build = dirs
        .iter()
        .fold(Build::new(), |build, dir| build.include_dir(dir));
    let build = defines.iter().fold(build, |build, define| {
        let (name, body) = define.split_once('=').unwrap_or((define, "1"));
        build.define(name, body)
    });
    let mut sessions = Vec::new();
    let mut hirs = Vec::new();
    for path in &units {
        let mut session = Session::new().building(build.clone());
        let Some(file) = session.open(path) else {
            continue;
        };
        let expanded = session.expand(file);
        hirs.push(lower(&parse_expanded(&session, &expanded.tokens)));
        sessions.push(session);
    }
    let design = Design::new(hirs.into_iter().map(Arc::new).collect());
    let elaboration = Elaboration::new(Arc::new(design));
    let design = elaboration.design();

    let mut lines = BTreeSet::new();
    for ((file, hir), session) in design.files().zip(&sessions) {
        let origins = session.origins();
        let mut homes = vec![hir.root(); hir.symbols().count()];
        for (id, scope) in hir.scopes() {
            for member in &scope.members {
                if let Member::Declare(symbol) = member {
                    homes[symbol.index()] = id;
                }
            }
        }
        // The definition each scope is part of.
        let definition = |mut scope: ScopeId| loop {
            if let Some(owner) = hir[scope].owner
                && let SymbolKind::Definition { .. } = hir[owner].kind
            {
                return Some(owner);
            }
            scope = hir[scope].parent?;
        };
        for (id, symbol) in hir.symbols() {
            if !matches!(
                symbol.kind,
                SymbolKind::Parameter(_) | SymbolKind::EnumMember { .. }
            ) {
                continue;
            }
            let Some(owner) = definition(homes[id.index()]) else {
                continue;
            };
            let body = elaboration.body(SymbolRef {
                file,
                symbol: owner,
            });
            let Some(body) = body else {
                continue;
            };
            // Where the name is spelled, which a macro may have written.
            let at = origins.spelled(origins.reported_at(symbol.name.span));
            let Some(path) = origins.path(at.src_id) else {
                continue;
            };
            let value = elaboration.value(body, id);
            lines.insert(format!(
                "{}:{} {} = {value}",
                path.display(),
                origins.line_col(at.src_id, at.start),
                symbol.name.text
            ));
        }
    }
    for line in lines {
        println!("{line}");
    }
}

/// The directories holding headers, and the files that are units: every
/// `.sv` no file includes by name.
fn select(repo: &Path) -> (BTreeSet<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut stack = vec![repo.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // A hidden directory, or a link into one, is a tool's copy.
            if entry.file_name().to_string_lossy().starts_with('.') || into_hidden(&entry.path()) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("sv" | "svh")
            ) {
                files.push(path);
            }
        }
    }
    files.sort();

    let mut included = HashSet::new();
    let mut dirs = BTreeSet::new();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let names = (text.lines())
            .filter_map(|line| line.trim_start().strip_prefix("`include"))
            .filter_map(|rest| rest.split('"').nth(1))
            .filter_map(|name| name.rsplit('/').next());
        included.extend(names.map(str::to_string));
        // A header is included by its name, or by its directory and name.
        if path.extension().is_some_and(|ext| ext == "svh") {
            let dir = path.parent().expect("a file is in a directory");
            dirs.insert(dir.to_path_buf());
            dirs.extend(dir.parent().map(Path::to_path_buf));
        }
    }
    let units = (files.iter())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sv"))
        .filter(|path| {
            let name = path.file_name().map(|it| it.to_string_lossy().to_string());
            !name.is_some_and(|name| included.contains(&name))
        })
        .cloned()
        .collect();
    (dirs, units)
}

/// The directories, definitions and files a filelist names, one a line,
/// its paths relative to it. Canonical, so that `slang` given the selection
/// names each file as astli does.
fn read(filelist: &Path) -> (BTreeSet<PathBuf>, Vec<String>, Vec<PathBuf>) {
    let Ok(text) = std::fs::read_to_string(filelist) else {
        eprintln!("cannot read {}", filelist.display());
        std::process::exit(1);
    };
    let base = filelist.parent().unwrap_or(Path::new("."));
    let path = |it: &str| base.join(it).canonicalize().unwrap_or(base.join(it));
    let (mut dirs, mut defines, mut units) = (BTreeSet::new(), Vec::new(), Vec::new());
    for line in text.lines().map(str::trim).filter(|it| !it.is_empty()) {
        if let Some(dir) = line.strip_prefix("+incdir+") {
            dirs.insert(path(dir));
        } else if let Some(define) = line.strip_prefix("+define+") {
            defines.push(define.to_string());
        } else if !line.starts_with("//") {
            units.push(path(line));
        }
    }
    (dirs, defines, units)
}

/// Whether `path` is a link whose target passes through a hidden directory.
fn into_hidden(path: &std::path::Path) -> bool {
    std::fs::read_link(path).is_ok_and(|target| {
        target.components().any(|part| {
            matches!(part, std::path::Component::Normal(name)
                if name.to_string_lossy().starts_with('.'))
        })
    })
}
