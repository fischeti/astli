#!/usr/bin/env bash
#
# Populates the gitignored `corpus/` with real SystemVerilog to test against,
# and `sv-tests/` with the conformance suite `scripts/sv-tests.py` runs.
# Records the resolved commit of each repo in corpus/MANIFEST so that a
# coverage number can be compared against the one that produced it.
#
#     scripts/fetch-corpus.sh [--sv-tests]
#
# `--sv-tests` fetches the suite alone, as CI does.
#
# Deliberately small. Add repos when there is a question they would answer --
# `uvm-core` once macros are handled.

set -euo pipefail

repos=(
    "https://github.com/pulp-platform/common_cells"
    "https://github.com/openhwgroup/cva6"
    "https://github.com/lowRISC/ibex"
    "https://github.com/lowRISC/opentitan"
    "https://github.com/pulp-platform/axi"
    "https://github.com/pulp-platform/FlooNoC"
    "https://github.com/pulp-platform/cheshire"
    "https://github.com/pulp-platform/iDMA"
    "https://github.com/pulp-platform/snitch_cluster"
)

# Kept out of `corpus/`: its tests are deliberately invalid as often as not,
# and every corpus test holds all of `corpus/` to being real code.
svtests="https://github.com/chipsalliance/sv-tests"
# Pinned, unlike the corpus: `scripts/sv-tests.py` lists the tests expected to
# fail, and CI fails when that list goes stale. Move it on purpose.
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
for url in "${repos[@]}"; do
    fetch "$url" "$corpus/$(basename "$url")"
done

{
    echo "# Resolved $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for url in "${repos[@]}"; do
        name="$(basename "$url")"
        printf '%s\t%s\t%s\n' "$name" "$(git -C "$corpus/$name" rev-parse HEAD)" "$url"
    done
    printf '%s\t%s\t%s\n' sv-tests "$(git -C "$root/sv-tests" rev-parse HEAD)" "$svtests"
} >"$corpus/MANIFEST"

echo
cat "$corpus/MANIFEST"
echo
echo "SystemVerilog files: $(find "$corpus" \( -name '*.sv' -o -name '*.svh' -o -name '*.v' -o -name '*.vh' \) | wc -l | tr -d ' ')"
