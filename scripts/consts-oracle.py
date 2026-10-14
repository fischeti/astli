#!/usr/bin/env python3
"""Compares the values astli gives each parameter and enum member of a
repository with slang's.

    scripts/consts-oracle.py corpus/axi [--show N]

astli's come from the `consts` example of astli-sema, every definition at its
defaults; slang's from pyslang (`pip install pyslang`), elaborating the same
files, each its own compilation unit, from the modules nothing instantiates.
Only what both evaluate is compared: slang elaborates a generate block only
when it is built, and astli leaves what it cannot evaluate unknown. Prints
how many agree, how many astli leaves unknown where slang has a value, and
each disagreement.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

import pyslang

ROOT = Path(__file__).resolve().parent.parent
LINE = re.compile(r"^(\S+:\d+:\d+) (\S+) = (.*)$")


def astli(repo, files_only):
    args = ["cargo", "run", "-q", "--release", "-p", "astli-sema", "--example", "consts", "--"]
    args += ["--files"] if files_only else []
    out = subprocess.run(args + [repo], cwd=ROOT, check=True, capture_output=True, text=True)
    return out.stdout.splitlines()


def value(constant):
    """A constant as astli writes a value; None for what it does not."""
    v = constant.value
    if isinstance(v, pyslang.SVInt):
        width, signing = v.bitWidth, "s" if v.isSigned else ""
        if v.hasUnknown:
            digits = v.toString(pyslang.LiteralBase.Binary, False)
            if len(digits) < width:
                pad = digits[0] if digits[0] in "xz" else "0"
                digits = pad * (width - len(digits)) + digits
            if len(set(digits)) == 1:
                digits = digits[0]
            return f"{width}'{signing}b{digits}"
        decimal = v.toString(pyslang.LiteralBase.Decimal, False)
        sign = "-" if decimal.startswith("-") else ""
        return f"{sign}{width}'{signing}d{decimal.lstrip('-')}"
    if isinstance(v, list):
        parts = [value(element) for element in v]
        if None in parts:
            return None
        return "'{" + ", ".join(parts) + "}"
    if isinstance(v, float):
        return repr(v)
    if isinstance(v, str):
        escaped = v.replace("\\", "\\\\").replace('"', '\\"')
        return '"' + escaped.replace("\n", "\\n").replace("\t", "\\t") + '"'
    return None


def slang(filelist):
    dirs = [line[len("+incdir+"):] for line in filelist if line.startswith("+incdir+")]
    files = [line for line in filelist if not line.startswith("+")]
    options = pyslang.parsing.PreprocessorOptions()
    options.additionalIncludePaths = dirs
    bag = pyslang.Bag([options])
    sources = pyslang.SourceManager()
    trees = [pyslang.syntax.SyntaxTree.fromFile(f, sources, bag) for f in files]
    compilation = pyslang.ast.Compilation(bag)
    for tree in trees:
        compilation.addSyntaxTree(tree)

    kind = pyslang.ast.SymbolKind
    found = {}

    def visit(scope):
        for member in scope:
            if member.kind in (kind.Parameter, kind.EnumValue):
                try:
                    text = value(member.value)
                except Exception:
                    text = None
                if text is not None:
                    at = member.location
                    place = (f"{sources.getFileName(at)}:{sources.getLineNumber(at)}"
                             f":{sources.getColumnNumber(at)}")
                    found.setdefault(place, (member.name, text))
            elif member.kind == kind.GenerateBlock and not member.isUninstantiated:
                visit(member)
            elif member.kind == kind.TypeAlias and member.targetType.type.kind == kind.EnumType:
                visit(member.targetType.type)

    for package in compilation.getPackages():
        visit(package)
    for top in compilation.getRoot().topInstances:
        visit(top.body)
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("repo")
    parser.add_argument("--show", type=int, default=40, help="disagreements to list")
    parser.add_argument("--unknown", action="store_true",
                        help="list what astli leaves unknown instead")
    args = parser.parse_args()

    theirs = slang(astli(args.repo, True))
    # A header included in several places may have a value in each.
    ours = {}
    for line in astli(args.repo, False):
        match = LINE.match(line)
        if match:
            ours.setdefault(match.group(1), set()).add(match.group(3))

    agree, unknown, differ = 0, 0, []
    for place, (name, text) in sorted(theirs.items()):
        if place not in ours:
            continue
        mine = ", ".join(sorted(ours[place]))
        if text in ours[place]:
            agree += 1
        # An aggregate astli knows only in part is unknown as a whole.
        elif "unknown" in mine:
            unknown += 1
            if args.unknown:
                differ.append(f"{place} {name}: astli unknown, slang {text}")
        else:
            differ.append(f"{place} {name}: astli {mine}, slang {text}")
    print(f"{agree} agree, {len(differ)} differ, {unknown} unknown to astli "
          f"of {len(theirs)} slang evaluates and {len(ours)} astli lists")
    for line in differ[: args.show]:
        print(line)
    return 1 if differ and not args.unknown else 0


if __name__ == "__main__":
    sys.exit(main())
