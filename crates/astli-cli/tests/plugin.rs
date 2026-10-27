//! The Claude Code plugin shares the crates' version: its users fetch a new
//! skill only when the version changes, so it has to move with each release.

use std::path::Path;

#[test]
fn the_plugin_has_the_version_of_the_release() {
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin/.claude-plugin/plugin.json");
    let text = std::fs::read_to_string(&manifest).expect("the plugin manifest");
    let version = format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION"));
    assert!(
        text.contains(&version),
        "{} should say {version}; the release-pr workflow writes it",
        manifest.display()
    );
}
