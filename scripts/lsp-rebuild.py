#!/usr/bin/env python3
"""Measures how long `astli lsp` takes to load a design, and to load it again
after an edit to one unit and to a header many units include, and what the
server keeps resident.

    cargo build --release -p astli-cli
    scripts/lsp-rebuild.py target/release/astli corpus/opentitan \\
        hw/ip/uart/rtl/uart_core.sv hw/ip/prim/rtl/prim_assert.sv [--runs N] [--edits N]

Each run starts a server, opens the unit and waits for its design to load,
then edits the unit `--edits` times, opens the header and edits it as often,
each edit appending a comment and waiting for the load after it. A load's
time is what the server logs it took, which leaves out the wait for edits to
settle; the units lowered are what it logs too.
"""

from __future__ import annotations

import argparse
import json
import queue
import re
import statistics
import subprocess
import sys
import threading
import time
from pathlib import Path

LOADED = re.compile(r"^loaded .*: (\d+) units, (\d+) lowered, in (\d+) ms$")


class Server:
    """A server over pipes, its log lines read on a thread of their own."""

    def __init__(self, binary: Path, root: Path):
        self.process = subprocess.Popen(
            [str(binary), "lsp"],
            cwd=root,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        assert self.process.stdin and self.process.stdout
        self.stdin, self.stdout = self.process.stdin, self.process.stdout
        self.logged: queue.Queue[str] = queue.Queue()
        self.answers: queue.Queue[dict] = queue.Queue()
        threading.Thread(target=self.read, daemon=True).start()
        self.ids = 0

    def read(self) -> None:
        out = self.stdout
        while True:
            length = None
            while (line := out.readline().decode()) not in ("\r\n", ""):
                if line.lower().startswith("content-length:"):
                    length = int(line.split(":")[1])
            if length is None:
                return
            message = json.loads(out.read(length))
            if message.get("method") == "window/logMessage":
                self.logged.put(message["params"]["message"])
            elif "id" in message and "method" not in message:
                self.answers.put(message)

    def send(self, message: dict) -> None:
        body = json.dumps({"jsonrpc": "2.0", **message}).encode()
        self.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
        self.stdin.flush()

    def request(self, method: str, params: dict | None) -> dict:
        self.ids += 1
        self.send({"id": self.ids, "method": method, "params": params})
        return self.answers.get(timeout=60)

    def notify(self, method: str, params: dict | None) -> None:
        self.send({"method": method, "params": params})

    def loaded(self) -> tuple[int, int, int]:
        """The next load's units, units lowered and milliseconds."""
        while True:
            said = self.logged.get(timeout=300)
            if match := LOADED.match(said):
                units, lowered, took = (int(it) for it in match.groups())
                return units, lowered, took
            if "panicked" in said:
                sys.exit(f"the server says: {said}")

    def rss(self) -> int:
        """Resident memory, in MB."""
        ps = ["ps", "-o", "rss=", "-p", str(self.process.pid)]
        return int(subprocess.check_output(ps)) // 1024

    def stop(self) -> None:
        self.request("shutdown", None)
        self.notify("exit", None)
        self.process.wait(timeout=30)


class Document:
    def __init__(self, server: Server, path: Path):
        self.server, self.uri = server, path.resolve().as_uri()
        self.text, self.version = path.read_text(), 1
        item = {"uri": self.uri, "languageId": "systemverilog"}
        item.update(version=1, text=self.text)
        server.notify("textDocument/didOpen", {"textDocument": item})

    def append(self) -> None:
        self.version += 1
        self.text += f"// edit {self.version}\n"
        document = {"uri": self.uri, "version": self.version}
        changes = [{"text": self.text}]
        self.server.notify(
            "textDocument/didChange",
            {"textDocument": document, "contentChanges": changes},
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("root", type=Path)
    parser.add_argument("unit", type=Path)
    parser.add_argument("header", type=Path)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--edits", type=int, default=3)
    args = parser.parse_args()
    binary, root = args.binary.resolve(), args.root.resolve()

    steps: dict[str, list[tuple[int, int, int]]] = {}
    rss: dict[str, list[int]] = {}
    for _ in range(args.runs):
        server = Server(binary, root)
        server.request(
            "initialize",
            {"processId": None, "rootUri": root.as_uri(), "capabilities": {}},
        )
        server.notify("initialized", {})
        unit = Document(server, root / args.unit)
        steps.setdefault("cold", []).append(server.loaded())
        rss.setdefault("after the cold load", []).append(server.rss())
        for _ in range(args.edits):
            unit.append()
            steps.setdefault("unit edited", []).append(server.loaded())
        header = Document(server, root / args.header)
        steps.setdefault("header opened", []).append(server.loaded())
        for _ in range(args.edits):
            header.append()
            steps.setdefault("header edited", []).append(server.loaded())
        # What a drop on the pool frees has had time to go.
        time.sleep(1)
        rss.setdefault("after the edits", []).append(server.rss())
        server.stop()

    print(f"{'step':<14} {'units':>6} {'lowered':>8} {'ms':>6} {'range':>12}")
    for step, loads in steps.items():
        units = statistics.median(it[0] for it in loads)
        lowered = sorted({it[1] for it in loads})
        took = [it[2] for it in loads]
        span = f"{min(took)}-{max(took)}"
        said = "/".join(map(str, lowered))
        print(
            f"{step:<14} {units:>6.0f} {said:>8} {statistics.median(took):>6.0f} {span:>12}"
        )
    for when, sizes in rss.items():
        print(
            f"resident {when}: {statistics.median(sizes):.0f} MB ({min(sizes)}-{max(sizes)})"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
