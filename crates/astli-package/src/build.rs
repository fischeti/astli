//! How the root builds a top with a simulator: which simulator, and the
//! profiles a build chooses from.

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::time::Duration;

use serde::Deserialize;
use serde::de::{self, Deserializer};
use usage_rs::ValueEnum;

/// The simulator a build drives, `[build] simulator`, or `--sim` in its
/// place.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Simulator {
    /// Verilator 5 or later
    #[default]
    Verilator,
}

impl Simulator {
    /// As the manifest writes it.
    pub fn name(self) -> &'static str {
        match self {
            Simulator::Verilator => "verilator",
        }
    }
}

/// What a build under one profile asks of the simulator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// Whether the simulation can write waveforms.
    pub trace: bool,
    /// Whether assertions are checked.
    pub assert: bool,
    /// The threads the simulation runs on.
    pub threads: NonZeroU32,
    /// Each simulator's own flags, `[profile.NAME.<simulator>] args`,
    /// passed after every flag astli passes.
    pub args: BTreeMap<Simulator, Vec<String>>,
    /// What every run of a top each simulator built gets,
    /// `[profile.NAME.<simulator>] run-args`, before the run's own.
    pub run_args: BTreeMap<Simulator, Vec<String>>,
}

impl Profile {
    /// What a build gets without `--release`: one thread, which a small
    /// model runs fastest on. Nothing is traced, since a waveform is for a
    /// debugging session, not the loop of editing and running again, and
    /// tracing slows every compile and run.
    pub fn dev() -> Profile {
        Profile {
            trace: false,
            assert: true,
            threads: NonZeroU32::MIN,
            args: BTreeMap::new(),
            run_args: BTreeMap::new(),
        }
    }

    /// What a regression or a long simulation is built with: fast to run.
    /// A large model simulates over twice as fast on four threads, which
    /// cost a tenth more to compile. Assertions stay, since they are what a
    /// regression checks.
    pub fn release() -> Profile {
        Profile {
            threads: NonZeroU32::new(4).expect("4 is not zero"),
            ..Profile::dev()
        }
    }

    /// The flags the profile gives `simulator`, none if it gives it none.
    pub fn args_for(&self, simulator: Simulator) -> &[String] {
        self.args.get(&simulator).map_or(&[], Vec::as_slice)
    }

    /// What the profile gives every run of a top `simulator` built.
    pub fn run_args_for(&self, simulator: Simulator) -> &[String] {
        self.run_args.get(&simulator).map_or(&[], Vec::as_slice)
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

/// The `[test]` table: what applies to every test of the package.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tests {
    /// How long one run of a test may take before it is ended and fails,
    /// written in whole seconds; none by default, since no limit fits every
    /// design.
    #[serde(deserialize_with = "seconds")]
    pub timeout: Option<Duration>,
    /// How many random seeds each test runs under, in place of the one its
    /// name gives; none by default, so that a rerun is the same run.
    pub seeds: Option<NonZeroU32>,
}

/// A number of seconds, more than none.
pub(crate) fn seconds<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Duration>, D::Error> {
    match u64::deserialize(deserializer)? {
        0 => Err(de::Error::invalid_value(
            de::Unexpected::Unsigned(0),
            &"a number of seconds, more than none",
        )),
        seconds => Ok(Some(Duration::from_secs(seconds))),
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
    trace: Option<bool>,
    assert: Option<bool>,
    threads: Option<NonZeroU32>,
    verilator: Flags,
}

impl Written {
    fn over(self, default: Profile) -> Profile {
        let mut profile = Profile {
            trace: self.trace.unwrap_or(default.trace),
            assert: self.assert.unwrap_or(default.assert),
            threads: self.threads.unwrap_or(default.threads),
            ..default
        };
        // Only what is written, so that a profile is its default until then.
        let Flags { args, run_args } = self.verilator;
        if !args.is_empty() {
            profile.args.insert(Simulator::Verilator, args);
        }
        if !run_args.is_empty() {
            profile.run_args.insert(Simulator::Verilator, run_args);
        }
        profile
    }
}

/// A `[profile.NAME.<simulator>]` table.
#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct Flags {
    args: Vec<String>,
    run_args: Vec<String>,
}
