//! A test of its own binary, since it sets the process's environment, which
//! another test running beside it could be reading.

use astli_testing::{Fixture, REPOSITORY_ENV, git};

/// Run from a hook or a `rebase --exec`, `git` inherits `GIT_DIR` naming the
/// repository it was started in, where `git init` elsewhere would reinitialise
/// it as a bare repository.
#[test]
fn git_reaches_only_the_directory_it_runs_in() {
    let fixture = Fixture::new("git");
    let config = fixture.file("gitconfig", "");
    let outer = fixture.dir("outer");
    let inner = fixture.dir("inner");
    let run = |dir, args: &[&str]| {
        let output = git(dir, &config).args(args).output().expect("git runs");
        assert!(output.status.success(), "{output:?}");
    };
    run(&outer, &["init", "--quiet"]);
    let before = std::fs::read_to_string(outer.join(".git/config")).expect("a config");
    // SAFETY: the only test of this binary, so nothing reads the environment
    // while it is written.
    unsafe {
        for name in REPOSITORY_ENV {
            std::env::set_var(name, outer.join(".git"));
        }
        std::env::set_var("GIT_WORK_TREE", &outer);
    }

    run(&inner, &["init", "--quiet"]);
    run(
        &inner,
        &["commit", "--quiet", "--allow-empty", "--message", "one"],
    );
    run(&inner, &["tag", "v1.0.0"]);

    let after = std::fs::read_to_string(outer.join(".git/config")).expect("a config");
    assert_eq!(after, before);
    assert!(inner.join(".git/refs/tags/v1.0.0").is_file());
    assert!(!outer.join(".git/refs/tags/v1.0.0").exists());
}
