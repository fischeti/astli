#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# ///
"""Runs astli over sv-tests the way the suite runs a parser.

Each test names the stages it exercises in `:type:`, and whether a tool must
reject it in `:should_fail_because:`. astli preprocesses and parses but does
not elaborate, so a test runs at `parsing` if it names that stage, else at
`preprocessing`, and otherwise is skipped: a test that only elaboration can
fail says nothing about a parser. A test passes when astli rejects it exactly
when it should, and never crashes.

astli exits 0 on input it only partly understands, so rejecting means any
error or warning -- `not-parsed` among them -- not the exit code.

    cargo build --release && scripts/sv-tests.py [--raw] [--all]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
METADATA = re.compile(r"^:([a-zA-Z_-]+):\s*(.+)", re.MULTILINE)
DIAGNOSTIC = re.compile(r"^\[([\w-]+)\] (?:Error|Warning):", re.MULTILINE)


@dataclass
class Test:
    path: Path
    mode: str
    # A library the test's tags name, such as UVM, comes first.
    files: list[Path]
    incdirs: list[Path]
    should_fail: bool
    defines: list[str]
    timeout: int


@dataclass
class Result:
    test: Test
    passed: bool
    outcome: str
    codes: list[str] = field(default_factory=list)


def load(path: Path, libs: dict, third_party: Path) -> Test | None:
    """The test at `path`, or `None` if it needs a stage astli lacks."""
    text = path.read_text(errors="replace")
    params: dict[str, str] = {}
    for name, value in METADATA.findall(text):
        params.setdefault(name.lower(), value.strip())

    runners = params.get("compatible-runners", "all").split()
    stages = params.get("type", "parsing elaboration").split()
    mode = next((m for m in ("parsing", "preprocessing") if m in stages), None)
    if "all" not in runners or mode is None:
        return None
    files, incdirs = [], []
    for tag in params.get("tags", "").split():
        lib = libs.get(tag, {})
        files += [third_party / f for f in lib.get("files", [])]
        incdirs += [third_party / d for d in lib.get("incdirs", [])]
    return Test(
        path=path,
        mode=mode,
        files=[*files, path],
        incdirs=[*incdirs, path.parent],
        should_fail="should_fail_because" in params,
        defines=params.get("defines", "").split(),
        timeout=int(params.get("timeout", "30")),
    )


def run(astli: Path, test: Test, raw: bool) -> Result:
    command = [str(astli), "-q", "-j", "1"]
    if test.mode == "preprocessing":
        command.append("preprocess")
    else:
        command += ["parse"] if raw else ["parse", "--expand"]
    for incdir in test.incdirs:
        command += ["-I", str(incdir)]
    for define in test.defines:
        command += ["-D", define]
    command += map(str, test.files)

    try:
        done = subprocess.run(
            command, capture_output=True, text=True, timeout=test.timeout, check=False
        )
    except subprocess.TimeoutExpired:
        return Result(test, False, "timeout")

    codes = DIAGNOSTIC.findall(done.stdout + done.stderr)
    # 1 is a reported failure; anything else is a panic or a signal.
    if done.returncode not in (0, 1):
        return Result(test, False, f"crash ({done.returncode})", codes)
    rejected = done.returncode != 0 or bool(codes)
    if rejected == test.should_fail:
        return Result(test, True, "pass", codes)
    outcome = "accepted invalid" if test.should_fail else "rejected valid"
    return Result(test, False, outcome, codes)


def chapter(test: Test, tests: Path) -> str:
    parts = test.path.relative_to(tests).parts
    return parts[0] if len(parts) > 1 else "."


def percent(passed: int, total: int) -> str:
    return f"{passed:5}/{total:<5} {100 * passed / total:5.1f}%" if total else ""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--astli", type=Path, default=ROOT / "target/release/astli")
    parser.add_argument("--suite", type=Path, default=ROOT / "sv-tests")
    parser.add_argument(
        "--raw",
        action="store_true",
        help="parse the source as written, as the formatter does, not its expansion",
    )
    parser.add_argument(
        "--all", action="store_true", help="list every failing test, not a sample"
    )
    args = parser.parse_args()

    tests_dir = args.suite / "tests"
    if not tests_dir.is_dir():
        sys.exit(f"no {tests_dir} -- run scripts/fetch-corpus.sh")
    if not args.astli.is_file():
        sys.exit(f"no {args.astli} -- run cargo build --release")

    libs = json.loads((args.suite / "conf/runners/libs.json").read_text())
    third_party = args.suite / "third_party"
    paths = sorted(tests_dir.rglob("*.sv"))
    tests = [t for p in paths if (t := load(p, libs, third_party)) is not None]
    with ThreadPoolExecutor(os.cpu_count()) as pool:
        results = list(pool.map(lambda t: run(args.astli, t, args.raw), tests))

    print(f"{len(paths)} tests, {len(paths) - len(tests)} skipped as elaboration-only")
    print()
    by_mode = defaultdict(list)
    by_chapter = defaultdict(list)
    for result in results:
        by_mode[result.test.mode].append(result.passed)
        by_chapter[chapter(result.test, tests_dir)].append(result.passed)
    for mode, passed in sorted(by_mode.items()):
        print(f"{mode:16} {percent(sum(passed), len(passed))}")
    all_passed = [r.passed for r in results]
    print(f"{'total':16} {percent(sum(all_passed), len(all_passed))}")

    print()
    number = lambda name: [int(n) for n in re.findall(r"\d+", name)] or [0]
    for name in sorted(by_chapter, key=number):
        passed = by_chapter[name]
        print(f"{name:16} {percent(sum(passed), len(passed))}")

    failures = [r for r in results if not r.passed]
    if not failures:
        return 0

    print()
    print("Failures by outcome:")
    for outcome, count in Counter(r.outcome for r in failures).most_common():
        print(f"  {count:5}  {outcome}")
    codes = Counter(c for r in failures if not r.test.should_fail for c in set(r.codes))
    if codes:
        print("Diagnostics on rejected valid tests:")
        for code, count in codes.most_common():
            print(f"  {count:5}  {code}")

    print()
    shown = failures if args.all else failures[:20]
    for result in shown:
        where = result.test.path.relative_to(tests_dir)
        codes = f" [{', '.join(sorted(set(result.codes)))}]" if result.codes else ""
        print(f"{result.outcome:16} {where}{codes}")
    if len(shown) < len(failures):
        print(f"... and {len(failures) - len(shown)} more; --all lists them")
    return 1


if __name__ == "__main__":
    sys.exit(main())
