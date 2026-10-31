//! PULP's EDA benchmarks: each variant checked from its top, over the
//! filelist it comes with, and what that prints compared with
//! `scripts/eda-benchmarks/check.txt`.
//!
//! `eda-benchmarks/` is gitignored and fetched by `scripts/fetch-corpus.sh`;
//! without it the test says so and passes. A changed `check.txt` is accepted
//! by running
//!
//! ```text
//! UPDATE_EXPECT=1 cargo nextest run --release -p astli-cli corpus_benchmarks
//! ```
//!
//! and the diff is the review: every error it holds is one `slang` reports
//! too, without the flags the benchmarks' own flow passes it.

use std::path::Path;
use std::process::Command;

use expect_test::expect_file;

#[test]
fn corpus_benchmarks_check_as_expected() {
    // Canonical, as the paths `astli` prints are.
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workspace = workspace.canonicalize().unwrap_or(workspace);
    let benchmarks = workspace.join("eda-benchmarks");
    let Ok(targets) = std::fs::read_to_string(benchmarks.join("targets.mk")) else {
        eprintln!("skipping: run scripts/fetch-corpus.sh to populate eda-benchmarks/");
        return;
    };

    let root = format!("{}/", benchmarks.display());
    let mut printed = String::new();
    // `<design>.<variant>.TOP := <top>`, one line each.
    for (target, top) in targets.lines().filter_map(|line| {
        let (target, top) = line.split_once(".TOP :=")?;
        Some((target.trim(), top.trim()))
    }) {
        let (design, variant) = target.split_once('.').expect("<design>.<variant>");
        let filelist = benchmarks.join(design).join("variants").join(variant);
        let output = Command::new(env!("CARGO_BIN_EXE_astli"))
            .args(["--diag", "short", "check", "--top", top, "-F"])
            .arg(filelist.join("filelist.f"))
            .output()
            .expect("the driver runs");
        printed +=
            &format!("$ astli check --top {top} -F {design}/variants/{variant}/filelist.f\n");
        for stream in [&output.stdout, &output.stderr] {
            for line in String::from_utf8_lossy(stream).lines() {
                printed += &line.replace(&root, "");
                printed.push('\n');
            }
        }
    }
    assert!(!printed.is_empty(), "no `.TOP :=` in targets.mk");
    expect_file![workspace.join("scripts/eda-benchmarks/check.txt")].assert_eq(&printed);
}
