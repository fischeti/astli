#!/usr/bin/env bash
#
# Populates the gitignored `corpus/` with real SystemVerilog to test against.
# Records the resolved commit of each repo in corpus/MANIFEST so that a
# coverage number can be compared against the one that produced it.
#
# Deliberately small. Add repos when there is a question they would answer --
# `uvm-core` once macros are handled, `opentitan` once conditionals are.

set -euo pipefail

repos=(
    "https://github.com/pulp-platform/common_cells"
    "https://github.com/openhwgroup/cva6"
    "https://github.com/lowRISC/ibex"
)

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
corpus="$root/corpus"
mkdir -p "$corpus"

for url in "${repos[@]}"; do
    name="$(basename "$url")"
    dir="$corpus/$name"
    if [ -d "$dir" ]; then
        echo "==> $name: updating"
        git -C "$dir" fetch --depth 1 origin HEAD
        git -C "$dir" reset --hard FETCH_HEAD
    else
        echo "==> $name: cloning"
        # No submodules: cva6 in particular pulls in gigabytes of toolchain
        # and verification IP that contain no SystemVerilog we need.
        git clone --depth 1 "$url" "$dir"
    fi
done

{
    echo "# Resolved $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for url in "${repos[@]}"; do
        name="$(basename "$url")"
        printf '%s\t%s\t%s\n' "$name" "$(git -C "$corpus/$name" rev-parse HEAD)" "$url"
    done
} >"$corpus/MANIFEST"

echo
cat "$corpus/MANIFEST"
echo
echo "SystemVerilog files: $(find "$corpus" \( -name '*.sv' -o -name '*.svh' -o -name '*.v' -o -name '*.vh' \) | wc -l | tr -d ' ')"
