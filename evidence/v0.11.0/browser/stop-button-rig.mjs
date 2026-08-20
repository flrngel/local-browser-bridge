// Proves the in-page Stop button with a real dispatched mouse event.
// The pill lives in a closed shadow root, so its position is taken from the
// host element rect in the page main world, and the click is delivered as a
// genuine Input.dispatchMouseEvent rather than a DOM .click().
import { spawn, execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import WebSocket from 'ws';

const RIG = process.env.RIG;
const [SERVER_BIN, EXT_DIR, CHROME, OUT] = process.argv.slice(2);
const CDP_PORT = 9338;
const PROFILE = `${RIG}/stop-profile`;
mkdirSync(OUT, { recursive: true });
const results = [];
const log = (m) => { console.log(m); appendFileSync(`${OUT}/rig.log`, `${m}\n`); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const record = (name, ok, detail) => {
  results.push({ name, ok, detail, at: new Date().toISOString() });
  log(`${ok ? 'PASS' : 'FAIL'} ${name} :: ${typeof detail === 'string' ? detail : JSON.stringify(detail).slice(0, 300)}`);
};

const server = spawn(SERVER_BIN, [], { stdio: ['ignore', 'pipe', 'pipe'] });
let token = ''; let so = '';
const onOut = (d) => { so += d.toString(); const m = so.match(/Extension token:\s*(\S+)/); if (m) token = m[1]; };
server.stdout.on('data', onOut); server.stderr.on('data', onOut);
for (let i = 0; i < 60 && !token; i++) await sleep(250);
const BASE = 'http://127.0.0.1:17373';
const H = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' };
const api = async (method, params = {}) => {
  const res = await fetch(`${BASE}/api/v1/command`, { method: 'POST', headers: H, body: JSON.stringify({ method, params }) });
  return { status: res.status, body: await res.json() };
};
const state = async () => (await (await fetch(`${BASE}/api/state`, { headers: H })).json()).state;

const chrome = spawn(CHROME, [
  `--user-data-dir=${PROFILE}`, `--load-extension=${EXT_DIR}`, `--remote-debugging-port=${CDP_PORT}`,
  '--no-first-run', '--no-default-browser-check', '--window-position=0,0', '--window-size=1280,860', 'about:blank',
], { stdio: 'ignore' });
await sleep(7000);

const targets = async () => (await fetch(`http://127.0.0.1:${CDP_PORT}/json/list`)).json();
function connect(target) {
  return new Promise(async (resolve) => {
    const ws = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((r) => ws.once('open', r));
    let id = 0;
    const pending = new Map();
    ws.on('message', (d) => {
      const m = JSON.parse(d);
      if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
    });
    resolve({
      send: (method, params = {}) => new Promise((res) => { id += 1; pending.set(id, res); ws.send(JSON.stringify({ id, method, params })); }),
      close: () => ws.close(),
    });
  });
}
async function evalIn(target, expression) {
  const c = await connect(target);
  const out = await c.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  c.close();
  return out.result?.result?.value;
}

let sw = null;
for (let i = 0; i < 30 && !sw; i++) {
  for (const t of (await targets()).filter((t) => t.type === 'service_worker')) {
    if (await evalIn(t, 'chrome.runtime.getManifest().name').catch(() => null) === 'Local Browser Bridge') { sw = t; break; }
  }
  if (!sw) await sleep(1000);
}
record('extension.loaded', Boolean(sw), sw && { version: await evalIn(sw, 'chrome.runtime.getManifest().version') });
await evalIn(sw, `chrome.storage.local.set({token:${JSON.stringify(token)},port:17373,enabled:true,fullAccess:true}).then(()=>'ok')`);
let connected = null;
for (let i = 0; i < 40 && !connected; i++) { const s = await state(); if (s.connected) connected = s; else await sleep(750); }
record('extension.connected', Boolean(connected), connected && { version: connected.extension.version });

await fetch(`http://127.0.0.1:${CDP_PORT}/json/new?${BASE}/demo`, { method: 'PUT' });
await sleep(2500);
const tabs = await api('tabs.list');
const tab = (tabs.body.result?.tabs || []).find((t) => String(t.url).includes('/demo'));
const tabId = tab.id;
const start = await api('browser.control.start', { tabId, ttlMs: 600000 });
record('browser.control.start', start.status === 200, { active: (start.body.result?.control ?? start.body.result)?.active });
await api('page.observe', { tabId });
await sleep(800);

// The bridge itself refuses to click its own overlay, by design.
const selfClick = await api('page.clickAt', { tabId, generation: (await api('page.observe', { tabId })).body.result.snapshot.generation, x: 1150, y: 40 });
record('bridge.refuses-to-click-its-own-stop', selfClick.status !== 200 && /CONTROL_UI_OCCLUSION/.test(JSON.stringify(selfClick.body)),
  { code: selfClick.body.error?.code, message: String(selfClick.body.error?.message || '').slice(0, 90) });

// Locate the closed-shadow pill through its host element rect, then click it
// for real with a dispatched input event, the way a person would.
const pageTarget = (await targets()).find((t) => t.type === 'page' && t.url.includes('/demo'));
const rect = JSON.parse(await evalIn(pageTarget, `
  (() => {
    const host = document.getElementById('__local_browser_bridge_control__');
    if (!host) return JSON.stringify({found:false});
    const r = host.getBoundingClientRect();
    return JSON.stringify({found:true, x:r.x, y:r.y, width:r.width, height:r.height, right:r.right, bottom:r.bottom});
  })()
`));
record('stop.pill-located', rect.found === true, rect);

execFileSync('/usr/sbin/screencapture', ['-x', '-R', '0,0,1280,420', `${OUT}/20-before-stop.png`]);

// The Stop button sits at the right end of the pill; aim inside its right edge.
const clickX = Math.round(rect.right - 26);
const clickY = Math.round(rect.y + rect.height / 2);
const page = await connect(pageTarget);
await page.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: clickX, y: clickY, buttons: 0 });
await sleep(150);
await page.send('Input.dispatchMouseEvent', { type: 'mousePressed', x: clickX, y: clickY, button: 'left', clickCount: 1, buttons: 1 });
await sleep(60);
await page.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: clickX, y: clickY, button: 'left', clickCount: 1, buttons: 0 });
record('stop.click-dispatched', true, { x: clickX, y: clickY, note: 'real Input.dispatchMouseEvent, not a DOM .click()' });
page.close();

await sleep(2500);
execFileSync('/usr/sbin/screencapture', ['-x', '-R', '0,0,1280,420', `${OUT}/21-after-stop.png`]);

const after = await api('browser.control.status');
const revocation = after.body.result?.revocation;
record('stop.lease-revoked', after.body.result?.active === false, { active: after.body.result?.active, reason: revocation?.reason });
record('stop.human-pause-latched', after.body.result?.humanPaused === true, { humanPaused: after.body.result?.humanPaused });

// The strong claim: after a human Stop, remote control cannot resume itself.
const restart = await api('browser.control.start', { tabId, ttlMs: 60000 });
record('stop.remote-restart-refused', restart.status !== 200, { status: restart.status, code: restart.body.error?.code, taxonomy: restart.body.taxonomy?.code });
const mutate = await api('page.observe', { tabId });
record('stop.remote-mutation-refused', mutate.status !== 200, { status: mutate.status, code: mutate.body.error?.code });

// The overlay must be gone from the page as well.
const overlayGone = await evalIn(pageTarget, `String(document.getElementById('__local_browser_bridge_control__') === null)`);
record('stop.page-overlay-removed', overlayGone === 'true', { overlayAbsent: overlayGone });

writeFileSync(`${OUT}/results.json`, JSON.stringify({
  recordedAt: new Date().toISOString(),
  scenario: 'in-page Stop button clicked with a dispatched input event',
  passed: results.filter((r) => r.ok).length, total: results.length, results,
}, null, 2));
log(`\n${results.filter((r) => r.ok).length}/${results.length} checks passed`);
try { chrome.kill(); } catch {}
try { server.kill(); } catch {}
await sleep(400);
process.exit(0);
