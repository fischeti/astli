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
    chosen(Target::current()?, package, named)
}

/// What [`choose`] chooses, from `here`, the current directory's package.
fn chosen(here: Target, package: Option<&str>, named: bool) -> Result<Target> {
    match package {
        Some(name) => {
            let workspace = lockfile::workspace(&here, "--package")?;
            let package = lockfile::named(&workspace, name)?.clone();
            Ok(Target::member(package, workspace))
        }
        None if !named && here.is_virtual()? => Err(term::error(
            "the workspace root here is virtual, and holds no design of its own",
        )
        .help(format!(
            "name one of its members with {}",
            mark::code("-p NAME")
        ))
        .into()),
        None => Ok(here),
    }
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
    let here = Target::current()?;
    let every = members.workspace || (members.package.is_none() && !named && here.is_virtual()?);
    if !every {
        return work(out, &chosen(here, members.package.as_deref(), named)?);
    }
    if named {
        return Err(Error::failed(format!(
            "{} acts on the packages' own files, not on files named",
            mark::code("--workspace")
        )));
    }
    let Some(workspace) = here.workspace()? else {
        // A package in no workspace is its only package, so that one CI
        // step runs alike in every repository.
        return work(out, &here);
    };
    let packages: Vec<_> = (workspace.packages())
        .filter(|it| !it.is_virtual())
        .collect();
    run.for_each_package();
    let mut failed = 0;
    for package in &packages {
        let target = Target::member((*package).clone(), workspace.clone());
        let package = &package.package;
        match work(out, &target) {
            Ok(()) => {}
            Err(err @ Error::Output(_)) => return Err(err),
            Err(err) => {
                failed += 1;
                out.flush()?;
                let owned = render::FORMAT.get().is_some_and(|it| it.owns_stderr());
                if !matches!(err, Error::Silent) && !(owned && matches!(err, Error::Tally(_))) {
                    let named = match &package.manifest.name {
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
