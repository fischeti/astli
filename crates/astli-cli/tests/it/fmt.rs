//! `fmt`.

use std::path::Path;

use crate::support::*;

#[test]
fn fmt_writes_a_file_in_place() {
    let fixture = Fixture::new("fmt-write");
    let file = fixture.file("tiny.sv", &TINY.replace("  assign", "      assign"));

    let output = driver(&fixture).args(["fmt".as_ref(), file.as_os_str()]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), TINY);
}

#[test]
fn fmt_check_counts_the_files_not_formatted_in_agreement() {
    let fixture = Fixture::new("fmt-check-count");
    let tidy = fixture.file("tidy.sv", TINY);
    let messy = TINY.replace("  assign", "      assign");
    let one = fixture.file("one.sv", &messy);
    let two = fixture.file("two.sv", &messy);

    let clean = driver(&fixture).args(["fmt".as_ref(), "--check".as_ref(), tidy.as_os_str()]);
    let single = driver(&fixture).args(["fmt".as_ref(), "--check".as_ref(), one.as_os_str()]);
    let both = driver(&fixture).args([
        "fmt".as_ref(),
        "--check".as_ref(),
        one.as_os_str(),
        two.as_os_str(),
    ]);

    assert!(clean.status.success(), "{}", stderr(&clean));
    assert!(stdout(&clean).is_empty(), "{}", stdout(&clean));
    let said = stderr(&single);
    assert!(said.contains("1 of 1 file is not formatted"), "{said}");
    let said = stderr(&both);
    assert!(said.contains("2 of 2 files are not formatted"), "{said}");
}

#[test]
fn fmt_diff_is_a_patch_that_formats_the_file() {
    let fixture = Fixture::new("fmt-diff");
    let tidy = fixture.file("tidy.sv", TINY);
    let messy = fixture.file("messy.sv", &TINY.replace("  assign", "      assign"));

    let output = driver(&fixture).args([
        "fmt".as_ref(),
        "--diff".as_ref(),
        tidy.as_os_str(),
        messy.as_os_str(),
    ]);

    assert_eq!(output.status.code(), Some(1));
    let messy = messy.display();
    assert_eq!(
        stdout(&output),
        format!(
            "\
--- {messy}
+++ {messy}
@@ -4,5 +4,5 @@
   input  logic [Width-1:0] a_i,
   output logic [Width-1:0] z_o
 );
-      assign z_o = ~a_i;
+  assign z_o = ~a_i;
 endmodule
"
        )
    );
}

#[test]
fn fmt_keeps_what_it_did_not_parse_without_a_word() {
    let fixture = Fixture::new("fmt-keeps-what-it-did-not-parse-without-a-word");
    let output = driver(&fixture).stdin(UNPARSED).args(["fmt", "-"]);

    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), UNPARSED);
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
}

#[test]
fn fmt_reports_a_file_that_ends_inside_a_module() {
    let fixture = Fixture::new("fmt-reports-a-file-that-ends-inside-a-module");
    let output = driver(&fixture)
        .stdin("module m;\n  logic q;\n")
        .args(["fmt", "-"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let said = stderr(&output);
    assert!(said.contains("unclosed-at-end-of-file"), "{said}");
    assert!(!said.contains("not-parsed"), "{said}");
}

#[test]
fn fmt_formats_stdin_to_stdout() {
    let fixture = Fixture::new("fmt-formats-stdin-to-stdout");
    let messy = TINY.replace("  assign", "      assign");

    let output = driver(&fixture).stdin(&messy).args(["fmt", "-"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), TINY);

    let output = driver(&fixture).stdin(&messy).args(["fmt", "--check", "-"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output), "<stdin>\n");
}

#[test]
fn fmt_formats_stdin_on_its_own() {
    let fixture = Fixture::new("fmt-stdin-alone");
    let file = fixture.file("tiny.sv", TINY);

    let output =
        driver(&fixture)
            .stdin(TINY)
            .args(["fmt".as_ref(), "-".as_ref(), file.as_os_str()]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
}

#[test]
fn fmt_takes_no_define_so_that_its_output_depends_on_the_file_alone() {
    let fixture = Fixture::new("fmt-takes-no-define-so-that-its-output-depends-on-the-file-alone");
    let output = driver(&fixture).args(["fmt", "-D", "X", "anything.sv"]);

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn astli_toml_sets_the_layout_found_from_a_directory_below_it() {
    let fixture = Fixture::new("fmt-config");
    fixture.file("astli.toml", "[fmt]\nindent = 4\n");
    fixture.file("rtl/m.sv", "module m;\nlogic q;\nendmodule\n");

    let output = driver(&fixture)
        .dir(fixture.path().join("rtl"))
        .args(["fmt", "m.sv"]);

    assert!(output.status.success(), "{}", stderr(&output));
    let written = std::fs::read_to_string(fixture.path().join("rtl/m.sv")).unwrap();
    assert_eq!(written, "module m;\n    logic q;\nendmodule\n");
}

#[test]
fn a_file_astli_toml_excludes_is_left_alone_named_or_found() {
    let fixture = Fixture::new("fmt-config-exclude");
    let config = "[fmt]\nexclude = [\"vendor\", \"rtl/*_old.sv\"]\n";
    fixture.file("astli.toml", config);
    let unformatted = "module m;\nlogic q;\nendmodule\n";
    fixture.file("vendor/ip/m.sv", unformatted);
    fixture.file("rtl/m_old.sv", unformatted);
    fixture.file("rtl/m.sv", unformatted);

    let found = driver(&fixture).args(["fmt", "--check"]);
    let named = driver(&fixture)
        .dir(fixture.path().join("rtl"))
        .args(["fmt", "--check", "m_old.sv"]);

    assert!(!found.status.success());
    let checked = stdout(&found);
    assert_eq!(checked.lines().count(), 1, "{checked}");
    assert!(Path::new(checked.trim()).ends_with("rtl/m.sv"), "{checked}");
    assert!(named.status.success(), "{}", stderr(&named));
    assert!(stdout(&named).is_empty(), "{}", stdout(&named));
}

#[test]
fn fmt_with_nothing_named_formats_headers_too() {
    let fixture = Fixture::new("discover-fmt");
    package(&fixture);

    let output = driver(&fixture).args(["fmt", "--check"]);

    assert!(!output.status.success());
    let listed = stdout(&output);
    let last = listed.lines().last().unwrap_or_default();
    assert!(
        Path::new(last).ends_with("rtl/gen/regs/defs.svh"),
        "{listed}"
    );
}
