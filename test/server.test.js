import assert from "node:assert/strict";
import test from "node:test";
import WebSocket from "ws";
import { createBridgeServer } from "../src/server.js";
import { createToken } from "../src/token.js";

const pixel = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

function waitFor(predicate, timeoutMs = 2_000) {
  const started = Date.now();
  return new Promise((resolve, reject) => {
    const tick = () => {
      if (predicate()) return resolve();
      if (Date.now() - started >= timeoutMs) return reject(new Error("Condition timed out"));
      setTimeout(tick, 10);
    };
    tick();
  });
}

function connectFakeExtension(baseUrl, token) {
  const socket = new WebSocket(`${baseUrl.replace("http", "ws")}/bridge?token=${encodeURIComponent(token)}`, {
    headers: { Origin: "chrome-extension://test-extension" },
  });
  socket.on("open", () => socket.send(JSON.stringify({
    type: "hello", version: "0.2.0-test", browser: "Test Chrome", mode: "full-access",
    capabilities: ["tabs.list", "page.observe", "page.evaluate", "page.clickAt", "page.typeText"],
  })));
  socket.on("message", (raw) => {
    const message = JSON.parse(raw.toString());
    if (message.type !== "command") return;
    let result;
    if (message.method === "tabs.list") {
      result = { activeTabId: 7, tabs: [{ id: 7, title: "Test tab", url: "https://example.test/", active: true }] };
    } else if (message.method === "tabs.activate") {
      result = { tabId: 7, active: true };
    } else if (message.method === "page.observe") {
      result = {
        screenshot: pixel,
        snapshot: {
          generation: "g1", title: "Test tab", url: "https://example.test/", bodyText: "Hello from the target page",
          viewport: { width: 800, height: 600 }, scroll: { x: 0, y: 0, maxY: 0 },
          elements: [{ ref: "e1", role: "button", name: "Continue", disabled: false, inViewport: true }],
        },
      };
    } else if (message.method === "page.evaluate") {
      result = { type: "string", value: "Eval works" };
    } else if (message.method === "page.clickAt") {
      result = { clicked: true, x: message.params.x, y: message.params.y };
    } else if (message.method === "page.typeText") {
      result = { typed: true, length: message.params.text.length };
    } else {
      result = { ok: true };
    }
    socket.send(JSON.stringify({ id: message.id, type: "result", ok: true, result }));
  });
  return socket;
}

async function uiSession(baseUrl) {
  const response = await fetch(`${baseUrl}/api/session`);
  const payload = await response.json();
  return { csrf: payload.csrfToken, cookie: response.headers.get("set-cookie").split(";")[0] };
}

test("serves the control UI with defensive headers", async (t) => {
  const bridge = createBridgeServer({ port: 0, token: createToken(), callTimeoutMs: 1_000 });
  const address = await bridge.listen();
  t.after(() => bridge.close());
  const baseUrl = `http://127.0.0.1:${address.port}`;
  const response = await fetch(baseUrl);
  assert.equal(response.status, 200);
  assert.match(await response.text(), /Browser Bridge/);
  assert.match(response.headers.get("content-security-policy"), /frame-ancestors 'none'/);
  assert.equal(response.headers.get("access-control-allow-origin"), null);
});

test("relays commands, stores observations, and serves screenshots", async (t) => {
  const token = createToken();
  const bridge = createBridgeServer({ port: 0, token, callTimeoutMs: 1_000 });
  const address = await bridge.listen();
  const baseUrl = `http://127.0.0.1:${address.port}`;
  const socket = connectFakeExtension(baseUrl, token);
  t.after(async () => { socket.close(); await bridge.close(); });
  await waitFor(() => bridge.state.connected && bridge.state.tabs.length === 1);
  assert.equal(bridge.state.extension.mode, "full-access");

  const session = await uiSession(baseUrl);
  const response = await fetch(`${baseUrl}/api/action`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-CSRF-Token": session.csrf, Cookie: session.cookie, Origin: baseUrl },
    body: JSON.stringify({ method: "tabs.activate", params: { tabId: 7 } }),
  });
  const payload = await response.json();
  assert.equal(response.status, 200);
  assert.equal(payload.state.targetTabId, 7);
  assert.equal(payload.state.observation.bodyText, "Hello from the target page");
  assert.equal(payload.state.observation.elements[0].ref, "e1");

  const screenshot = await fetch(`${baseUrl}${payload.state.observation.screenshotUrl}`);
  assert.equal(screenshot.status, 200);
  assert.equal(screenshot.headers.get("content-type"), "image/png");
  assert.equal((await screenshot.arrayBuffer()).byteLength > 10, true);
});

test("relays Full Access commands through the authenticated API", async (t) => {
  const token = createToken();
  const bridge = createBridgeServer({ port: 0, token, callTimeoutMs: 1_000 });
  const address = await bridge.listen();
  const baseUrl = `http://127.0.0.1:${address.port}`;
  const socket = connectFakeExtension(baseUrl, token);
  t.after(async () => { socket.close(); await bridge.close(); });
  await waitFor(() => bridge.state.connected && bridge.state.tabs.length === 1);

  const response = await fetch(`${baseUrl}/api/v1/command`, {
    method: "POST",
    headers: { "Authorization": `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify({ method: "page.evaluate", params: { tabId: 7, expression: "document.title" } }),
  });
  const payload = await response.json();
  assert.equal(response.status, 200);
  assert.deepEqual(payload.result, { type: "string", value: "Eval works" });

  const invalid = await fetch(`${baseUrl}/api/v1/command`, {
    method: "POST",
    headers: { "Authorization": `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify({ method: "page.clickAt", params: { tabId: 7, x: 10, y: -1 } }),
  });
  assert.equal(invalid.status, 400);
  assert.equal((await invalid.json()).error.code, "BAD_REQUEST");
});

test("rejects cross-origin UI commands and unauthenticated API commands", async (t) => {
  const bridge = createBridgeServer({ port: 0, token: createToken(), callTimeoutMs: 200 });
  const address = await bridge.listen();
  t.after(() => bridge.close());
  const baseUrl = `http://127.0.0.1:${address.port}`;
  const session = await uiSession(baseUrl);

  const crossOrigin = await fetch(`${baseUrl}/api/action`, {
    method: "POST",
    headers: { "Content-Type": "application/json", "X-CSRF-Token": session.csrf, Cookie: session.cookie, Origin: "https://evil.example" },
    body: JSON.stringify({ method: "tabs.list", params: {} }),
  });
  assert.equal(crossOrigin.status, 403);
  assert.equal((await crossOrigin.json()).error.code, "ORIGIN_REJECTED");

  const api = await fetch(`${baseUrl}/api/v1/command`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ method: "tabs.list", params: {} }),
  });
  assert.equal(api.status, 401);
});

test("rejects WebSocket clients that are not browser extensions", async (t) => {
  const token = createToken();
  const bridge = createBridgeServer({ port: 0, token, callTimeoutMs: 200 });
  const address = await bridge.listen();
  t.after(() => bridge.close());
  const baseUrl = `ws://127.0.0.1:${address.port}`;

  const status = await new Promise((resolve) => {
    const socket = new WebSocket(`${baseUrl}/bridge?token=${encodeURIComponent(token)}`, { headers: { Origin: "https://evil.example" } });
    socket.on("unexpected-response", (_request, response) => resolve(response.statusCode));
    socket.on("error", () => resolve(0));
  });
  assert.equal(status, 403);
});
