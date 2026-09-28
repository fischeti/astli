#!/usr/bin/env bash
#
# Builds the documentation site into site/build/, or previews it with
# `scripts/site.sh serve`. Needs uv, and the `usage` CLI for the command
# reference.
#
# The reference comes from the CLI's own spec, so it describes the checked-out
# `astli`. That is why the site deploys from a release tag.

set -euo pipefail

cd "$(dirname "$0")/../site"

cargo run --quiet -- __usage_spec__ |
    usage generate markdown --file - --out-file content/reference/cli.md

uv run --locked zensical "${1:-build}"
