// Live Chrome evidence for the frame boundaries left open after v0.11.1.
//
// The positive run must use the exact unpacked published extension. A second,
// explicitly labelled fault-injection run copies that extension and changes
// only the chrome.debugger target construction so child commands intentionally
// omit sessionId. That run is diagnostic stub evidence, never release proof.
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  appendFileSync,
  cpSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const [SERVER_BIN, EXT_DIR, CHROME_BIN, OUT, RIG_DIR, EXTENSION_ZIP, EVIDENCE_KIND = 'published'] = process.argv.slice(2);
if (![SERVER_BIN, EXT_DIR, CHROME_BIN, OUT, RIG_DIR, EXTENSION_ZIP].every(Boolean)) {
  throw new Error('Usage: node frame-boundaries-rig.mjs SERVER EXT_DIR CHROME OUT RIG_DIR EXTENSION_ZIP [published|candidate]');
}
if (!['published', 'candidate'].includes(EVIDENCE_KIND)) throw new Error('Evidence kind must be published or candidate');
const EXTENSION_VERSION = JSON.parse(readFileSync(path.join(EXT_DIR, 'manifest.json'), 'utf8')).version;
const POSITIVE_NAME = EVIDENCE_KIND;

const FIXTURES = new URL('./fixtures/', import.meta.url);
const BRIDGE_PORT = 17408;
const ROOT_PORT = 18100;
const CHILD_PORT = 18101;
const GRANDCHILD_PORT = 18102;
mkdirSync(OUT, { recursive: true });
const logPath = path.join(OUT, 'frame-rig.log');
writeFileSync(logPath, '');
const results = [];

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const log = (message) => {
  console.log(message);
  appendFileSync(logPath, `${message}\n`);
};
const record = (run, name, ok, detail) => {
  const entry = { run, name, ok, detail, at: new Date().toISOString() };
  results.push(entry);
  const rendered = typeof detail === 'string' ? detail : JSON.stringify(detail);
  log(`${ok ? 'PASS' : 'FAIL'} ${run}.${name} :: ${String(rendered).slice(0, 500)}`);
  return entry;
};
const sha256 = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');

function serve(directory, host, port) {
  const server = http.createServer((request, response) => {
    const pathname = request.url === '/' ? '/root.html' : String(request.url).split('?')[0];
    const file = path.join(directory, pathname);
    try {
      const data = readFileSync(file);
      response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      response.end(data);
    } catch {
      response.writeHead(404);
      response.end('not found');
    }
  });
  server.listen(port, host);
  return server;
}

function terminate(process) {
  if (!process || process.exitCode !== null) return;
  try { process.kill('SIGTERM'); } catch {}
}

async function targets(cdpPort) {
  return (await fetch(`http://127.0.0.1:${cdpPort}/json/list`)).json();
}

function connect(target) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    const pending = new Map();
    let nextId = 0;
    socket.addEventListener('open', () => resolve({
      send(method, params = {}) {
        return new Promise((accept, decline) => {
          nextId += 1;
          pending.set(nextId, { accept, decline });
          socket.send(JSON.stringify({ id: nextId, method, params }));
        });
      },
      close() { socket.close(); },
    }));
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(String(event.data));
      const waiter = pending.get(message.id);
      if (!waiter) return;
      pending.delete(message.id);
      if (message.error) waiter.decline(new Error(message.error.message));
      else waiter.accept(message);
    });
    socket.addEventListener('error', () => reject(new Error('CDP WebSocket failed')));
  });
}

async function evalIn(target, expression) {
  const session = await connect(target);
  try {
    const response = await session.send('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    return response.result?.result?.value;
  } finally {
    session.close();
  }
}

async function runBrowser({ name, extensionDir, cdpPort, profileDir, faultInjection }) {
  const tokenPath = path.join(RIG_DIR, `${name}-token`);
  rmSync(profileDir, { recursive: true, force: true });
  rmSync(tokenPath, { force: true });
  const server = spawn(SERVER_BIN, ['--no-update-check'], {
    env: { ...process.env, LBB_PORT: String(BRIDGE_PORT), LBB_TOKEN_PATH: tokenPath },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let serverOutput = '';
  let token = '';
  const onServerOutput = (data) => {
    serverOutput += data.toString();
    token ||= serverOutput.match(/Extension token:\s*(\S+)/)?.[1] ?? '';
  };
  server.stdout.on('data', onServerOutput);
  server.stderr.on('data', onServerOutput);
  for (let attempt = 0; attempt < 80 && !token; attempt += 1) await sleep(125);
  if (!token) throw new Error(`Server did not publish a token: ${serverOutput}`);

  const chrome = spawn(CHROME_BIN, [
    `--user-data-dir=${profileDir}`,
    `--load-extension=${extensionDir}`,
    `--remote-debugging-port=${cdpPort}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-background-networking',
    '--window-position=0,0',
    '--window-size=1280,900',
    'about:blank',
  ], { stdio: 'ignore' });

  const base = `http://127.0.0.1:${BRIDGE_PORT}`;
  const headers = { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' };
  const api = async (method, params = {}) => {
    const response = await fetch(`${base}/api/v1/command`, {
      method: 'POST',
      headers,
      body: JSON.stringify({ method, params }),
    });
    return { status: response.status, body: await response.json() };
  };
  const state = async () => {
    const response = await fetch(`${base}/api/state`, { headers });
    const body = await response.json();
    return body.state ?? body;
  };

  try {
    let serviceWorker = null;
    for (let attempt = 0; attempt < 60 && !serviceWorker; attempt += 1) {
      try {
        for (const target of (await targets(cdpPort)).filter((item) => item.type === 'service_worker')) {
          if (await evalIn(target, 'chrome.runtime.getManifest().name').catch(() => null) === 'Local Browser Bridge') {
            serviceWorker = target;
            break;
          }
        }
      } catch {}
      if (!serviceWorker) await sleep(250);
    }
    record(name, 'extension-loaded', Boolean(serviceWorker), serviceWorker && {
      version: await evalIn(serviceWorker, 'chrome.runtime.getManifest().version'),
      faultInjection,
    });
    if (!serviceWorker) throw new Error('Extension service worker was not found');
    await evalIn(serviceWorker, `chrome.storage.local.set({token:${JSON.stringify(token)},port:${BRIDGE_PORT},enabled:true,fullAccess:true}).then(()=>'ok')`);

    let connected = null;
    for (let attempt = 0; attempt < 60 && !connected; attempt += 1) {
      const current = await state();
      if (current.connected && current.extension) connected = current;
      else await sleep(250);
    }
    record(name, 'extension-connected', Boolean(connected), connected && {
      version: connected.extension.version,
      mode: connected.extension.mode,
    });
    if (!connected) throw new Error('Extension did not connect');

    const rootUrl = `http://localhost:${ROOT_PORT}/root.html`;
    await fetch(`http://127.0.0.1:${cdpPort}/json/new?${encodeURIComponent(rootUrl)}`, { method: 'PUT' });
    await sleep(2_000);
    const listed = await api('tabs.list');
    const tab = (listed.body.result?.tabs ?? []).find((item) => item.url === rootUrl);
    record(name, 'fixture-tab-found', Boolean(tab), tab && { id: tab.id, url: tab.url, title: tab.title });
    if (!tab) throw new Error('Fixture tab was not found through the bridge');

    const started = await api('browser.control.start', { tabId: tab.id, ttlMs: 600_000 });
    record(name, 'control-started', started.status === 200, {
      status: started.status,
      active: (started.body.result?.control ?? started.body.result)?.active,
    });
    await sleep(750);

    const observation = await api('page.observe', { tabId: tab.id });
    const snapshot = observation.body.result?.snapshot;
    const frameSummary = snapshot?.frameSummary;
    const frameElements = (snapshot?.elements ?? []).filter((element) => element.frameRef);
    record(name, 'observation-succeeded', observation.status === 200, {
      status: observation.status,
      frameSummary,
      frames: snapshot?.frames,
    });

    if (!faultInjection) {
      try {
        const screenshot = `00-${EVIDENCE_KIND}-frame-observation.png`;
        execFileSync('/usr/sbin/screencapture', ['-x', '-R', '0,0,1280,720', path.join(OUT, screenshot)]);
        record(name, 'observation-screenshot-captured', true, screenshot);
      } catch (error) {
        record(name, 'observation-screenshot-captured', false, String(error.message));
      }
      const depthOne = (snapshot?.frames ?? []).find((frame) => frame.depth === 1);
      const depthTwo = (snapshot?.frames ?? []).find((frame) => frame.depth === 2);
      record(name, 'same-process-frame-reported', frameSummary?.skipped?.some((skip) =>
        skip.reason === 'same_process_frame' && skip.urlOrigin === `http://localhost:${ROOT_PORT}`), frameSummary?.skipped ?? []);
      record(name, 'depth-one-frame-merged', Boolean(depthOne), depthOne ?? snapshot?.frames ?? []);
      record(name, 'depth-two-frame-merged', Boolean(depthTwo), depthTwo ?? snapshot?.frames ?? []);

      const depthOneInput = frameElements.find((element) => element.name === 'depth one field');
      record(name, 'depth-one-input-visible', Boolean(depthOneInput), depthOneInput && {
        ref: depthOneInput.ref,
        bounds: depthOneInput.bounds,
      });
      if (depthOneInput) {
        const refused = await api('page.fill', {
          tabId: tab.id,
          generation: snapshot.generation,
          ref: depthOneInput.ref,
          text: 'must not be written',
        });
        record(name, 'frame-fill-refused', refused.status === 400
          && refused.body.error?.code === 'FRAME_ACTION_UNSUPPORTED'
          && refused.body.taxonomy?.code === 'invalid_request'
          && refused.body.taxonomy?.retriable === false, {
          status: refused.status,
          code: refused.body.error?.code,
          taxonomy: refused.body.taxonomy,
        });
      }

      const grandchildButton = frameElements.find((element) => element.name === 'Depth two grandchild button');
      record(name, 'depth-two-button-visible', Boolean(grandchildButton), grandchildButton && {
        ref: grandchildButton.ref,
        bounds: grandchildButton.bounds,
        frameRef: grandchildButton.frameRef,
      });
      if (grandchildButton) {
        const clicked = await api('page.click', {
          tabId: tab.id,
          generation: snapshot.generation,
          ref: grandchildButton.ref,
        });
        record(name, 'depth-two-click-dispatched', clicked.status === 200, {
          status: clicked.status,
          result: clicked.body.result?.clicked,
          error: clicked.body.error,
        });
        await sleep(400);
        const grandchildTarget = (await targets(cdpPort)).find((target) => target.url.includes('/grandchild.html'));
        const targetState = grandchildTarget
          ? await evalIn(grandchildTarget, "document.getElementById('grandchild-log')?.textContent")
          : 'grandchild target unavailable';
        record(name, 'depth-two-trusted-click-landed', targetState === 'grandchild-click:true', targetState);
        try {
          execFileSync('/usr/sbin/screencapture', ['-x', '-R', '0,0,1280,720', path.join(OUT, '10-depth-two-click.png')]);
          record(name, 'depth-two-screenshot-captured', true, '10-depth-two-click.png');
        } catch (error) {
          record(name, 'depth-two-screenshot-captured', false, String(error.message));
        }
      }
    } else {
      record(name, 'routing-probe-failed-closed', frameSummary?.supported === false
        && frameSummary?.reason === 'session_routing_unverified'
        && (snapshot?.frames ?? []).length === 0
        && frameElements.length === 0, {
        frameSummary,
        frames: snapshot?.frames,
        frameElementCount: frameElements.length,
      });
      const lease = await api('browser.control.status');
      record(name, 'routing-refusal-kept-lease', lease.status === 200 && lease.body.result?.active === true, {
        status: lease.status,
        active: lease.body.result?.active,
      });
    }

    await api('browser.control.stop').catch(() => {});
    return {
      chromeTargets: (await targets(cdpPort)).filter((target) =>
        target.type === 'iframe' || target.type === 'page').map((target) => ({ type: target.type, url: target.url })),
    };
  } finally {
    terminate(chrome);
    terminate(server);
    await sleep(500);
    rmSync(tokenPath, { force: true });
    rmSync(profileDir, { recursive: true, force: true });
  }
}

const fixtureDirectory = fileURLToPath(FIXTURES);
const servers = [
  serve(fixtureDirectory, '127.0.0.1', ROOT_PORT),
  serve(fixtureDirectory, '127.0.0.1', CHILD_PORT),
  // IPv6 loopback is a third site distinct from localhost and 127.0.0.1,
  // without exposing the fixture listener beyond this machine.
  serve(fixtureDirectory, '::1', GRANDCHILD_PORT),
];

const stubExtension = path.join(RIG_DIR, 'session-routing-stub-extension');
rmSync(stubExtension, { recursive: true, force: true });
cpSync(EXT_DIR, stubExtension, { recursive: true });
const stubBackground = path.join(stubExtension, 'background.js');
const originalBackground = readFileSync(stubBackground, 'utf8');
const targetLine = 'const target = sessionId ? { tabId, sessionId } : { tabId };';
const replacementLine = 'const target = sessionId ? { tabId } : { tabId }; // evidence-only session-routing fault injection';
if (originalBackground.split(targetLine).length !== 2) {
  throw new Error('Fault-injection target was not unique in background.js');
}
writeFileSync(stubBackground, originalBackground.replace(targetLine, replacementLine));

const metadata = {
  recordedAt: new Date().toISOString(),
  build: EVIDENCE_KIND === 'published' ? `v${EXTENSION_VERSION}` : `v${EXTENSION_VERSION} local candidate`,
  chrome: execFileSync(CHROME_BIN, ['--version'], { encoding: 'utf8' }).trim(),
  extensionZip: {
    file: path.basename(EXTENSION_ZIP),
    sha256: sha256(EXTENSION_ZIP),
  },
  positiveExtension: {
    source: EVIDENCE_KIND === 'published'
      ? 'exact unpacked published extension ZIP'
      : 'local candidate extension packaged from the current worktree',
    backgroundSha256: sha256(path.join(EXT_DIR, 'background.js')),
  },
  routingStub: {
    source: `copy of the ${EVIDENCE_KIND} extension with exactly one evidence-only target-construction replacement`,
    replacement: { before: targetLine, after: replacementLine },
    backgroundSha256: sha256(stubBackground),
  },
};

try {
  metadata.positive = await runBrowser({
    name: POSITIVE_NAME,
    extensionDir: EXT_DIR,
    cdpPort: 19408,
    profileDir: path.join(RIG_DIR, 'published-profile'),
    faultInjection: false,
  });
  metadata.routingStubRun = await runBrowser({
    name: 'routing-stub',
    extensionDir: stubExtension,
    cdpPort: 19409,
    profileDir: path.join(RIG_DIR, 'routing-stub-profile'),
    faultInjection: true,
  });
} finally {
  for (const server of servers) server.close();
  rmSync(stubExtension, { recursive: true, force: true });
}

const passed = results.filter((result) => result.ok).length;
const resultFile = EVIDENCE_KIND === 'published' ? 'frame-results.json' : 'frame-candidate-results.json';
writeFileSync(path.join(OUT, resultFile), JSON.stringify({
  ...metadata,
  passed,
  total: results.length,
  results,
}, null, 2));
log(`\n${passed}/${results.length} checks passed`);
process.exitCode = passed === results.length ? 0 : 1;
