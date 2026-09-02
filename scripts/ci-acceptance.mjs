#!/usr/bin/env node
// CI-hosted product acceptance for Local Browser Bridge.
//
// Runs the real packaged (or freshly built) server, helper, and extension on
// the current machine and records every check as
// {name, lane, status: pass|fail|skip, required, reason, evidence} in
// <out>/acceptance.json, together with PNG screenshots and redacted logs.
//
// Usage:
//   node scripts/ci-acceptance.mjs --mode source|artifact --dir DIR --out DIR
//        [--version V] [--chrome PATH] [--extension-dir DIR]
//        [--lanes server,shell,browser,computer] [--source-sha SHA]
//
// Node 24, no npm dependencies. Works on macOS and Windows.

import { spawn, spawnSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer as createNetServer } from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..");
const IS_WINDOWS = process.platform === "win32";
const IS_MACOS = process.platform === "darwin";
const OS_NAME = IS_WINDOWS ? "windows" : IS_MACOS ? "macos" : process.platform;
const ALL_LANES = ["server", "shell", "browser", "computer"];

const WINDOWS_FIXTURE_TITLE = "LBB Windows Fixture Target";
const MACOS_FIXTURE_TITLE = "LBB CI Acceptance Fixture";
const FIXTURE_TITLE = IS_WINDOWS ? WINDOWS_FIXTURE_TITLE : MACOS_FIXTURE_TITLE;
const TYPED_TEXT = "lbb-ci";
const SET_VALUE = "ci-value";

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

function parseArgs(argv) {
  const options = {
    mode: "",
    dir: "",
    out: "",
    version: "",
    chrome: process.env.LBB_ACCEPTANCE_CHROME || "",
    extensionDir: "",
    lanes: ALL_LANES.slice(),
    sourceSha: process.env.GITHUB_SHA || "",
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const value = () => {
      index += 1;
      if (index >= argv.length) {
        throw new Error(`${argument} requires a value`);
      }
      return argv[index];
    };
    switch (argument) {
      case "--mode":
        options.mode = value();
        break;
      case "--dir":
        options.dir = path.resolve(value());
        break;
      case "--out":
        options.out = path.resolve(value());
        break;
      case "--version":
        options.version = value();
        break;
      case "--chrome":
        options.chrome = path.resolve(value());
        break;
      case "--extension-dir":
        options.extensionDir = path.resolve(value());
        break;
      case "--lanes":
        options.lanes = value()
          .split(",")
          .map((lane) => lane.trim())
          .filter(Boolean);
        break;
      case "--source-sha":
        options.sourceSha = value();
        break;
      case "--help":
      case "-h":
        console.log(
          "Usage: node scripts/ci-acceptance.mjs --mode source|artifact --dir DIR --out DIR [--version V] [--chrome PATH] [--extension-dir DIR] [--lanes a,b] [--source-sha SHA]",
        );
        process.exit(0);
        break;
      default:
        throw new Error(`Unknown argument: ${argument}`);
    }
  }
  if (!["source", "artifact"].includes(options.mode)) {
    throw new Error("--mode must be source or artifact");
  }
  if (!options.dir || !options.out) {
    throw new Error("--dir and --out are required");
  }
  for (const lane of options.lanes) {
    if (!ALL_LANES.includes(lane)) {
      throw new Error(`Unknown lane: ${lane}`);
    }
  }
  if (!options.version) {
    options.version = readSourceVersion();
  }
  if (!/^[0-9]+\.[0-9]+\.[0-9]+$/.test(options.version)) {
    throw new Error(`Invalid version: ${options.version}`);
  }
  return options;
}

function readSourceVersion() {
  const cargo = readFileSync(path.join(REPO_ROOT, "Cargo.toml"), "utf8");
  const match = cargo.match(/^version = "([^"]+)"$/m);
  if (!match) {
    throw new Error("Could not read the package version from Cargo.toml");
  }
  return match[1];
}

// ---------------------------------------------------------------------------
// Small utilities
// ---------------------------------------------------------------------------

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function sha256Hex(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function base64Url(bytes) {
  return bytes.toString("base64").replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

async function freePort() {
  return new Promise((resolve, reject) => {
    const server = createNetServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

async function waitUntil(label, predicate, { timeoutMs = 30_000, intervalMs = 250 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const value = await predicate();
      if (value) {
        return value;
      }
    } catch (error) {
      lastError = error;
    }
    await sleep(intervalMs);
  }
  throw new Error(`Timed out after ${timeoutMs} ms waiting for ${label}${lastError ? `: ${lastError.message}` : ""}`);
}

function readJsonWithRetry(file, attempts = 10) {
  let lastError = null;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      return JSON.parse(readFileSync(file, "utf8"));
    } catch (error) {
      lastError = error;
      const wait = Date.now() + 50;
      while (Date.now() < wait) {
        // brief synchronous backoff while the writer finishes
      }
    }
  }
  throw lastError;
}

function truncate(value, limit = 400) {
  const text = typeof value === "string" ? value : JSON.stringify(value);
  if (text === undefined) {
    return null;
  }
  return text.length > limit ? `${text.slice(0, limit)}…` : text;
}

function errorSummary(response) {
  return {
    status: response.status,
    code: response.error?.code ?? null,
    message: truncate(String(response.error?.message ?? ""), 240),
    taxonomy: response.taxonomy?.code ?? null,
  };
}

function summarizeCommandResult(result) {
  if (!result || typeof result !== "object") {
    return result;
  }
  const copy = { ...result };
  for (const heavy of ["elements", "windows", "screenshot", "tabs", "text", "html"]) {
    if (heavy in copy) {
      const value = copy[heavy];
      copy[heavy] = Array.isArray(value) ? `[${value.length} items]` : typeof value === "string" ? `[${value.length} chars]` : "[omitted]";
    }
  }
  return copy;
}

// ---------------------------------------------------------------------------
// Result recording
// ---------------------------------------------------------------------------

class Recorder {
  constructor(out) {
    this.out = out;
    this.checks = [];
  }

  record(lane, name, status, { required = true, reason = null, evidence = null } = {}) {
    const entry = {
      name,
      lane,
      status,
      required,
      reason: reason === null || reason === undefined ? null : truncate(reason, 600),
      evidence: evidence === null || evidence === undefined ? null : evidence,
      at: new Date().toISOString(),
    };
    this.checks.push(entry);
    const marker = status === "pass" ? "PASS" : status === "fail" ? "FAIL" : "SKIP";
    const suffix = entry.reason ? ` :: ${entry.reason}` : "";
    console.log(`${marker} ${lane}/${name}${required ? "" : " (optional)"}${suffix}`);
    return entry;
  }

  pass(lane, name, evidence = null, extra = {}) {
    return this.record(lane, name, "pass", { evidence, ...extra });
  }

  fail(lane, name, reason, evidence = null, extra = {}) {
    return this.record(lane, name, "fail", { reason, evidence, ...extra });
  }

  skip(lane, name, reason, extra = {}) {
    return this.record(lane, name, "skip", { reason, required: false, ...extra });
  }

  summary() {
    const summary = { total: this.checks.length, pass: 0, fail: 0, skip: 0, requiredFail: 0 };
    for (const check of this.checks) {
      summary[check.status] += 1;
      if (check.status === "fail" && check.required) {
        summary.requiredFail += 1;
      }
    }
    return summary;
  }
}

// ---------------------------------------------------------------------------
// Process management
// ---------------------------------------------------------------------------

class Processes {
  constructor(out, secrets) {
    this.out = out;
    this.secrets = secrets;
    this.children = [];
  }

  redact(text) {
    let output = String(text);
    for (const secret of this.secrets) {
      if (secret) {
        output = output.split(secret).join("<redacted>");
      }
    }
    // The Agent Fetch capability is derived from the token and is itself a credential.
    return output.replace(/\/api\/v1\/fetch\/[A-Za-z0-9_-]+/g, "/api/v1/fetch/<redacted>");
  }

  spawn(label, command, args, { cwd, env, logName } = {}) {
    const logPath = path.join(this.out, "logs", `${logName || label}.log`);
    mkdirSync(path.dirname(logPath), { recursive: true });
    const child = spawn(command, args, {
      cwd: cwd || this.out,
      env: { ...process.env, ...(env || {}) },
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: false,
    });
    const chunks = [];
    const onData = (chunk) => {
      const text = this.redact(chunk.toString("utf8"));
      chunks.push(text);
      try {
        writeFileSync(logPath, chunks.join(""));
      } catch {
        // log writes are best effort
      }
    };
    child.stdout.on("data", onData);
    child.stderr.on("data", onData);
    child.on("error", (error) => onData(`\n[spawn error] ${error.message}\n`));
    const entry = { label, child, exited: false, code: null, logPath };
    child.on("exit", (code, signal) => {
      entry.exited = true;
      entry.code = code ?? signal;
    });
    this.children.push(entry);
    return entry;
  }

  async stop(entry, { graceMs = 3_000 } = {}) {
    if (!entry || entry.exited) {
      return;
    }
    try {
      entry.child.kill();
    } catch {
      // already gone
    }
    const deadline = Date.now() + graceMs;
    while (!entry.exited && Date.now() < deadline) {
      await sleep(100);
    }
    if (!entry.exited && !IS_WINDOWS) {
      try {
        entry.child.kill("SIGKILL");
      } catch {
        // ignore
      }
    }
    if (!entry.exited && IS_WINDOWS) {
      spawnSync("taskkill", ["/PID", String(entry.child.pid), "/T", "/F"], { stdio: "ignore" });
    }
  }

  async stopAll() {
    for (const entry of this.children.slice().reverse()) {
      await this.stop(entry);
    }
  }
}

// ---------------------------------------------------------------------------
// Bridge REST client
// ---------------------------------------------------------------------------

class Bridge {
  constructor(port, token) {
    this.port = port;
    this.token = token;
    this.base = `http://127.0.0.1:${port}`;
    this.sequence = 0;
  }

  headers(extra = {}) {
    return { Authorization: `Bearer ${this.token}`, "Content-Type": "application/json", ...extra };
  }

  async command(method, params = {}, { callId, withAuth = true } = {}) {
    this.sequence += 1;
    const id = callId ?? `acc-${method}-${Date.now()}-${this.sequence}`;
    const response = await fetch(`${this.base}/api/v1/command`, {
      method: "POST",
      headers: withAuth ? this.headers() : { "Content-Type": "application/json" },
      body: JSON.stringify({ method, params, callId: id }),
    });
    const text = await response.text();
    let body = {};
    try {
      body = JSON.parse(text);
    } catch {
      body = { raw: text };
    }
    return {
      status: response.status,
      body,
      result: body.result,
      error: body.error,
      taxonomy: body.taxonomy,
      state: body.state,
      callId: id,
    };
  }

  async state() {
    const response = await fetch(`${this.base}/api/state`, { headers: this.headers() });
    const body = response.status === 200 ? await response.json() : null;
    return { status: response.status, body: body?.state ?? null };
  }

  async health() {
    const response = await fetch(`${this.base}/health`);
    return { status: response.status, body: response.status === 200 ? await response.json() : null };
  }

  async binary(pathname) {
    const response = await fetch(`${this.base}${pathname}`, { headers: { Authorization: `Bearer ${this.token}` } });
    const bytes = Buffer.from(await response.arrayBuffer());
    return { status: response.status, type: response.headers.get("content-type"), bytes };
  }
}

function isPng(bytes) {
  return bytes.length > 8 && bytes[0] === 0x89 && bytes.subarray(1, 4).toString("ascii") === "PNG";
}

function isJpeg(bytes) {
  return bytes.length > 4 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff;
}

// Saves a fetched screenshot in its native encoding (the browser observation
// screenshot is JPEG, the native computer frame is PNG).
function saveImage(fileStem, bytes) {
  const extension = isPng(bytes) ? ".png" : isJpeg(bytes) ? ".jpg" : null;
  if (!extension) {
    return null;
  }
  const file = `${fileStem}${extension}`;
  writeFileSync(file, bytes);
  return { file: path.basename(file), bytes: bytes.length };
}

function savePngFromDataUrl(file, dataUrl) {
  if (typeof dataUrl !== "string" || dataUrl.length < 64) {
    return null;
  }
  const encoded = dataUrl.replace(/^data:image\/\w+;base64,/, "");
  const bytes = Buffer.from(encoded, "base64");
  if (!isPng(bytes)) {
    return null;
  }
  writeFileSync(file, bytes);
  return { file: path.basename(file), bytes: bytes.length };
}

// ---------------------------------------------------------------------------
// Product inventory
// ---------------------------------------------------------------------------

function extractArchive(archive, destination) {
  mkdirSync(destination, { recursive: true });
  let command;
  let args;
  if (archive.endsWith(".zip")) {
    if (IS_WINDOWS) {
      command = "powershell.exe";
      args = ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", `Expand-Archive -LiteralPath ${JSON.stringify(archive)} -DestinationPath ${JSON.stringify(destination)} -Force`];
    } else {
      command = "unzip";
      args = ["-q", "-o", archive, "-d", destination];
    }
  } else {
    command = "tar";
    args = ["-xzf", archive, "-C", destination];
  }
  const result = spawnSync(command, args, { stdio: "pipe" });
  if (result.status !== 0) {
    throw new Error(`${command} failed for ${path.basename(archive)}: ${result.stderr?.toString() || result.error?.message}`);
  }
}

function verifyChecksums(dir) {
  const manifestPath = path.join(dir, "SHA256SUMS.txt");
  const manifest = readFileSync(manifestPath, "utf8");
  const verified = [];
  for (const line of manifest.split("\n").filter(Boolean)) {
    const match = line.match(/^([0-9a-f]{64})  (\S+)$/);
    if (!match) {
      throw new Error(`Malformed checksum line: ${line}`);
    }
    const [, expected, name] = match;
    const actual = sha256Hex(readFileSync(path.join(dir, name)));
    if (actual !== expected) {
      throw new Error(`Checksum mismatch for ${name}`);
    }
    verified.push({ name, sha256: actual });
  }
  return { manifestSha256: sha256Hex(readFileSync(manifestPath)), assets: verified };
}

function resolveInventory(options) {
  const { mode, dir, out, version } = options;
  const inventory = { mode, sourceDir: dir, checksums: null };
  if (mode === "artifact") {
    inventory.checksums = verifyChecksums(dir);
    const extensionZip = path.join(dir, `local-browser-bridge-extension-v${version}.zip`);
    inventory.extensionDir = path.join(out, "candidate", "extension");
    extractArchive(extensionZip, inventory.extensionDir);
    if (IS_WINDOWS) {
      inventory.server = path.join(dir, `local-browser-bridge-v${version}-windows-x86_64.exe`);
      inventory.helper = path.join(dir, `local-computer-helper-v${version}-windows-x86_64.exe`);
      inventory.serverKind = "desktop-host";
    } else {
      const archive = path.join(dir, `local-browser-bridge-v${version}-macos-universal.tar.gz`);
      const payload = path.join(out, "candidate", "macos");
      extractArchive(archive, payload);
      inventory.server = path.join(payload, "local-browser-bridge");
      inventory.helper = path.join(payload, "Local Computer Helper.app", "Contents", "MacOS", "local-computer-helper");
      inventory.desktopHost = path.join(payload, "Local Browser Bridge.app", "Contents", "MacOS", "local-browser-bridge-desktop");
      inventory.serverKind = "cli";
    }
  } else {
    inventory.extensionDir = options.extensionDir || path.join(REPO_ROOT, "extension");
    if (IS_WINDOWS) {
      inventory.server = path.join(dir, "local-browser-bridge-desktop.exe");
      inventory.helper = path.join(dir, "local-computer-helper.exe");
      inventory.serverKind = "desktop-host";
    } else {
      inventory.server = path.join(dir, "local-browser-bridge");
      inventory.helper = path.join(dir, "local-computer-helper");
      inventory.serverKind = "cli";
    }
  }
  for (const [label, file] of [
    ["server", inventory.server],
    ["helper", inventory.helper],
    ["extension manifest", path.join(inventory.extensionDir, "manifest.json")],
  ]) {
    if (!existsSync(file)) {
      throw new Error(`Missing ${label}: ${file}`);
    }
  }
  inventory.extensionManifest = JSON.parse(readFileSync(path.join(inventory.extensionDir, "manifest.json"), "utf8"));
  return inventory;
}

function binaryVersion(file) {
  const result = spawnSync(file, ["--version"], { encoding: "utf8", timeout: 20_000 });
  if (result.status !== 0) {
    throw new Error(`--version failed for ${path.basename(file)}: ${result.stderr || result.error?.message}`);
  }
  return result.stdout.trim();
}

// ---------------------------------------------------------------------------
// Fixture page server
// ---------------------------------------------------------------------------

const FIXTURE_INDEX = `<!doctype html>
<html><head><meta charset="utf-8"><title>LBB acceptance page</title>
<style>body{font-family:sans-serif;padding:24px}#log{border:1px solid #888;padding:8px;min-height:24px}</style></head>
<body>
<h1 id="heading">LBB acceptance page</h1>
<p id="intro">This page exists so the bridge can observe text, click a button, fill a field, and evaluate JavaScript.</p>
<button id="hello-button" onclick="document.getElementById('log').textContent='button-clicked:'+(++window.__clicks)">Say hello</button>
<label for="name-field">Name</label>
<input id="name-field" type="text" placeholder="type here" oninput="document.getElementById('echo').textContent='typed:'+this.value">
<a id="second-link" href="/second.html">Go to second page</a>
<div id="log">no-click-yet</div>
<div id="echo">nothing-typed</div>
<script>window.__clicks = 0; window.__acceptance = 'ready';</script>
</body></html>
`;

const FIXTURE_SECOND = `<!doctype html>
<html><head><meta charset="utf-8"><title>LBB second page</title></head>
<body><h1 id="second-heading">Second page reached</h1><p>Navigation target for page.navigate and page.waitFor.</p></body></html>
`;

async function startFixtureSite() {
  const port = await freePort();
  const server = createHttpServer((request, response) => {
    const url = new URL(request.url, "http://127.0.0.1");
    const body = url.pathname === "/second.html" ? FIXTURE_SECOND : url.pathname === "/" || url.pathname === "/index.html" ? FIXTURE_INDEX : null;
    if (body === null) {
      response.writeHead(404, { "Content-Type": "text/plain" });
      response.end("not found");
      return;
    }
    response.writeHead(200, { "Content-Type": "text/html; charset=utf-8", "Cache-Control": "no-store" });
    response.end(body);
  });
  await new Promise((resolve, reject) => {
    server.on("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  server.unref();
  return { port, base: `http://127.0.0.1:${port}`, close: () => server.close() };
}

// ---------------------------------------------------------------------------
// Chrome DevTools helpers
// ---------------------------------------------------------------------------

class Cdp {
  constructor(port) {
    this.port = port;
    this.base = `http://127.0.0.1:${port}`;
  }

  async targets() {
    const response = await fetch(`${this.base}/json/list`);
    return response.json();
  }

  async version() {
    const response = await fetch(`${this.base}/json/version`);
    return response.json();
  }

  async evaluate(target, expression) {
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
      socket.onopen = resolve;
      socket.onerror = () => reject(new Error("CDP socket failed"));
    });
    try {
      const message = await new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error("CDP evaluate timed out")), 15_000);
        socket.onmessage = (event) => {
          const parsed = JSON.parse(event.data);
          if (parsed.id === 1) {
            clearTimeout(timer);
            resolve(parsed);
          }
        };
        socket.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, returnByValue: true, awaitPromise: true } }));
      });
      if (message.result?.exceptionDetails) {
        throw new Error(message.result.exceptionDetails.text || "CDP evaluate threw");
      }
      return message.result?.result?.value;
    } finally {
      socket.close();
    }
  }

  async findExtensionWorker(name, timeoutMs = 45_000) {
    return waitUntil(
      "extension service worker",
      async () => {
        const targets = await this.targets();
        for (const target of targets.filter((entry) => entry.type === "service_worker")) {
          try {
            const manifestName = await this.evaluate(target, "chrome.runtime.getManifest().name");
            if (manifestName === name) {
              return target;
            }
          } catch {
            // the worker may still be starting
          }
        }
        return null;
      },
      { timeoutMs, intervalMs: 1_000 },
    );
  }
}

// ---------------------------------------------------------------------------
// Lanes
// ---------------------------------------------------------------------------

async function startServer(context, { shell, label }) {
  const { inventory, processes, token } = context;
  const port = await freePort();
  const cwd = path.join(context.out, `server-cwd-${label}`);
  mkdirSync(cwd, { recursive: true });
  const env = {
    LBB_TOKEN: token,
    LBB_PORT: String(port),
    LBB_DISABLE_UPDATE_CHECK: "1",
    LBB_ENABLE_SHELL: shell ? "1" : "0",
  };
  const args = inventory.serverKind === "desktop-host" ? [] : ["--no-update-check"];
  const entry = processes.spawn(`server-${label}`, inventory.server, args, { cwd, env });
  const bridge = new Bridge(port, token);
  const health = await waitUntil(
    `server ${label} /health`,
    async () => {
      if (entry.exited) {
        throw new Error(`server exited with ${entry.code}`);
      }
      const response = await bridge.health();
      return response.status === 200 ? response.body : null;
    },
    { timeoutMs: 45_000 },
  );
  return { entry, bridge, port, health, cwd };
}

async function serverLane(context) {
  const { recorder, inventory, version } = context;
  const lane = "server";
  try {
    const reported = binaryVersion(inventory.server);
    const expected = inventory.serverKind === "desktop-host" ? `local-browser-bridge-desktop ${version}` : `local-browser-bridge ${version}`;
    if (reported === expected) {
      recorder.pass(lane, "binary.version", { reported });
    } else {
      recorder.fail(lane, "binary.version", `expected "${expected}", got "${reported}"`);
    }
  } catch (error) {
    recorder.fail(lane, "binary.version", error.message);
  }

  let server;
  try {
    server = await startServer(context, { shell: true, label: "main" });
    recorder.pass(lane, "boot.health", { port: "ephemeral", health: server.health });
  } catch (error) {
    recorder.fail(lane, "boot.health", error.message);
    return null;
  }
  context.server = server;
  const { bridge } = server;

  if (server.health.version === version) {
    recorder.pass(lane, "health.version", { version });
  } else {
    recorder.fail(lane, "health.version", `health reported ${server.health.version}, expected ${version}`);
  }

  try {
    const state = await bridge.state();
    const agentFetch = state.body?.agentFetch;
    const ok = state.status === 200 && typeof agentFetch?.baseUrl === "string" && agentFetch.baseUrl.startsWith(`http://127.0.0.1:${server.port}/api/v1/fetch/`);
    if (ok) {
      recorder.pass(lane, "api.state", {
        revision: state.body.revision,
        connected: state.body.connected,
        computerConnected: state.body.computerConnected,
        shellEnabled: state.body.shell?.enabled,
        agentFetchEnabled: agentFetch.enabled,
      });
      context.agentFetchBase = agentFetch.baseUrl;
    } else {
      recorder.fail(lane, "api.state", `status ${state.status}, agentFetch=${truncate(agentFetch)}`);
    }
  } catch (error) {
    recorder.fail(lane, "api.state", error.message);
  }

  try {
    const response = await fetch(`${bridge.base}/api/state`);
    if (response.status === 401) {
      recorder.pass(lane, "api.state.unauthenticated-401", { status: 401 });
    } else {
      recorder.fail(lane, "api.state.unauthenticated-401", `expected 401, got ${response.status}`);
    }
  } catch (error) {
    recorder.fail(lane, "api.state.unauthenticated-401", error.message);
  }

  try {
    const response = await fetch(`${bridge.base}/`);
    const html = await response.text();
    const ok = response.status === 200 && /<html/i.test(html) && /Local Browser Bridge/.test(html);
    if (ok) {
      recorder.pass(lane, "dashboard.html", { status: response.status, bytes: html.length });
    } else {
      recorder.fail(lane, "dashboard.html", `status ${response.status}, html ${html.length} bytes`);
    }
  } catch (error) {
    recorder.fail(lane, "dashboard.html", error.message);
  }

  try {
    const response = await fetch(`${bridge.base}/api/session`, { headers: bridge.headers() });
    const body = await response.json();
    if (response.status === 200 && body.ok === true && typeof body.csrfToken === "string") {
      recorder.pass(lane, "dashboard.session", { status: 200, expiresAfterIdleSeconds: body.expiresAfterIdleSeconds });
    } else {
      recorder.fail(lane, "dashboard.session", `status ${response.status}`);
    }
  } catch (error) {
    recorder.fail(lane, "dashboard.session", error.message);
  }
  return server;
}

async function shellLane(context) {
  const { recorder, server } = context;
  const lane = "shell";
  if (!server) {
    recorder.fail(lane, "run.success", "server lane did not boot");
    return;
  }
  const { bridge } = server;

  const status = await bridge.command("shell.status");
  if (status.status === 200 && status.result?.enabled === true) {
    recorder.pass(lane, "status.enabled", summarizeCommandResult(status.result));
  } else {
    recorder.fail(lane, "status.enabled", truncate(errorSummary(status)));
  }

  const success = await bridge.command("shell.run", { command: "echo lbb-shell-ok" });
  if (success.status === 200 && success.result?.exitCode === 0 && /lbb-shell-ok/.test(success.result.stdout || "")) {
    recorder.pass(lane, "run.success", { exitCode: 0, stdout: truncate(success.result.stdout, 80) });
  } else {
    recorder.fail(lane, "run.success", truncate(success.status === 200 ? success.result : errorSummary(success)));
  }

  const failure = await bridge.command("shell.run", { command: "exit 3" });
  if (failure.status === 200 && failure.result?.exitCode === 3) {
    recorder.pass(lane, "run.nonzero-exit", { exitCode: 3 });
  } else {
    recorder.fail(lane, "run.nonzero-exit", truncate(failure.status === 200 ? failure.result : errorSummary(failure)));
  }

  const sleepCommand = IS_WINDOWS ? "Start-Sleep -Seconds 20" : "sleep 20";
  const timeout = await bridge.command("shell.run", { command: sleepCommand, timeoutMs: 1_500 });
  if (timeout.status === 200 && timeout.result?.timedOut === true) {
    recorder.pass(lane, "run.timeout", { timedOut: true, durationMs: timeout.result.durationMs ?? null });
  } else {
    recorder.fail(lane, "run.timeout", truncate(timeout.status === 200 ? timeout.result : errorSummary(timeout)));
  }

  const floodCommand = IS_WINDOWS
    ? "$line = 'x' * 1024; for ($i = 0; $i -lt 1200; $i++) { [Console]::Out.WriteLine($line) }"
    : "yes 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' | head -c 1300000";
  const bounded = await bridge.command("shell.run", { command: floodCommand, timeoutMs: 60_000 });
  const stdoutLength = Buffer.byteLength(bounded.result?.stdout || "", "utf8");
  if (bounded.status === 200 && bounded.result?.stdoutTruncated === true && stdoutLength <= 1024 * 1024) {
    recorder.pass(lane, "run.bounded-output", { stdoutBytes: stdoutLength, stdoutTruncated: true });
  } else {
    recorder.fail(lane, "run.bounded-output", truncate(bounded.status === 200 ? { ...bounded.result, stdout: `[${stdoutLength} bytes]` } : errorSummary(bounded)));
  }

  try {
    const base = context.agentFetchBase;
    if (!base) {
      throw new Error("agentFetch.baseUrl was not published by /api/state");
    }
    const callId = `fetch-${Date.now()}`;
    const url = `${base}/shell.run?callId=${encodeURIComponent(callId)}&command=${encodeURIComponent("str:echo lbb-fetch-ok")}`;
    const response = await fetch(url);
    const body = await response.json();
    if (response.status === 200 && body.result?.exitCode === 0 && /lbb-fetch-ok/.test(body.result.stdout || "")) {
      recorder.pass(lane, "agent-fetch.run", { status: 200, callId: body.callId ?? callId });
    } else {
      recorder.fail(lane, "agent-fetch.run", truncate({ status: response.status, body: summarizeCommandResult(body) }));
    }
    const replay = await fetch(url);
    const replayBody = await replay.json();
    if (replay.status === 200 && replayBody.replayed === true) {
      recorder.pass(lane, "agent-fetch.replay", { replayed: true });
    } else {
      recorder.fail(lane, "agent-fetch.replay", truncate({ status: replay.status, body: summarizeCommandResult(replayBody) }));
    }
  } catch (error) {
    recorder.fail(lane, "agent-fetch.run", error.message);
  }

  const unauthenticated = await bridge.command("shell.run", { command: "echo nope" }, { withAuth: false });
  if (unauthenticated.status === 401) {
    recorder.pass(lane, "run.unauthenticated-401", { status: 401 });
  } else {
    recorder.fail(lane, "run.unauthenticated-401", `expected 401, got ${unauthenticated.status}`);
  }
}

// Runs after the main server has been stopped: the Windows desktop host is a
// single-instance process guarded by a named mutex, so a second host can only
// start once the first one is gone.
async function shellDisabledLane(context) {
  const { recorder } = context;
  const lane = "shell";
  let disabled = null;
  try {
    disabled = await startServer(context, { shell: false, label: "shell-disabled" });
    const refused = await disabled.bridge.command("shell.run", { command: "echo nope" });
    if (refused.status === 403 && refused.error?.code === "SHELL_DISABLED") {
      recorder.pass(lane, "run.disabled-403", errorSummary(refused));
    } else {
      recorder.fail(lane, "run.disabled-403", truncate(errorSummary(refused)));
    }
  } catch (error) {
    recorder.fail(lane, "run.disabled-403", error.message);
  } finally {
    if (disabled) {
      await context.processes.stop(disabled.entry);
    }
  }
}

async function browserLane(context) {
  const { recorder, server, inventory, processes, out, options, version } = context;
  const lane = "browser";
  if (!server) {
    recorder.fail(lane, "extension.connected", "server lane did not boot");
    return;
  }
  if (!options.chrome) {
    recorder.fail(lane, "chrome.launch", "no --chrome executable was supplied");
    return;
  }
  const { bridge } = server;
  const site = await startFixtureSite();
  context.site = site;
  const cdpPort = await freePort();
  const profile = path.join(out, "chrome-profile");
  mkdirSync(profile, { recursive: true });
  const chromeArgs = [
    `--user-data-dir=${profile}`,
    `--load-extension=${inventory.extensionDir}`,
    `--remote-debugging-port=${cdpPort}`,
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-background-networking",
    "--disable-component-update",
    "--disable-sync",
    "--disable-features=Translate,OptimizationHints,MediaRouter",
    "--use-mock-keychain",
    "--password-store=basic",
    "--window-size=1024,720",
    "--window-position=40,40",
    "about:blank",
  ];
  const chrome = processes.spawn("chrome", options.chrome, chromeArgs, { cwd: out });
  const cdp = new Cdp(cdpPort);
  try {
    const info = await waitUntil(
      "Chrome DevTools endpoint",
      async () => {
        if (chrome.exited) {
          throw new Error(`Chrome exited with ${chrome.code}`);
        }
        return cdp.version();
      },
      { timeoutMs: 60_000, intervalMs: 500 },
    );
    context.chromeVersion = info.Browser;
    recorder.pass(lane, "chrome.launch", { browser: info.Browser, protocol: info["Protocol-Version"] });
  } catch (error) {
    recorder.fail(lane, "chrome.launch", error.message);
    return;
  }

  let worker;
  try {
    worker = await cdp.findExtensionWorker("Local Browser Bridge");
    const extensionVersion = await cdp.evaluate(worker, "chrome.runtime.getManifest().version");
    await cdp.evaluate(
      worker,
      `chrome.storage.local.set({token:${JSON.stringify(bridge.token)},port:${server.port},enabled:true,fullAccess:true}).then(() => 'storage-set')`,
    );
    recorder.pass(lane, "extension.bootstrap", { extensionVersion, workerUrl: worker.url.replace(/chrome-extension:\/\/[a-p]{32}/, "chrome-extension://<id>") });
  } catch (error) {
    recorder.fail(lane, "extension.bootstrap", error.message);
    return;
  }

  try {
    const state = await waitUntil(
      "extension connection",
      async () => {
        const current = await bridge.state();
        return current.body?.connected && current.body.extension ? current.body : null;
      },
      { timeoutMs: 45_000, intervalMs: 500 },
    );
    const extensionVersion = state.extension?.version;
    if (extensionVersion === version) {
      recorder.pass(lane, "extension.connected", { extensionVersion, fullAccess: state.extension?.fullAccess ?? null });
    } else {
      recorder.fail(lane, "extension.connected", `extension version ${extensionVersion} does not match ${version}`);
    }
  } catch (error) {
    recorder.fail(lane, "extension.connected", error.message);
    return;
  }

  const tabsBefore = await bridge.command("tabs.list");
  if (tabsBefore.status === 200 && Array.isArray(tabsBefore.result?.tabs)) {
    recorder.pass(lane, "tabs.list", { count: tabsBefore.result.tabs.length });
  } else {
    recorder.fail(lane, "tabs.list", truncate(errorSummary(tabsBefore)));
  }

  const created = await bridge.command("tabs.new", { url: `${site.base}/index.html` });
  const tabId = created.result?.tabId;
  if (created.status === 200 && typeof tabId === "number") {
    recorder.pass(lane, "tabs.new", { tabId });
  } else {
    recorder.fail(lane, "tabs.new", truncate(errorSummary(created)));
    return;
  }
  await sleep(1_500);

  const start = await bridge.command("browser.control.start", { tabId, ttlMs: 300_000 });
  const control = start.result?.control ?? start.result;
  if (start.status === 200 && control?.active === true) {
    recorder.pass(lane, "control.start", { active: true, sessionId: control.sessionId ?? null });
  } else {
    recorder.fail(lane, "control.start", truncate(errorSummary(start)));
    return;
  }
  await sleep(500);

  const observe = async (label) => {
    const response = await bridge.command("page.observe", { tabId });
    const snapshot = response.result?.snapshot ?? response.result ?? {};
    const elements = Array.isArray(snapshot.elements) ? snapshot.elements : [];
    let screenshot = savePngFromDataUrl(path.join(out, `browser-observe-${label}.png`), snapshot.screenshot);
    let screenshotError = null;
    if (!screenshot && response.status === 200) {
      try {
        const state = await bridge.state();
        const locator = state.body?.observation?.screenshotUrl;
        if (!locator) {
          throw new Error("state.observation.screenshotUrl is absent");
        }
        const shot = await bridge.binary(locator);
        screenshot = shot.status === 200 ? saveImage(path.join(out, `browser-observe-${label}`), shot.bytes) : null;
        if (!screenshot) {
          screenshotError = `screenshot status ${shot.status} (${shot.type}) ${shot.bytes.subarray(0, 200).toString("utf8")}`;
        }
      } catch (error) {
        screenshot = null;
        screenshotError = error.message;
      }
    }
    const text = String(snapshot.bodyText ?? snapshot.text ?? "");
    return { response, snapshot, elements, generation: snapshot.generation, screenshot, screenshotError, text };
  };

  let observation = await observe("1-initial");
  {
    const { response, snapshot, elements, screenshot, screenshotError, text } = observation;
    const textOk = /LBB acceptance page/.test(text);
    if (response.status === 200 && elements.length > 0 && textOk && screenshot) {
      recorder.pass(lane, "page.observe", { url: snapshot.url, title: snapshot.title, elements: elements.length, generation: snapshot.generation, screenshot });
    } else {
      recorder.fail(lane, "page.observe", truncate(response.status === 200 ? { elements: elements.length, textOk, screenshot, screenshotError, keys: Object.keys(snapshot) } : errorSummary(response)));
      return;
    }
  }

  try {
    const state = await bridge.state();
    const locator = state.body?.observation?.screenshotUrl;
    if (!locator) {
      throw new Error("state.observation.screenshotUrl is absent");
    }
    const shot = await bridge.binary(locator);
    const saved = shot.status === 200 ? saveImage(path.join(out, "browser-api-screenshot"), shot.bytes) : null;
    if (saved) {
      recorder.pass(lane, "api.screenshot", { ...saved, type: shot.type, width: state.body.observation.screenshotWidth ?? null, height: state.body.observation.screenshotHeight ?? null });
    } else {
      recorder.fail(lane, "api.screenshot", `status ${shot.status}, type ${shot.type}`);
    }
  } catch (error) {
    recorder.fail(lane, "api.screenshot", error.message);
  }

  const findElement = (elements, pattern, roles) =>
    elements.find((element) => pattern.test(element.name || "") && (!roles || roles.some((role) => new RegExp(role, "i").test(element.role || ""))));

  const button = findElement(observation.elements, /say hello/i);
  if (button) {
    const click = await bridge.command("page.click", { tabId, ref: button.ref, generation: observation.generation });
    await sleep(400);
    const evaluated = await bridge.command("page.evaluate", { tabId, expression: "document.getElementById('log').textContent" });
    const value = JSON.stringify(evaluated.result ?? null);
    if (click.status === 200 && evaluated.status === 200 && /button-clicked:1/.test(value)) {
      recorder.pass(lane, "page.click", { ref: button.ref, log: truncate(value, 80) });
    } else {
      recorder.fail(lane, "page.click", truncate({ click: click.status === 200 ? "ok" : errorSummary(click), evaluated: value }));
    }
  } else {
    recorder.fail(lane, "page.click", "observe did not expose the fixture button", { elements: observation.elements.slice(0, 20).map((element) => ({ ref: element.ref, role: element.role, name: element.name })) });
  }

  observation = await observe("2-after-click");
  const input = findElement(observation.elements, /name|type here/i, ["textbox", "input"]) ?? observation.elements.find((element) => /textbox/i.test(element.role || ""));
  if (input) {
    const fill = await bridge.command("page.fill", { tabId, ref: input.ref, generation: observation.generation, text: "bridge acceptance" });
    await sleep(300);
    const evaluated = await bridge.command("page.evaluate", { tabId, expression: "document.getElementById('name-field').value + '|' + document.getElementById('echo').textContent" });
    const value = JSON.stringify(evaluated.result ?? null);
    if (fill.status === 200 && /bridge acceptance\|typed:bridge acceptance/.test(value)) {
      recorder.pass(lane, "page.fill", { ref: input.ref, value: truncate(value, 80) });
    } else {
      recorder.fail(lane, "page.fill", truncate({ fill: fill.status === 200 ? "ok" : errorSummary(fill), evaluated: value }));
    }
  } else {
    recorder.fail(lane, "page.fill", "observe did not expose the fixture text input");
  }

  {
    const evaluated = await bridge.command("page.evaluate", { tabId, expression: "window.__acceptance + ':' + (6 * 7)" });
    const value = JSON.stringify(evaluated.result ?? null);
    if (evaluated.status === 200 && /ready:42/.test(value)) {
      recorder.pass(lane, "page.evaluate", { value: truncate(value, 80) });
    } else {
      recorder.fail(lane, "page.evaluate", truncate(evaluated.status === 200 ? value : errorSummary(evaluated)));
    }
  }

  const navigate = await bridge.command("page.navigate", { tabId, url: `${site.base}/second.html` });
  if (navigate.status === 200) {
    recorder.pass(lane, "page.navigate", summarizeCommandResult(navigate.result));
  } else {
    recorder.fail(lane, "page.navigate", truncate(errorSummary(navigate)));
  }

  const waited = await bridge.command("page.waitFor", { tabId, text: "Second page reached", timeoutMs: 10_000 });
  if (waited.status === 200 && waited.result?.satisfied === true) {
    recorder.pass(lane, "page.waitFor", summarizeCommandResult(waited.result));
  } else {
    recorder.fail(lane, "page.waitFor", truncate(waited.status === 200 ? waited.result : errorSummary(waited)));
  }
  await observe("3-second-page");

  const status = await bridge.command("browser.control.status");
  if (status.status === 200 && status.result?.active === true) {
    recorder.pass(lane, "control.status", { active: true, turn: status.result.turn ?? null });
  } else {
    recorder.fail(lane, "control.status", truncate(status.status === 200 ? status.result : errorSummary(status)));
  }

  const stop = await bridge.command("browser.control.stop");
  if (stop.status === 200 && stop.result?.active === false) {
    recorder.pass(lane, "control.stop", { active: false });
  } else {
    recorder.fail(lane, "control.stop", truncate(stop.status === 200 ? stop.result : errorSummary(stop)));
  }
}

function runPermissionProbe(helper) {
  const result = spawnSync(helper, ["--request-permissions"], { encoding: "utf8", timeout: 60_000 });
  if (result.status !== 0) {
    throw new Error(`--request-permissions exited with ${result.status}: ${(result.stderr || "").slice(0, 300)}`);
  }
  return JSON.parse(result.stdout);
}

function buildFixture(context) {
  const fixtureDir = path.join(context.out, "fixture");
  mkdirSync(fixtureDir, { recursive: true });
  if (IS_WINDOWS) {
    const source = path.join(REPO_ROOT, "tests", "fixtures", "windows", "WindowsComputerUseFixture.ps1");
    const executable = path.join(fixtureDir, "LbbAcceptanceFixture.exe");
    const powershell = path.join(process.env.SystemRoot || "C:\\Windows", "System32", "WindowsPowerShell", "v1.0", "powershell.exe");
    const sourceSha = sha256Hex(readFileSync(source));
    const build = spawnSync(
      powershell,
      ["-NoLogo", "-NoProfile", "-NonInteractive", "-File", source, "-BuildExecutablePath", executable, "-ExpectedSourceSha256", sourceSha],
      { encoding: "utf8", timeout: 300_000 },
    );
    if (build.status !== 0 || !existsSync(executable)) {
      throw new Error(`fixture build failed: ${(build.stdout || "").slice(-400)} ${(build.stderr || "").slice(-400)}`);
    }
    return { executable, sourceSha, evidenceDir: path.join(fixtureDir, "evidence") };
  }
  const source = path.join(REPO_ROOT, "tests", "fixtures", "macos", "CiAcceptanceFixture.swift");
  const executable = path.join(fixtureDir, "CiAcceptanceFixture");
  const build = spawnSync("xcrun", ["swiftc", "-O", source, "-o", executable], { encoding: "utf8", timeout: 300_000 });
  if (build.status !== 0 || !existsSync(executable)) {
    throw new Error(`fixture build failed: ${(build.stderr || "").slice(-600)}`);
  }
  return { executable, sourceSha: sha256Hex(readFileSync(source)), statePath: path.join(fixtureDir, "state.json") };
}

async function startFixture(context, fixture) {
  const { processes } = context;
  if (IS_WINDOWS) {
    const notepad = path.join(process.env.SystemRoot || "C:\\Windows", "System32", "notepad.exe");
    if (existsSync(notepad)) {
      processes.spawn("anchor-notepad", notepad, [], { cwd: context.out });
      await sleep(1_500);
    }
    mkdirSync(fixture.evidenceDir, { recursive: true });
    const entry = processes.spawn("fixture", fixture.executable, ["--evidence-directory", fixture.evidenceDir], { cwd: context.out });
    const readyPath = path.join(fixture.evidenceDir, "fixture-ready.json");
    const ready = await waitUntil(
      "Windows fixture ready file",
      () => {
        if (entry.exited) {
          throw new Error(`fixture exited with ${entry.code}`);
        }
        return existsSync(readyPath) ? readJsonWithRetry(readyPath) : null;
      },
      { timeoutMs: 60_000 },
    );
    fixture.entry = entry;
    fixture.ready = ready;
    fixture.readState = () => readJsonWithRetry(path.join(fixture.evidenceDir, "fixture-state.json"));
    return fixture;
  }
  const entry = processes.spawn("fixture", fixture.executable, [], { cwd: context.out, env: { LBB_FIXTURE_STATE: fixture.statePath } });
  await waitUntil(
    "macOS fixture state file",
    () => {
      if (entry.exited) {
        throw new Error(`fixture exited with ${entry.code}`);
      }
      return existsSync(fixture.statePath) ? readJsonWithRetry(fixture.statePath) : null;
    },
    { timeoutMs: 60_000 },
  );
  fixture.entry = entry;
  fixture.readState = () => readJsonWithRetry(fixture.statePath);
  fixture.bringToFront = () => {
    const script = `tell application "System Events" to set frontmost of (first process whose unix id is ${entry.child.pid}) to true`;
    const result = spawnSync("osascript", ["-e", script], { encoding: "utf8", timeout: 20_000 });
    return result.status === 0 ? null : (result.stderr || "osascript failed").trim();
  };
  fixture.bringToFront();
  return fixture;
}

// Polls computer.status until the fixture window is reported as focused,
// re-requesting frontmost on macOS between polls.
async function waitFixtureFocused(bridge, fixture, timeoutMs = 20_000) {
  let lastWindows = [];
  try {
    return await waitUntil(
      "fixture window focus",
      async () => {
        const status = await bridge.command("computer.status");
        lastWindows = Array.isArray(status.result?.windows) ? status.result.windows : [];
        const window = lastWindows.find((candidate) => candidate.title === FIXTURE_TITLE);
        if (window?.focused === true) {
          return window;
        }
        if (fixture.bringToFront) {
          fixture.bringToFront();
        }
        return null;
      },
      { timeoutMs, intervalMs: 1_000 },
    );
  } catch (error) {
    const summary = lastWindows.map((window) => ({ title: window.title, appName: window.appName, focused: window.focused }));
    throw new Error(`${error.message}; windows=${JSON.stringify(summary).slice(0, 600)}`);
  }
}

function fixtureTextState(state, field) {
  const value = state?.[field];
  if (value && typeof value === "object") {
    return { length: value.length, sha256: value.sha256 };
  }
  if (typeof value === "string") {
    return { length: value.length, sha256: sha256Hex(Buffer.from(value, "utf8")) };
  }
  return { length: null, sha256: null };
}

async function waitFixtureText(fixture, field, expected, timeoutMs = 15_000) {
  const expectedSha = sha256Hex(Buffer.from(expected, "utf8"));
  return waitUntil(
    `fixture ${field} == ${expected}`,
    () => {
      const current = fixtureTextState(fixture.readState(), field);
      return current.sha256 === expectedSha && current.length === expected.length ? current : null;
    },
    { timeoutMs },
  );
}

async function waitFixtureCounter(fixture, field, predicate, timeoutMs = 15_000) {
  return waitUntil(
    `fixture ${field}`,
    () => {
      const state = fixture.readState();
      const value = field.split(".").reduce((acc, key) => (acc && typeof acc === "object" ? acc[key] : undefined), state);
      return predicate(value) ? { [field]: value } : null;
    },
    { timeoutMs },
  );
}

async function computerLane(context) {
  const { recorder, server, inventory, processes, out, version } = context;
  const lane = "computer";
  if (!server) {
    recorder.fail(lane, "helper.connected", "server lane did not boot");
    return;
  }
  const { bridge } = server;

  try {
    const reported = binaryVersion(inventory.helper);
    if (reported === `local-computer-helper ${version}`) {
      recorder.pass(lane, "helper.version", { reported });
    } else {
      recorder.fail(lane, "helper.version", `unexpected helper version "${reported}"`);
    }
  } catch (error) {
    recorder.fail(lane, "helper.version", error.message);
  }

  let fixture;
  try {
    fixture = buildFixture(context);
    fixture = await startFixture(context, fixture);
    recorder.pass(lane, "fixture.ready", { sourceSha256: fixture.sourceSha, title: FIXTURE_TITLE });
  } catch (error) {
    recorder.fail(lane, "fixture.ready", error.message);
    return;
  }
  await sleep(1_000);

  let probe = null;
  try {
    probe = runPermissionProbe(inventory.helper);
    context.probe = probe;
    recorder.pass(lane, "permission.probe", {
      screenCaptureReady: probe.screenCaptureReady,
      inputReady: probe.inputReady,
      semanticReady: probe.semanticReady,
      windowCount: probe.windowCount,
    });
  } catch (error) {
    recorder.fail(lane, "permission.probe", error.message);
    return;
  }

  // Permission gating: Windows must fully pass; macOS may legitimately lack
  // Screen Recording or Accessibility on a hosted runner. In that case the
  // positive checks are skipped and the documented refusals become required.
  const captureReady = probe.screenCaptureReady === true;
  const semanticReady = probe.semanticReady === true;
  const inputReady = probe.inputReady === true;
  const gated = IS_MACOS && !(captureReady && semanticReady && inputReady);
  context.computerMode = gated ? "permission-gated-refusal" : "native";
  const positive = (name, ok, evidence, reason, gatedBy) => {
    if (gated && !gatedBy) {
      recorder.skip(lane, name, "permission-unavailable", { evidence: probe });
      return;
    }
    if (ok) {
      recorder.pass(lane, name, evidence);
    } else {
      recorder.fail(lane, name, reason, evidence);
    }
  };

  const helperEnv = { LBB_TOKEN: bridge.token, LBB_PORT: String(server.port) };
  const helperCwd = path.join(out, "helper-cwd");
  mkdirSync(helperCwd, { recursive: true });
  const helper = processes.spawn("helper", inventory.helper, [], { cwd: helperCwd, env: helperEnv });
  try {
    await waitUntil(
      "helper connection",
      async () => {
        if (helper.exited) {
          throw new Error(`helper exited with ${helper.code}`);
        }
        const state = await bridge.state();
        return state.body?.computerConnected === true;
      },
      { timeoutMs: 60_000, intervalMs: 500 },
    );
    recorder.pass(lane, "helper.connected", { computerConnected: true });
  } catch (error) {
    recorder.fail(lane, "helper.connected", error.message);
    return;
  }

  const status = await bridge.command("computer.status");
  const windows = Array.isArray(status.result?.windows) ? status.result.windows : [];
  const fixtureWindow = windows.find((window) => window.title === FIXTURE_TITLE) ?? (fixture.ready ? windows.find((window) => String(window.id) === String(fixture.ready.targetHwnd)) : null);
  if (status.status === 200 && typeof status.result?.backend === "string") {
    recorder.pass(lane, "status", {
      platform: status.result.platform,
      backend: status.result.backend,
      inputReady: status.result.inputReady,
      semanticReady: status.result.semanticReady,
      frameReady: status.result.frameReady,
      windowCount: windows.length,
    });
  } else {
    recorder.fail(lane, "status", truncate(errorSummary(status)));
    return;
  }
  if (fixtureWindow) {
    recorder.pass(lane, "window.select", { windowId: String(fixtureWindow.id), title: fixtureWindow.title, appName: fixtureWindow.appName ?? null });
  } else {
    recorder.fail(lane, "window.select", `fixture window "${FIXTURE_TITLE}" was not listed`, { titles: windows.slice(0, 30).map((window) => window.title) });
    return;
  }
  const windowId = String(fixtureWindow.id);

  if (IS_MACOS && !gated) {
    try {
      const focused = await waitFixtureFocused(bridge, fixture);
      recorder.pass(lane, "window.frontmost", { windowId: String(focused.id), focused: true });
    } catch (error) {
      recorder.fail(lane, "window.frontmost", error.message);
    }
  }

  const expectCode = (name, response, code, messagePattern) => {
    const summary = errorSummary(response);
    const ok = response.status !== 200 && summary.code === code && (!messagePattern || messagePattern.test(String(response.error?.message || "")));
    if (ok) {
      recorder.pass(lane, name, summary);
    } else {
      recorder.fail(lane, name, `expected ${code}${messagePattern ? ` matching ${messagePattern}` : ""}`, response.status === 200 ? summarizeCommandResult(response.result) : summary);
    }
  };

  // -- observe -------------------------------------------------------------
  const observeOnce = async () => {
    const response = await bridge.command("computer.observe", { windowId });
    const frame = response.result ?? {};
    const elements = Array.isArray(frame.elements) ? frame.elements : [];
    return { response, frame, elements };
  };

  let observed = await observeOnce();
  if (!captureReady) {
    expectCode("refusal.observe.capture-failed", observed.response, "COMPUTER_CAPTURE_FAILED", IS_MACOS ? /Screen Recording/ : null);
    const share = await bridge.command("computer.share.start", { windowId, fps: 2 });
    expectCode("refusal.share.capture-failed", share, "COMPUTER_CAPTURE_FAILED");
    for (const name of ["observe", "observe.screenshot", "share.start", "share.status", "share.stop", "invoke", "setValue", "click", "typeText"]) {
      positive(name, false, null, "capture unavailable");
    }
    return;
  }
  if (observed.response.status !== 200) {
    recorder.fail(lane, "observe", truncate(errorSummary(observed.response)));
    return;
  }
  const frameSummary = (frame) => ({
    frameId: frame.frameId,
    windowTitle: frame.windowTitle,
    appName: frame.appName,
    imageWidth: frame.imageWidth,
    imageHeight: frame.imageHeight,
    semanticAvailable: frame.semanticAvailable,
    semanticMode: frame.semanticMode,
    elements: (frame.elements || []).length,
    deliveryMode: frame.deliveryMode,
  });
  recorder.pass(lane, "observe", frameSummary(observed.frame));

  try {
    const state = await bridge.state();
    const locator = state.body?.computerObservation?.screenshotUrl;
    if (!locator) {
      throw new Error("state.computerObservation.screenshotUrl is absent");
    }
    const shot = await bridge.binary(locator);
    if (shot.status === 200 && isPng(shot.bytes)) {
      writeFileSync(path.join(out, "computer-observe.png"), shot.bytes);
      recorder.pass(lane, "observe.screenshot", { bytes: shot.bytes.length, width: state.body.computerObservation.screenshotWidth ?? null, height: state.body.computerObservation.screenshotHeight ?? null });
    } else {
      recorder.fail(lane, "observe.screenshot", `status ${shot.status}, type ${shot.type}`);
    }
  } catch (error) {
    recorder.fail(lane, "observe.screenshot", error.message);
  }

  // -- share ---------------------------------------------------------------
  {
    const start = await bridge.command("computer.share.start", { windowId, fps: 2 });
    if (start.status === 200 && start.result?.active === true) {
      recorder.pass(lane, "share.start", { id: start.result.id ?? null, captureBackend: start.result.captureBackend ?? null, captureScope: start.result.captureScope ?? null });
      await sleep(2_500);
      const shareStatus = await bridge.command("computer.share.status");
      const sequence = shareStatus.result?.sequence ?? shareStatus.result?.sourceSequence ?? null;
      if (shareStatus.status === 200 && shareStatus.result?.active === true) {
        recorder.pass(lane, "share.status", { active: true, sequence, fps: shareStatus.result.fps ?? null, droppedFrames: shareStatus.result.droppedFrames ?? null });
      } else {
        recorder.fail(lane, "share.status", truncate(shareStatus.status === 200 ? shareStatus.result : errorSummary(shareStatus)));
      }
      const stop = await bridge.command("computer.share.stop");
      if (stop.status === 200 && stop.result?.active === false) {
        recorder.pass(lane, "share.stop", { stopped: stop.result.stopped ?? null, reason: stop.result.reason ?? null });
      } else {
        recorder.fail(lane, "share.stop", truncate(stop.status === 200 ? stop.result : errorSummary(stop)));
      }
    } else {
      recorder.fail(lane, "share.start", truncate(errorSummary(start)));
      recorder.fail(lane, "share.status", "share did not start");
      recorder.fail(lane, "share.stop", "share did not start");
    }
  }

  // -- semantic ------------------------------------------------------------
  observed = await observeOnce();
  const findSemantic = (elements, name) => elements.find((element) => element.name === name);
  if (!semanticReady) {
    const ok = observed.frame.semanticAvailable === false;
    if (ok) {
      recorder.pass(lane, "refusal.observe.semantic-unavailable", { semanticAvailable: false, semanticMode: observed.frame.semanticMode ?? null });
    } else {
      recorder.fail(lane, "refusal.observe.semantic-unavailable", "semanticAvailable was not false while Accessibility is absent", frameSummary(observed.frame));
    }
    positive("invoke", false, null, "semantic unavailable");
    positive("setValue", false, null, "semantic unavailable");
  } else {
    const button = findSemantic(observed.elements, "Increment Counter");
    if (!button) {
      recorder.fail(lane, "invoke", "Increment Counter element was not observed", { names: observed.elements.slice(0, 40).map((element) => element.name) });
    } else {
      const action = (button.actions || []).includes("press") ? "press" : (button.actions || []).includes("invoke") ? "invoke" : undefined;
      const invoke = await bridge.command("computer.invoke", { frameId: observed.frame.frameId, elementRef: button.ref, ...(action ? { action } : {}) });
      try {
        if (invoke.status !== 200) {
          throw new Error(truncate(errorSummary(invoke)));
        }
        const counted = await waitFixtureCounter(fixture, "invokeCount", (value) => value === 1);
        recorder.pass(lane, "invoke", { ...counted, action: invoke.result?.action ?? action ?? null, effect: invoke.result?.effect ?? invoke.result?.backendEffect ?? null });
      } catch (error) {
        recorder.fail(lane, "invoke", error.message);
      }
    }

    observed = await observeOnce();
    const field = findSemantic(observed.elements, "Fixture Value Input");
    if (!field) {
      recorder.fail(lane, "setValue", "Fixture Value Input element was not observed", { names: observed.elements.slice(0, 40).map((element) => element.name) });
    } else {
      const setValue = await bridge.command("computer.setValue", { frameId: observed.frame.frameId, elementRef: field.ref, value: SET_VALUE });
      try {
        if (setValue.status !== 200) {
          throw new Error(truncate(errorSummary(setValue)));
        }
        const proof = await waitFixtureText(fixture, "semanticValue", SET_VALUE);
        recorder.pass(lane, "setValue", { ...proof, effect: setValue.result?.effect ?? null });
      } catch (error) {
        recorder.fail(lane, "setValue", error.message);
      }
    }
  }

  // -- pointer and keyboard ------------------------------------------------
  observed = await observeOnce();
  const center = (element) => ({ x: Math.round(element.bounds.x + element.bounds.width / 2), y: Math.round(element.bounds.y + element.bounds.height / 2) });
  if (!inputReady) {
    const surface = findSemantic(observed.elements, "Pixel Input Surface");
    const point = surface ? center(surface) : { x: Math.round(observed.frame.imageWidth / 2), y: Math.round(observed.frame.imageHeight / 2) };
    const click = await bridge.command("computer.click", { frameId: observed.frame.frameId, ...point });
    expectCode("refusal.click.input-failed", click, "COMPUTER_INPUT_FAILED");
    positive("click", false, null, "input unavailable");
    positive("typeText", false, null, "input unavailable");
    return;
  }
  const surface = findSemantic(observed.elements, "Pixel Input Surface");
  if (!surface) {
    recorder.fail(lane, "click", "Pixel Input Surface element was not observed", { names: observed.elements.slice(0, 40).map((element) => element.name) });
  } else {
    const point = center(surface);
    const click = await bridge.command("computer.click", { frameId: observed.frame.frameId, ...point });
    try {
      if (click.status !== 200) {
        throw new Error(truncate(errorSummary(click)));
      }
      const counted = IS_WINDOWS
        ? await waitFixtureCounter(fixture, "messageCounters.mouseDown", (value) => typeof value === "number" && value >= 1)
        : await waitFixtureCounter(fixture, "clicks", (value) => value === 1);
      recorder.pass(lane, "click", { point, ...counted, effect: click.result?.effect ?? null, provenance: click.result?.inputDeliveryProvenance ?? click.result?.provenance ?? null });
    } catch (error) {
      recorder.fail(lane, "click", error.message, { point });
    }
  }

  observed = await observeOnce();
  const focusedInput = findSemantic(observed.elements, "Focused Text Input");
  if (!focusedInput) {
    recorder.fail(lane, "typeText", "Focused Text Input element was not observed");
  } else {
    const point = center(focusedInput);
    const focusClick = await bridge.command("computer.click", { frameId: observed.frame.frameId, ...point });
    observed = await observeOnce();
    const typed = await bridge.command("computer.typeText", { frameId: observed.frame.frameId, text: TYPED_TEXT });
    try {
      if (focusClick.status !== 200) {
        throw new Error(`focus click: ${truncate(errorSummary(focusClick))}`);
      }
      if (typed.status !== 200) {
        throw new Error(truncate(errorSummary(typed)));
      }
      const proof = await waitFixtureText(fixture, "focusedText", TYPED_TEXT);
      recorder.pass(lane, "typeText", { ...proof, effect: typed.result?.effect ?? null });
    } catch (error) {
      recorder.fail(lane, "typeText", error.message);
    }
  }
  await observeOnce();
  try {
    const state = await bridge.state();
    const locator = state.body?.computerObservation?.screenshotUrl;
    if (locator) {
      const shot = await bridge.binary(locator);
      if (shot.status === 200 && isPng(shot.bytes)) {
        writeFileSync(path.join(out, "computer-observe-final.png"), shot.bytes);
      }
    }
  } catch {
    // evidence only
  }
}

function captureDesktop(out) {
  const file = path.join(out, "desktop-final.png");
  try {
    if (IS_MACOS) {
      spawnSync("screencapture", ["-x", file], { timeout: 20_000 });
    } else if (IS_WINDOWS) {
      const script = [
        "Add-Type -AssemblyName System.Windows.Forms,System.Drawing",
        "$b = [System.Windows.Forms.SystemInformation]::VirtualScreen",
        "$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height",
        "$g = [System.Drawing.Graphics]::FromImage($bmp)",
        "$g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)",
        `$bmp.Save(${JSON.stringify(file)}, [System.Drawing.Imaging.ImageFormat]::Png)`,
      ].join("; ");
      spawnSync("powershell.exe", ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script], { timeout: 30_000 });
    }
  } catch {
    // evidence only
  }
  return existsSync(file);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (existsSync(options.out)) {
    rmSync(options.out, { recursive: true, force: true });
  }
  mkdirSync(options.out, { recursive: true });
  const token = base64Url(randomBytes(32));
  const recorder = new Recorder(options.out);
  const processes = new Processes(options.out, [token]);
  const startedAt = new Date().toISOString();
  const context = {
    options,
    out: options.out,
    version: options.version,
    token,
    recorder,
    processes,
    inventory: null,
    server: null,
    agentFetchBase: null,
    probe: null,
    computerMode: null,
    chromeVersion: null,
  };

  let fatal = null;
  try {
    context.inventory = resolveInventory(options);
    recorder.pass("server", "inventory", {
      mode: options.mode,
      serverKind: context.inventory.serverKind,
      extensionManifestVersion: context.inventory.extensionManifest.version,
      checksums: context.inventory.checksums,
    });
  } catch (error) {
    recorder.fail("server", "inventory", error.message);
    fatal = error;
  }

  if (!fatal) {
    const lanes = options.lanes;
    try {
      if (lanes.includes("server")) {
        await serverLane(context);
      } else {
        context.server = await startServer(context, { shell: true, label: "main" });
        const state = await context.server.bridge.state();
        context.agentFetchBase = state.body?.agentFetch?.baseUrl ?? null;
      }
      if (lanes.includes("shell")) {
        await shellLane(context);
      }
      if (lanes.includes("browser")) {
        await browserLane(context);
      }
      if (lanes.includes("computer")) {
        await computerLane(context);
      }
      if (lanes.includes("shell")) {
        await processes.stop(context.server?.entry);
        await shellDisabledLane(context);
      }
    } catch (error) {
      fatal = error;
      recorder.fail("server", "unexpected-exception", `${error.message}\n${error.stack || ""}`);
    }
  }

  const desktopScreenshot = captureDesktop(options.out);
  try {
    context.site?.close();
  } catch {
    // ignore
  }
  await processes.stopAll();

  const summary = recorder.summary();
  const ok = summary.requiredFail === 0 && !fatal;
  const report = {
    schemaVersion: 1,
    product: "local-browser-bridge",
    os: OS_NAME,
    arch: process.arch,
    mode: options.mode,
    version: options.version,
    sourceSha: options.sourceSha || null,
    runId: process.env.GITHUB_RUN_ID || null,
    runAttempt: process.env.GITHUB_RUN_ATTEMPT || null,
    runnerOs: process.env.RUNNER_OS || null,
    runnerImage: process.env.ImageOS || null,
    node: process.version,
    startedAt,
    finishedAt: new Date().toISOString(),
    chrome: context.chromeVersion,
    extensionManifestVersion: context.inventory?.extensionManifest?.version ?? null,
    candidateArtifactId: process.env.LBB_CANDIDATE_ARTIFACT_ID || null,
    candidateArtifactSha256: process.env.LBB_CANDIDATE_ZIP_SHA256 || null,
    candidateChecksums: context.inventory?.checksums ?? null,
    probe: context.probe,
    computerMode: context.computerMode,
    lanes: options.lanes,
    desktopScreenshot: desktopScreenshot ? "desktop-final.png" : null,
    summary,
    ok,
    checks: recorder.checks,
  };
  writeFileSync(path.join(options.out, "acceptance.json"), `${JSON.stringify(report, null, 2)}\n`);

  const files = readdirSync(options.out).filter((name) => name.endsWith(".png") || name.endsWith(".jpg"));
  console.log(
    `\nacceptance ${ok ? "PASSED" : "FAILED"} on ${OS_NAME} (${options.mode}): ${summary.pass} pass, ${summary.fail} fail (${summary.requiredFail} required), ${summary.skip} skip; ${files.length} screenshots; computer mode ${context.computerMode ?? "n/a"}`,
  );
  process.exit(ok ? 0 : 1);
}

main().catch((error) => {
  console.error(error);
  process.exit(2);
});
