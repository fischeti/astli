#!/usr/bin/env bash
#
# Populates the gitignored `corpus/` with real SystemVerilog to test against,
# and `sv-tests/` with the conformance suite `scripts/sv-tests.py` runs.
# Each repo is pinned, and gets the `astli.toml` and `astli.lock` that
# `scripts/corpus/` holds for it, if any: a manifest describes one commit of
# its repo, so the pin moves on purpose, with the manifest in the same
# commit. Records the commit of each repo in corpus/MANIFEST.
#
#     scripts/fetch-corpus.sh [--sv-tests]
#
# `--sv-tests` fetches the suite alone, as CI does.
#
# Deliberately small. Add repos when there is a question they would answer.

set -euo pipefail

# Each a URL, the commit it is pinned to, and the directory it goes in if
# not the URL's last part.
repos=(
    "https://github.com/pulp-platform/common_cells e73baaec2ca665cd80c3c384e9258e35242b829c"
    "https://github.com/openhwgroup/cva6 742dc82a0c706e0a3bfb98b709fd3ee06d50d12a"
    "https://github.com/lowRISC/ibex a896ef22777a40ad3f5a2a3b97fecd5116480c4c"
    "https://github.com/lowRISC/opentitan 478afb033bb2d3e62cb4c5dd097f578de461b2b5"
    "https://github.com/pulp-platform/axi 70b8e54fd460e3308e58be596ceb3566a6e3576e"
    "https://github.com/pulp-platform/FlooNoC c48b6456079b634ccce94231f619bf5990d18331"
    "https://github.com/pulp-platform/cheshire 6c6dd5e834347f72c7be31cd6c41021ec293c5b9"
    "https://github.com/pulp-platform/iDMA a6b1d6657c03998b6df0baf905d7babd006ba614"
    "https://github.com/pulp-platform/snitch_cluster 8142c0f2da6b08dd60f2169d15fb4079a9f44a76"
    # Accellera's UVM 1.2 as released, which the testbenches of OpenTitan,
    # ibex and cva6 are built against; Accellera publishes it as a tarball.
    "https://github.com/chipsalliance/uvm-verilator c68f035f9c8e965807082495c5a667729afe3aa8 uvm"
)

# Kept out of `corpus/`: its tests are deliberately invalid as often as not,
# and every corpus test holds all of `corpus/` to being real code.
svtests="https://github.com/chipsalliance/sv-tests"
# Pinned too: `scripts/sv-tests.py` lists the tests expected to fail, and CI
# fails when that list goes stale. Move it on purpose.
svtests_rev="c4229f3bd5220e6d3ba8f390e5d09c87e462e9c7"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
corpus="$root/corpus"

# No submodules: cva6 in particular pulls in gigabytes of toolchain and
# verification IP that contain no SystemVerilog we need.
fetch() {
    local url="$1" dir="$2" rev="${3:-HEAD}"
    echo "==> $(basename "$url")"
    if [ ! -d "$dir" ]; then
        git init -q "$dir"
        git -C "$dir" remote add origin "$url"
    fi
    git -C "$dir" fetch --depth 1 origin "$rev"
    git -C "$dir" reset --hard FETCH_HEAD
}

fetch "$svtests" "$root/sv-tests" "$svtests_rev"
# The libraries its tests are tagged with; the rest of its submodules are
# cores and toolchains, gigabytes of them.
git -C "$root/sv-tests" submodule update --init --depth 1 \
    third_party/tests/uvm third_party/tests/uvm-1.2
if [ "${1:-}" = --sv-tests ]; then
    exit 0
fi

mkdir -p "$corpus"
for repo in "${repos[@]}"; do
    read -r url rev name <<<"$repo"
    name="${name:-$(basename "$url")}"
    fetch "$url" "$corpus/$name" "$rev"
    for file in astli.toml astli.lock; do
        if [ -f "$root/scripts/corpus/$name/$file" ]; then
            cp "$root/scripts/corpus/$name/$file" "$corpus/$name/"
        fi
    done
done

# Only UVM's sources: its documentation holds a second copy of them, and its
# examples are testbenches of their own, which clash by name as one.
find "$corpus/uvm" -mindepth 1 -maxdepth 1 ! -name src ! -name .git ! -name 'LICENSE*' \
    -exec rm -rf {} +

{
    echo "# Fetched $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for repo in "${repos[@]}"; do
        read -r url _ name <<<"$repo"
        name="${name:-$(basename "$url")}"
        printf '%s\t%s\t%s\n' "$name" "$(git -C "$corpus/$name" rev-parse HEAD)" "$url"
    done
    printf '%s\t%s\t%s\n' sv-tests "$(git -C "$root/sv-tests" rev-parse HEAD)" "$svtests"
} >"$corpus/MANIFEST"

echo
cat "$corpus/MANIFEST"
echo
echo "SystemVerilog files: $(find "$corpus" \( -name '*.sv' -o -name '*.svh' -o -name '*.v' -o -name '*.vh' \) | wc -l | tr -d ' ')"
