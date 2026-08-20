// Live-Chrome evidence rig for Local Browser Bridge 0.11.0.
// Covers the two things 0.11 adds beyond the 0.10 matrix:
//   1. the dialog defect found live against 0.10 must now be fixed;
//   2. cross-origin (OOPIF) observation and clicking must work on a real
//      out-of-process iframe served from a second local origin.
import { spawn, execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import WebSocket from 'ws';

const RIG = process.env.RIG;
const SERVER_BIN = process.argv[2];
const EXT_DIR = process.argv[3];
const CHROME = process.argv[4];
const OUT = process.argv[5];
const CDP_PORT = 9337;
const PROFILE = `${RIG}/v011-profile`;

mkdirSync(OUT, { recursive: true });
const results = [];
const log = (m) => { console.log(m); appendFileSync(`${OUT}/rig.log`, `${m}\n`); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function record(name, ok, detail) {
  results.push({ name, ok, detail, at: new Date().toISOString() });
  log(`${ok ? 'PASS' : 'FAIL'} ${name} :: ${typeof detail === 'string' ? detail : JSON.stringify(detail).slice(0, 320)}`);
}

// ---------- two separate origins so Chrome really creates an OOPIF ----------
function serve(dir, port) {
  const server = http.createServer((req, res) => {
    const file = path.join(dir, req.url === '/' ? 'index.html' : req.url.split('?')[0]);
    fs.readFile(file, (err, data) => {
      if (err) { res.writeHead(404); res.end('not found'); return; }
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      res.end(data);
    });
  });
  server.listen(port, '127.0.0.1');
  return server;
}
// parent is reached over localhost, child over 127.0.0.1: different sites -> separate process
const parentServer = serve(`${RIG}/frames`, 8099);
const childServer = serve(`${RIG}/frames-child`, 8100);

// ---------- bridge server ----------
log('starting packaged server');
const server = spawn(SERVER_BIN, [], { stdio: ['ignore', 'pipe', 'pipe'] });
let token = '';
let serverOut = '';
const onOut = (d) => { serverOut += d.toString(); const m = serverOut.match(/Extension token:\s*(\S+)/); if (m) token = m[1]; };
server.stdout.on('data', onOut); server.stderr.on('data', onOut);
for (let i = 0; i < 60 && !token; i++) await sleep(250);
if (!token) { log(serverOut); throw new Error('no token from server'); }
const BASE = 'http://127.0.0.1:17373';
const H = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' };
const api = async (method, params = {}, extra = {}) => {
  const res = await fetch(`${BASE}/api/v1/command`, { method: 'POST', headers: H, body: JSON.stringify({ method, params, ...extra }) });
  return { status: res.status, body: await res.json() };
};
const state = async () => {
  const body = await (await fetch(`${BASE}/api/state`, { headers: H })).json();
  return body.state ?? body;
};

// ---------- chrome ----------
const chrome = spawn(CHROME, [
  `--user-data-dir=${PROFILE}`, `--load-extension=${EXT_DIR}`,
  `--remote-debugging-port=${CDP_PORT}`, '--no-first-run', '--no-default-browser-check',
  '--window-position=0,0', '--window-size=1280,900', 'about:blank',
], { stdio: 'ignore' });
await sleep(7000);

const cdpTargets = async () => (await fetch(`http://127.0.0.1:${CDP_PORT}/json/list`)).json();
async function evalIn(target, expression) {
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => ws.once('open', r));
  const out = await new Promise((resolve) => {
    ws.on('message', (d) => { const m = JSON.parse(d); if (m.id === 1) resolve(m); });
    ws.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } }));
  });
  ws.close();
  return out.result?.result?.value;
}
let sw = null;
for (let i = 0; i < 30 && !sw; i++) {
  for (const t of (await cdpTargets()).filter((t) => t.type === 'service_worker')) {
    if (await evalIn(t, 'chrome.runtime.getManifest().name').catch(() => null) === 'Local Browser Bridge') { sw = t; break; }
  }
  if (!sw) await sleep(1000);
}
if (!sw) throw new Error('bridge service worker not found');
record('extension.loaded', true, { version: await evalIn(sw, 'chrome.runtime.getManifest().version') });
await evalIn(sw, `chrome.storage.local.set({token:${JSON.stringify(token)},port:17373,enabled:true,fullAccess:true}).then(()=>'ok')`);
let connected = null;
for (let i = 0; i < 40 && !connected; i++) { const s = await state(); if (s.connected && s.extension) connected = s; else await sleep(750); }
record('extension.connected', Boolean(connected), connected && { version: connected.extension.version });
if (!connected) throw new Error('extension never connected');

const osShot = (name) => {
  try { execFileSync('/usr/sbin/screencapture', ['-x', '-R', '0,0,1280,620', `${OUT}/${name}`]); return name; }
  catch (e) { return `unavailable: ${e.message}`; }
};
async function observe(tabId) {
  const r = await api('page.observe', { tabId });
  const snap = r.body.result?.snapshot;
  return { status: r.status, body: r.body, snapshot: snap, gen: snap?.generation, elements: snap?.elements || [], frames: snap?.frames, frameSummary: snap?.frameSummary };
}

// ---------- open the cross-origin parent page ----------
await fetch(`http://127.0.0.1:${CDP_PORT}/json/new?http://localhost:8099/parent.html`, { method: 'PUT' });
await sleep(3000);
const tabs = await api('tabs.list');
const tab = (tabs.body.result?.tabs || []).find((t) => String(t.url).includes('parent.html'));
record('tabs.list.cross-origin-parent', Boolean(tab), tab && { url: tab.url, title: tab.title });
if (!tab) throw new Error('parent tab not found');
const tabId = tab.id;

const start = await api('browser.control.start', { tabId, ttlMs: 600000 });
record('browser.control.start', start.status === 200, { active: (start.body.result?.control ?? start.body.result)?.active });
await sleep(1200);

// ---------- 1. cross-origin frame observation (new in 0.11) ----------
let o = await observe(tabId);
const frameElements = o.elements.filter((e) => e.crossOrigin === true || (e.frameRef && e.frameRef !== null));
record('frames.observed', Array.isArray(o.frames) && o.frames.length > 0, { frames: o.frames, frameSummary: o.frameSummary });
record('frames.elements-merged', frameElements.length > 0, { count: frameElements.length, sample: frameElements.slice(0, 3).map((e) => `${e.ref}=${e.role}:${e.name} origin=${e.frameUrlOrigin}`) });
const childButton = o.elements.find((e) => e.name === 'Cross origin child button');
record('frames.child-button-visible', Boolean(childButton), childButton && { ref: childButton.ref, bounds: childButton.bounds, crossOrigin: childButton.crossOrigin, frameUrlOrigin: childButton.frameUrlOrigin });
record('frames.ref-grammar', Boolean(childButton && /^[a-z0-9-]+\.f\d+\.e\d+$/.test(childButton.ref)), childButton?.ref);
record('evidence.frames-screenshot', true, osShot('10-cross-origin-frames.png'));

// ---------- 2. click INSIDE the cross-origin frame ----------
if (childButton) {
  const click = await api('page.click', { tabId, ref: childButton.ref, generation: o.gen });
  record('frames.click-in-cross-origin-frame', click.status === 200, click.body.result?.clicked ?? click.body.error);
  await sleep(500);
  const childState = await api('page.evaluate', { tabId, expression: "String(document.querySelector('iframe') ? 'checked-via-frame' : 'no-frame')" });
  record('frames.click-evaluate-ran', childState.status === 200, JSON.stringify(childState.body.result).slice(0, 120));
  // read the child's own log through CDP, since page.evaluate cannot cross the origin boundary
  const childTarget = (await cdpTargets()).find((t) => String(t.url).includes('child.html'));
  const childLog = childTarget ? await evalIn(childTarget, "String(document.getElementById('child-log').textContent)") : 'child target not found';
  record('frames.trusted-click-landed-in-child', /child-click:true/.test(String(childLog)), childLog);
  record('evidence.frames-click-screenshot', true, osShot('11-cross-origin-click.png'));
}

// ---------- 3. dialog regression: must NOT revoke the lease any more ----------
await api('page.evaluate', { tabId, expression: "setTimeout(function(){window.confirm('bridge dialog regression')},250); 'scheduled'" });
await sleep(2500);
const dialogState = await state();
record('dialog.pending-state-visible', Boolean(dialogState.pendingDialog), dialogState.pendingDialog || 'pendingDialog stayed null');
const duringDialog = await api('page.observe', { tabId });
record('dialog.blocks-with-blocked_by_dialog', duringDialog.body.error?.code === 'BLOCKED_BY_DIALOG',
  { status: duringDialog.status, code: duringDialog.body.error?.code, taxonomy: duringDialog.body.taxonomy?.code });
const leaseDuring = await api('browser.control.status');
record('dialog.lease-survives-open-dialog', leaseDuring.body.result?.active === true,
  { active: leaseDuring.body.result?.active, revocation: leaseDuring.body.result?.revocation?.reason });
record('evidence.dialog-screenshot', true, osShot('12-dialog-open-lease-alive.png'));
const handled = await api('page.handleDialog', { tabId, accept: false });
record('page.handleDialog', handled.status === 200, handled.body.result || handled.body.error);
await sleep(900);
const afterDialog = await observe(tabId);
record('dialog.observe-works-after-handling', afterDialog.status === 200, { turn: afterDialog.body.result?.control?.turn, elements: afterDialog.elements.length });
const leaseAfter = await api('browser.control.status');
record('dialog.lease-still-active-after-handling', leaseAfter.body.result?.active === true, { active: leaseAfter.body.result?.active });

// ---------- 4. still fail-closed: a real navigation must still revoke ----------
const nav = await api('page.navigate', { tabId, url: 'http://localhost:8099/parent.html' });
record('navigation.authorized-still-works', nav.status === 200, nav.body.result ? 'navigated' : nav.body.error);

// ---------- finish ----------
const finalState = await state();
writeFileSync(`${OUT}/results.json`, JSON.stringify({
  recordedAt: new Date().toISOString(),
  target: 'packaged v0.11.0 release artifacts',
  chrome: 'Google Chrome for Testing 152.0.7977.54 (isolated profile)',
  crossOriginSetup: 'parent http://localhost:8099 embeds child http://127.0.0.1:8100 (separate sites => out-of-process iframe)',
  extension: finalState.extension && { version: finalState.extension.version, mode: finalState.extension.mode },
  passed: results.filter((r) => r.ok).length,
  total: results.length,
  results,
}, null, 2));
log(`\n${results.filter((r) => r.ok).length}/${results.length} checks passed`);
try { for (const t of (await cdpTargets()).filter((t) => t.type === 'page')) { const ws = new WebSocket(t.webSocketDebuggerUrl); await new Promise((r) => ws.once('open', r)); ws.send(JSON.stringify({ id: 1, method: 'Page.handleJavaScriptDialog', params: { accept: false } })); await sleep(120); ws.close(); } } catch {}
try { chrome.kill(); } catch {}
try { server.kill(); } catch {}
parentServer.close(); childServer.close();
await sleep(400);
process.exit(0);
