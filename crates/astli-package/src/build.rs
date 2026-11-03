//! How the root builds a top with a simulator: which simulator, and the
//! profiles a build chooses from.

use std::num::NonZeroU32;

use serde::Deserialize;

/// The simulator a build drives, `[build] simulator`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Simulator {
    #[default]
    Verilator,
}

/// How hard a build optimises the simulation, at the cost of compiling it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Opt {
    /// The simulator's own default, which keeps a build quick.
    Default,
    /// What makes the simulation fastest.
    High,
}

/// What a build under one profile asks of the simulator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub opt: Opt,
    /// Whether the simulation can write waveforms.
    pub trace: bool,
    /// Whether assertions are checked.
    pub assert: bool,
    /// The threads the simulation runs on.
    pub threads: NonZeroU32,
    /// Verilator's own flags, `[profile.NAME.verilator] args`, passed after
    /// every flag astli passes.
    pub verilator_args: Vec<String>,
}

impl Profile {
    /// What a build gets without `--release`: quick to compile, and
    /// waveforms to debug with.
    pub fn dev() -> Profile {
        Profile {
            opt: Opt::Default,
            trace: true,
            assert: true,
            threads: NonZeroU32::MIN,
            verilator_args: Vec::new(),
        }
    }

    /// What a regression or a long simulation is built with: fast to run.
    /// Assertions stay, since they are what a regression checks, and so
    /// does one thread, since more slow a small model down.
    pub fn release() -> Profile {
        Profile {
            opt: Opt::High,
            trace: false,
            ..Profile::dev()
        }
    }
}

/// The profiles a build chooses from, as the root's `[profile]` sets them
/// over their defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profiles {
    pub dev: Profile,
    pub release: Profile,
}

impl Default for Profiles {
    fn default() -> Profiles {
        Profiles {
            dev: Profile::dev(),
            release: Profile::release(),
        }
    }
}

/// The `[build]` table.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct BuildSection {
    pub(crate) simulator: Simulator,
}

/// The `[profile]` table. Only `dev` and `release` are known, so that a
/// misspelt profile is refused rather than never chosen.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ProfileSection {
    dev: Written,
    release: Written,
}

impl ProfileSection {
    pub(crate) fn read(self) -> Profiles {
        Profiles {
            dev: self.dev.over(Profile::dev()),
            release: self.release.over(Profile::release()),
        }
    }
}

/// A `[profile.NAME]` table: what it does not write, the profile's default
/// keeps.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Written {
    opt: Option<Opt>,
    trace: Option<bool>,
    assert: Option<bool>,
    threads: Option<NonZeroU32>,
    verilator: Flags,
}

impl Written {
    fn over(self, default: Profile) -> Profile {
        Profile {
            opt: self.opt.unwrap_or(default.opt),
            trace: self.trace.unwrap_or(default.trace),
            assert: self.assert.unwrap_or(default.assert),
            threads: self.threads.unwrap_or(default.threads),
            verilator_args: self.verilator.args,
        }
    }
}

/// A `[profile.NAME.<simulator>]` table.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Flags {
    args: Vec<String>,
}
