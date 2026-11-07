const vscode = require('vscode');
const cp = require('child_process');
const path = require('path');

let diagnostics;
let output;

function compiler(document) {
  const configured = vscode.workspace.getConfiguration('magic').get('compilerPath', 'magicc');
  if (configured && configured !== 'magicc') return configured;
  const root = vscode.workspace.getWorkspaceFolder(document.uri);
  if (root) {
    const candidate = path.join(root.uri.fsPath, 'target', 'release', 'magicc');
    return candidate;
  }
  return 'magicc';
}

function parseDiagnostics(text) {
  const out = [];
  const re = /^(.+?):(\d+)(?::(\d+))?:\s*(.*)$/;
  for (const line of text.split(/\r?\n/)) {
    const m = re.exec(line.trim());
    if (!m) continue;
    const lineNo = Math.max(0, parseInt(m[2], 10) - 1);
    const colNo = m[3] ? Math.max(0, parseInt(m[3], 10) - 1) : 0;
    const range = new vscode.Range(lineNo, colNo, lineNo, colNo + 1);
    out.push(new vscode.Diagnostic(range, m[4], vscode.DiagnosticSeverity.Error));
  }
  return out;
}

async function check(document) {
  if (!document) document = vscode.window.activeTextEditor?.document;
  if (!document || document.languageId !== 'magic') {
    vscode.window.showInformationMessage('MAGIC: open a .magic file first');
    return;
  }
  await document.save();
  const exe = compiler(document);
  const args = ['check', document.uri.fsPath, '--backend', 'c'];
  cp.execFile(exe, args, { cwd: path.dirname(document.uri.fsPath), timeout: 20000 }, (err, stdout, stderr) => {
    diagnostics.clear();
    const text = `${stdout || ''}\n${stderr || ''}`;
    const items = parseDiagnostics(text);
    diagnostics.set(document.uri, items);
    if (err && items.length === 0) {
      output.clear();
      output.appendLine(text.trim() || String(err));
      output.show(true);
      vscode.window.showErrorMessage('MAGIC: compiler failed; see Output → MAGIC');
    } else if (items.length === 0) {
      vscode.window.setStatusBarMessage('MAGIC: check OK', 3000);
    } else {
      vscode.window.showErrorMessage(`MAGIC: ${items.length} diagnostic(s)`);
    }
  });
}

function build(document) {
  if (!document) document = vscode.window.activeTextEditor?.document;
  if (!document || document.languageId !== 'magic') return;
  document.save();
  const exe = compiler(document);
  const terminal = vscode.window.createTerminal('MAGIC build');
  terminal.show();
  terminal.sendText(`${exe} build ${JSON.stringify(document.uri.fsPath)} --fast`, true);
}

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection('magic');
  output = vscode.window.createOutputChannel('MAGIC');
  context.subscriptions.push(diagnostics, output);
  context.subscriptions.push(vscode.commands.registerCommand('magic.check', check));
  context.subscriptions.push(vscode.commands.registerCommand('magic.build', build));
  context.subscriptions.push(vscode.workspace.onDidSaveTextDocument(doc => {
    if (doc.languageId === 'magic') check(doc);
  }));
}

function deactivate() {}

module.exports = { activate, deactivate };
