#!/usr/bin/env bash
#
# Builds the documentation site into site/build/, or previews it with
# `scripts/site.sh serve`. Needs uv, and the `usage` CLI for the command
# reference.
#
# The command and lint rule references come from the CLI itself, so they
# describe the checked-out `astli`. That is why the site deploys from a
# release tag.

set -euo pipefail

cd "$(dirname "$0")/../site"

cargo run --quiet -- __usage_spec__ |
    usage generate markdown --file - --out-file content/reference/cli.md

# `lint --list` pads its columns with spaces, and a description has single
# spaces only. A rule's options follow it, each indented, as its default
# setting and what it is.
cargo run --quiet -- lint --list | awk -F '  +' '
    BEGIN {
        print "# Lint rules\n"
        print "Every rule [`astli lint`](../linting.md) has, by group, at the level its"
        print "group gives it, and the [options](#options) a rule takes. `astli lint --list`"
        print "prints the same."
    }
    /^ / {
        options = options sprintf("| `%s` | `%s` | %s |\n", rule, $2, $3)
        next
    }
    $2 != group {
        group = $2
        printf "\n## `%s`\n\nAt `%s` by default.\n\n| Rule | |\n| --- | --- |\n", group, $3
    }
    { rule = $1; printf "| `%s` | %s |\n", $1, $4 }
    END {
        print "\n## Options\n"
        print "What `[lint.rules.<rule>]` in `astli.toml` may set of a rule, at its default."
        print "A value set replaces the default whole.\n"
        print "| Rule | Default | |\n| --- | --- | --- |"
        printf "%s", options
    }
' >content/reference/lint-rules.md

uv run --locked zensical "${1:-build}"
