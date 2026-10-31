#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyslang==12.0.0"]
# ///
"""Scores `astli check` on real code with realistic mistakes injected.

    cargo build --release && scripts/check-mutants.py [ibex/full ...] [--per N] [--seed S] [--show N]

The corpus compiles, so it cannot show an error `check` misses. Each design
here is one of PULP's EDA benchmarks, a variant `scripts/eda-benchmarks/
check.txt` names with its top and filelist, copied to a scratch directory
so the mutants never touch `eda-benchmarks/`. From what slang elaborates of
it, through pyslang, the script finds the places a mistake of each kind
could be made, samples `--per` of each, and makes each mistake alone:

- `misspelled`: a signal's name, at one use, with two letters swapped;
- `wrong-port`: two connections to ports of one instance and direction, but
  of different types, swapped;
- `enum-cast`: an enum assigned its member as a number, or a cast to an enum
  dropped;
- `second-driver`: a variable some block or assignment drives, assigned
  again at the end of its module;
- `missing-port`: one named connection left out.

Each mutant keeps every line where it was, so each tool's diagnostics compare
by line with its own on the design as it is, and what either reports anew is
the mistake's. A mutant is counted by whether slang rejects it, and whether
`check` reports an error: one slang rejects and `check` does not is missed,
and one `check` rejects and slang does not is a false error, which is listed
and fails the run. A mistake slang only warns about, as a port's width, is
counted, but neither caught nor missed.
"""

from __future__ import annotations

import argparse
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

import pyslang

ROOT = Path(__file__).resolve().parent.parent
BENCHMARKS = ROOT / "eda-benchmarks"
COMMAND = re.compile(
    r"^\$ astli check --top (\S+) -F ((\S+)/variants/(\S+)/filelist\.f)$"
)
DIAGNOSTIC = re.compile(r"^(.+?):(\d+):\d+: (error|warning)\[([\w-]+)\]: (.*)$")
WORD = re.compile(rb"[A-Za-z_][A-Za-z0-9_$]*")

# What slang's driver makes errors, outside its compatibility modes, of the
# warnings it lets a tool's user turn off.
PROMOTED = {
    f"DiagCode({name})"
    for name in [
        "DuplicateDefinition",
        "BadProceduralForce",
        "UnknownSystemName",
        "NonstandardHierarchicalCross",
        "NonstandardStringConcat",
        "NonstandardInside",
        "MixedVarAssigns",
        "MultipleContAssigns",
        "MultipleAlwaysAssigns",
        "MisplacedTrailingSeparator",
        "InitializerRequired",
        "ConcatWithStringInt",
    ]
}

KINDS = ["misspelled", "wrong-port", "enum-cast", "second-driver", "missing-port"]

# One variant of each design: the fullest of those slang builds in a few
# seconds, where a design has several.
DEFAULT = [
    "ara/default",
    "cheshire/default",
    "cv32e40p/default",
    "cva6/default",
    "cvfpu/full",
    "ibex/full",
    "ita/default",
    "mempool/minpool",
    "nvdla/top",
    "opope/default",
    "pulp_c910/default",
    "redmule/default",
    "serial_link/default",
    "snitch_cluster/default",
    "spatz/default",
    "spi_host/default",
]


@dataclass
class Mutant:
    kind: str
    # Each `(path, start, end, text)` replaces a range of a file's bytes.
    edits: list[tuple[str, int, int, bytes]]
    what: str


def variants() -> dict[str, tuple[str, str]]:
    """Each variant `check.txt` names, as `design/variant`, with its top and
    its filelist relative to `eda-benchmarks/`."""
    found = {}
    for line in (ROOT / "scripts/eda-benchmarks/check.txt").read_text().splitlines():
        match = COMMAND.match(line)
        if match:
            found[f"{match[3]}/{match[4]}"] = (match[1], match[2])
    return found


def read_filelist(path: Path) -> list[str]:
    """The filelist, each path in it made absolute."""
    out = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if line.startswith("+incdir+"):
            out.append(
                "+incdir+" + os.path.normpath(path.parent / line[len("+incdir+") :])
            )
        elif line.startswith("+"):
            out.append(line)
        elif line:
            out.append(os.path.normpath(path.parent / line))
    return out


def compile(filelist: list[str], top: str):
    preprocessor = pyslang.parsing.PreprocessorOptions()
    preprocessor.additionalIncludePaths = [
        line[len("+incdir+") :] for line in filelist if line.startswith("+incdir+")
    ]
    preprocessor.predefines = [
        line[len("+define+") :] for line in filelist if line.startswith("+define+")
    ]
    options = pyslang.ast.CompilationOptions()
    options.defaultTimeScale = pyslang.TimeScale.fromString("1ns/1ps")
    options.errorLimit = 0
    options.topModules = {top}
    bag = pyslang.Bag([preprocessor, options])
    sources = pyslang.SourceManager()
    compilation = pyslang.ast.Compilation(bag)
    for file in (line for line in filelist if not line.startswith("+")):
        compilation.addSyntaxTree(
            pyslang.syntax.SyntaxTree.fromFile(file, sources, bag)
        )
    return compilation, sources


def slang(filelist: list[str], top: str) -> tuple[set[tuple], set[tuple]]:
    """Each error slang reports, and each warning, by where the user wrote
    it."""
    compilation, sources = compile(filelist, top)
    diagnostics = list(compilation.getAllDiagnostics())
    # Two drivers of one variable are found by a pass of their own, over
    # the design once elaborated.
    compilation.freeze()
    analysis = pyslang.analysis.AnalysisManager()
    analysis.analyze(compilation)
    diagnostics += analysis.getDiagnostics()
    errors, warnings = set(), set()
    for diagnostic in diagnostics:
        at = sources.getFullyOriginalLoc(diagnostic.location)
        name = sources.getFileName(at) if at.buffer else ""
        line = sources.getLineNumber(at) if at.buffer else 0
        code = str(diagnostic.code)
        found = errors if diagnostic.isError() or code in PROMOTED else warnings
        found.add((os.path.realpath(name) if name else "", line, code))
    return errors, warnings


def astli(binary: Path, filelist: Path, top: str) -> set[tuple] | None:
    """Each error `check` reports; None where it fails."""
    args = [
        str(binary),
        "-q",
        "--diag",
        "short",
        "check",
        "--top",
        top,
        "-F",
        str(filelist),
    ]
    out = subprocess.run(args, capture_output=True, text=True, check=False)
    if out.returncode not in (0, 1):
        return None
    found = set()
    for line in (out.stdout + out.stderr).splitlines():
        match = DIAGNOSTIC.match(line)
        if match and match[3] == "error":
            found.add((os.path.realpath(match[1]), int(match[2]), match[4]))
    return found


class Sites:
    """Where each kind of mistake could be made, in what slang builds."""

    def __init__(self, compilation, sources):
        self.sources = sources
        compilation.getAllDiagnostics()
        compilation.freeze()
        self.analysis = pyslang.analysis.AnalysisManager()
        self.analysis.analyze(compilation)
        self.found: dict[str, dict[tuple, Mutant]] = {kind: {} for kind in KINDS}
        self.texts: dict[str, bytes] = {}
        compilation.getRoot().visit(self.visit)

    def text(self, path: str) -> bytes:
        if path not in self.texts:
            self.texts[path] = Path(path).read_bytes()
        return self.texts[path]

    def span(self, node) -> tuple[str, int, int] | None:
        """Where `node` is in its file, unless a macro wrote it."""
        if node is None:
            return None
        range = node.sourceRange
        if not (
            self.sources.isFileLoc(range.start) and self.sources.isFileLoc(range.end)
        ):
            return None
        path = os.path.realpath(self.sources.getFileName(range.start))
        if path != os.path.realpath(self.sources.getFileName(range.end)):
            return None
        return path, range.start.offset, range.end.offset

    def add(self, kind: str, key: tuple, mutant: Mutant):
        self.found[kind].setdefault(key, mutant)

    def visit(self, node):
        ast = pyslang.ast
        if isinstance(node, ast.GenerateBlockSymbol) and node.isUninstantiated:
            return ast.VisitAction.Skip
        if isinstance(node, ast.NamedValueExpression):
            self.misspelled(node)
        elif isinstance(node, ast.AssignmentExpression):
            self.enum_cast(node)
        elif isinstance(node, (ast.ProceduralBlockSymbol, ast.ContinuousAssignSymbol)):
            node.visit(self.driven)
        elif isinstance(node, ast.InstanceSymbol) and not node.isInterface:
            self.connections(node)
        return ast.VisitAction.Advance

    def misspelled(self, expr):
        kind = pyslang.ast.SymbolKind
        if expr.symbol.kind not in (kind.Variable, kind.Net):
            return
        syntax = expr.syntax
        if syntax is None or syntax.kind != pyslang.syntax.SyntaxKind.IdentifierName:
            return
        at = self.span(syntax)
        if at is None:
            return
        path, start, end = at
        name = self.text(path)[start:end]
        if name.decode(errors="replace") != expr.symbol.name:
            return
        self.add(
            "misspelled",
            at,
            Mutant("misspelled", [(path, start, end, name)], name.decode()),
        )

    def enum_cast(self, expr):
        if expr.syntax is None or not expr.left.type.isEnum:
            return
        right = expr.right
        syntax = right.syntax
        at = self.span(syntax)
        if at is None:
            return
        path, start, end = at
        if syntax.kind == pyslang.syntax.SyntaxKind.CastExpression:
            inner = self.span(syntax.right)
            if inner is None:
                return
            text = self.text(path)[inner[1] : inner[2]]
        elif (
            isinstance(right, pyslang.ast.NamedValueExpression)
            and right.symbol.kind == pyslang.ast.SymbolKind.EnumValue
        ):
            value = right.symbol.value.value
            if value.hasUnknown:
                return
            text = f"{value.bitWidth}'d{value.toString(pyslang.LiteralBase.Decimal, False)}"
            text = text.encode()
        else:
            return
        was = self.text(path)[start:end].decode(errors="replace")
        self.add(
            "enum-cast",
            at,
            Mutant(
                "enum-cast", [(path, start, end, text)], f"{was} as {text.decode()}"
            ),
        )

    def driven(self, node):
        """Each variable of a module's own scope `node` assigns."""
        ast = pyslang.ast
        if isinstance(node, ast.AssignmentExpression) and isinstance(
            node.left, ast.NamedValueExpression
        ):
            symbol = node.left.symbol
            if symbol.kind != ast.SymbolKind.Variable:
                return ast.VisitAction.Advance
            body = symbol.parentScope.containingInstance
            own = f"{body.parentInstance.hierarchicalPath}.{symbol.name}"
            if (
                symbol.hierarchicalPath == own
                and symbol.type.isIntegral
                and not symbol.type.isEnum
            ):
                self.second_driver(symbol, body)
        return ast.VisitAction.Advance

    def second_driver(self, symbol, body):
        definition = body.definition
        if definition.definitionKind != pyslang.ast.DefinitionKind.Module:
            return
        # slang's analysis drops an assignment in a branch a constant rules
        # out, which the standard still counts, and astli does: a second
        # driver of what only such a branch drives is no mistake slang sees.
        if not self.analysis.getDrivers(symbol):
            return
        end = definition.syntax.endmodule
        if not self.sources.isFileLoc(end.location):
            return
        path = os.path.realpath(self.sources.getFileName(end.location))
        offset = end.location.offset
        text = f"assign {symbol.name} = '0; ".encode()
        self.add(
            "second-driver",
            (path, offset, symbol.name),
            Mutant(
                "second-driver",
                [(path, offset, offset, text)],
                f"{symbol.name} in {definition.name}",
            ),
        )

    def connections(self, instance):
        syntax = instance.syntax
        if syntax is None or not hasattr(syntax, "connections"):
            return
        ports = {}
        for connection in instance.portConnections:
            port = connection.port
            if isinstance(port, pyslang.ast.PortSymbol):
                ports[port.name] = port
        named = []
        for node in syntax.connections:
            if (
                getattr(node, "kind", None)
                != pyslang.syntax.SyntaxKind.NamedPortConnection
            ):
                continue
            if node.expr is None:
                continue
            at = self.span(node)
            expr = self.span(node.expr)
            if at is None or expr is None:
                continue
            named.append((node.name.valueText, at, expr))

        for name, (path, start, end), _ in named:
            edit = self.omit(path, start, end)
            if edit:
                self.add(
                    "missing-port",
                    (path, start),
                    Mutant("missing-port", [edit], f".{name} of {instance.name}"),
                )

        for i, (name, _, (path, start, end)) in enumerate(named):
            port = ports.get(name)
            if port is None or b"\n" in self.text(path)[start:end]:
                continue
            for other, _, (other_path, other_start, other_end) in named[i + 1 :]:
                theirs = ports.get(other)
                if (
                    theirs is None
                    or other_path != path
                    or theirs.direction != port.direction
                    or theirs.type.isMatching(port.type)
                    or b"\n" in self.text(path)[other_start:other_end]
                ):
                    continue
                text = self.text(path)
                self.add(
                    "wrong-port",
                    (path, start),
                    Mutant(
                        "wrong-port",
                        [
                            (path, start, end, text[other_start:other_end]),
                            (path, other_start, other_end, text[start:end]),
                        ],
                        f".{name} and .{other} of {instance.name}",
                    ),
                )
                break

    def omit(self, path: str, start: int, end: int) -> tuple | None:
        """An edit leaving out the connection at `start..end`, with the comma
        separating it from the next or the one before; none where a comment
        stands between."""
        text = self.text(path)
        after = end
        while after < len(text) and text[after : after + 1].isspace():
            after += 1
        if text[after : after + 1] == b",":
            end = after + 1
        else:
            before = start
            while before > 0 and text[before - 1 : before].isspace():
                before -= 1
            if text[before - 1 : before] != b",":
                return None
            start = before - 1
        return path, start, end, b"\n" * text[start:end].count(b"\n")


def misspell(name: bytes, text: bytes, rng: random.Random) -> bytes | None:
    """`name` with two neighbouring letters swapped, unless that spells a
    name `text` already has."""
    words = set(WORD.findall(text))
    places = [i for i in range(1, len(name) - 1) if name[i] != name[i + 1]]
    rng.shuffle(places)
    for i in places:
        typo = name[:i] + name[i + 1 : i + 2] + name[i : i + 1] + name[i + 2 :]
        if typo not in words:
            return typo
    return None


def apply(edits: list[tuple[str, int, int, bytes]], originals: dict[str, bytes]):
    by_file: dict[str, list] = {}
    for path, start, end, text in edits:
        by_file.setdefault(path, []).append((start, end, text))
    for path, changes in by_file.items():
        text = originals.setdefault(path, Path(path).read_bytes())
        for start, end, new in sorted(changes, reverse=True):
            text = text[:start] + new + text[end:]
        Path(path).write_bytes(text)


def run(name: str, top: str, filelist: str, args) -> tuple[Counter, list, list]:
    design = name.split("/")[0]
    counts: Counter = Counter()
    missed, false = [], []
    with tempfile.TemporaryDirectory() as scratch:
        copy = Path(scratch) / design
        shutil.copytree(BENCHMARKS / design, copy, symlinks=True)
        flist = Path(scratch) / filelist
        files = read_filelist(flist)
        errors, warnings = slang(files, top)
        ours = astli(args.astli, flist, top)
        if ours is None:
            sys.exit(f"{name}: astli check fails on the design as it is")

        compilation, sources = compile(files, top)
        sites = Sites(compilation, sources)
        del compilation, sources
        rng = random.Random(f"{args.seed}:{name}")
        for kind in KINDS:
            places = sorted(sites.found[kind].items(), key=lambda item: str(item[0]))
            rng.shuffle(places)
            taken = 0
            for _, mutant in places:
                if taken == args.per:
                    break
                if kind == "misspelled":
                    path, start, end, word = mutant.edits[0]
                    typo = misspell(word, sites.text(path), rng)
                    if typo is None:
                        continue
                    mutant.edits = [(path, start, end, typo)]
                    mutant.what = f"{word.decode()} as {typo.decode()}"
                taken += 1
                originals: dict[str, bytes] = {}
                try:
                    apply(mutant.edits, originals)
                    new_errors, new_warnings = slang(files, top)
                    reported = astli(args.astli, flist, top)
                finally:
                    for path, text in originals.items():
                        Path(path).write_bytes(text)
                path, start = mutant.edits[0][:2]
                line = sites.text(path)[:start].count(b"\n") + 1
                where = f"{os.path.relpath(path, os.path.realpath(scratch))}:{line}"
                if reported is None:
                    counts[kind, "crash"] += 1
                    false.append(f"{where}: [{kind}] {mutant.what}: astli check fails")
                    continue
                theirs = sorted({code[9:-1] for *_, code in new_errors - errors})
                warned = sorted({code[9:-1] for *_, code in new_warnings - warnings})
                mine = sorted({code for *_, code in reported - ours})
                rejected, reported = bool(theirs), bool(mine)
                counts[kind, "mutants"] += 1
                counts[kind, "rejected"] += rejected
                counts[kind, "warned"] += not rejected and bool(warned)
                counts[kind, "caught"] += rejected and reported
                if rejected and not reported:
                    missed.append(
                        f"{where}: [{kind}] {mutant.what}: slang {', '.join(theirs)}"
                    )
                if reported and not rejected:
                    counts[kind, "false"] += 1
                    false.append(
                        f"{where}: [{kind}] {mutant.what}: astli {', '.join(mine)}, "
                        f"slang warns {', '.join(warned) or 'nothing'}"
                    )
    return counts, missed, false


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("variants", nargs="*", default=DEFAULT, help="as `ibex/full`")
    parser.add_argument("--astli", type=Path, default=ROOT / "target/release/astli")
    parser.add_argument(
        "--per", type=int, default=10, help="mutants of each kind per variant"
    )
    parser.add_argument("--seed", default="0")
    parser.add_argument(
        "--show", type=int, default=5, help="misses to list per variant"
    )
    args = parser.parse_args()
    args.astli = args.astli.resolve()
    if not args.astli.is_file():
        sys.exit(f"no {args.astli} -- run cargo build --release")
    if not BENCHMARKS.is_dir():
        sys.exit(f"no {BENCHMARKS} -- run scripts/fetch-corpus.sh")
    known = variants()

    total: Counter = Counter()
    failed = False
    for name in args.variants:
        if name not in known:
            sys.exit(f"{name} is no variant scripts/eda-benchmarks/check.txt names")
        counts, missed, false = run(name, *known[name], args)
        total.update(counts)
        print(
            f"{name}: "
            + ", ".join(
                f"{kind} {counts[kind, 'caught']}/{counts[kind, 'rejected']}"
                for kind in KINDS
            ),
            flush=True,
        )
        for line in missed[: args.show]:
            print(f"  missed {line}")
        for line in false:
            print(f"  FALSE {line}")
        failed |= bool(false)

    print()
    print(
        f"{'kind':<14} {'mutants':>8} {'rejected':>9} {'caught':>7} {'warned':>7} {'false':>6}"
    )
    for kind in KINDS:
        print(
            f"{kind:<14} {total[kind, 'mutants']:>8} {total[kind, 'rejected']:>9} "
            f"{total[kind, 'caught']:>7} {total[kind, 'warned']:>7} "
            f"{total[kind, 'false'] + total[kind, 'crash']:>6}"
        )
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
