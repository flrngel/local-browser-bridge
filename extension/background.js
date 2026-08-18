import {
  VERSION,
  DEFAULT_PORT,
  allowedKey,
  classifyRisk,
  isSensitiveField,
  isUrlAllowed,
  normalizeAllowedHost,
  safeUrlForDisplay,
} from "./lib.js";

const DEFAULTS = {
  token: "",
  port: DEFAULT_PORT,
  enabled: true,
  fullAccess: true,
  allowedHosts: ["localhost", "127.0.0.1"],
  connectionStatus: "not-configured",
  connectionDetail: "Paste the token printed by the local server.",
  pendingApproval: null,
};
const RECONNECT_MAX_MS = 30_000;
const PING_INTERVAL_MS = 20_000;
const COMMANDS = new Set([
  "status", "tabs.list", "tabs.activate", "tabs.new", "tabs.close", "page.observe", "page.navigate",
  "page.back", "page.forward", "page.reload", "page.click", "page.fill", "page.select", "page.key", "page.scroll",
  "page.clickAt", "page.typeText", "page.evaluate",
]);

let socket = null;
let pingTimer = null;
let reconnectTimer = null;
let reconnectDelay = 1_000;
let lastCaptureAt = new Map();

async function settings() {
  const stored = await chrome.storage.local.get(DEFAULTS);
  return {
    ...stored,
    port: Number(stored.port) || DEFAULT_PORT,
    allowedHosts: Array.isArray(stored.allowedHosts) ? stored.allowedHosts.map(normalizeAllowedHost).filter(Boolean) : DEFAULTS.allowedHosts,
  };
}

async function setStatus(status, detail = "") {
  await chrome.storage.local.set({ connectionStatus: status, connectionDetail: detail });
  const color = status === "connected" ? "#82d94d" : status === "connecting" ? "#f3bd4e" : "#e36b5d";
  await chrome.action.setBadgeBackgroundColor({ color }).catch(() => {});
  await chrome.action.setBadgeText({ text: status === "connected" ? "ON" : status === "connecting" ? "…" : "!" }).catch(() => {});
}

function send(message) {
  if (socket?.readyState !== WebSocket.OPEN) return false;
  try {
    socket.send(JSON.stringify(message));
    return true;
  } catch {
    return false;
  }
}

function clearSocket() {
  if (pingTimer) clearInterval(pingTimer);
  pingTimer = null;
  if (socket) {
    socket.onclose = null;
    socket.close();
  }
  socket = null;
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    void connect();
  }, reconnectDelay);
  reconnectDelay = Math.min(reconnectDelay * 2, RECONNECT_MAX_MS);
}

async function connect() {
  const config = await settings();
  if (!config.enabled) {
    clearSocket();
    await setStatus("paused", "Bridge control is paused.");
    return;
  }
  if (!config.token) {
    clearSocket();
    await setStatus("not-configured", "Paste the token printed by the local server.");
    return;
  }
  if (socket && [WebSocket.OPEN, WebSocket.CONNECTING].includes(socket.readyState)) return;

  await setStatus("connecting", `Connecting to 127.0.0.1:${config.port}`);
  const nextSocket = new WebSocket(`ws://127.0.0.1:${config.port}/bridge?token=${encodeURIComponent(config.token)}`);
  socket = nextSocket;

  nextSocket.onopen = async () => {
    reconnectDelay = 1_000;
    await setStatus("connected", `Connected to 127.0.0.1:${config.port}`);
    send({
      type: "hello",
      version: VERSION,
      browser: navigator.userAgent.includes("Edg/") ? "Microsoft Edge" : "Google Chrome",
      mode: config.fullAccess ? "full-access" : "safe",
      capabilities: [...COMMANDS],
    });
    if (pingTimer) clearInterval(pingTimer);
    pingTimer = setInterval(() => send({ type: "ping" }), PING_INTERVAL_MS);
  };

  nextSocket.onmessage = (event) => {
    let message;
    try { message = JSON.parse(event.data); } catch { return; }
    if (message.type === "pong") return;
    if (message.type !== "command" || typeof message.id !== "string" || typeof message.method !== "string") return;
    void dispatch(message.method, message.params ?? {}, false)
      .then((result) => send({ id: message.id, type: "result", ok: true, result }))
      .catch((error) => send({
        id: message.id,
        type: "result",
        ok: false,
        error: { code: error.code ?? error.message?.split(":")[0] ?? "COMMAND_FAILED", message: error.message },
      }));
  };

  nextSocket.onerror = () => {};
  nextSocket.onclose = async () => {
    if (socket === nextSocket) socket = null;
    if (pingTimer) clearInterval(pingTimer);
    pingTimer = null;
    await setStatus("disconnected", "Local server unavailable; retrying automatically.");
    scheduleReconnect();
  };
}

async function getTab(tabId) {
  if (!Number.isInteger(tabId)) throw new Error("BAD_TAB: tabId is required");
  const tab = await chrome.tabs.get(tabId);
  if (!tab?.id) throw new Error("BAD_TAB: target tab does not exist");
  return tab;
}

async function assertAllowedTab(tab) {
  const config = await settings();
  const verdict = isUrlAllowed(tab.url ?? "", config.allowedHosts, config.port, config.fullAccess);
  if (!verdict.allowed) throw new Error(`SITE_BLOCKED: ${verdict.reason}`);
  return verdict;
}

async function contentRequest(tabId, payload) {
  const message = { type: "LBB_CONTENT", ...payload };
  try {
    const response = await chrome.tabs.sendMessage(tabId, message);
    if (!response?.ok) throw new Error(response?.error ?? "Content command failed");
    return response.result;
  } catch (firstError) {
    try {
      await chrome.scripting.executeScript({ target: { tabId }, files: ["content.js"] });
      const response = await chrome.tabs.sendMessage(tabId, message);
      if (!response?.ok) throw new Error(response?.error ?? "Content command failed");
      return response.result;
    } catch (secondError) {
      throw new Error(`PAGE_UNAVAILABLE: ${secondError.message || firstError.message}`);
    }
  }
}

async function captureTab(tab) {
  if (!tab.active) {
    await chrome.tabs.update(tab.id, { active: true });
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const last = lastCaptureAt.get(tab.windowId) ?? 0;
  const wait = 550 - (Date.now() - last);
  if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
  const dataUrl = await chrome.tabs.captureVisibleTab(tab.windowId, { format: "jpeg", quality: 78 });
  lastCaptureAt.set(tab.windowId, Date.now());
  return dataUrl;
}

async function debuggerCommand(tabId, method, params) {
  return chrome.debugger.sendCommand({ tabId }, method, params);
}

async function withDebugger(tabId, operation) {
  let attached = false;
  try {
    await chrome.debugger.attach({ tabId }, "1.3");
    attached = true;
    return await operation();
  } finally {
    if (attached) await chrome.debugger.detach({ tabId }).catch(() => {});
  }
}

async function trustedClick(tabId, description, fallback) {
  const x = description.bounds.x + description.bounds.width / 2;
  const y = description.bounds.y + description.bounds.height / 2;
  try {
    return await withDebugger(tabId, async () => {
      await debuggerCommand(tabId, "Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
      await debuggerCommand(tabId, "Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
      await debuggerCommand(tabId, "Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
      return { clicked: true, trusted: true };
    });
  } catch {
    return fallback();
  }
}

async function trustedClickAt(tabId, x, y, button = "left", clickCount = 1) {
  return withDebugger(tabId, async () => {
    await debuggerCommand(tabId, "Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await debuggerCommand(tabId, "Input.dispatchMouseEvent", { type: "mousePressed", x, y, button, clickCount });
    await debuggerCommand(tabId, "Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button, clickCount });
    return { clicked: true, trusted: true, x, y, button, clickCount };
  });
}

const KEY_CODES = {
  Tab: 9, Enter: 13, Escape: 27, Backspace: 8, ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40,
  PageUp: 33, PageDown: 34, End: 35, Home: 36, Space: 32, Delete: 46, Insert: 45,
};

function parseKeyChord(chord) {
  const parts = String(chord).split("+").map((part) => part.trim()).filter(Boolean);
  if (!parts.length || parts.length > 5) throw new Error("BAD_KEY: enter a key or chord such as Enter, Meta+A, or Control+L");
  const key = parts.pop();
  let modifiers = 0;
  for (const modifier of parts) {
    if (/^(alt|option)$/i.test(modifier)) modifiers |= 1;
    else if (/^(control|ctrl)$/i.test(modifier)) modifiers |= 2;
    else if (/^(meta|command|cmd)$/i.test(modifier)) modifiers |= 4;
    else if (/^shift$/i.test(modifier)) modifiers |= 8;
    else throw new Error(`BAD_KEY: unsupported modifier ${modifier}`);
  }
  const normalizedKey = key === " " ? "Space" : key;
  const isLetter = /^[a-z]$/i.test(normalizedKey);
  const isDigit = /^\d$/.test(normalizedKey);
  const isFunction = /^F(?:[1-9]|1[0-2])$/.test(normalizedKey);
  const isNamed = Object.hasOwn(KEY_CODES, normalizedKey) || ["ContextMenu", "CapsLock", "PrintScreen", "Pause"].includes(normalizedKey);
  if (!isLetter && !isDigit && !isFunction && !isNamed && normalizedKey.length !== 1) {
    throw new Error(`BAD_KEY: unsupported key ${normalizedKey}`);
  }
  const keyCode = KEY_CODES[normalizedKey]
    ?? (isLetter ? normalizedKey.toUpperCase().charCodeAt(0) : isDigit ? normalizedKey.charCodeAt(0) : isFunction ? 111 + Number(normalizedKey.slice(1)) : normalizedKey.charCodeAt(0));
  const code = isLetter ? `Key${normalizedKey.toUpperCase()}` : isDigit ? `Digit${normalizedKey}` : normalizedKey;
  return { key: normalizedKey === "Space" ? " " : normalizedKey, code, keyCode, modifiers };
}

async function trustedKey(tabId, chord, fullAccess) {
  if (!fullAccess && !allowedKey(chord)) throw new Error("BAD_KEY: key is not allowlisted in Safe mode");
  const { key, code, keyCode, modifiers } = parseKeyChord(chord);
  return withDebugger(tabId, async () => {
    const params = { key, code, modifiers, windowsVirtualKeyCode: keyCode, nativeVirtualKeyCode: keyCode };
    await debuggerCommand(tabId, "Input.dispatchKeyEvent", { type: "rawKeyDown", ...params });
    await debuggerCommand(tabId, "Input.dispatchKeyEvent", { type: "keyUp", ...params });
    return { pressed: chord };
  });
}

async function insertText(tabId, value) {
  return withDebugger(tabId, async () => {
    await debuggerCommand(tabId, "Input.insertText", { text: value });
    return { typed: true, length: value.length };
  });
}

async function evaluateJavaScript(tabId, expression) {
  return withDebugger(tabId, async () => {
    const response = await debuggerCommand(tabId, "Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture: true,
      timeout: 12_000,
    });
    if (response.exceptionDetails) {
      const detail = response.exceptionDetails.exception?.description || response.exceptionDetails.text || "JavaScript evaluation failed";
      throw new Error(`EVALUATION_FAILED: ${detail}`);
    }
    return {
      type: response.result?.type ?? "undefined",
      value: Object.hasOwn(response.result ?? {}, "value")
        ? response.result.value
        : (response.result?.unserializableValue ?? response.result?.description),
    };
  });
}

async function queueApproval(method, params, tabId, description, risk) {
  const pending = {
    id: crypto.randomUUID(), method, params, tabId, ref: params.ref, label: description.name || description.role,
    risk, createdAt: Date.now(), expiresAt: Date.now() + 120_000,
  };
  await chrome.storage.local.set({ pendingApproval: pending });
  await chrome.action.setBadgeBackgroundColor({ color: "#f3bd4e" });
  await chrome.action.setBadgeText({ text: "?" });
  return { status: "approval_required", approvalId: pending.id, risk, label: pending.label, expiresAt: pending.expiresAt };
}

async function dispatch(method, params, approved) {
  if (!COMMANDS.has(method)) throw new Error("UNKNOWN_COMMAND: method is not supported");
  switch (method) {
    case "status": {
      const config = await settings();
      return { connected: socket?.readyState === WebSocket.OPEN, enabled: config.enabled, fullAccess: config.fullAccess, allowedHosts: config.allowedHosts };
    }
    case "tabs.list": {
      const config = await settings();
      const tabs = await chrome.tabs.query({});
      const allowedTabs = tabs.filter((tab) => isUrlAllowed(tab.url ?? "", config.allowedHosts, config.port, config.fullAccess).allowed);
      const active = allowedTabs.find((tab) => tab.active && tab.lastFocusedWindow) ?? allowedTabs.find((tab) => tab.active);
      return {
        activeTabId: active?.id ?? null,
        tabs: allowedTabs.filter((tab) => Number.isInteger(tab.id)).map((tab) => ({
          id: tab.id,
          title: String(tab.title ?? "").slice(0, 300),
          url: safeUrlForDisplay(tab.url ?? ""),
          active: Boolean(tab.active),
        })),
      };
    }
    case "tabs.activate": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await chrome.tabs.update(tab.id, { active: true });
      await chrome.windows.update(tab.windowId, { focused: true });
      return { tabId: tab.id, active: true };
    }
    case "tabs.new": {
      const tab = await chrome.tabs.create({ url: "about:blank", active: true });
      return { tabId: tab.id };
    }
    case "tabs.close": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const config = await settings();
      if (!config.fullAccess && !approved) return queueApproval(method, params, tab.id, { name: tab.title || "Untitled tab", role: "tab" }, "close a browser tab");
      await chrome.tabs.remove(tab.id);
      return { closed: true, tabId: params.tabId };
    }
    case "page.navigate": {
      const tab = await getTab(params.tabId);
      const config = await settings();
      let destination;
      try { destination = new URL(String(params.url)); } catch { throw new Error("BAD_URL: enter a complete HTTP or HTTPS URL"); }
      const verdict = isUrlAllowed(destination.href, config.allowedHosts, config.port, config.fullAccess);
      if (!verdict.allowed) throw new Error(`SITE_BLOCKED: ${verdict.reason}`);
      await chrome.tabs.update(tab.id, { url: verdict.url, active: true });
      return { tabId: tab.id, url: safeUrlForDisplay(verdict.url) };
    }
    case "page.back": {
      const tab = await getTab(params.tabId); await assertAllowedTab(tab); await chrome.tabs.goBack(tab.id); return { navigated: "back" };
    }
    case "page.forward": {
      const tab = await getTab(params.tabId); await assertAllowedTab(tab); await chrome.tabs.goForward(tab.id); return { navigated: "forward" };
    }
    case "page.reload": {
      const tab = await getTab(params.tabId); await assertAllowedTab(tab); await chrome.tabs.reload(tab.id); return { reloaded: true };
    }
    case "page.observe": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const snapshot = await contentRequest(tab.id, { method: "snapshot" });
      const screenshot = await captureTab(tab);
      for (const element of snapshot.elements ?? []) element.risk = classifyRisk(element);
      return { snapshot, screenshot };
    }
    case "page.click": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const description = await contentRequest(tab.id, { method: "prepareClick", ref: params.ref, generation: params.generation });
      if (description.disabled) throw new Error("ELEMENT_DISABLED: target cannot be clicked");
      const risk = classifyRisk(description);
      const config = await settings();
      if (!config.fullAccess && risk && !approved) return queueApproval(method, params, tab.id, description, risk);
      return trustedClick(tab.id, description, () => contentRequest(tab.id, { method: "clickFallback", ref: params.ref, generation: params.generation }));
    }
    case "page.fill": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const description = await contentRequest(tab.id, { method: "describe", ref: params.ref, generation: params.generation });
      const config = await settings();
      if (!config.fullAccess && (isSensitiveField(description) || description.sensitive)) throw new Error("SENSITIVE_FIELD: enter passwords, payment data, and one-time codes manually");
      return contentRequest(tab.id, {
        method: "fill", ref: params.ref, generation: params.generation, text: String(params.text ?? ""), allowSensitive: config.fullAccess,
      });
    }
    case "page.select": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      return contentRequest(tab.id, { method: "select", ref: params.ref, generation: params.generation, value: String(params.value ?? "") });
    }
    case "page.key": {
      const tab = await getTab(params.tabId); await assertAllowedTab(tab);
      const config = await settings();
      return trustedKey(tab.id, String(params.key), config.fullAccess);
    }
    case "page.scroll": {
      const tab = await getTab(params.tabId); await assertAllowedTab(tab);
      return contentRequest(tab.id, { method: "scroll", deltaX: Number(params.deltaX) || 0, deltaY: Number(params.deltaY) || 0 });
    }
    case "page.clickAt": {
      const tab = await getTab(params.tabId); await assertAllowedTab(tab);
      const config = await settings();
      if (!config.fullAccess) throw new Error("FULL_ACCESS_REQUIRED: enable Full Access in the extension popup");
      return trustedClickAt(tab.id, Number(params.x), Number(params.y), String(params.button ?? "left"), Number(params.clickCount) || 1);
    }
    case "page.typeText": {
      const tab = await getTab(params.tabId); await assertAllowedTab(tab);
      const config = await settings();
      if (!config.fullAccess) throw new Error("FULL_ACCESS_REQUIRED: enable Full Access in the extension popup");
      return insertText(tab.id, String(params.text ?? ""));
    }
    case "page.evaluate": {
      const tab = await getTab(params.tabId); await assertAllowedTab(tab);
      const config = await settings();
      if (!config.fullAccess) throw new Error("FULL_ACCESS_REQUIRED: enable Full Access in the extension popup");
      return evaluateJavaScript(tab.id, String(params.expression ?? ""));
    }
    default:
      throw new Error("UNKNOWN_COMMAND");
  }
}

async function popupState() {
  const config = await settings();
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  let currentHost = "";
  try { currentHost = new URL(tab?.url ?? "").hostname; } catch {}
  const pending = config.pendingApproval?.expiresAt > Date.now() ? config.pendingApproval : null;
  if (!pending && config.pendingApproval) await chrome.storage.local.set({ pendingApproval: null });
  return {
    enabled: config.enabled,
    fullAccess: config.fullAccess,
    port: config.port,
    tokenConfigured: Boolean(config.token),
    connectionStatus: config.connectionStatus,
    connectionDetail: config.connectionDetail,
    allowedHosts: config.allowedHosts,
    currentHost,
    currentHostAllowed: currentHost ? isUrlAllowed(`https://${currentHost}/`, config.allowedHosts, config.port, config.fullAccess).allowed : false,
    pendingApproval: pending ? { id: pending.id, label: pending.label, risk: pending.risk, expiresAt: pending.expiresAt } : null,
  };
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (sender.id !== chrome.runtime.id || message?.type !== "LBB_POPUP") return undefined;
  (async () => {
    switch (message.action) {
      case "getState": return popupState();
      case "saveConnection": {
        const port = Number(message.port);
        if (!Number.isInteger(port) || port < 1 || port > 65_535) throw new Error("Port must be between 1 and 65535");
        const updates = { port };
        if (String(message.token ?? "").trim()) updates.token = String(message.token).trim();
        await chrome.storage.local.set(updates);
        clearSocket();
        await connect();
        return popupState();
      }
      case "toggleEnabled":
        await chrome.storage.local.set({ enabled: Boolean(message.enabled) });
        clearSocket();
        await connect();
        return popupState();
      case "toggleFullAccess":
        await chrome.storage.local.set({ fullAccess: Boolean(message.fullAccess), pendingApproval: null });
        clearSocket();
        await connect();
        return popupState();
      case "allowCurrent": {
        const state = await popupState();
        const host = normalizeAllowedHost(state.currentHost);
        if (!host) throw new Error("The current tab is not an HTTP or HTTPS site");
        const config = await settings();
        await chrome.storage.local.set({ allowedHosts: [...new Set([...config.allowedHosts, host])] });
        return popupState();
      }
      case "removeHost": {
        const host = normalizeAllowedHost(message.host);
        const config = await settings();
        await chrome.storage.local.set({ allowedHosts: config.allowedHosts.filter((item) => item !== host) });
        return popupState();
      }
      case "addHost": {
        const host = normalizeAllowedHost(message.host);
        if (!host) throw new Error("Enter a hostname such as example.com or *.example.com");
        const config = await settings();
        await chrome.storage.local.set({ allowedHosts: [...new Set([...config.allowedHosts, host])] });
        return popupState();
      }
      case "approve": {
        const config = await settings();
        const pending = config.pendingApproval;
        if (!pending || pending.id !== message.id || pending.expiresAt <= Date.now()) throw new Error("Approval expired");
        await chrome.storage.local.set({ pendingApproval: null });
        try {
          const result = await dispatch(pending.method, pending.params, true);
          send({ type: "event", name: "approval.resolved", data: { id: pending.id, method: pending.method, ok: true, result } });
        } catch (error) {
          send({ type: "event", name: "approval.resolved", data: { id: pending.id, method: pending.method, ok: false, error: error.message } });
          throw error;
        } finally {
          await setStatus(socket?.readyState === WebSocket.OPEN ? "connected" : "disconnected", "Approval handled.");
        }
        return popupState();
      }
      case "reject": {
        const config = await settings();
        if (config.pendingApproval?.id === message.id) {
          await chrome.storage.local.set({ pendingApproval: null });
          send({ type: "event", name: "approval.rejected", data: { id: message.id } });
        }
        return popupState();
      }
      default: throw new Error("Unknown popup action");
    }
  })().then((result) => sendResponse({ ok: true, result })).catch((error) => sendResponse({ ok: false, error: error.message }));
  return true;
});

chrome.runtime.onInstalled.addListener(() => {
  void chrome.storage.local.get(DEFAULTS).then((stored) => chrome.storage.local.set({
    enabled: stored.enabled,
    fullAccess: stored.fullAccess,
    port: stored.port,
    allowedHosts: stored.allowedHosts,
    connectionStatus: stored.connectionStatus,
    connectionDetail: stored.connectionDetail,
    pendingApproval: stored.pendingApproval,
  })).then(connect);
});
chrome.runtime.onStartup.addListener(() => void connect());
chrome.alarms.create("local-browser-bridge-reconnect", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "local-browser-bridge-reconnect" && socket?.readyState !== WebSocket.OPEN) void connect();
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "local" && (changes.token || changes.port || changes.enabled || changes.fullAccess)) {
    clearSocket();
    void connect();
  }
});

void connect();
