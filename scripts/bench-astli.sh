#!/bin/sh
# Runs `astli` with the arguments given, as a benchmark in `codspeed/`
# runs it. Its output is dropped, and only a crash fails the benchmark: the
# designs benchmarked are real code, so `astli` finding problems in them,
# exit 1, is what it should do. A top or filelist misspelled exits 1 too, so
# run a new benchmark once with `scripts/bench.py -k`: one that takes a few
# milliseconds read nothing.
#
# `-C DIR` first runs it in `DIR` instead, relative to the configuration's
# working directory, which CodSpeed sets for every benchmark alike: a
# workspace is run from where it is.
if [ "${1:-}" = -C ]; then
    cd "$2" || exit 2
    shift 2
fi
astli "$@" >/dev/null 2>&1
[ $? -le 1 ]
