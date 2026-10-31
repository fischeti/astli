//! Fetching from a real host, for the tests named `network_*`.
//!
//! Every other test fetches from repositories it makes on disk, so none of
//! them would notice HTTPS breaking: its TLS is configured through features of
//! crates `gix` depends on, which an upgrade of `gix` can quietly leave
//! behind. Only CI runs these, since they need the network.

use crate::common::Fixture;

#[test]
fn network_a_mirror_fetches_over_https() {
    let fixture = Fixture::new("https");
    let url = "https://github.com/pulp-platform/common_verification";
    let mirror = (fixture.store().mirror("common_verification", url)).expect("a mirror over HTTPS");
    let versions = mirror.versions().expect("its versions");
    assert!(
        versions
            .iter()
            .any(|(version, _)| version.to_string() == "0.2.5"),
        "{url} has a v0.2.5 tag"
    );
}
