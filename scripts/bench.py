#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml"]
# ///
"""Runs the benchmarks `codspeed/main.yml` lists with hyperfine, on this machine.

CodSpeed counts the work of each run on one thread. This measures the run on
every core, so that what a change does to how work spreads over threads
shows too. Given a baseline binary, each benchmark runs it as well, and the
table says how the first compares.

    scripts/fetch-corpus.sh --eda-benchmarks
    scripts/bench.py target/release/astli [BASELINE] [--serial] [--runs N] [-k TEXT]
                     [--config FILE]

`--serial` runs on one thread, as CodSpeed does, `-k` only the benchmarks
whose name holds TEXT, and `--config` another list, such as
`scripts/codspeed/pr.yml`, what a pull request runs. Exits 1 if a run crashed.
"""

from __future__ import annotations

import argparse
import json
import os
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("baseline", type=Path, nargs="?")
    parser.add_argument("--serial", action="store_true")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("-k", dest="only", default="")
    parser.add_argument(
        "--config", type=Path, default=ROOT / "scripts/codspeed/main.yml"
    )
    args = parser.parse_args()

    config = yaml.safe_load(args.config.read_text())
    cwd = args.config.resolve().parent / config["options"]["working-directory"]
    binaries = [path.resolve() for path in (args.binary, args.baseline) if path]
    env = dict(os.environ)
    if args.serial:
        env.update(ASTLI_JOBS="1", RAYON_NUM_THREADS="1")

    crashed = False
    print(f"{'benchmark':<16} {'wall':>9} {'cpu':>9} {'instructions':>13}", end="")
    print(f" {'wall':>7} {'instr':>7}" if args.baseline else "")
    for bench in config["benchmarks"]:
        if args.only not in bench["name"]:
            continue
        # What follows the wrapper is what `astli` is given, but for the
        # directory it runs in, as `bench-astli.sh -C` takes it.
        words = shlex.split(bench["exec"])[1:]
        at = cwd
        if words[:1] == ["-C"]:
            at, words = cwd / words[1], words[2:]
        commands = [shlex.join([str(binary), *words]) for binary in binaries]
        results = hyperfine(commands, at, env, args.runs)
        crashed |= any(code > 1 for result in results for code in result["codes"])
        new = results[0]
        line = f"{bench['name']:<16} {ms(new['wall']):>9} {ms(new['cpu']):>9} "
        line += f"{giga(new['instructions']):>13}"
        if args.baseline:
            old = results[1]
            line += f" {change(new['wall'], old['wall']):>7}"
            line += f" {change(new['instructions'], old['instructions']):>7}"
        print(line, flush=True)
    if crashed:
        print("a run exited with a code above 1: astli crashed", file=sys.stderr)
    return int(crashed)


def hyperfine(commands: list[str], cwd: Path, env: dict, runs: int) -> list[dict]:
    with tempfile.NamedTemporaryFile(suffix=".json") as out:
        # Its warnings are of exit 1, which is astli finding problems in
        # real code, as it should.
        run = subprocess.run(
            ["hyperfine", "-N", "-i", "--warmup", "1", "--runs", str(runs)]
            + ["--output", "null", "--style", "none", "--export-json", out.name]
            + commands,
            cwd=cwd,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        if run.returncode != 0:
            sys.exit(run.stderr)
        return [summary(result) for result in json.load(open(out.name))["results"]]


def summary(result: dict) -> dict:
    """The means of one command's runs, from either of hyperfine's formats:
    the newer one counts instructions too, where the system lets it."""
    if "summary" in result:
        measures = result["summary"]
        mean = lambda name: measures.get(name, {}).get("mean")
        return {
            "wall": mean("time_wall_clock"),
            "cpu": mean("time_cpu"),
            "instructions": mean("instructions"),
            "codes": [run["exit_code"] for run in result["measurements"]],
        }
    return {
        "wall": result["mean"],
        "cpu": result["user"] + result["system"],
        "instructions": None,
        "codes": result["exit_codes"],
    }


def ms(seconds: float | None) -> str:
    return "-" if seconds is None else f"{seconds * 1000:.0f} ms"


def giga(count: float | None) -> str:
    return "-" if count is None else f"{count / 1e9:.3f} G"


def change(new: float | None, old: float | None) -> str:
    return "-" if new is None or not old else f"{100 * (new - old) / old:+.1f}%"


if __name__ == "__main__":
    sys.exit(main())
