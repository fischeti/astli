//! The JUnit report `astli test --junit` writes.

use std::time::Duration;

/// The test cases of one package's test suite in a JUnit report, written.
#[derive(Default)]
pub(super) struct Suite {
    /// The package's name, which is also each case's class: how most CI
    /// views group the cases of a report.
    name: String,
    cases: String,
    tests: usize,
    failures: usize,
    errors: usize,
    skipped: usize,
    time: Duration,
}

/// How a test case of a JUnit report went.
pub(super) enum Verdict<'a> {
    Passed,
    /// It failed for this reason, and wrote this.
    Failed(&'a str, &'a [u8]),
    /// It was never built, for this reason.
    Error(&'a str),
    Skipped(&'a str),
}

impl Suite {
    /// The suite of the package called `name`, of no case yet.
    pub(super) fn new(name: String) -> Suite {
        Suite {
            name,
            ..Suite::default()
        }
    }

    pub(super) fn case(&mut self, name: &str, time: Duration, verdict: Verdict) {
        self.tests += 1;
        self.time += time;
        let opened = format!(
            "    <testcase name=\"{}\" classname=\"{}\" time=\"{:.3}\"",
            escaped(name),
            escaped(&self.name),
            time.as_secs_f64()
        );
        let body = match verdict {
            Verdict::Passed => {
                self.cases.push_str(&format!("{opened}/>\n"));
                return;
            }
            Verdict::Failed(why, output) => {
                self.failures += 1;
                let output = String::from_utf8_lossy(output);
                format!(
                    "      <failure message=\"{}\">{}</failure>\n",
                    escaped(why),
                    escaped(&output)
                )
            }
            Verdict::Error(why) => {
                self.errors += 1;
                format!("      <error message=\"{}\"/>\n", escaped(why))
            }
            Verdict::Skipped(why) => {
                self.skipped += 1;
                format!("      <skipped message=\"{}\"/>\n", escaped(why))
            }
        };
        self.cases
            .push_str(&format!("{opened}>\n{body}    </testcase>\n"));
    }
}

/// A JUnit report of `suites`. Written by hand, as it is a few elements,
/// and their text and attributes escaped.
pub(super) fn junit(suites: &[Suite]) -> String {
    let sum = |of: fn(&Suite) -> usize| suites.iter().map(of).sum::<usize>();
    let time: Duration = suites.iter().map(|suite| suite.time).sum();
    let mut text = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <testsuites name=\"astli test\" tests=\"{}\" failures=\"{}\" errors=\"{}\" \
         skipped=\"{}\" time=\"{:.3}\">\n",
        sum(|it| it.tests),
        sum(|it| it.failures),
        sum(|it| it.errors),
        sum(|it| it.skipped),
        time.as_secs_f64()
    );
    for suite in suites {
        text.push_str(&format!(
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" \
             skipped=\"{}\" time=\"{:.3}\">\n{}  </testsuite>\n",
            escaped(&suite.name),
            suite.tests,
            suite.failures,
            suite.errors,
            suite.skipped,
            suite.time.as_secs_f64(),
            suite.cases
        ));
    }
    text.push_str("</testsuites>\n");
    text
}

/// `text` as XML text or an attribute's value: what would end either
/// escaped, and what XML 1.0 cannot hold at all, as a simulation's output
/// may, replaced.
fn escaped(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\t' | '\n' | '\r' => escaped.push(c),
            c if c < ' ' || c == '\u{fffe}' || c == '\u{ffff}' => escaped.push('\u{fffd}'),
            c => escaped.push(c),
        }
    }
    escaped
}
