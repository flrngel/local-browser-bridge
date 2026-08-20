// Live stock-Chrome evidence for Local Browser Bridge 0.11.1.
//
// This rig uses a fresh disposable Chrome profile and the browser-target CDP
// Extensions domain because stock Chrome 151 ignores --load-extension. It
// deliberately does not synthesize the browser-chrome clicks itself: after
// the READY line, an operator must target the named native control with a
// real OS-level click. The rig verifies the resulting bridge state and removes
// the profile and bearer token in its finally block. Extensions.loadUnpacked
// is intentionally treated as an ephemeral test install; restart persistence
// is outside this scenario.
import { execFileSync, spawn } from "node:child_process";
import {
  createHash,
  randomBytes,
} from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, resolve } from "node:path";
import { arch, platform, release } from "node:os";

const [serverArgument, extensionArgument, chromeArgument, outputArgument, scratchArgument] = process.argv.slice(2);
if (!serverArgument || !extensionArgument || !chromeArgument || !outputArgument || !scratchArgument) {
  throw new Error("usage: native-warning-rig.mjs SERVER EXTENSION_DIR CHROME OUTPUT_DIR SCRATCH_DIR");
}

const SERVER_BIN = resolve(serverArgument);
const EXTENSION_DIR = resolve(extensionArgument);
const CHROME_BIN = resolve(chromeArgument);
const OUTPUT_DIR = resolve(outputArgument);
const SCRATCH_DIR = resolve(scratchArgument);
const PROFILE_DIR = resolve(SCRATCH_DIR, "native-warning-v0.11.1-profile");
const UI_INTERACTION_PATH = resolve(OUTPUT_DIR, "native-warning-click.json");
const SERVER_PORT = 17_409;
const CDP_PORT = 9_342;
const BASE = `http://127.0.0.1:${SERVER_PORT}`;
const token = randomBytes(32).toString("base64url");
const headers = { Authorization: `Bearer ${token}`, "Content-Type": "application/json" };
const results = [];
const events = [];
let server = null;
let chrome = null;
let browser = null;
let extensionId = "";
let tabId = null;
let targetId = "";
let chromeLaunchCount = 0;
let serverOutput = "";
let cleaned = false;

mkdirSync(OUTPUT_DIR, { recursive: true });
mkdirSync(SCRATCH_DIR, { recursive: true });
if (existsSync(PROFILE_DIR)) throw new Error(`refusing to reuse disposable profile: ${PROFILE_DIR}`);
if (existsSync(UI_INTERACTION_PATH)) rmSync(UI_INTERACTION_PATH, { force: false });

const sleep = (ms) => new Promise((resolveSleep) => setTimeout(resolveSleep, ms));
const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const commandOutput = (command, args) => execFileSync(command, args, { encoding: "utf8" }).trim();
const safeDetail = (detail) => JSON.parse(JSON.stringify(detail, (key, value) => {
  if (/token|authorization/i.test(key)) return "[redacted]";
  if (typeof value === "string" && value.includes(token)) return value.replaceAll(token, "[redacted]");
  return value;
}));
const note = (message) => {
  process.stdout.write(`${message}\n`);
};
const record = (name, ok, detail) => {
  const row = { name, ok: Boolean(ok), detail: safeDetail(detail), at: new Date().toISOString() };
  results.push(row);
  note(`${row.ok ? "PASS" : "FAIL"} ${name} :: ${JSON.stringify(row.detail).slice(0, 500)}`);
  return row.ok;
};

class CdpConnection {
  constructor(url) {
    this.url = url;
    this.nextId = 0;
    this.pending = new Map();
    this.events = new Map();
  }

  async open() {
    this.socket = new WebSocket(this.url);
    await new Promise((resolveOpen, rejectOpen) => {
      this.socket.addEventListener("open", resolveOpen, { once: true });
      this.socket.addEventListener("error", rejectOpen, { once: true });
    });
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data));
      if (message.id && this.pending.has(message.id)) {
        const { resolve: resolvePending, reject: rejectPending } = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) rejectPending(new Error(`${message.error.code}: ${message.error.message}`));
        else resolvePending(message.result ?? {});
        return;
      }
      if (!message.method) return;
      for (const listener of this.events.get(message.method) ?? []) listener(message.params ?? {});
    });
    return this;
  }

  send(method, params = {}, sessionId = undefined) {
    const id = ++this.nextId;
    return new Promise((resolvePending, rejectPending) => {
      this.pending.set(id, { resolve: resolvePending, reject: rejectPending });
      this.socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  }

  on(method, listener) {
    if (!this.events.has(method)) this.events.set(method, new Set());
    this.events.get(method).add(listener);
  }

  close() {
    try { this.socket?.close(); } catch {}
  }
}

async function eventually(label, predicate, timeoutMs = 30_000, intervalMs = 250) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const value = await predicate();
      if (value) return value;
    } catch (error) {
      lastError = error;
    }
    await sleep(intervalMs);
  }
  throw new Error(`${label} did not become true${lastError ? `: ${lastError.message}` : ""}`);
}

async function api(method, params = {}) {
  const response = await fetch(`${BASE}/api/v1/command`, {
    method: "POST",
    headers,
    body: JSON.stringify({ method, params }),
  });
  return { status: response.status, body: await response.json() };
}

async function bridgeState() {
  const response = await fetch(`${BASE}/api/state`, { headers });
  const body = await response.json();
  return body.state ?? body;
}

function launchServer() {
  server = spawn(SERVER_BIN, ["--no-update-check"], {
    env: {
      ...process.env,
      LBB_DISABLE_UPDATE_CHECK: "true",
      LBB_PORT: String(SERVER_PORT),
      LBB_TOKEN: token,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const collect = (chunk) => {
    serverOutput = `${serverOutput}${chunk}`.slice(-64_000);
  };
  server.stdout.on("data", collect);
  server.stderr.on("data", collect);
}

async function connectBrowser() {
  const version = await eventually("Chrome CDP endpoint", async () => {
    const response = await fetch(`http://127.0.0.1:${CDP_PORT}/json/version`).catch(() => null);
    return response?.ok ? response.json() : null;
  }, 30_000);
  browser = await new CdpConnection(version.webSocketDebuggerUrl).open();
  return version;
}

async function launchChrome({ first }) {
  chromeLaunchCount += 1;
  const argv = [
    `--user-data-dir=${PROFILE_DIR}`,
    `--remote-debugging-port=${CDP_PORT}`,
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-background-networking",
    "--window-position=0,0",
    "--window-size=1280,860",
    first ? "about:blank" : `${BASE}/demo`,
  ];
  chrome = spawn(CHROME_BIN, argv, { detached: true, stdio: "ignore" });
  const cdpVersion = await connectBrowser();
  events.push({
    event: "chrome.launched",
    launch: chromeLaunchCount,
    pid: chrome.pid,
    argv: [CHROME_BIN, ...argv],
    browser: cdpVersion.Browser,
    protocolVersion: cdpVersion["Protocol-Version"],
    at: new Date().toISOString(),
  });
  return cdpVersion;
}

async function stopChrome() {
  browser?.close();
  browser = null;
  if (!chrome?.pid) return;
  const pid = chrome.pid;
  try { process.kill(-pid, "SIGTERM"); } catch {
    try { chrome.kill("SIGTERM"); } catch {}
  }
  await eventually("disposable Chrome exit", async () => {
    const response = await fetch(`http://127.0.0.1:${CDP_PORT}/json/version`).catch(() => null);
    return !response?.ok;
  }, 15_000).catch(() => false);
  events.push({ event: "chrome.stopped", launch: chromeLaunchCount, pid, at: new Date().toISOString() });
  chrome = null;
}

async function extensionMetadata() {
  const listed = await browser.send("Extensions.getExtensions");
  return (listed.extensions ?? []).find((entry) => entry.id === extensionId) ?? null;
}

async function configureExtension() {
  // Chrome 151 exposes Extensions.setStorageItems in the protocol schema but
  // rejects it on this browser target with "No associated browser context".
  // Configure the just-loaded worker directly instead; all bytes stay on the
  // loopback CDP connection and the containing profile is removed afterwards.
  const worker = await eventually("extension service worker", async () => {
    const targets = await browser.send("Target.getTargets");
    return (targets.targetInfos ?? []).find((entry) => entry.type === "service_worker"
      && entry.url.startsWith(`chrome-extension://${extensionId}/`)) ?? null;
  }, 20_000, 250);
  const attached = await browser.send("Target.attachToTarget", { targetId: worker.targetId, flatten: true });
  const values = {
    token,
    port: SERVER_PORT,
    enabled: true,
    fullAccess: true,
    allowedHosts: ["localhost", "127.0.0.1"],
  };
  await browser.send("Runtime.evaluate", {
    expression: `chrome.storage.local.set(${JSON.stringify(values)}).then(()=>true)`,
    awaitPromise: true,
    returnByValue: true,
  }, attached.sessionId);
  await browser.send("Target.detachFromTarget", { sessionId: attached.sessionId }).catch(() => {});
  return eventually("extension connection", async () => {
    const current = await bridgeState();
    return current.connected === true && current.extension?.version === "0.11.1" ? current : null;
  }, 40_000, 500);
}

async function findDemoTab() {
  const listing = await api("tabs.list");
  return (listing.body.result?.tabs ?? []).find((entry) => String(entry.url).includes(`${SERVER_PORT}/demo`)) ?? null;
}

async function startLease() {
  const started = await api("browser.control.start", { tabId, ttlMs: 600_000 });
  const state = started.body.result?.control ?? started.body.result;
  record("control.start", started.status === 200 && state?.active === true, {
    status: started.status,
    active: state?.active,
    tabId: state?.tabId,
    turn: state?.turn,
  });
  if (started.status !== 200) throw new Error(`control start failed: ${JSON.stringify(safeDetail(started.body))}`);
  await api("page.observe", { tabId });
  return eventually("page pill", async () => {
    const targets = await browser.send("Target.getTargets");
    const page = (targets.targetInfos ?? []).find((entry) => entry.type === "page" && entry.url.includes(`${SERVER_PORT}/demo`));
    if (!page) return null;
    targetId = page.targetId;
    const attached = await browser.send("Target.attachToTarget", { targetId, flatten: true });
    const evaluated = await browser.send("Runtime.evaluate", {
      expression: "JSON.stringify((()=>{const host=document.getElementById('__local_browser_bridge_control__');return {pill:Boolean(host),text:document.body.innerText.slice(0,200)}})())",
      returnByValue: true,
    }, attached.sessionId).catch(() => null);
    await browser.send("Target.detachFromTarget", { sessionId: attached.sessionId }).catch(() => {});
    return evaluated?.result?.value ? JSON.parse(evaluated.result.value) : null;
  }, 20_000);
}

async function waitForControl(predicate, label, timeoutMs = 180_000) {
  return eventually(label, async () => {
    const response = await api("browser.control.status");
    return predicate(response.body.result ?? {}) ? response : null;
  }, timeoutMs, 250);
}

async function main() {
  launchServer();
  await eventually("packaged server", async () => {
    const response = await fetch(`${BASE}/health`).catch(() => null);
    return response?.ok ? true : null;
  }, 20_000);
  record("release.server", commandOutput(SERVER_BIN, ["--version"]) === "local-browser-bridge 0.11.1", {
    version: commandOutput(SERVER_BIN, ["--version"]),
    path: SERVER_BIN,
  });

  const chromeVersion = await launchChrome({ first: true });
  record("browser.stock-chrome", chromeVersion.Browser === "Chrome/151.0.7922.138", {
    browser: chromeVersion.Browser,
    executable: CHROME_BIN,
  });

  const loaded = await browser.send("Extensions.loadUnpacked", { path: EXTENSION_DIR });
  extensionId = loaded.id;
  const installed = await extensionMetadata();
  record("extension.loaded-through-browser-cdp", installed?.version === "0.11.1" && installed?.enabled === true, {
    protocolMethod: "Extensions.loadUnpacked",
    id: extensionId,
    name: installed?.name,
    version: installed?.version,
    path: installed?.path,
    enabled: installed?.enabled,
  });
  const connected = await configureExtension();
  record("extension.connected", connected.connected === true, {
    version: connected.extension?.version,
    browser: connected.extension?.browser,
    mode: connected.extension?.mode,
  });

  const created = await browser.send("Target.createTarget", { url: `${BASE}/demo` });
  targetId = created.targetId;
  const demo = await eventually("demo tab", findDemoTab, 20_000, 250);
  tabId = demo.id;
  record("fixture.opened", true, { tabId, title: demo.title, url: demo.url });
  const pill = await startLease();
  record("page.pill-visible", pill.pill === true, pill);
  note("READY native-cancel :: capture the disposable Chrome window, then click the exact native Cancel control");

  const canceled = await waitForControl((control) => control.active === false && control.revocation?.reason === "canceled_by_user", "native Cancel revocation");
  record("native-cancel.revoked", true, {
    active: canceled.body.result.active,
    reason: canceled.body.result.revocation?.reason,
  });
  record("native-cancel.human-pause", canceled.body.result.humanPaused === true, {
    humanPaused: canceled.body.result.humanPaused,
    reason: canceled.body.result.humanPause?.reason,
  });
  const refused = await api("browser.control.start", { tabId, ttlMs: 60_000 });
  record("native-cancel.remote-restart-refused", refused.status !== 200 && refused.body.error?.code === "HUMAN_CONTROL_PAUSED", {
    status: refused.status,
    code: refused.body.error?.code,
    taxonomy: refused.body.taxonomy?.code,
  });
  const targets = await browser.send("Target.getTargets");
  const page = (targets.targetInfos ?? []).find((entry) => entry.type === "page" && entry.targetId === targetId);
  const attached = page ? await browser.send("Target.attachToTarget", { targetId, flatten: true }) : null;
  const overlay = attached ? await browser.send("Runtime.evaluate", {
    expression: "String(document.getElementById('__local_browser_bridge_control__') !== null)",
    returnByValue: true,
  }, attached.sessionId).catch(() => null) : null;
  if (attached) await browser.send("Target.detachFromTarget", { sessionId: attached.sessionId }).catch(() => {});
  record("native-cancel.page-overlay-removed", overlay?.result?.value === "false", {
    overlayPresent: overlay?.result?.value,
  });
  note("READY native-after-cancel :: capture the exact disposable Chrome window; cleanup begins in 30 seconds");
  await sleep(30_000);
}

try {
  await main();
} catch (error) {
  record("rig.completed", false, { message: error.message, stack: error.stack?.split("\n").slice(0, 8) });
  process.exitCode = 1;
} finally {
  if (browser && extensionId) {
    await browser.send("Extensions.uninstall", { id: extensionId }).catch(() => {});
  }
  await stopChrome().catch(() => {});
  if (server?.pid) {
    try { server.kill("SIGTERM"); } catch {}
  }
  await sleep(500);
  if (existsSync(PROFILE_DIR)) rmSync(PROFILE_DIR, { recursive: true, force: false });
  cleaned = !existsSync(PROFILE_DIR);

  const packagePath = resolve(SCRATCH_DIR, "release", "local-browser-bridge-extension-v0.11.1.zip");
  const artifact = {
    recordedAt: new Date().toISOString(),
    scenario: "stock Chrome native debugger warning Cancel",
    release: {
      version: "0.11.1",
      serverPath: SERVER_BIN,
      serverSha256: sha256(SERVER_BIN),
      extensionDirectory: EXTENSION_DIR,
      extensionZip: existsSync(packagePath) ? packagePath : null,
      extensionZipSha256: existsSync(packagePath) ? sha256(packagePath) : null,
    },
    environment: {
      browserVersion: commandOutput(CHROME_BIN, ["--version"]),
      operatingSystem: `${platform()} ${release()}`,
      architecture: arch(),
      browserProfile: "fresh disposable profile removed after the run",
      extensionInstallSurface: "browser-target CDP Extensions.loadUnpacked",
      serverPort: SERVER_PORT,
      cdpPort: CDP_PORT,
    },
    hygiene: {
      bearerTokenPersistedInEvidence: false,
      disposableProfileRemoved: cleaned,
      dedicatedChromeProcessStopped: chrome === null,
      serverStopped: server?.killed === true,
    },
    passed: results.filter((row) => row.ok).length,
    total: results.length,
    results,
    processEvents: events,
    nativeUiInteraction: existsSync(UI_INTERACTION_PATH)
      ? JSON.parse(readFileSync(UI_INTERACTION_PATH, "utf8"))
      : null,
    screenshots: [
      "10-native-warning-and-page-pill.png",
      "11-native-after-cancel.png",
    ].filter((name) => existsSync(resolve(OUTPUT_DIR, name))).map((name) => ({
      file: name,
      sha256: sha256(resolve(OUTPUT_DIR, name)),
    })),
  };
  writeFileSync(resolve(OUTPUT_DIR, "native-warning-results.json"), `${JSON.stringify(artifact, null, 2)}\n`);
  note(`${artifact.passed}/${artifact.total} checks passed; profile cleanup=${cleaned}`);
}
