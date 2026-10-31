#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# ///
"""Runs astli over sv-tests the way the suite runs a parser.

Each test names the stages it exercises in `:type:`, and whether a tool must
reject it in `:should_fail_because:`. A test runs at the first of `parsing`,
`preprocessing` and `elaboration` it names, and is skipped if it names none:
only simulation can fail it. A test passes when astli rejects it exactly
when it should, and never crashes.

astli exits 0 on input it only partly understands, so at parsing and
preprocessing rejecting means any error or warning -- `not-parsed` among
them -- not the exit code. At elaboration a test runs through `astli check
--top`, from every module the test does not instantiate, which rejects it
with an error and not a warning.

Exits 1 when the failures differ from the `EXPECTED_TO_FAIL` sets, either
way, so CI catches a regression and a stale list alike.

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

# Each is a gap in docs/limitations.md, at the commit fetch-corpus.sh pins.
EXPECTED_TO_FAIL: set[str] = set()
# `check` reports a subset of the errors elaboration finds, and none of these.
EXPECTED_TO_FAIL_ELABORATION = {
    "chapter-10/10.3--proc-assignment--bad.sv",
    "chapter-11/11.4.14.3--unpack_stream_inv.sv",
    "chapter-13/13.4.1--function-void-return.sv",
    "chapter-13/13.4.4--fork-invalid.sv",
    "chapter-14/14.3--clocking-block-signals-error.sv",
    "chapter-18/18.17.2--if-else-production-statements_0_fail.sv",
    "chapter-18/18.17.2--if-else-production-statements_2_fail.sv",
    "chapter-18/18.17.3--case-production-statements_0_fail.sv",
    "chapter-18/18.17.6--aborting-productions-break-and-return_2_fail.sv",
    "chapter-18/18.5.1--explicit-external-constraint_1.sv",
    "chapter-18/18.5.10--variable-ordering_1.sv",
    "chapter-18/18.5.14--soft-constraints_2.sv",
    "chapter-18/18.5.2--pure-constraint_2.sv",
    "chapter-18/18.5.4--distribution_2.sv",
    "chapter-18/18.6.3--behavior-of-randomization-methods_4.sv",
    "chapter-18/18.8--disabling-random-variables-with-rand_mode_4.sv",
    "chapter-18/18.8--disabling-random-variables-with-rand_mode_5.sv",
    "chapter-18/18.9--controlling-constraints-with-constraint_mode_1.sv",
    "chapter-18/18.9--controlling-constraints-with-constraint_mode_2.sv",
    "chapter-22/22.7--timescale-basic-3.sv",
    "chapter-22/22.7--timescale-basic-4.sv",
    "chapter-22/22.9--unconnected_drive-invalid-1.sv",
    "chapter-22/22.9--unconnected_drive-invalid-2.sv",
    "chapter-22/22.9--unconnected_drive-invalid-3.sv",
    "chapter-5/5.10-structure-arrays-illegal.sv",
    "chapter-6/6.12--real_bit_select.sv",
    "chapter-6/6.12--real_bit_select_idx.sv",
    "chapter-6/6.12--real_edge.sv",
    "chapter-6/6.19--enum_value_inv.sv",
    "chapter-6/6.19--enum_xx_inv.sv",
    "chapter-6/6.19--enum_xx_inv_order.sv",
    "chapter-6/6.19.4--enum_numerical_expr_no_cast.sv",
    "chapter-6/6.20.5--specparam_inv.sv",
    "chapter-6/6.5--variable_redeclare.sv",
    "chapter-7/arrays/packed/variable-slice-zero.sv",
    "chapter-7/structures/packed/default-value.sv",
    "chapter-8/8.21--abstract_class_inst.sv",
    "chapter-8/8.25.1--parametrized_class_invalid_scope_resolution.sv",
    "chapter-8/8.26.4--illegal_forward_def_implements.sv",
    "chapter-8/8.26.4--illegal_implements_parameter.sv",
    "chapter-8/8.26.5--invalid_interface_instantiation.sv",
    "chapter-8/8.26.6.1--name_conflict_unresolved.sv",
    "chapter-8/8.26.6.2--parameter_type_conflict_unresolved.sv",
    "chapter-8/8.26.6.3--diamond_relationship_parametrized.sv",
    "chapter-9/9.3.3--fork_return.sv",
    "generic/member/class_member_test_5.sv",
    "generic/typedef/typedef_test_28__bad.sv",
}
# An `ifdef that splits a list is left as written until it is expanded.
EXPECTED_TO_FAIL_RAW = {
    "generic/typedef/typedef_test_26.sv",
    "generic/typedef/typedef_test_27.sv",
}


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
    """The test at `path`, or `None` if only simulation can fail it."""
    text = path.read_text(errors="replace")
    params: dict[str, str] = {}
    for name, value in METADATA.findall(text):
        params.setdefault(name.lower(), value.strip())

    runners = params.get("compatible-runners", "all").split()
    stages = params.get("type", "parsing elaboration").split()
    stage = ("parsing", "preprocessing", "elaboration")
    mode = next((m for m in stage if m in stages), None)
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
    if test.mode == "elaboration":
        command.append("check")
        # Elaborated from every module nothing instantiates, as a simulator
        # given no top would.
        tops = subprocess.run(
            [str(astli), "files", "--emit", "tops", *map(str, test.files)],
            capture_output=True, text=True, timeout=test.timeout, check=False,
        )
        for top in tops.stdout.split():
            command += ["--top", top]
    elif test.mode == "preprocessing":
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
    rejected = done.returncode != 0 or (bool(codes) and test.mode != "elaboration")
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
    tests = [
        t for p in paths if (t := load(p, libs, third_party)) is not None
    ]
    with ThreadPoolExecutor(os.cpu_count()) as pool:
        results = list(pool.map(lambda t: run(args.astli, t, args.raw), tests))

    skipped = len(paths) - len(tests)
    print(f"{len(paths)} tests, {skipped} skipped as only simulation can fail them")
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
    expected = EXPECTED_TO_FAIL | EXPECTED_TO_FAIL_ELABORATION
    expected |= EXPECTED_TO_FAIL_RAW if args.raw else set()
    where = lambda r: r.test.path.relative_to(tests_dir).as_posix()
    # A crash is never expected, even of a test that should fail another way.
    known = lambda r: where(r) in expected and r.outcome in (
        "rejected valid",
        "accepted invalid",
    )
    unexpected = [r for r in failures if not known(r)]
    now_passing = sorted(expected - {where(r) for r in failures})

    if failures:
        print()
        print("Failures by outcome:")
        for outcome, count in Counter(r.outcome for r in failures).most_common():
            print(f"  {count:5}  {outcome}")
        codes = Counter(
            c for r in failures if not r.test.should_fail for c in set(r.codes)
        )
        if codes:
            print("Diagnostics on rejected valid tests:")
            for code, count in codes.most_common():
                print(f"  {count:5}  {code}")

        print()
        # The unexpected first, so that a sample shows them.
        ordered = unexpected + [r for r in failures if known(r)]
        shown = ordered if args.all else ordered[:20]
        for result in shown:
            codes = f" [{', '.join(sorted(set(result.codes)))}]" if result.codes else ""
            mark = " (expected)" if known(result) else ""
            print(f"{result.outcome:16} {where(result)}{codes}{mark}")
        if len(shown) < len(ordered):
            print(f"... and {len(ordered) - len(shown)} more; --all lists them")

    if now_passing:
        print()
        print("Expected to fail, but did not; take them off the list:")
        for path in now_passing:
            print(f"  {path}")
    print()
    if unexpected or now_passing:
        print(
            f"{len(unexpected)} unexpected failures, {len(now_passing)} unexpected passes"
        )
        return 1
    print("As expected.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
