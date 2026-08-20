// Live-Chrome evidence rig for Local Browser Bridge 0.10.0.
// Starts the packaged server, loads the packaged extension in an isolated
// Chrome for Testing profile, and exercises the 0.10 feature matrix over the
// bearer REST API, recording machine-readable results plus screenshots.
import { spawn, execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import net from 'node:net';
import WebSocket from 'ws';

const RIG = process.env.RIG;
const SERVER_BIN = process.argv[2];
const EXT_DIR = process.argv[3];
const CHROME = process.argv[4];
const OUT = process.argv[5];
const CDP_PORT = 9335;
const PROFILE = `${RIG}/evidence-profile`;

mkdirSync(OUT, { recursive: true });
const results = [];
const log = (m) => { console.log(m); appendFileSync(`${OUT}/rig.log`, `${m}\n`); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function record(name, ok, detail) {
  results.push({ name, ok, detail, at: new Date().toISOString() });
  log(`${ok ? 'PASS' : 'FAIL'} ${name} :: ${typeof detail === 'string' ? detail : JSON.stringify(detail).slice(0, 300)}`);
}

// ---------- server ----------
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
log('launching isolated Chrome for Testing with the packaged extension');
const chrome = spawn(CHROME, [
  `--user-data-dir=${PROFILE}`, `--load-extension=${EXT_DIR}`,
  `--remote-debugging-port=${CDP_PORT}`, '--no-first-run', '--no-default-browser-check',
  '--window-position=0,0', '--window-size=1280,860', 'about:blank',
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

let bridgeSw = null;
for (let i = 0; i < 30 && !bridgeSw; i++) {
  for (const t of (await cdpTargets()).filter((t) => t.type === 'service_worker')) {
    const n = await evalIn(t, 'chrome.runtime.getManifest().name').catch(() => null);
    if (n === 'Local Browser Bridge') { bridgeSw = t; break; }
  }
  if (!bridgeSw) await sleep(1000);
}
if (!bridgeSw) throw new Error('bridge extension service worker not found');
record('extension.loaded', true, { id: new URL(bridgeSw.url).host, version: await evalIn(bridgeSw, 'chrome.runtime.getManifest().version') });
await evalIn(bridgeSw, `chrome.storage.local.set({token:${JSON.stringify(token)},port:17373,enabled:true,fullAccess:true}).then(()=>'ok')`);

let connected = null;
for (let i = 0; i < 40 && !connected; i++) {
  const s = await state();
  if (s.connected === true && s.extension) connected = s; else await sleep(750);
}
record('extension.connected', Boolean(connected), connected ? { version: connected.extension.version, browser: connected.extension.browser, mode: connected.extension.mode } : 'never connected');
if (!connected) throw new Error('extension never connected');

// ---------- demo target ----------
await fetch(`http://127.0.0.1:${CDP_PORT}/json/new?${BASE}/demo`, { method: 'PUT' });
await sleep(2500);
const tabs = await api('tabs.list');
const demoTab = (tabs.body.result?.tabs || []).find((t) => String(t.url).includes('/demo'));
record('tabs.list', Boolean(demoTab), demoTab && { id: demoTab.id, url: demoTab.url, title: demoTab.title });
if (!demoTab) throw new Error('demo tab not found');
const tabId = demoTab.id;

const shot = async (name) => {
  const s = await state();
  const url = s.observation?.screenshotUrl;
  if (!url) return 'no screenshot bound';
  const res = await fetch(`${BASE}${url}`, { headers: H });
  if (!res.ok) return `screenshot ${res.status}`;
  const buf = Buffer.from(await res.arrayBuffer());
  writeFileSync(`${OUT}/${name}`, buf);
  return { file: name, bytes: buf.length, contentHash: s.observation?.contentHash, width: s.observation?.screenshotWidth, height: s.observation?.screenshotHeight };
};
const osShot = (name) => {
  try { execFileSync('/usr/sbin/screencapture', ['-x', '-R', '0,0,1280,860', `${OUT}/${name}`]); return name; }
  catch (e) { return `unavailable: ${e.message}`; }
};
async function ensureControl() {
  const s = await api('browser.control.status');
  if (s.body.result?.active) return true;
  const r = await api('browser.control.start', { tabId, ttlMs: 600000 });
  return r.status === 200;
}
async function observe() {
  const r = await api('page.observe', { tabId });
  const snap = r.body.result?.snapshot;
  return { status: r.status, body: r.body, snapshot: snap, gen: snap?.generation, elements: snap?.elements || [] };
}
const refFor = (o, pred) => o.elements.find(pred)?.ref;
// Bring a named target into the viewport; 0.10 refuses to act on off-screen
// elements by design, so the agent is expected to scroll and re-observe.
async function observeWithVisible(name) {
  let o = await observe();
  let el = o.elements.find((e) => e.name === name);
  if (el && el.inViewport === false) {
    await api('page.scroll', { tabId, generation: o.gen, deltaX: 0, deltaY: Math.max(0, Math.round(el.bounds.y - 120)) });
    await sleep(400);
    o = await observe();
    el = o.elements.find((e) => e.name === name);
  }
  return { o, el };
}

// ---------- 1. lease + native Chrome warning ----------
const start = await api('browser.control.start', { tabId, ttlMs: 600000 });
record('browser.control.start', start.status === 200 && start.body.result?.control?.active !== false,
  { active: (start.body.result?.control ?? start.body.result)?.active, expiresAt: (start.body.result?.control ?? start.body.result)?.expiresAt });
await sleep(1200);
record('evidence.native-warning-screenshot', true, osShot('01-native-debugger-warning.png'));

// ---------- 2. observe + epoch refs ----------
let o = await observe();
record('page.observe', o.status === 200 && o.elements.length > 0, { generation: o.gen, elementCount: o.elements.length, title: o.snapshot?.title });
const allEpoch = o.elements.length > 0 && o.elements.every((e) => String(e.ref).startsWith(`${o.gen}.`));
record('refs.epoch-embedded', allEpoch, { sample: o.elements.slice(0, 4).map((e) => `${e.ref}=${e.role}:${e.name}`) });
record('observation.screenshot-metadata', true, await shot('02-observe.jpg'));

// ---------- 3. waitFor ----------
const waitOk = await api('page.waitFor', { tabId, text: 'Browser Bridge demo target', timeoutMs: 3000 });
record('page.waitFor.satisfied', waitOk.status === 200 && waitOk.body.result?.satisfied === true, waitOk.body.result);
const waitTimeout = await api('page.waitFor', { tabId, text: 'string that never appears anywhere', timeoutMs: 1200 });
record('page.waitFor.timeout-is-structured', waitTimeout.status !== 200 && waitTimeout.body.error?.code === 'WAIT_TIMEOUT',
  { status: waitTimeout.status, code: waitTimeout.body.error?.code, taxonomy: waitTimeout.body.taxonomy?.code, hint: waitTimeout.body.taxonomy?.recoveryHint });
record('page.waitFor.timeout-keeps-lease', (await api('browser.control.status')).body.result?.active === true, 'lease still active after wait timeout');

// ---------- 4. hover ----------
let vis = await observeWithVisible('Coordinate target');
record('page.scroll.brings-target-into-view', vis.el?.inViewport === true, { inViewport: vis.el?.inViewport, bounds: vis.el?.bounds });
const hover = await api('page.hover', { tabId, ref: vis.el?.ref, generation: vis.o.gen });
record('page.hover', hover.status === 200, hover.body.result || hover.body.error);
const afterHover = await api('page.evaluate', { tabId, expression: 'String(document.body.dataset.lastAction || "none")' });
record('page.hover.no-click-dispatched', !/coordinate:/.test(JSON.stringify(afterHover.body.result || '')), JSON.stringify(afterHover.body.result ?? afterHover.body.error));

// ---------- 5. modifier click ----------
vis = await observeWithVisible('Coordinate target');
const shiftClick = await api('page.click', { tabId, ref: vis.el?.ref, generation: vis.o.gen, modifiers: ['Shift'], clickCount: 1 });
record('page.click.modifiers', shiftClick.status === 200, shiftClick.body.result || shiftClick.body.error);
const lastAction = await api('page.evaluate', { tabId, expression: 'String(document.body.dataset.lastAction)' });
record('page.click.trusted-event-observed', /coordinate:true/.test(JSON.stringify(lastAction.body.result || '')), JSON.stringify(lastAction.body.result ?? lastAction.body.error));

// ---------- 6. batch ----------
o = await observe();
await api('page.scroll', { tabId, generation: o.gen, deltaX: 0, deltaY: -5000 });
await sleep(400);
o = await observe();
const batch = await api('page.batch', {
  tabId, generation: o.gen,
  actions: [
    { method: 'page.fill', ref: refFor(o, (e) => e.name === 'Display name'), text: 'Ada' },
    { method: 'page.select', ref: refFor(o, (e) => e.name === 'Favorite color'), value: 'blue' },
  ],
});
record('page.batch', batch.status === 200, batch.body.result || batch.body.error);
const batchState = await api('page.evaluate', { tabId, expression: 'JSON.stringify({name:document.getElementById("name").value,color:document.getElementById("color").value})' });
record('page.batch.applied-both-steps', /Ada/.test(JSON.stringify(batchState.body.result)) && /blue/.test(JSON.stringify(batchState.body.result)), JSON.stringify(batchState.body.result));
record('page.batch.screenshot', true, await shot('03-batch.jpg'));

// ---------- 7. normalized coordinates ----------
o = await observe();
const normClick = await api('page.clickAt', { tabId, generation: o.gen, x: 500, y: 500, coordinateSpace: 'normalized1000' });
record('coordinates.normalized1000', normClick.status === 200, normClick.body.result || normClick.body.error);

// ---------- 8. callId idempotency ----------
await ensureControl();
const callId = `evidence-${Date.now()}`;
const first = await api('page.evaluate', { tabId, expression: '1+1' }, { callId });
const replay = await api('page.evaluate', { tabId, expression: '1+1' }, { callId });
record('callId.replay', first.status === 200 && replay.body.replayed === true && JSON.stringify(first.body.result) === JSON.stringify(replay.body.result),
  { firstStatus: first.status, replayed: replay.body.replayed, identical: JSON.stringify(first.body.result) === JSON.stringify(replay.body.result) });
const reused = await api('page.evaluate', { tabId, expression: '2+2' }, { callId });
record('callId.reuse-refused', reused.status === 409 && reused.body.error?.code === 'CALL_ID_REUSED', { status: reused.status, code: reused.body.error?.code });

// ---------- 9. taxonomy on stale ref ----------
const stale = await api('page.click', { tabId, ref: 'deadbeef.e1', generation: 'deadbeef' });
record('error.taxonomy', Boolean(stale.body.taxonomy?.code && stale.body.taxonomy?.recoveryHint),
  { legacy: stale.body.error?.code, taxonomy: stale.body.taxonomy });

// ---------- 10. Host guard (raw socket: fetch forbids a Host header) ----------
const rawHost = await new Promise((resolve) => {
  const sock = net.connect(17373, '127.0.0.1', () => {
    sock.write('GET /health HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n');
  });
  let buf = '';
  sock.on('data', (d) => { buf += d.toString(); });
  sock.on('end', () => resolve(buf.split('\r\n')[0]));
  sock.on('error', (e) => resolve(`error ${e.message}`));
});
record('security.host-guard', /403/.test(rawHost), { statusLine: rawHost });
const rawOk = await new Promise((resolve) => {
  const sock = net.connect(17373, '127.0.0.1', () => sock.write('GET /health HTTP/1.1\r\nHost: 127.0.0.1:17373\r\nConnection: close\r\n\r\n'));
  let buf = '';
  sock.on('data', (d) => { buf += d.toString(); });
  sock.on('end', () => resolve(buf.split('\r\n')[0]));
  sock.on('error', (e) => resolve(`error ${e.message}`));
});
record('security.loopback-host-allowed', /200/.test(rawOk), { statusLine: rawOk });

// ---------- 11. dialog interception (runs last: may end the lease) ----------
await ensureControl();
await api('page.evaluate', { tabId, expression: "setTimeout(function(){window.confirm('bridge dialog evidence')},250); 'scheduled'" });
await sleep(2500);
const dialogState = await state();
record('dialog.pending-state', Boolean(dialogState.pendingDialog), dialogState.pendingDialog || 'pendingDialog stayed null');
const blocked = await api('page.observe', { tabId });
record('dialog.blocks-with-blocked_by_dialog', blocked.body.error?.code === 'BLOCKED_BY_DIALOG',
  { status: blocked.status, code: blocked.body.error?.code, taxonomy: blocked.body.taxonomy?.code });
record('evidence.dialog-screenshot', true, osShot('04-dialog-open.png'));
const leaseDuringDialog = await api('browser.control.status');
record('dialog.lease-survives-open-dialog', leaseDuringDialog.body.result?.active === true,
  { active: leaseDuringDialog.body.result?.active, revocation: leaseDuringDialog.body.result?.revocation?.reason });
const handled = await api('page.handleDialog', { tabId, accept: false });
record('page.handleDialog', handled.status === 200, handled.body.result || handled.body.error);
await sleep(800);
record('dialog.observe-works-after-handling', (await api('page.observe', { tabId })).status === 200, 'observe after dialog handling');

// ---------- 12. stop ----------
const stop = await api('browser.control.stop');
record('browser.control.stop', stop.status === 200 || stop.body.error?.code === 'CONTROL_REVOKED', stop.body.result || stop.body.error);
await sleep(600);
record('evidence.after-stop-screenshot', true, osShot('05-after-stop.png'));

// ---------- finish ----------
const finalState = await state();
writeFileSync(`${OUT}/results.json`, JSON.stringify({
  recordedAt: new Date().toISOString(),
  target: 'packaged v0.10.0 release artifacts',
  chrome: 'Google Chrome for Testing 152.0.7977.54 (isolated profile)',
  extension: finalState.extension && { version: finalState.extension.version, browser: finalState.extension.browser, mode: finalState.extension.mode },
  passed: results.filter((r) => r.ok).length,
  total: results.length,
  results,
}, null, 2));
log(`\n${results.filter((r) => r.ok).length}/${results.length} checks passed`);
// leave no dialog blocking a real browser window
try {
  for (const t of (await cdpTargets()).filter((t) => t.type === 'page')) {
    const ws = new WebSocket(t.webSocketDebuggerUrl);
    await new Promise((r) => ws.once('open', r));
    ws.send(JSON.stringify({ id: 1, method: 'Page.handleJavaScriptDialog', params: { accept: false } }));
    await sleep(150); ws.close();
  }
} catch {}
try { chrome.kill(); } catch {}
try { server.kill(); } catch {}
await sleep(500);
process.exit(0);
