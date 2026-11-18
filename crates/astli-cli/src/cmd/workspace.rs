//! Which package a command acts on: the one `-p` names, the current
//! directory's, or under `--workspace` each package of the workspace in
//! turn.

use std::io::Write;

use astli_package::mark;

use crate::cli::Members;
use crate::error::{Error, Result};
use crate::lockfile::{self, Target};
use crate::render::{self, Out, plural};
use crate::run::Run;
use crate::term;

/// The package a command that acts on one acts on: the workspace package
/// `package` names, as `-p` asks, or the current directory's. Without one,
/// a virtual workspace root has no design to act on, so is refused unless
/// `named` says files were named, which the command reads instead.
pub fn choose(package: Option<&str>, named: bool) -> Result<Target> {
    match package {
        Some(name) => {
            let workspace = lockfile::workspace("--package")?;
            let root = lockfile::named(&workspace, name)?.root.clone();
            Ok(Target::new(root, Some(workspace)))
        }
        None if !named && virtual_here()? => Err(term::error(
            "the workspace root here is virtual, and holds no design of its own",
        )
        .help(format!(
            "name one of its members with {}",
            mark::code("-p NAME")
        ))
        .into()),
        None => Target::current(),
    }
}

/// Whether the package the current directory is in is a virtual workspace
/// root.
fn virtual_here() -> Result<bool> {
    let here = std::env::current_dir().map_err(|err| Error::io(".", err))?;
    let package = astli_package::Package::find(&here)?;
    Ok(package.is_some_and(|it| it.is_virtual))
}

/// Runs `work` on each package `members` chooses: the one `-p` names, or
/// the current directory's, or with `--workspace` each package of the
/// workspace in turn, the root first, as at a virtual root without either,
/// which holds no files and stands for its members; outside a workspace,
/// `--workspace` runs on the one package. A package's failure is
/// reported under its name and the others still run, so that one CI job
/// sees all of them; the run then fails. `named` says whether files were
/// named, which the command reads instead of a package's, so once is
/// enough.
pub fn members(
    run: &Run,
    members: &Members,
    named: bool,
    out: &mut Out,
    mut work: impl FnMut(&mut Out, &Target) -> Result,
) -> Result {
    let every = members.workspace || (members.package.is_none() && !named && virtual_here()?);
    if !every {
        return work(out, &choose(members.package.as_deref(), named)?);
    }
    if named {
        return Err(Error::failed(format!(
            "{} acts on the packages' own files, not on files named",
            mark::code("--workspace")
        )));
    }
    let Some(workspace) = lockfile::workspace_here()? else {
        // A package in no workspace is its only package, so that one CI
        // step runs alike in every repository.
        return work(out, &Target::current()?);
    };
    let packages: Vec<_> = (workspace.packages()).filter(|it| !it.is_virtual).collect();
    run.for_each_package();
    let mut failed = 0;
    for package in &packages {
        let target = Target::new(package.root.clone(), Some(workspace.clone()));
        match work(out, &target) {
            Ok(()) => {}
            Err(err @ Error::Output(_)) => return Err(err),
            Err(err) => {
                failed += 1;
                out.flush()?;
                let owned = render::FORMAT.get().is_some_and(|it| it.owns_stderr());
                if !matches!(err, Error::Silent) && !(owned && matches!(err, Error::Tally(_))) {
                    let named = match &package.name {
                        Some(name) => mark::name(name),
                        None => mark::path(package.root.display()),
                    };
                    let mut problem = err.problem();
                    problem.message = format!("{named}: {}", problem.message);
                    problem.report(run);
                }
            }
        }
    }
    match failed {
        0 => Ok(()),
        _ => Err(Error::tally(format!(
            "{failed} of {} failed",
            plural(packages.len(), "package")
        ))),
    }
}
