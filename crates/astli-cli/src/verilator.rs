//! Building a top with Verilator: finding it, and the flags a profile
//! turns into.

use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use astli_package::{Opt, Profile, mark};

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

    /// Builds `top` into `dir` from the file list at `list`, as `profile`
    /// asks, on `jobs` threads, or all of them for 0.
    pub fn build(
        &self,
        list: &Path,
        dir: &Path,
        top: &str,
        profile: &Profile,
        jobs: usize,
    ) -> Result {
        let mut command = Command::new("verilator");
        command.args(["--binary", "--timing"]);
        // Its reports repeat what the status lines say.
        if self.numbers >= QUIET {
            command.arg("--quiet");
        }
        // `check` has judged the design, so what is left of Verilator's
        // warnings is shown without failing the build, and its lint is off:
        // real designs rarely pass it without waivers.
        command.args(["-Wno-fatal", "-Wno-lint", "-Wno-style"]);
        command.arg("-j").arg(jobs.to_string());
        command.arg("--top-module").arg(top);
        command.arg("--Mdir").arg(dir);
        command.args(flags(profile));
        command.arg("-f").arg(list);
        command.args(&profile.verilator_args);
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

/// What `profile`'s knobs ask of Verilator.
pub fn flags(profile: &Profile) -> Vec<String> {
    let mut flags = Vec::new();
    if profile.opt == Opt::High {
        flags.push("-O3".to_string());
    }
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
