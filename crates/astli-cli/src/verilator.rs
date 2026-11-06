//! Building a top with Verilator: finding it, and the flags a profile
//! turns into.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use astli_package::{Profile, mark};

use crate::error::{Error, Result};
use crate::term;

/// The first major version that builds a testbench alone, with `--binary`
/// and `--timing`.
const OLDEST: u32 = 5;

/// The first version that takes `--quiet`, 5.024; an older one prints its
/// reports whatever it is given.
const QUIET: (u32, u32) = (5, 24);

/// The Verilator on the `PATH`.
pub struct Verilator {
    /// As `verilator --version` prints it, `5.054`.
    pub version: String,
    /// The major and minor version.
    numbers: (u32, u32),
}

impl Verilator {
    /// Finds Verilator, refusing one too old to build a testbench alone.
    pub fn find() -> Result<Verilator> {
        let output = match Command::new("verilator").arg("--version").output() {
            Ok(output) if output.status.success() => output,
            Ok(_) => {
                return Err(Error::failed(format!(
                    "{} failed",
                    mark::code("verilator --version")
                )));
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                return Err(term::error("Verilator is not on the PATH")
                    .help(format!(
                        "{} needs Verilator {OLDEST} or later",
                        mark::code("astli build")
                    ))
                    .into());
            }
            Err(err) => return Err(Error::failed(format!("cannot run Verilator: {err}"))),
        };
        // `Verilator 5.054 2026-11-01 rev v5.054`
        let text = String::from_utf8_lossy(&output.stdout);
        let version = text.split_whitespace().nth(1).unwrap_or_default();
        let mut parts = version.split('.').map(|it| it.parse::<u32>().ok());
        let numbers = match (parts.next().flatten(), parts.next().flatten()) {
            (Some(major), minor) => Some((major, minor.unwrap_or(0))),
            (None, _) => None,
        };
        match numbers {
            Some(numbers) if numbers.0 >= OLDEST => Ok(Verilator {
                version: version.to_string(),
                numbers,
            }),
            Some(_) => Err(term::error(format!(
                "Verilator {} is too old to build a testbench alone",
                mark::version(version)
            ))
            .help(format!(
                "{} needs Verilator {OLDEST} or later, for {} and {}",
                mark::code("astli build"),
                mark::code("--binary"),
                mark::code("--timing")
            ))
            .into()),
            None => Err(Error::failed(format!(
                "cannot read a version from {}: {}",
                mark::code("verilator --version"),
                text.trim()
            ))),
        }
    }

    /// What Verilator is given to build `top` into `dir` from the file list
    /// at `list`, as `profile` asks: everything that decides what it
    /// builds, so the jobs it builds on are not among them.
    pub fn args(&self, list: &Path, dir: &Path, top: &str, profile: &Profile) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec!["--binary".into(), "--timing".into()];
        // Its reports repeat what the status lines say.
        if self.numbers >= QUIET {
            args.push("--quiet".into());
        }
        // `check` has judged the design, so what is left of Verilator's
        // warnings is shown without failing the build, and its lint is off:
        // real designs rarely pass it without waivers. An elaboration-time
        // `$error` or `$fatal` is the design refusing its parameters, which
        // Verilator would otherwise only warn of.
        args.extend(
            [
                "-Wno-fatal",
                "-Wno-lint",
                "-Wno-style",
                "-Werror-USERERROR",
                "-Werror-USERFATAL",
            ]
            .map(OsString::from),
        );
        args.extend(["--top-module".into(), top.into()]);
        args.extend(["--Mdir".into(), dir.into()]);
        args.extend(flags(profile).into_iter().map(OsString::from));
        args.extend(["-f".into(), list.into()]);
        args.extend(profile.verilator_args.iter().map(OsString::from));
        args
    }

    /// Builds `top` with `args`, on `jobs` threads, or all of them for 0.
    pub fn build(&self, args: &[OsString], top: &str, jobs: usize) -> Result {
        let mut command = Command::new("verilator");
        command.args(args).arg("-j").arg(jobs.to_string());
        // What compiled before is compiled again from the cache, where one is
        // installed and Verilator was not told of it when it was built.
        if std::env::var_os("OBJCACHE").is_none() && on_path("ccache") {
            command.env("OBJCACHE", "ccache");
        }
        // Standard output holds a command's results, and what Verilator
        // writes is about the run.
        command.stdout(std::io::stderr());
        let status = (command.status())
            .map_err(|err| Error::failed(format!("cannot run Verilator: {err}")))?;
        match status.success() {
            true => Ok(()),
            false => Err(Error::failed(format!(
                "Verilator could not build {}",
                mark::name(top)
            ))),
        }
    }
}

/// Where a build of the module `top` into `dir` leaves its executable.
pub fn executable(dir: &Path, top: &str) -> PathBuf {
    dir.join(format!("V{top}"))
}

/// Whether the program `name` is in a directory of the `PATH`.
fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

/// What `profile`'s knobs ask of Verilator.
pub fn flags(profile: &Profile) -> Vec<String> {
    let mut flags = Vec::new();
    if profile.trace {
        flags.push("--trace-fst".to_string());
    }
    if !profile.assert {
        flags.push("--no-assert".to_string());
    }
    if profile.threads.get() > 1 {
        flags.push("--threads".to_string());
        flags.push(profile.threads.to_string());
    }
    flags
}
