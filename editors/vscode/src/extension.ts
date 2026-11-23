import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";

const exe = process.platform === "win32" ? "astli.exe" : "astli";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (e) => {
      if (e.affectsConfiguration("astli.path")) {
        await stop();
        await start(context);
      }
    }),
  );
  await start(context);
}

export function deactivate() {
  return stop();
}

async function start(context: vscode.ExtensionContext) {
  const command = findBinary(context);
  if (!command) {
    vscode.window.showErrorMessage(
      "astli: no `astli` found. Install astli, or set `astli.path` to the binary.",
    );
    return;
  }
  client = new LanguageClient(
    "astli",
    "astli",
    // No `transport`: naming stdio makes the client append `--stdio`, which
    // `astli lsp` does not take. Without one it talks over stdio all the same.
    { command, args: ["lsp"] },
    {
      documentSelector: [
        { scheme: "file", language: "systemverilog" },
        { scheme: "file", language: "verilog" },
      ],
    },
  );
  await client.start();
}

async function stop() {
  const old = client;
  client = undefined;
  await old?.stop();
}

// The setting first, then `PATH`, so that an install the user manages
// outranks the binary this extension happened to ship with.
function findBinary(context: vscode.ExtensionContext): string | undefined {
  const configured = vscode.workspace.getConfiguration("astli").get<string>("path");
  if (configured) {
    return configured;
  }
  for (const dir of (process.env.PATH ?? "").split(path.delimiter)) {
    const candidate = path.join(dir, exe);
    if (dir && isFile(candidate)) {
      return candidate;
    }
  }
  const bundled = context.asAbsolutePath(path.join("bundled", exe));
  return isFile(bundled) ? bundled : undefined;
}

function isFile(file: string): boolean {
  try {
    return fs.statSync(file).isFile();
  } catch {
    return false;
  }
}
