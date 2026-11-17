//! Building a top with Verilator: finding it, the flags a profile turns
//! into, and what its runs take.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use astli_package::{Profile, Simulator, mark};

use crate::error::{Error, Result};
use crate::term;

/// The first major version that builds a testbench alone, with `--binary`
/// and `--timing`.
const OLDEST: u32 = 5;

/// The first version that takes `--quiet`, 5.024; an older one prints its
/// reports whatever it is given.
const QUIET: (u32, u32) = (5, 24);

/// The largest seed Verilator takes.
pub const MAX_SEED: u32 = i32::MAX as u32;

/// The Verilator on the `PATH`.
#[derive(Clone)]
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
    pub fn args(
        &self,
        list: &Path,
        dir: &Path,
        top: &str,
        params: &[(String, String)],
        profile: &Profile,
    ) -> Vec<OsString> {
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
        args.extend((params.iter()).map(|(name, value)| format!("-G{name}={value}").into()));
        args.extend(["--Mdir".into(), dir.into()]);
        args.extend(flags(profile).into_iter().map(OsString::from));
        args.extend(["-f".into(), list.into()]);
        args.extend((profile.args_for(Simulator::Verilator).iter()).map(OsString::from));
        args
    }

    /// Builds `top` with `args` in the directory `at`, on `jobs` threads, or
    /// all of them for 0.
    pub fn build(&self, args: &[OsString], at: &Path, top: &str, jobs: usize) -> Result {
        let mut command = Command::new("verilator");
        // Where the manifest is, so that a relative path in a profile's
        // `args` is read from there, wherever astli was started.
        command.current_dir(at);
        command.args(args).arg("-j").arg(jobs.to_string());
        // What compiled before is compiled again from the cache, where one is
        // installed and Verilator was not told of it when it was built.
        if std::env::var_os("OBJCACHE").is_none() && on_path("ccache") {
            command.env("OBJCACHE", "ccache");
        }
        // Standard output holds a command's results, and what Verilator
        // writes is about the run. It is written through `term`, a line at
        // a time, since written straight to stderr it would cut into a
        // spinner drawn there.
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = (command.spawn())
            .map_err(|err| Error::failed(format!("cannot run Verilator: {err}")))?;
        let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
        std::thread::scope(|scope| {
            scope.spawn(|| forwarded(stdout));
            forwarded(stderr);
        });
        let status =
            (child.wait()).map_err(|err| Error::failed(format!("cannot run Verilator: {err}")))?;
        match status.success() {
            true => Ok(()),
            false => Err(Error::failed(format!(
                "Verilator could not build {}",
                mark::name(top)
            ))),
        }
    }
}

/// Writes what `pipe` carries to stderr through `term`, a line at a time,
/// until it closes.
fn forwarded(pipe: Option<impl Read>) {
    let Some(pipe) = pipe else {
        return;
    };
    let mut lines = BufReader::new(pipe);
    let mut line = Vec::new();
    while lines
        .read_until(b'\n', &mut line)
        .is_ok_and(|read| read > 0)
    {
        if !line.ends_with(b"\n") {
            line.push(b'\n');
        }
        let _ = term::write(&line);
        line.clear();
    }
}

/// Where a build of the module `top` into `dir` leaves its executable.
pub fn executable(dir: &Path, top: &str) -> PathBuf {
    dir.join(format!("V{top}"))
}

/// Waits until the clock is past the second in which the newest file in
/// `dir` was written. A Make that compares times in whole seconds, as the
/// 3.81 macOS ships does, takes an object written in the same second as
/// the source Verilator then rewrites for current, and links the old
/// model. Deleting the objects would serve any Make too, but compile the
/// whole model again wherever no compiler cache is installed.
pub fn after_last_build(dir: &Path) {
    let newest = (std::fs::read_dir(dir).into_iter().flatten().flatten())
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max();
    let Some(since) = newest.and_then(|it| it.duration_since(UNIX_EPOCH).ok()) else {
        return;
    };
    let next = UNIX_EPOCH + Duration::from_secs(since.as_secs() + 1);
    if let Ok(wait) = next.duration_since(SystemTime::now()) {
        std::thread::sleep(wait);
    }
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
