#!/bin/sh
# Installs the latest release of astli. Hands off to the installer the
# release itself carries, so this file needs no change from one release to the
# next. Arguments pass through to it: `sh -s -- --help` lists them.
set -eu
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/fischeti/astli/releases/latest/download/astli-cli-installer.sh | sh -s -- "$@"
