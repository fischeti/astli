#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyslang==12.0.0"]
# ///
"""Holds what `astli check` reports on real builds to slang's errors.

    cargo build --release && scripts/check-oracle.py [corpus/axi ...] [--top T] [--show N]

A design is a corpus repository with its dependencies checked out (`bender
checkout`), built as `bender script flist-plus` writes it with the targets
`DESIGNS` names: the files, include directories and defines a simulator
would be given. So no file another includes is read as a unit, and nothing
a build would supply is missing. astli runs `check` over that filelist;
slang, through pyslang, compiles the same files, each its own compilation
unit, elaborated from the modules nothing instantiates or from `--top`.

Each error astli reports, and each `not-parsed`, is counted by what slang
says on the same line: an error, a warning, or nothing in a definition it
never elaborates. Nothing in one it does is a false error, which is listed
and fails the run; `--unchecked` lists the others too.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import pyslang

ROOT = Path(__file__).resolve().parent.parent
DIAGNOSTIC = re.compile(r"^(.+?):(\d+):\d+: (error|warning)\[([\w-]+)\]: (.*)$")

# Each with the flags its own simulation flow passes to bender, and the
# `simulation` target bender adds to a simulator's script itself. cheshire's
# test bench needs flash and EEPROM models its makefile downloads, so it is
# its RTL alone.
DESIGNS = {
    "axi": "-t simulation -t test -t rtl",
    "cheshire": "-t rtl -t cva6 -t cv64a6_imafdchsclic_sv39_wb",
    "common_cells": "-t simulation -t test -t tb -t formal -t synth_test",
    "FlooNoC": "-t simulation -t rtl -t test -t floo_test -t snitch_cluster -t idma_test"
    " -t cc_no_deprecated",
    "snitch_cluster": "-t simulation -t snitch_cluster_wrapper -t rtl -t cc_no_deprecated"
    " -t tech_cells_generic_include_tc_sync -t test -t snitch_cluster:tb -DDEBUG",
}


def build(repo: Path) -> list[str]:
    """The filelist bender writes for `repo`."""
    args = ["bender", "script", "flist-plus", *DESIGNS[repo.name].split()]
    out = subprocess.run(args, cwd=repo, check=True, capture_output=True, text=True)
    return [line for line in out.stdout.splitlines() if line.strip()]


def place(path: str, line: int) -> tuple[str, int]:
    return os.path.realpath(path), line


def astli(binary: Path, filelist: list[str], tops: list[str]) -> list[tuple]:
    """What `check` reports that must be an error: each error, and each
    construct it did not parse."""
    with tempfile.NamedTemporaryFile("w", suffix=".f") as f:
        f.write("\n".join(filelist) + "\n")
        f.flush()
        args = [str(binary), "-q", "--diagnostics", "short", "check", "-f", f.name]
        for top in tops:
            args += ["--top", top]
        out = subprocess.run(args, capture_output=True, text=True, check=False)
    if out.returncode not in (0, 1):
        sys.exit(f"astli check failed ({out.returncode}):\n{out.stderr}")
    found = []
    for line in (out.stdout + out.stderr).splitlines():
        match = DIAGNOSTIC.match(line)
        if match and (match[3] == "error" or match[4] == "not-parsed"):
            found.append((place(match[1], int(match[2])), match[4], match[5]))
    return found


def slang(
    filelist: list[str], tops: list[str]
) -> tuple[dict[tuple, bool], list[tuple]]:
    """Each line slang reports on, and whether it reports an error there; and
    the lines of each definition it never elaborated."""
    preprocessor = pyslang.parsing.PreprocessorOptions()
    preprocessor.additionalIncludePaths = [
        line[len("+incdir+") :] for line in filelist if line.startswith("+incdir+")
    ]
    preprocessor.predefines = [
        line[len("+define+") :] for line in filelist if line.startswith("+define+")
    ]
    options = pyslang.ast.CompilationOptions()
    # A simulator applies one where a file sets none; without it, slang
    # rejects a design whose files mix the two.
    options.defaultTimeScale = pyslang.TimeScale.fromString("1ns/1ps")
    # Past its limit slang stops elaborating, and what it never reached
    # would make astli's errors there look false.
    options.errorLimit = 0
    if tops:
        options.topModules = set(tops)
    bag = pyslang.Bag([preprocessor, options])
    sources = pyslang.SourceManager()
    compilation = pyslang.ast.Compilation(bag)
    for file in (line for line in filelist if not line.startswith("+")):
        compilation.addSyntaxTree(
            pyslang.syntax.SyntaxTree.fromFile(file, sources, bag)
        )

    found = {}
    for diagnostic in compilation.getAllDiagnostics():
        # Where the user wrote it: a macro's error is reported at its call,
        # as astli reports it.
        at = sources.getFullyOriginalLoc(diagnostic.location)
        if not at.buffer:
            continue
        key = place(sources.getFileName(at), sources.getLineNumber(at))
        found[key] = found.get(key, False) or diagnostic.isError()

    # slang checks a definition's body only where it elaborates it: from a
    # top, or in a generate block it builds. astli checks every definition.
    built = {top.body.definition.name for top in compilation.getRoot().topInstances}
    unreached = []
    for definition in compilation.getDefinitions():
        if definition.instanceCount or definition.name in built:
            continue
        span = definition.syntax.sourceRange
        path, line = place(
            sources.getFileName(span.start), sources.getLineNumber(span.start)
        )
        unreached.append((path, line, sources.getLineNumber(span.end)))
    return found, unreached


def reached(at: tuple[str, int], unreached: list[tuple]) -> bool:
    path, line = at
    return not any(path == p and start <= line <= end for p, start, end in unreached)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "repos",
        nargs="*",
        type=Path,
        default=[ROOT / "corpus" / name for name in DESIGNS],
    )
    parser.add_argument("--astli", type=Path, default=ROOT / "target/release/astli")
    parser.add_argument(
        "--top",
        action="append",
        default=[],
        help="elaborate from this module on both sides",
    )
    parser.add_argument(
        "--show", type=int, default=20, help="findings to list per design"
    )
    parser.add_argument(
        "--unchecked",
        action="store_true",
        help="list where slang warns or elaborates nothing too",
    )
    args = parser.parse_args()
    if not args.astli.is_file():
        sys.exit(f"no {args.astli} -- run cargo build --release")

    failed = False
    for repo in args.repos:
        repo = repo.resolve()
        if repo.name not in DESIGNS:
            sys.exit(f"{repo.name} has no targets in DESIGNS")
        if not (repo / ".bender").is_dir():
            sys.exit(f"no {repo}/.bender -- run bender checkout there")
        filelist = build(repo)
        ours = astli(args.astli, filelist, args.top)
        theirs, unreached = slang(filelist, args.top)

        shared = [finding for finding in ours if theirs.get(finding[0])]
        # slang only warns on some of what the standard does not allow, as an
        # override of a parameter the definition lacks.
        warned = [finding for finding in ours if theirs.get(finding[0]) is False]
        rest = [finding for finding in ours if finding[0] not in theirs]
        unchecked = [finding for finding in rest if not reached(finding[0], unreached)]
        false = [finding for finding in rest if reached(finding[0], unreached)]
        files = sum(not line.startswith("+") for line in filelist)
        print(
            f"{repo.name}: {files} files; astli reports {len(ours)}: {len(shared)} as slang does, "
            f"{len(warned)} where slang warns, {len(unchecked)} where it elaborates nothing, "
            f"{len(false)} false"
        )
        listed = false + (warned + unchecked if args.unchecked else [])
        for (path, line), code, message in listed[: args.show]:
            print(f"  {os.path.relpath(path, ROOT)}:{line}: [{code}] {message}")
        failed |= bool(false)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
