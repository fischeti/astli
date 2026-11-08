#!/bin/sh
# Runs `astli` with the arguments given, as a benchmark in `codspeed/`
# runs it. Its output is dropped, and only a crash fails the benchmark: the
# designs benchmarked are real code, so `astli` finding problems in them,
# exit 1, is what it should do. A top or filelist misspelled exits 1 too, so
# run a new benchmark once with `scripts/bench.py -k`: one that takes a few
# milliseconds read nothing.
astli "$@" >/dev/null 2>&1
[ $? -le 1 ]
