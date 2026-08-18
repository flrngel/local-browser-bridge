import { randomBytes } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createServer as createHttpServer } from "node:http";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { ExtensionHub } from "./hub.js";
import { tokensEqual } from "./token.js";

const PROJECT_ROOT = fileURLToPath(new URL("..", import.meta.url));
const DEFAULT_PUBLIC_DIR = join(PROJECT_ROOT, "public");
const MAX_BODY_BYTES = 128 * 1024;
const MAX_ACTIVITY = 80;
const ACTION_METHODS = new Set([
  "status",
  "tabs.list",
  "tabs.activate",
  "tabs.new",
  "tabs.close",
  "page.observe",
  "page.navigate",
  "page.back",
  "page.forward",
  "page.reload",
  "page.click",
  "page.fill",
  "page.select",
  "page.key",
  "page.scroll",
]);

const STATIC_FILES = new Map([
  ["/", ["index.html", "text/html; charset=utf-8"]],
  ["/app.js", ["app.js", "text/javascript; charset=utf-8"]],
  ["/styles.css", ["styles.css", "text/css; charset=utf-8"]],
  ["/favicon.svg", ["favicon.svg", "image/svg+xml"]],
  ["/demo", ["demo.html", "text/html; charset=utf-8"]],
  ["/demo.css", ["demo.css", "text/css; charset=utf-8"]],
  ["/demo.js", ["demo.js", "text/javascript; charset=utf-8"]],
]);

function securityHeaders(response) {
  response.setHeader(
    "Content-Security-Policy",
    "default-src 'self'; img-src 'self' data:; connect-src 'self'; script-src 'self'; style-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
  );
  response.setHeader("Referrer-Policy", "no-referrer");
  response.setHeader("X-Content-Type-Options", "nosniff");
  response.setHeader("X-Frame-Options", "DENY");
  response.setHeader("Cross-Origin-Resource-Policy", "same-origin");
  response.setHeader("Cache-Control", "no-store");
}

function isLoopbackHost(hostHeader) {
  if (typeof hostHeader !== "string") return false;
  const hostname = hostHeader.startsWith("[")
    ? hostHeader.slice(1, hostHeader.indexOf("]"))
    : hostHeader.split(":")[0];
  return hostname === "127.0.0.1" || hostname === "localhost" || hostname === "::1";
}

function parseCookies(header = "") {
  const cookies = {};
  for (const pair of header.split(";")) {
    const separator = pair.indexOf("=");
    if (separator < 0) continue;
    const name = pair.slice(0, separator).trim();
    const value = pair.slice(separator + 1).trim();
    if (name) cookies[name] = decodeURIComponent(value);
  }
  return cookies;
}

function writeJson(response, status, payload) {
  securityHeaders(response);
  response.statusCode = status;
  response.setHeader("Content-Type", "application/json; charset=utf-8");
  response.end(JSON.stringify(payload));
}

function asInteger(value, name) {
  const number = Number(value);
  if (!Number.isInteger(number) || number < 0) throw badRequest(`${name} must be a non-negative integer`);
  return number;
}

function asString(value, name, maxLength = 2_048) {
  if (typeof value !== "string") throw badRequest(`${name} must be a string`);
  if (value.length > maxLength) throw badRequest(`${name} is too long`);
  return value;
}

function badRequest(message) {
  return Object.assign(new Error(message), { statusCode: 400, code: "BAD_REQUEST" });
}

function sanitizeParams(method, input, targetTabId) {
  const source = input && typeof input === "object" && !Array.isArray(input) ? input : {};
  const tabId = source.tabId === undefined || source.tabId === null
    ? targetTabId
    : asInteger(source.tabId, "tabId");
  const withTab = () => {
    if (!Number.isInteger(tabId)) throw badRequest("Select a target tab first");
    return { tabId };
  };

  switch (method) {
    case "status":
    case "tabs.list":
    case "tabs.new":
      return {};
    case "tabs.activate":
    case "tabs.close":
      return { tabId: asInteger(source.tabId, "tabId") };
    case "page.observe":
    case "page.back":
    case "page.forward":
    case "page.reload":
      return withTab();
    case "page.navigate":
      return { ...withTab(), url: asString(source.url, "url", 4_096) };
    case "page.click":
      return {
        ...withTab(),
        ref: asString(source.ref, "ref", 80),
        generation: asString(source.generation, "generation", 100),
      };
    case "page.fill":
      return {
        ...withTab(),
        ref: asString(source.ref, "ref", 80),
        generation: asString(source.generation, "generation", 100),
        text: asString(source.text, "text", 10_000),
      };
    case "page.select":
      return {
        ...withTab(),
        ref: asString(source.ref, "ref", 80),
        generation: asString(source.generation, "generation", 100),
        value: asString(source.value, "value", 1_000),
      };
    case "page.key":
      return { ...withTab(), key: asString(source.key, "key", 40) };
    case "page.scroll": {
      const deltaX = Number(source.deltaX ?? 0);
      const deltaY = Number(source.deltaY ?? 0);
      if (!Number.isFinite(deltaX) || !Number.isFinite(deltaY)) throw badRequest("Scroll deltas must be numbers");
      return {
        ...withTab(),
        deltaX: Math.max(-5_000, Math.min(5_000, Math.trunc(deltaX))),
        deltaY: Math.max(-5_000, Math.min(5_000, Math.trunc(deltaY))),
      };
    }
    default:
      throw badRequest("Unsupported action");
  }
}

async function readJson(request) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) throw Object.assign(new Error("Request body is too large"), { statusCode: 413, code: "BODY_TOO_LARGE" });
    chunks.push(chunk);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");
  } catch {
    throw badRequest("Request body must be valid JSON");
  }
}

function decodeScreenshot(dataUrl) {
  if (typeof dataUrl !== "string") return null;
  const match = /^data:image\/(png|jpeg);base64,([A-Za-z0-9+/=]+)$/.exec(dataUrl);
  if (!match) return null;
  const buffer = Buffer.from(match[2], "base64");
  if (buffer.length > 8 * 1024 * 1024) throw badRequest("Screenshot exceeds the 8 MB limit");
  return { buffer, contentType: match[1] === "png" ? "image/png" : "image/jpeg" };
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function boundedText(value, maxLength) {
  return String(value ?? "").slice(0, maxLength);
}

function sanitizeTab(value) {
  if (!value || !Number.isInteger(value.id) || value.id < 0) return null;
  return {
    id: value.id,
    title: boundedText(value.title, 300),
    url: boundedText(value.url, 4_096),
    active: Boolean(value.active),
  };
}

function sanitizeElement(value) {
  if (!value || typeof value !== "object") return null;
  const ref = boundedText(value.ref, 80);
  if (!ref) return null;
  const bounds = value.bounds && typeof value.bounds === "object"
    ? Object.fromEntries(["x", "y", "width", "height"].map((key) => [key, Number.isFinite(Number(value.bounds[key])) ? Math.round(Number(value.bounds[key])) : 0]))
    : null;
  return {
    ref,
    role: boundedText(value.role, 80),
    name: boundedText(value.name, 500),
    type: boundedText(value.type, 80),
    disabled: Boolean(value.disabled),
    checked: value.checked === undefined ? undefined : Boolean(value.checked),
    selected: value.selected === undefined ? undefined : Boolean(value.selected),
    sensitive: Boolean(value.sensitive),
    inViewport: Boolean(value.inViewport),
    risk: value.risk ? boundedText(value.risk, 200) : null,
    bounds,
  };
}

export function createBridgeServer({
  host = "127.0.0.1",
  port = 17_373,
  token,
  publicDir = DEFAULT_PUBLIC_DIR,
  callTimeoutMs = 15_000,
} = {}) {
  if (!token) throw new Error("A bridge token is required");
  if (host !== "127.0.0.1" && host !== "::1" && host !== "localhost") {
    throw new Error("Refusing to bind to a non-loopback host");
  }

  const sessions = new Map();
  const eventStreams = new Set();
  let screenshot = null;
  const state = {
    revision: 0,
    connected: false,
    extension: null,
    targetTabId: null,
    tabs: [],
    observation: null,
    activity: [],
  };

  const httpServer = createHttpServer((request, response) => {
    void route(request, response).catch((error) => {
      const status = Number(error.statusCode) || (error.code === "EXTENSION_OFFLINE" ? 503 : 500);
      writeJson(response, status, {
        ok: false,
        error: {
          code: error.code ?? "INTERNAL_ERROR",
          message: status >= 500 && !error.code ? "Internal server error" : error.message,
        },
      });
    });
  });
  const hub = new ExtensionHub(httpServer, { token, callTimeoutMs });

  function bump(eventName = "state") {
    state.revision += 1;
    const payload = `event: ${eventName}\ndata: ${JSON.stringify({ revision: state.revision })}\n\n`;
    for (const stream of eventStreams) stream.write(payload);
  }

  function log(method, status, message) {
    state.activity.unshift({
      id: randomBytes(8).toString("hex"),
      at: new Date().toISOString(),
      method,
      status,
      message,
    });
    state.activity = state.activity.slice(0, MAX_ACTIVITY);
  }

  function publicState() {
    return {
      ...state,
      observation: state.observation
        ? {
            ...state.observation,
            screenshotUrl: screenshot ? `/api/screenshot?revision=${state.revision}` : null,
          }
        : null,
    };
  }

  function createSession(response) {
    if (sessions.size >= 1_000) {
      const cutoff = Date.now() - 12 * 60 * 60 * 1_000;
      for (const [sessionId, session] of sessions) {
        if (session.touchedAt < cutoff || sessions.size >= 1_000) sessions.delete(sessionId);
        if (sessions.size < 800) break;
      }
    }
    const id = randomBytes(24).toString("base64url");
    const csrf = randomBytes(24).toString("base64url");
    sessions.set(id, { csrf, touchedAt: Date.now() });
    response.setHeader("Set-Cookie", `lbb_session=${id}; Path=/; HttpOnly; SameSite=Strict; Max-Age=43200`);
    return { id, csrf };
  }

  function getSession(request, response, shouldCreate = true) {
    const id = parseCookies(request.headers.cookie).lbb_session;
    const session = id ? sessions.get(id) : null;
    if (session) {
      session.touchedAt = Date.now();
      return { id, ...session };
    }
    return shouldCreate ? createSession(response) : null;
  }

  function assertUiMutation(request, response) {
    const session = getSession(request, response, false);
    if (!session || !tokensEqual(request.headers["x-csrf-token"] ?? "", session.csrf)) {
      throw Object.assign(new Error("Invalid UI session"), { statusCode: 403, code: "CSRF_REJECTED" });
    }
    const expectedOrigin = `http://${request.headers.host}`;
    if (request.headers.origin !== expectedOrigin) {
      throw Object.assign(new Error("Cross-origin command rejected"), { statusCode: 403, code: "ORIGIN_REJECTED" });
    }
  }

  async function refreshTabs() {
    const result = await hub.call("tabs.list", {});
    state.tabs = Array.isArray(result?.tabs) ? result.tabs.map(sanitizeTab).filter(Boolean).slice(0, 500) : [];
    if (!state.tabs.some((tab) => tab.id === state.targetTabId)) {
      state.targetTabId = state.tabs.some((tab) => tab.id === result?.activeTabId)
        ? result.activeTabId
        : (state.tabs[0]?.id ?? null);
      state.observation = null;
      screenshot = null;
    }
    bump("tabs");
    return result;
  }

  async function refreshObservation(tabId = state.targetTabId) {
    if (!Number.isInteger(tabId)) throw badRequest("Select a target tab first");
    const result = await hub.call("page.observe", { tabId });
    screenshot = decodeScreenshot(result?.screenshot);
    const snapshot = result?.snapshot && typeof result.snapshot === "object" ? result.snapshot : {};
    state.targetTabId = tabId;
    state.observation = {
      tabId,
      capturedAt: new Date().toISOString(),
      title: boundedText(snapshot.title, 500),
      url: boundedText(snapshot.url, 4_096),
      generation: boundedText(snapshot.generation, 100),
      viewport: snapshot.viewport ?? null,
      scroll: snapshot.scroll ?? null,
      selectedText: boundedText(snapshot.selectedText, 5_000),
      bodyText: boundedText(snapshot.bodyText, 20_000),
      elements: Array.isArray(snapshot.elements) ? snapshot.elements.map(sanitizeElement).filter(Boolean).slice(0, 250) : [],
      screenshotUrl: null,
    };
    log("page.observe", "ok", `Observed tab ${tabId}`);
    bump("observation");
    return result;
  }

  async function performAction(method, rawParams) {
    if (!ACTION_METHODS.has(method)) throw badRequest("Unsupported action");
    const params = sanitizeParams(method, rawParams, state.targetTabId);

    if (method === "tabs.list") return refreshTabs();
    if (method === "page.observe") return refreshObservation(params.tabId);

    try {
      const result = await hub.call(method, params);
      if (method === "tabs.activate") state.targetTabId = params.tabId;
      if (method === "tabs.new" && Number.isInteger(result?.tabId)) state.targetTabId = result.tabId;

      if (result?.status === "approval_required") {
        log(method, "approval", `${result.risk ?? "Sensitive action"}; approve it in the extension popup`);
        bump("approval");
        return result;
      }

      log(method, "ok", method === "page.fill" ? `Filled ${params.ref}` : `${method} completed`);
      if (method.startsWith("tabs.")) await refreshTabs();

      const observationDelay = {
        "tabs.activate": 150,
        "page.navigate": 700,
        "page.back": 500,
        "page.forward": 500,
        "page.reload": 600,
        "page.click": 350,
        "page.fill": 100,
        "page.select": 200,
        "page.key": 250,
        "page.scroll": 150,
      }[method];
      if (observationDelay !== undefined && Number.isInteger(state.targetTabId)) {
        await delay(observationDelay);
        try {
          await refreshObservation(state.targetTabId);
        } catch (error) {
          log("page.observe", "warning", error.message);
          bump("warning");
        }
      }
      return result;
    } catch (error) {
      log(method, "error", error.message);
      bump("error");
      throw error;
    }
  }

  async function route(request, response) {
    if (!isLoopbackHost(request.headers.host)) {
      writeJson(response, 400, { ok: false, error: { code: "HOST_REJECTED", message: "Only loopback hosts are accepted" } });
      return;
    }

    const url = new URL(request.url ?? "/", `http://${request.headers.host}`);

    if (request.method === "GET" && url.pathname === "/health") {
      writeJson(response, 200, { ok: true, extensionConnected: hub.connected, version: "0.1.0" });
      return;
    }

    if (request.method === "GET" && url.pathname === "/api/session") {
      const session = getSession(request, response, true);
      writeJson(response, 200, { ok: true, csrfToken: session.csrf });
      return;
    }

    if (request.method === "GET" && url.pathname === "/api/state") {
      getSession(request, response, true);
      writeJson(response, 200, { ok: true, state: publicState() });
      return;
    }

    if (request.method === "GET" && url.pathname === "/api/screenshot") {
      if (!screenshot) {
        writeJson(response, 404, { ok: false, error: { code: "NO_SCREENSHOT", message: "No screenshot has been captured" } });
        return;
      }
      securityHeaders(response);
      response.statusCode = 200;
      response.setHeader("Content-Type", screenshot.contentType);
      response.setHeader("Content-Length", screenshot.buffer.length);
      response.end(screenshot.buffer);
      return;
    }

    if (request.method === "GET" && url.pathname === "/api/events") {
      getSession(request, response, true);
      securityHeaders(response);
      response.statusCode = 200;
      response.setHeader("Content-Type", "text/event-stream; charset=utf-8");
      response.setHeader("Connection", "keep-alive");
      response.flushHeaders();
      response.write(`event: state\ndata: ${JSON.stringify({ revision: state.revision })}\n\n`);
      const heartbeat = setInterval(() => response.write(": heartbeat\n\n"), 15_000);
      eventStreams.add(response);
      request.on("close", () => {
        clearInterval(heartbeat);
        eventStreams.delete(response);
      });
      return;
    }

    if (request.method === "POST" && url.pathname === "/api/action") {
      assertUiMutation(request, response);
      const body = await readJson(request);
      const method = asString(body.method, "method", 80);
      const result = await performAction(method, body.params);
      writeJson(response, 200, { ok: true, result, state: publicState() });
      return;
    }

    if (request.method === "POST" && url.pathname === "/api/v1/command") {
      const authorization = request.headers.authorization ?? "";
      const supplied = authorization.startsWith("Bearer ") ? authorization.slice(7) : "";
      if (!tokensEqual(supplied, token)) {
        writeJson(response, 401, { ok: false, error: { code: "UNAUTHORIZED", message: "Bearer token required" } });
        return;
      }
      const body = await readJson(request);
      const method = asString(body.method, "method", 80);
      const result = await performAction(method, body.params);
      writeJson(response, 200, { ok: true, result, state: publicState() });
      return;
    }

    if (request.method === "GET" && STATIC_FILES.has(url.pathname)) {
      const [filename, contentType] = STATIC_FILES.get(url.pathname);
      const content = await readFile(join(publicDir, filename));
      securityHeaders(response);
      response.statusCode = 200;
      response.setHeader("Content-Type", contentType);
      response.end(content);
      return;
    }

    writeJson(response, 404, { ok: false, error: { code: "NOT_FOUND", message: "Not found" } });
  }

  hub.on("connection", (connected) => {
    state.connected = connected;
    if (!connected) {
      state.extension = null;
      state.tabs = [];
      state.targetTabId = null;
      state.observation = null;
      screenshot = null;
    }
    log("bridge", connected ? "ok" : "warning", connected ? "Browser extension connected" : "Browser extension disconnected");
    bump("connection");
  });

  hub.on("hello", (hello) => {
    state.extension = {
      version: hello.version,
      browser: hello.browser,
      capabilities: hello.capabilities,
      connectedAt: hello.connectedAt,
    };
    bump("hello");
    void refreshTabs().catch((error) => {
      log("tabs.list", "warning", error.message);
      bump("warning");
    });
  });

  hub.on("extensionEvent", ({ name, data }) => {
    if (name === "approval.resolved") {
      log("approval", data.ok ? "ok" : "error", data.ok ? "Human approved and executed the action" : String(data.error ?? "Approval failed"));
      bump("approval");
      if (data.ok) {
        void delay(300).then(async () => {
          await refreshTabs();
          if (data.method !== "tabs.close" && Number.isInteger(state.targetTabId)) await refreshObservation(state.targetTabId);
        }).catch(() => {});
      }
      return;
    }
    if (name === "approval.rejected") {
      log("approval", "warning", "Human rejected the pending action");
      bump("approval");
    }
  });

  return {
    hub,
    state,
    async listen() {
      await new Promise((resolve, reject) => {
        httpServer.once("error", reject);
        httpServer.listen(port, host, () => {
          httpServer.off("error", reject);
          resolve();
        });
      });
      return httpServer.address();
    },
    async close() {
      for (const stream of eventStreams) stream.end();
      eventStreams.clear();
      hub.close();
      await new Promise((resolve, reject) => httpServer.close((error) => (error ? reject(error) : resolve())));
    },
    address() {
      return httpServer.address();
    },
  };
}
