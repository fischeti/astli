//! What resolution chooses between: commits, and finite sets of them.
//!
//! A version requirement admits the tagged commits whose versions it
//! matches, and a `rev` admits one commit, so every set a requirement makes
//! is a list of commits. Sets over such lists are exact, and a commit
//! without a tag is compatible only with itself.

use std::collections::BTreeSet;
use std::fmt;

use astli_package::mark;
use gix::ObjectId;
use semver::Version;

/// One version of a package.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Candidate {
    /// What is on disk: the root, or a package a path names.
    Local,
    /// A commit, and the highest version its tags give it. Ordered by
    /// version, an untagged commit lowest, so that the highest is the one
    /// to prefer.
    Commit {
        version: Option<Version>,
        id: ObjectId,
    },
}

impl fmt::Display for Candidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Nothing to tell apart: what is on disk is the one version.
            Candidate::Local => Ok(()),
            Candidate::Commit {
                version: Some(version),
                ..
            } => write!(f, "{}", mark::version(version)),
            Candidate::Commit { version: None, id } => {
                write!(f, "{}", mark::version(id.to_hex_with_len(10)))
            }
        }
    }
}

/// The candidates in `members`, or all but those.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Candidates {
    all_but: bool,
    members: BTreeSet<Candidate>,
}

impl Candidates {
    /// Exactly `members`.
    pub fn of(members: impl IntoIterator<Item = Candidate>) -> Candidates {
        Candidates {
            all_but: false,
            members: members.into_iter().collect(),
        }
    }

    /// The highest of `known` the set holds.
    pub fn highest<'a>(
        &'a self,
        known: impl IntoIterator<Item = &'a Candidate>,
    ) -> Option<&'a Candidate> {
        match self.all_but {
            false => self.members.last(),
            true => (known.into_iter())
                .filter(|candidate| !self.members.contains(*candidate))
                .max(),
        }
    }

    /// How many it holds, where that is known: a measure of how constrained
    /// a package is, to settle the most constrained first.
    pub fn len(&self) -> Option<usize> {
        (!self.all_but).then_some(self.members.len())
    }
}

impl pubgrub::VersionSet for Candidates {
    type V = Candidate;

    fn empty() -> Candidates {
        Candidates::default()
    }

    fn singleton(candidate: Candidate) -> Candidates {
        Candidates::of([candidate])
    }

    fn complement(&self) -> Candidates {
        Candidates {
            all_but: !self.all_but,
            members: self.members.clone(),
        }
    }

    fn intersection(&self, other: &Candidates) -> Candidates {
        let members = match (self.all_but, other.all_but) {
            (false, false) => self.members.intersection(&other.members).cloned().collect(),
            (false, true) => self.members.difference(&other.members).cloned().collect(),
            (true, false) => other.members.difference(&self.members).cloned().collect(),
            (true, true) => self.members.union(&other.members).cloned().collect(),
        };
        Candidates {
            all_but: self.all_but && other.all_but,
            members,
        }
    }

    fn contains(&self, candidate: &Candidate) -> bool {
        self.members.contains(candidate) != self.all_but
    }
}

impl fmt::Display for Candidates {
    /// The highest few, as alternatives: `1.1.0 or 1.0.0`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const SHOWN: usize = 4;
        if self.members.is_empty() {
            return write!(
                f,
                "{}",
                if self.all_but {
                    "any version"
                } else {
                    "no version"
                }
            );
        }
        if self.all_but {
            write!(f, "any version but ")?;
        }
        let mut shown: Vec<String> = (self.members.iter().rev().take(SHOWN))
            .map(Candidate::to_string)
            .collect();
        if self.members.len() > SHOWN {
            shown.push(format!("{} older", self.members.len() - SHOWN));
        }
        match shown.split_last() {
            Some((last, [])) => write!(f, "{last}"),
            Some((last, rest)) => write!(f, "{} or {last}", rest.join(", ")),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use gix::ObjectId;
    use pubgrub::VersionSet;
    use semver::Version;

    use super::{Candidate, Candidates};

    fn at(version: Option<&str>, byte: u8) -> Candidate {
        Candidate::Commit {
            version: version.map(|it| Version::parse(it).unwrap()),
            id: ObjectId::from_bytes_or_panic(&[byte; 20]),
        }
    }

    #[test]
    fn sets_combine_as_sets_do() {
        let (a, b, c) = (at(Some("1.0.0"), 1), at(Some("1.1.0"), 2), at(None, 3));
        let ab = Candidates::of([a.clone(), b.clone()]);
        let bc = Candidates::of([b.clone(), c.clone()]);

        assert_eq!(ab.intersection(&bc), Candidates::singleton(b.clone()));
        assert_eq!(
            ab.intersection(&bc.complement()),
            Candidates::singleton(a.clone())
        );
        assert_eq!(
            ab.union(&bc),
            Candidates::of([a.clone(), b.clone(), c.clone()])
        );
        assert!(ab.complement().contains(&c) && !ab.complement().contains(&a));
        assert_eq!(ab.intersection(&ab.complement()), Candidates::empty());
    }

    #[test]
    fn the_highest_version_is_preferred_and_an_untagged_commit_least() {
        let (a, b, c) = (at(Some("1.0.0"), 1), at(Some("1.1.0"), 2), at(None, 3));
        let known = [a.clone(), b.clone(), c.clone()];

        assert_eq!(Candidates::of(known.clone()).highest(&known), Some(&b));
        assert_eq!(
            Candidates::singleton(b).complement().highest(&known),
            Some(&a)
        );
        assert_eq!(Candidates::singleton(c.clone()).highest(&known), Some(&c));
    }
}
