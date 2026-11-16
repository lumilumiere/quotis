// Controls only a new fixture-backed Quotis process through its own debugging port.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import http from 'node:http';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';

const executable = path.resolve(process.argv[2] || 'target/release/quotis.exe');
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'quotis-desktop-smoke-'));
const sleep = ms => new Promise(r => setTimeout(r, ms));
const server = net.createServer();
server.listen(0, '127.0.0.1');
await once(server, 'listening');
const port = server.address().port;
await new Promise(r => server.close(r));
const env = { ...process.env };
for (const name of ['HOME', 'USERPROFILE', 'APPDATA', 'LOCALAPPDATA', 'XDG_CONFIG_HOME', 'CLAUDE_CONFIG_DIR', 'CODEX_HOME', 'QUOTIS_DATA_DIR']) {
  env[name] = path.join(root, name);
  fs.mkdirSync(env[name], { recursive: true });
}
delete env.OPENROUTER_API_KEY;
delete env.DEEPSEEK_API_KEY;
env.WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = `--remote-debugging-port=${port} --remote-debugging-address=127.0.0.1`;
env.WEBVIEW2_USER_DATA_FOLDER = path.join(root, 'webview');
const providers = Object.fromEntries(['claude','codex','copilot','cursor','gemini','qwen','opencode','openrouter','deepseek'].map(id => [id, { enabled: id === 'gemini', dir: path.join(root, id) }]));
fs.mkdirSync(path.join(root, 'gemini/tmp'), { recursive: true });
fs.writeFileSync(path.join(root, 'gemini/tmp/session.jsonl'), JSON.stringify({ timestamp: new Date().toISOString(), tokens: { total: 42 } }) + '\n');
fs.writeFileSync(path.join(env.QUOTIS_DATA_DIR, 'settings.json'), JSON.stringify({ providers, always_on_top: false, autostart: false }));
const started = performance.now();
const app = spawn(executable, [], { env, windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
let stderr = '';
app.stderr.on('data', b => stderr += b);
app.on('error', e => { stderr += e.message; });
const sockets = [];
let completed = false;
const report = { executable, root, pid: app.pid, checks: [], measurements: {} };

async function until(fn, timeout = 20000) {
  const end = performance.now() + timeout;
  let lastError;
  while (performance.now() < end) {
    try { const result = await fn(); if (result) return result; } catch (e) { lastError = e.message; }
    await sleep(100);
  }
  throw new Error(`Timed out waiting for fixture app: ${lastError || 'condition false'}`);
}
async function connect(target) {
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  sockets.push(ws);
  await once(ws, 'open');
  let seq = 0;
  const pending = new Map();
  ws.addEventListener('message', ({ data }) => {
    const m = JSON.parse(data);
    if (pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  });
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++seq;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
    pending.set(id, m => { clearTimeout(timer); m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result); });
    ws.send(JSON.stringify({ id, method, params }));
  });
  return {
    send,
    evaluate: async expression => {
      const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
      return r.result.value;
    },
  };
}
const targets = () => new Promise((resolve, reject) => {
  const req = http.get({ host: '127.0.0.1', port, path: '/json/list', agent: false }, res => {
    let body = '';
    res.on('data', b => body += b);
    res.on('end', () => { try { resolve(JSON.parse(body)); } catch (e) { reject(e); } });
  });
  req.on('error', reject);
  req.setTimeout(1000, () => req.destroy(new Error('CDP discovery timeout')));
});
function sampleTree() {
  const script = `$all = Get-CimInstance Win32_Process; $ids = [Collections.Generic.HashSet[int]]::new(); [void]$ids.Add(${app.pid}); do { $count = $ids.Count; foreach ($p in $all) { if ($ids.Contains([int]$p.ParentProcessId)) { [void]$ids.Add([int]$p.ProcessId) } } } while ($ids.Count -ne $count); Get-Process -Id @($ids) -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,CPU,PrivateMemorySize64,WorkingSet64 | ConvertTo-Json -Compress`;
  return JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { encoding: 'utf8', windowsHide: true }));
}
try {
  await until(() => fs.existsSync(path.join(env.QUOTIS_DATA_DIR, 'snapshot.json')));
  report.measurements.startupToSnapshotMs = Math.round(performance.now() - started);
  const target = await until(async () => (await targets()).find(t => t.type === 'page' && !t.url.includes('settings.html')));
  const main = await connect(target);
  const invoke = async (command, args = {}) => main.evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`);
  await until(() => main.evaluate('document.getElementById("pill-name").textContent === "Gemini CLI"'));
  report.checks.push('startup and fixture collection');
  let settings = await invoke('get_settings');
  for (const mode of ['island', 'notch', 'panel']) {
    settings = { ...settings, mode };
    await invoke('save_settings', { settings });
    await until(() => main.evaluate(`document.getElementById('island').dataset.mode === ${JSON.stringify(mode)}`));
    if (mode !== 'panel') {
      await main.evaluate('document.getElementById("pill").click()');
      await until(() => main.evaluate('document.getElementById("pill").getAttribute("aria-expanded") === "true"'));
      await main.evaluate('window.dispatchEvent(new KeyboardEvent("keydown", {key:"Escape"}))');
      await until(() => main.evaluate('document.getElementById("pill").getAttribute("aria-expanded") === "false"'));
    }
    await sleep(350);
    const screenshot = await main.send('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync(path.join(root, `${mode}.png`), Buffer.from(screenshot.data, 'base64'));
    report.checks.push(`${mode} mode and scoped DOM interactions`);
  }
  await invoke('open_settings');
  const settingsTarget = await until(async () => (await targets()).find(t => t.url.includes('settings.html')));
  const form = await connect(settingsTarget);
  assert.equal(await form.evaluate('document.getElementById("mode").value'), 'panel');
  report.checks.push('settings window loads persisted values');
  const hostile = '<img id="injected" src=x onerror="window.attacked=true">';
  const snapshot = await invoke('get_usage');
  snapshot.providers.find(p => p.id === 'gemini').notes = [hostile];
  await invoke('plugin:event|emit', { event: 'usage', payload: snapshot });
  await until(() => main.evaluate(`document.getElementById('list').textContent.includes(${JSON.stringify(hostile)})`));
  assert.equal(await main.evaluate('document.getElementById("injected") === null && window.attacked !== true'), true);
  report.checks.push('actual renderer treats hostile provider text as text');
  settings = { ...settings, mode: 'island' };
  await invoke('save_settings', { settings });
  await sleep(600);
  // Target the HWND belonging to this exact child process. No global mouse/keyboard input.
  const move = `$p = Get-Process -Id ${app.pid}; Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class QuotisSmokeMove { [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int height, uint flags); }'; if ($p.MainWindowHandle -eq 0) { throw 'fixture app has no HWND' }; if (-not [QuotisSmokeMove]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, 70, 110, 0, 0, 0x15)) { throw 'move failed' }`;
  execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', move], { windowsHide: true });
  const dragged = await until(async () => {
    const s = await invoke('get_settings');
    return s.position?.[1] === 110 ? s.position : false;
  }, 3000);
  console.log(JSON.stringify({ stage: 'ready-to-quit', root, dragged }));
  invoke('quit').catch(() => {});
  await until(() => app.exitCode !== null);
  const persisted = JSON.parse(fs.readFileSync(path.join(env.QUOTIS_DATA_DIR, 'settings.json'), 'utf8'));
  assert.deepEqual(persisted.position, dragged, 'immediate quit must flush the dragged position');
  assert.equal(app.exitCode, 0);
  report.checks.push('drag followed by immediate normal quit persists position');
  completed = true;
} finally {
  for (const socket of sockets) socket.close();
  if (app.exitCode === null) app.kill();
  report.result = completed ? 'PASS' : 'FAIL';
  report.stderr = stderr;
  fs.writeFileSync(path.join(root, 'report.json'), JSON.stringify(report, null, 2));
  console.log(JSON.stringify(report, null, 2));
}
