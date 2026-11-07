#!/usr/bin/env bash
# Fails when the checked-out commit breaks the library API of the last
# release without the pull request declaring it: a `!` after the type or
# scope of its title or of one of its commit subjects, or a
# `BREAKING CHANGE:` footer.
#
#   scripts/semver-checks.sh <base>
#
# <base> is the branch the pull request merges into; its commits are those
# since the merge base with it. PR_TITLE, when set, is the pull request's
# title.
#
# cargo-semver-checks runs twice, against the merge base and against the
# last release tag, and only what both report counts. A break against the
# merge base alone is in something no release has shipped; against the tag
# alone, it is already on <base>, declared by the pull request that put it
# there.
set -euo pipefail

base=${1:?usage: scripts/semver-checks.sh <base>}
merge_base=$(git merge-base "$base" HEAD)
tag=$(git tag --list 'v*' --sort=-v:refname | head -1)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

# The breaks a run reports, one `crate lint: item` per line, sorted. This
# version has no machine-readable output, so this relies on the layout it
# prints: `Checking <crate> v...` before each crate's results, then for each
# failing lint a line `--- failure <lint>: <summary> ---` and, after a
# `Failed in:` line, one indented line per item until a blank one. An item
# line names the item, then where it is: ` in /...`, `, in /...` or
# `, previously in file /...`. Where is a path into either side of the
# comparison, so it differs between the runs and is cut off; the rest names
# the item the same way in both, unless <base> already changed it since the
# tag, which is then a break the release has anyway.
breaks() {
    awk '
        $1 == "Checking" { crate = $2; next }
        /^--- failure [^:]+:/ { lint = $3; sub(/:$/, "", lint); next }
        /^Failed in:/ { items = 1; next }
        items && /^[[:space:]]*$/ { items = 0; next }
        items {
            item = $0
            sub(/^[[:space:]]+/, "", item)
            sub(/,? (previously )?(in|at) (file )?\/.*$/, "", item)
            print crate " " lint ": " item
        }
    ' "$scratch/$1.log" | sort -u
}

# The command and the crates that serve only it, which promise no API. A
# break fails the run without an `error:` line; anything else that fails it,
# such as a crate that does not build on one side, fails the job, since a
# crate that was not checked could hide a break.
semver_checks() {
    local status=0
    cargo semver-checks --workspace \
        --exclude astli-cli --exclude astli-package --exclude astli-resolve \
        --release-type patch --color never --baseline-rev "$1" \
        >"$scratch/$2.log" 2>&1 || status=$?
    breaks "$2" >"$scratch/$2.txt"
    if grep -q '^error' "$scratch/$2.log" || { ((status != 0)) && [[ ! -s $scratch/$2.txt ]]; }; then
        cat "$scratch/$2.log"
        exit 1
    fi
}

semver_checks "$merge_base" merge-base
if [[ ! -s $scratch/merge-base.txt ]]; then
    echo "No break against the merge base $merge_base."
    exit 0
fi
semver_checks "$tag" tag

comm -12 "$scratch/merge-base.txt" "$scratch/tag.txt" >"$scratch/both.txt"
if [[ ! -s $scratch/both.txt ]]; then
    echo "Breaks only what $tag did not ship, or what $base already broke."
    exit 0
fi

# A conventional commit's `!`, as in `feat(index)!:`, or its footer.
subjects=$(
    printf '%s\n' "${PR_TITLE:-}"
    git log --format='%s' "$merge_base..HEAD"
)
bodies=$(git log --format='%b' "$merge_base..HEAD")
if grep -qE '^[a-z]+(\([^)]*\))?!:' <<<"$subjects" ||
    grep -qE '^BREAKING[ -]CHANGE:' <<<"$bodies"; then
    echo "Breaks the API $tag released, as declared:"
    sed 's/^/  /' "$scratch/both.txt"
    exit 0
fi

echo "Breaks the API $tag released, without declaring it:"
sed 's/^/  /' "$scratch/both.txt"
echo
echo "Mark the commit that breaks it with a \`!\`, as in \`feat(index)!:\`,"
echo "so that the next release is a minor one. In full, against the merge base:"
echo
cat "$scratch/merge-base.log"
exit 1
