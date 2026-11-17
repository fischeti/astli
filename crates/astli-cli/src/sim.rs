//! The simulator a build drives, behind one type: each step of a build or a
//! run that differs between simulators is a `match` here, so that another
//! is a variant the compiler points at each of them for. Supervising a
//! simulation once it is built is the same for every one, and is here too.

use std::ffi::OsString;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use astli_package::{Profile, Simulator};

use crate::cmd::build::Listed;
use crate::error::{Error, Result};
use crate::term;
use crate::verilator::{self, Verilator};

/// How long a run goes before it is said to be still running.
pub(crate) const SLOW: Duration = Duration::from_secs(60);

/// A simulator found on this machine.
#[derive(Clone)]
pub(crate) enum Backend {
    Verilator(Verilator),
}

impl Backend {
    /// Finds `simulator`, refusing a version too old to build with.
    pub(crate) fn find(simulator: Simulator) -> Result<Backend> {
        match simulator {
            Simulator::Verilator => Verilator::find().map(Backend::Verilator),
        }
    }

    /// Which simulator this is.
    pub(crate) fn simulator(&self) -> Simulator {
        match self {
            Backend::Verilator(_) => Simulator::Verilator,
        }
    }

    /// The simulator's name, as its makers write it.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Backend::Verilator(_) => "Verilator",
        }
    }

    /// The simulator's version, as it prints it.
    pub(crate) fn version(&self) -> &str {
        match self {
            Backend::Verilator(verilator) => &verilator.version,
        }
    }

    /// What the simulator is given to build the top `listed` lists into
    /// `dir`, as `profile` asks: everything that decides what it builds, so
    /// the jobs it builds on are not among them.
    pub(crate) fn args(&self, listed: &Listed, dir: &Path, profile: &Profile) -> Vec<OsString> {
        match self {
            Backend::Verilator(verilator) => {
                verilator.args(&list(dir), dir, &listed.module, &listed.params, profile)
            }
        }
    }

    /// Builds the top `listed` lists into `dir` with `args`, from the
    /// directory `at`, on `jobs` threads, or all of them for 0. The file
    /// list is written first, into `dir`, where it is kept.
    pub(crate) fn build(
        &self,
        listed: &Listed,
        dir: &Path,
        args: &[OsString],
        at: &Path,
        jobs: usize,
    ) -> Result {
        let list = list(dir);
        match self {
            Backend::Verilator(verilator) => {
                // Before anything of this build is written, which would be
                // newest.
                verilator::after_last_build(dir);
                std::fs::write(&list, &listed.text).map_err(|err| Error::io(&list, err))?;
                verilator.build(args, at, &listed.name, jobs)
            }
        }
    }

    /// Where a build of the module `top` into `dir` leaves its executable.
    pub(crate) fn executable(&self, dir: &Path, top: &str) -> PathBuf {
        match self {
            Backend::Verilator(_) => verilator::executable(dir, top),
        }
    }

    /// What a run of a test of the package rooted at `root` is given first,
    /// to run under `seed`: where its package is, and the seed.
    pub(crate) fn run_args(&self, root: &Path, seed: u32) -> Vec<OsString> {
        let root = OsString::from(format!("+astli_root={}", root.display()));
        match self {
            Backend::Verilator(_) => vec![root, format!("+verilator+seed+{seed}").into()],
        }
    }

    /// The largest seed a run takes: one less than a power of two, so that
    /// a seed drawn from a hash is masked to it.
    pub(crate) fn max_seed(&self) -> u32 {
        match self {
            Backend::Verilator(_) => verilator::MAX_SEED,
        }
    }
}

/// Where a build into `dir` keeps the file list it reads.
fn list(dir: &Path) -> PathBuf {
    dir.join("files.f")
}

/// How a run ended.
pub(crate) enum Ended {
    Status(ExitStatus),
    /// It ran for so long and was killed.
    TimedOut(Duration),
}

/// Runs the simulation `executable` in `dir`, emptied first, with its
/// output in `log`, given `args`, and killed past `timeout`; `slow` is
/// called once it has run for longer than [`SLOW`].
pub(crate) fn simulated(
    executable: &Path,
    dir: &Path,
    log: &Path,
    args: &[OsString],
    timeout: Option<Duration>,
    slow: impl Fn(),
) -> Result<Ended> {
    match std::fs::remove_dir_all(dir) {
        Err(err) if err.kind() != ErrorKind::NotFound => return Err(Error::io(dir, err)),
        _ => {}
    }
    std::fs::create_dir_all(dir).map_err(|err| Error::io(dir, err))?;
    let file = File::create(log).map_err(|err| Error::io(log, err))?;
    let copy = file.try_clone().map_err(|err| Error::io(log, err))?;
    let mut child = Command::new(executable)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(copy)
        .stderr(file)
        .spawn()
        .map_err(|err| Error::io(executable, err))?;
    let started = Instant::now();
    let mut told = false;
    loop {
        if let Some(status) = child.try_wait().map_err(|err| Error::io(executable, err))? {
            return Ok(Ended::Status(status));
        }
        let elapsed = started.elapsed();
        if let Some(timeout) = timeout.filter(|it| elapsed >= *it) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Ended::TimedOut(timeout));
        }
        if !told && elapsed >= SLOW {
            told = true;
            slow();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Why a simulation that ended with `status` failed.
pub(crate) fn ended_by(status: ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("the simulation exited with status {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("the simulation was ended by signal {signal}");
        }
    }
    format!("the simulation ended without a status: {status}")
}

/// How a run ends whose simulation a signal ended: with the status a shell
/// gives it, 128 and the signal, and only a note, since Verilator before
/// 5.040 ends a `$fatal` or a `$stop` by aborting, after saying why.
#[cfg(unix)]
pub(crate) fn signalled(status: ExitStatus) -> Error {
    use std::os::unix::process::ExitStatusExt;

    let signal = status.signal().unwrap_or(0);
    term::note(format!("the simulation was ended by signal {signal}")).report();
    Error::Exited(u8::try_from(128 + signal).unwrap_or(u8::MAX))
}

/// Every process on Windows ends with a status.
#[cfg(not(unix))]
pub(crate) fn signalled(status: ExitStatus) -> Error {
    Error::failed(format!("the simulation ended without a status: {status}"))
}

/// Writes a run's output to stdout, where a simulation's goes, in one
/// piece, so that runs in parallel do not interleave.
pub(crate) fn shown_output(output: &[u8]) {
    term::suspend(|| {
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(output);
        // What follows, a status or the steps drawn again, starts a line.
        if output.last().is_some_and(|last| *last != b'\n') {
            let _ = stdout.write_all(b"\n");
        }
        let _ = stdout.flush();
    });
}
