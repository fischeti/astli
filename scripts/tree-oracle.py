#!/usr/bin/env python3
"""Compares the instance tree astli elaborates from a top with slang's.

    scripts/tree-oracle.py corpus/axi --top axi_xbar [--top ...] [--show N]

The repository is one design, its files as the `consts` example of
astli-sema selects them. astli's tree is `astli elab`'s; slang's comes from
pyslang (`pip install pyslang`). Each side is reduced to a set of paths: an
instance with its definition, a gate's or a UDP's with its keyword or name,
a generate block built, an instance array once, by its name, its first
element standing for the rest. An unnamed instance has no path. Prints how many
paths agree and each that only one side has; a path astli leaves undecided
counts as neither.
"""

import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import pyslang

ROOT = Path(__file__).resolve().parent.parent


def filelist(repo):
    args = ["cargo", "run", "-q", "--release", "-p", "astli-sema", "--example", "consts", "--"]
    out = subprocess.run(args + ["--files", repo], cwd=ROOT, check=True,
                         capture_output=True, text=True)
    return out.stdout.splitlines()


def astli(lines, tops):
    with tempfile.NamedTemporaryFile("w", suffix=".f") as f:
        f.write("\n".join(lines) + "\n")
        f.flush()
        args = ["cargo", "run", "-q", "--release", "--", "elab", "-f", f.name]
        for top in tops:
            args += ["--top", top]
        out = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    if out.returncode != 0:
        sys.exit(out.stderr)
    paths, undecided = set(), set()
    stack = []
    for line in out.stdout.splitlines():
        depth = (len(line) - len(line.lstrip(" "))) // 2
        text = line.strip()
        if depth == 0:
            stack = [text]
            paths.add(text)
            continue
        del stack[depth:]
        name, _, rest = text.partition(": ")
        name = re.sub(r" \[.*\]$", "", name)
        stack.append(name)
        path = ".".join(stack)
        if rest == "undecided":
            # Each arm that may be built.
            for arm in name.split(", "):
                undecided.add(".".join(stack[:-1] + [arm]))
        elif rest.endswith(", declared nowhere"):
            paths.add(f"{path}: {rest[: -len(', declared nowhere')]}?")
        elif rest:
            paths.add(f"{path}: {re.sub(r' #[(].*$', '', rest)}")
        else:
            paths.add(path)
    return paths, undecided


def slang(lines, tops):
    dirs = [line[len("+incdir+"):] for line in lines if line.startswith("+incdir+")]
    files = [line for line in lines if not line.startswith("+")]
    preprocessor = pyslang.parsing.PreprocessorOptions()
    preprocessor.additionalIncludePaths = dirs
    compilation_options = pyslang.ast.CompilationOptions()
    compilation_options.topModules = set(tops)
    bag = pyslang.Bag([preprocessor, compilation_options])
    sources = pyslang.SourceManager()
    trees = [pyslang.syntax.SyntaxTree.fromFile(f, sources, bag) for f in files]
    compilation = pyslang.ast.Compilation(bag)
    for tree in trees:
        compilation.addSyntaxTree(tree)

    kind = pyslang.ast.SymbolKind
    paths = set()

    def visit(scope, path):
        for member in scope:
            if member.kind == kind.Instance:
                here = f"{path}.{member.name}"
                paths.add(f"{here}: {member.definition.name}")
                visit(member.body, here)
            elif member.kind == kind.PrimitiveInstance and member.name:
                paths.add(f"{path}.{member.name}: {member.primitiveType.name}")
            elif member.kind == kind.InstanceArray:
                elements = list(member.elements)
                while elements and elements[0].kind == kind.InstanceArray:
                    elements = list(elements[0].elements)
                if elements and elements[0].kind == kind.PrimitiveInstance:
                    paths.add(f"{path}.{member.name}: {elements[0].primitiveType.name}")
                elif elements:
                    here = f"{path}.{member.name}"
                    paths.add(f"{here}: {elements[0].definition.name}")
                    visit(elements[0].body, here)
            elif member.kind == kind.UninstantiatedDef:
                paths.add(f"{path}.{member.name}: {member.definitionName}?")
            elif member.kind == kind.GenerateBlock and not member.isUninstantiated:
                here = f"{path}.{member.externalName or member.name}"
                paths.add(here)
                visit(member, here)
            elif member.kind == kind.GenerateBlockArray:
                name = member.externalName or member.name
                for entry in member.entries:
                    if entry.isUninstantiated:
                        continue
                    here = f"{path}.{name}[{entry.arrayIndex}]"
                    paths.add(here)
                    visit(entry, here)

    for top in compilation.getRoot().topInstances:
        if top.name in tops:
            paths.add(top.name)
            visit(top.body, top.name)
    return paths


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("repo")
    parser.add_argument("--top", action="append", required=True)
    parser.add_argument("--show", type=int, default=40, help="differences to list")
    args = parser.parse_args()

    lines = filelist(args.repo)
    ours, undecided = astli(lines, args.top)
    theirs = slang(lines, args.top)

    def decided(path):
        return not any(path == u or path.startswith(u + ".") or path.startswith(u + "[")
                       for u in undecided)

    theirs = {path for path in theirs if decided(path.split(": ")[0])}
    only_ours, only_theirs = sorted(ours - theirs), sorted(theirs - ours)
    print(f"{len(ours & theirs)} agree, {len(only_ours)} only astli's, "
          f"{len(only_theirs)} only slang's, {len(undecided)} undecided")
    for path in only_ours[: args.show]:
        print(f"astli: {path}")
    for path in only_theirs[: args.show]:
        print(f"slang: {path}")
    return 1 if only_ours or only_theirs else 0


if __name__ == "__main__":
    sys.exit(main())
