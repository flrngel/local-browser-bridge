import {
  VERSION,
  PROTOCOL_VERSION,
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
  controllerId: "",
  bridgeSessionId: "",
  outboundSequence: 0,
  eventSequence: 0,
};
const RECONNECT_MAX_MS = 30_000;
const PING_INTERVAL_MS = 20_000;
const AUTH_NEGOTIATION_TIMEOUT_MS = 3_000;
const AUTH_MAX_FRAME_BYTES = 8 * 1024;
const AUTH_MAX_INBOUND_FRAMES = 4;
const CONTENT_TIMEOUT_MS = 6_000;
// page.waitFor is bounded by its own clamped timeout (max 12s) plus this
// margin, so the content call outlives the wait but stays under the server's
// 15s command deadline.
const WAIT_CONTENT_TIMEOUT_MARGIN_MS = 1_500;
const WAIT_TIMEOUT_DEFAULT_MS = 5_000;
const WAIT_TIMEOUT_MIN_MS = 100;
const WAIT_TIMEOUT_MAX_MS = 12_000;
const WAIT_MUTATION_QUIET_MIN_MS = 250;
const WAIT_MUTATION_QUIET_MAX_MS = 5_000;
const DEBUGGER_LIFECYCLE_TIMEOUT_MS = 3_000;
// JavaScript dialog metadata is bounded before it is stored or published.
const DIALOG_MESSAGE_MAX_CHARS = 500;
const DIALOG_PROMPT_TEXT_MAX_CHARS = 1_000;
// page.batch runs at most this many sub-actions, and only these snapshot
// bound page interactions may appear inside a batch: no navigation, no
// evaluation, and no nested batching.
const BATCH_MAX_ACTIONS = 10;
const BATCH_SUBMETHODS = new Set(["page.click", "page.fill", "page.select", "page.key", "page.scroll"]);
const COMMANDS = new Set([
  "status", "browser.control.start", "browser.control.status", "browser.control.stop",
  "tabs.list", "tabs.activate", "tabs.new", "tabs.close", "page.observe", "page.navigate",
  "page.back", "page.forward", "page.reload", "page.click", "page.fill", "page.select", "page.key", "page.scroll",
  "page.clickAt", "page.typeText", "page.evaluate", "page.waitFor", "page.hover",
  "page.batch", "page.handleDialog",
]);
const PAUSE_ALLOWED_COMMANDS = new Set([
  "status", "tabs.list", "browser.control.status", "browser.control.stop", "page.waitFor",
]);
// While a JavaScript dialog is pending, the renderer main thread is frozen:
// every content-script or Runtime call would only time out and revoke the
// lease. Only these commands are dispatched, because each one stays off the
// renderer (browser-process state, a best-effort overlay hide, or the
// browser-side CDP dialog resolution itself); everything else fails fast
// with BLOCKED_BY_DIALOG before any content or CDP call. This mirrors the
// server's dialog gate exactly.
const DIALOG_TOLERANT_COMMANDS = new Set([
  "status", "tabs.list", "browser.control.status", "browser.control.stop", "page.handleDialog",
]);

let socket = null;
let pingTimer = null;
let reconnectTimer = null;
let reconnectDelay = 1_000;
const DEBUGGER_TIMEOUT_MS = 6_000;
const CONTROL_TTL_DEFAULT_MS = 5 * 60_000;
const CONTROL_TTL_MIN_MS = 15_000;
const CONTROL_TTL_MAX_MS = 15 * 60_000;
const CONTROL_HEARTBEAT_MS = 10_000;
const CONTROL_STORAGE_KEY = "browserControlLease";
const CONTROL_REVOCATION_KEY = "browserControlRevocation";
const CONTROL_CLEANUPS_KEY = "browserControlCleanups";
const CONTROL_INPUTS_KEY = "browserControlHeldInputs";
const CREATED_TABS_KEY = "bridgeCreatedTabs";
const HUMAN_CONTROL_PAUSE_KEY = "browserControlHumanPause";
const HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY = "browserControlHumanPauseUncertain";
const HUMAN_PAUSE_REASONS = new Set(["released_by_user", "canceled_by_user"]);
const SECURITY_SETTING_KEYS = new Set(["token", "port", "enabled", "fullAccess", "allowedHosts"]);
const IRREVERSIBLE_CONTENT_METHODS = new Set(["fill", "select", "scroll"]);
let controlLease = null;
let controlEpoch = 0;
let lastControlRevocation = null;
let humanControlPause = null;
let humanControlPauseUncertain = null;
let controlHeartbeatTimer = null;
let intentionalDetachTabId = null;
let controlStatePromise = null;
let bridgeCreatedTabs = new Set();
let controllerId = "";
let protocolServerSessionId = "";
let protocolSessionReady = false;
let protocolConnectionId = "";
let outboundSequence = 0;
let eventSequence = 0;
let protocolIdentityPromise = null;
let sequenceWrite = Promise.resolve();
let transportRotation = Promise.resolve();
const expectedInternalSettings = new Map();
const canceledCommandKeys = new Set();
const activeCommandContexts = new Map();
const heldMouseInputs = new Map();
const heldKeyInputs = new Map();
const pendingDebuggerAttaches = new Map();
const pendingDebuggerDetaches = new Map();
const pendingControlTeardowns = new Map();
const pendingControlCleanups = new Map();
const activeControlCaptures = new Map();

function withTimeout(promise, timeoutMs, label, code = "DEBUGGER_TIMEOUT") {
  let timer;
  return Promise.race([
    promise,
    new Promise((_, reject) => {
      timer = setTimeout(() => {
        const error = new Error(`${code}: ${label} exceeded ${timeoutMs}ms`);
        error.code = code;
        reject(error);
      }, timeoutMs);
    }),
  ]).finally(() => clearTimeout(timer));
}

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

async function initializeProtocolIdentity() {
  if (protocolIdentityPromise) return protocolIdentityPromise;
  protocolIdentityPromise = (async () => {
    const stored = await chrome.storage.local.get({ controllerId: "", bridgeSessionId: "", outboundSequence: 0, eventSequence: 0 });
    controllerId = String(stored.controllerId || stored.bridgeSessionId || crypto.randomUUID());
    outboundSequence = Math.max(0, Number(stored.outboundSequence) || 0);
    eventSequence = Math.max(0, Number(stored.eventSequence) || 0);
    await chrome.storage.local.set({ controllerId, outboundSequence, eventSequence });
  })();
  return protocolIdentityPromise;
}

function currentControlOwner() {
  return protocolSessionReady && protocolServerSessionId
    ? `server:${protocolServerSessionId}`
    : `local:${controllerId}`;
}

function commandKey(sessionId, id, sequence) {
  return `${sessionId}:${sequence}:${id}`;
}

function commandUsesControl(method) {
  // page.waitFor is read-only and never holds the control lease, so a
  // canceled wait must not revoke an unrelated control session.
  return method === "browser.control.start"
    || (method.startsWith("page.") && method !== "page.waitFor");
}

function assertNoPendingControlLifecycle() {
  if (pendingControlCleanups.size > 0
    || pendingControlTeardowns.size > 0
    || pendingDebuggerAttaches.size > 0
    || pendingDebuggerDetaches.size > 0
    || heldMouseInputs.size > 0
    || heldKeyInputs.size > 0) {
    throw new Error("CONTROL_CLEANUP_PENDING: finish verified cleanup for the previous debugger lifecycle before starting any tab");
  }
}

function humanControlPausedError() {
  const error = new Error("HUMAN_CONTROL_PAUSED: a user paused all remote browser control; use Resume in the extension popup before any remote browser mutation can run");
  error.code = "HUMAN_CONTROL_PAUSED";
  return error;
}

function assertHumanControlAvailable() {
  if (humanControlPause?.paused || humanControlPauseUncertain?.paused) throw humanControlPausedError();
}

function latchHumanControlPause(reason, lease = controlLease) {
  if (!HUMAN_PAUSE_REASONS.has(reason)) return null;
  humanControlPause = {
    paused: true,
    reason,
    at: Date.now(),
    tabId: Number.isInteger(lease?.tabId) ? lease.tabId : null,
    sessionId: typeof lease?.sessionId === "string" ? lease.sessionId : null,
  };
  return humanControlPause;
}

async function persistHumanControlPause() {
  if (!humanControlPause?.paused) throw new Error("HUMAN_PAUSE_STATE_INVALID: no pause is available to persist");
  const marker = {
    paused: true,
    reason: "pause_persistence_uncertain",
    at: Date.now(),
    pause: { ...humanControlPause },
  };
  humanControlPauseUncertain = marker;
  let markerStored = false;
  let markerError = null;
  try {
    await chrome.storage.session.set({ [HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY]: marker });
    markerStored = true;
  } catch (error) {
    markerError = error;
  }
  try {
    await chrome.storage.local.set({ [HUMAN_CONTROL_PAUSE_KEY]: humanControlPause });
    if (markerStored) {
      await chrome.storage.session.set({ [HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY]: null }).catch(() => {});
    }
    humanControlPauseUncertain = null;
    return;
  } catch (error) {
    if (!markerStored) {
      await chrome.storage.session.set({ [HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY]: marker }).catch(() => {});
    }
    const failure = new Error("HUMAN_PAUSE_PERSIST_FAILED: browser control remains paused, but durable pause confirmation failed; use the popup to retry recovery");
    failure.code = "HUMAN_PAUSE_PERSIST_FAILED";
    failure.cause = error ?? markerError;
    throw failure;
  }
}

async function resumeHumanControlFromPopup() {
  const previousPause = humanControlPause ?? humanControlPauseUncertain?.pause ?? {
    paused: true,
    reason: "pause_persistence_uncertain",
    at: Date.now(),
    tabId: null,
    sessionId: null,
  };
  try {
    await chrome.storage.local.set({ [HUMAN_CONTROL_PAUSE_KEY]: null });
    await chrome.storage.session.set({ [HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY]: null });
  } catch (error) {
    humanControlPause = previousPause;
    humanControlPauseUncertain = {
      paused: true,
      reason: "resume_persistence_uncertain",
      at: Date.now(),
      pause: { ...previousPause },
    };
    await Promise.allSettled([
      chrome.storage.local.set({ [HUMAN_CONTROL_PAUSE_KEY]: previousPause }),
      chrome.storage.session.set({ [HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY]: humanControlPauseUncertain }),
    ]);
    throw error;
  }
  humanControlPause = null;
  humanControlPauseUncertain = null;
}

async function noteHumanPausePersistenceFailure(error, tabId, reason) {
  lastControlRevocation = {
    ...(lastControlRevocation ?? {}),
    tabId: Number.isInteger(tabId) ? tabId : null,
    reason,
    at: lastControlRevocation?.at ?? Date.now(),
    requiresExplicitStart: true,
    pausePersistenceUncertain: true,
  };
  await persistControlState().catch(() => {});
  await setStatus(
    "safety-paused",
    "Remote browser control is paused; open the extension popup to recover pause persistence.",
  ).catch(() => {});
  send({
    type: "event",
    name: "browser.control.pause_persistence_failed",
    data: { tabId, reason, error: error.message, control: publicControlState() },
  });
}

function rememberCanceledCommand(key) {
  canceledCommandKeys.add(key);
  if (canceledCommandKeys.size > 256) canceledCommandKeys.delete(canceledCommandKeys.values().next().value);
}

function cancelCommandContext(context, reason) {
  if (!context) return;
  context.canceled = true;
  context.cancelReason = reason;
  rememberCanceledCommand(context.key);
  for (const reject of context.cancelWaiters ?? []) reject(commandCanceledError(reason));
  context.cancelWaiters?.clear();
}

function commandCanceledError(boundary) {
  const error = new Error(`COMMAND_CANCELED: server canceled the command before ${boundary}`);
  error.code = "COMMAND_CANCELED";
  return error;
}

function assertCommandActive(context, boundary = "execution") {
  if (context && (context.canceled || canceledCommandKeys.has(context.key))) {
    throw commandCanceledError(boundary);
  }
}

function withCommandCancellation(promise, context, boundary) {
  if (!context) return promise;
  assertCommandActive(context, boundary);
  promise.catch(() => {});
  let rejectCancellation;
  const cancellation = new Promise((_, reject) => {
    rejectCancellation = reject;
    context.cancelWaiters.add(rejectCancellation);
  });
  return Promise.race([promise, cancellation]).finally(() => {
    context.cancelWaiters.delete(rejectCancellation);
  });
}

function captureLeaseAuthority(lease = controlLease) {
  if (!lease) throw new Error("CONTROL_REQUIRED: start browser control before acting");
  return {
    tabId: lease.tabId,
    sessionId: lease.sessionId,
    epoch: lease.epoch,
    documentEpoch: lease.documentEpoch,
  };
}

function assertLeaseAuthority(authority, context, boundary) {
  assertCommandActive(context, boundary);
  if (!authority
    || !controlLease
    || controlLease.tabId !== authority.tabId
    || controlLease.sessionId !== authority.sessionId
    || controlLease.epoch !== authority.epoch
    || controlEpoch !== authority.epoch) {
    const error = new Error(`CONTROL_CANCELED: browser control changed before ${boundary}`);
    error.code = "CONTROL_CANCELED";
    throw error;
  }
  if (Number.isSafeInteger(authority.documentEpoch)
    && authority.documentEpoch !== controlLease.documentEpoch) {
    const error = new Error(`DOCUMENT_CHANGED: the top-level document changed before ${boundary}`);
    error.code = "DOCUMENT_CHANGED";
    throw error;
  }
}

function outcomeUnknownError(boundary, cause = null) {
  const error = new Error(`ACTION_OUTCOME_UNKNOWN: control changed after ${boundary}; do not retry automatically`);
  error.code = "ACTION_OUTCOME_UNKNOWN";
  if (cause) error.cause = cause;
  return error;
}

function assertLeaseAuthorityAfterDispatch(authority, context, boundary) {
  try {
    assertLeaseAuthority(authority, context, boundary);
  } catch (error) {
    throw outcomeUnknownError(boundary, error);
  }
}

function assertCommandActiveAfterDispatch(context, boundary) {
  try {
    assertCommandActive(context, boundary);
  } catch (error) {
    throw outcomeUnknownError(boundary, error);
  }
}

function assertHumanControlAvailableAfterDispatch(boundary) {
  try {
    assertHumanControlAvailable();
  } catch (error) {
    throw outcomeUnknownError(boundary, error);
  }
}

async function commandSideEffect(context, boundary, operation) {
  assertCommandActive(context, `${boundary} dispatch`);
  assertHumanControlAvailable();
  const result = await withCommandCancellation(operation(), context, boundary);
  assertCommandActiveAfterDispatch(context, boundary);
  assertHumanControlAvailableAfterDispatch(boundary);
  return result;
}

async function leaseSideEffect(authority, context, boundary, operation) {
  assertLeaseAuthority(authority, context, `${boundary} dispatch`);
  assertHumanControlAvailable();
  const result = await withCommandCancellation(operation(), context, boundary);
  assertLeaseAuthorityAfterDispatch(authority, context, boundary);
  assertHumanControlAvailableAfterDispatch(boundary);
  return result;
}

function cancelCommandContextsForSession(sessionId, reason) {
  if (!sessionId) return;
  for (const context of activeCommandContexts.values()) {
    if (context.sessionId !== sessionId) continue;
    cancelCommandContext(context, reason);
  }
}

function send(message) {
  if (socket?.readyState !== WebSocket.OPEN || !protocolServerSessionId) return false;
  try {
    outboundSequence += 1;
    if (message.type === "event") eventSequence += 1;
    const envelope = {
      ...message,
      protocolVersion: PROTOCOL_VERSION,
      sessionId: protocolServerSessionId,
      controllerId,
      connectionId: protocolConnectionId,
      sequence: Number.isSafeInteger(message.sequence) ? message.sequence : outboundSequence,
      controllerSequence: outboundSequence,
      ...(message.type === "event" ? { eventSequence } : {}),
    };
    socket.send(JSON.stringify(envelope));
    sequenceWrite = sequenceWrite.then(() => chrome.storage.local.set({ outboundSequence, eventSequence })).catch(() => {});
    return true;
  } catch {
    return false;
  }
}

async function clearSocket(reason = "transport_rotated") {
  cancelCommandContextsForSession(protocolServerSessionId, reason);
  await stopControl(reason, { requireExplicitStart: true });
  if (pingTimer) clearInterval(pingTimer);
  pingTimer = null;
  if (socket) {
    socket.onclose = null;
    socket.close();
  }
  socket = null;
  protocolServerSessionId = "";
  protocolSessionReady = false;
}

function settingFingerprint(value) {
  return JSON.stringify(value);
}

function markInternalSettings(updates) {
  for (const [key, value] of Object.entries(updates)) {
    if (SECURITY_SETTING_KEYS.has(key)) {
      expectedInternalSettings.set(key, { fingerprint: settingFingerprint(value), expiresAt: Date.now() + 5_000 });
    }
  }
}

function consumeInternalSettings(changes) {
  const relevant = Object.entries(changes).filter(([key]) => SECURITY_SETTING_KEYS.has(key));
  if (!relevant.length) return false;
  const internal = relevant.every(([key, change]) => {
    const expected = expectedInternalSettings.get(key);
    return expected?.expiresAt >= Date.now() && expected.fingerprint === settingFingerprint(change.newValue);
  });
  for (const [key] of relevant) expectedInternalSettings.delete(key);
  return internal;
}

function updateSecuritySettings(updates, reason) {
  const operation = transportRotation.then(async () => {
    await clearSocket(reason);
    markInternalSettings(updates);
    await chrome.storage.local.set(updates);
    await connect();
  });
  transportRotation = operation.catch(() => {});
  return operation;
}

function queueTransportRotation(reason) {
  transportRotation = transportRotation
    .then(async () => {
      await clearSocket(reason);
      await connect();
    })
    .catch(() => {});
  return transportRotation;
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    void connect();
  }, reconnectDelay);
  reconnectDelay = Math.min(reconnectDelay * 2, RECONNECT_MAX_MS);
}

function decodeBase64Url32(value, label) {
  if (typeof value !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(value)) {
    throw new Error(`${label} must be 32-byte base64url without padding`);
  }
  const padded = `${value.replaceAll("-", "+").replaceAll("_", "/")}=`;
  const decoded = atob(padded);
  if (decoded.length !== 32) throw new Error(`${label} must decode to exactly 32 bytes`);
  return Uint8Array.from(decoded, (character) => character.charCodeAt(0));
}

function encodeBase64Url(bytes) {
  const binary = String.fromCharCode(...bytes);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/u, "");
}

async function importAuthKey(token) {
  return crypto.subtle.importKey(
    "raw",
    decodeBase64Url32(token, "Extension token"),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign", "verify"],
  );
}

function serverAuthPayload(sessionId, clientNonce, serverNonce) {
  return `LBB-WS-AUTH-V1\nserver\nbrowser-extension\n${sessionId}\n${clientNonce}\n${serverNonce}`;
}

function clientAuthPayload(sessionId, clientNonce, serverNonce) {
  return `LBB-WS-AUTH-V1\nclient\nbrowser-extension\n${sessionId}\n${clientNonce}\n${serverNonce}`;
}

async function verifyAuthProof(key, proof, payload) {
  const signature = decodeBase64Url32(proof, "Server proof");
  return crypto.subtle.verify("HMAC", key, signature, new TextEncoder().encode(payload));
}

async function createAuthProof(key, payload) {
  const signature = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(payload));
  return encodeBase64Url(new Uint8Array(signature));
}

function exactObjectKeys(value, expected) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return false;
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  return actual.length === wanted.length && actual.every((key, index) => key === wanted[index]);
}

async function connect() {
  await initializeProtocolIdentity();
  const config = await settings();
  if (!config.enabled) {
    await clearSocket("bridge_paused");
    await setStatus("paused", "Bridge control is paused.");
    return;
  }
  if (!config.token) {
    await clearSocket("not_configured");
    await setStatus("not-configured", "Paste the token printed by the local server.");
    return;
  }
  if (socket && [WebSocket.OPEN, WebSocket.CONNECTING].includes(socket.readyState)) return;

  await setStatus("connecting", `Connecting to 127.0.0.1:${config.port}`);
  protocolConnectionId = crypto.randomUUID();
  protocolServerSessionId = "";
  protocolSessionReady = false;
  const nextSocket = new WebSocket(`ws://127.0.0.1:${config.port}/bridge`);
  socket = nextSocket;
  let authResponseSent = false;
  let authInProgress = false;
  let authSessionId = "";
  let authServerNonce = "";
  let authClientNonce = "";
  let preauthInboundFrames = 0;
  let welcomed = false;
  let ready = false;
  let lastCommandSequence = -1;
  let commandChain = Promise.resolve();
  let negotiationTimer = null;

  const protocolFailure = (detail) => {
    if (socket !== nextSocket) return;
    void setStatus("protocol-error", detail);
    try { nextSocket.close(1002, "Protocol validation failed"); } catch {}
  };

  nextSocket.onopen = () => {
    reconnectDelay = 1_000;
    negotiationTimer = setTimeout(
      () => protocolFailure("The local server did not complete mutual authentication within 3 seconds."),
      AUTH_NEGOTIATION_TIMEOUT_MS,
    );
    authClientNonce = encodeBase64Url(crypto.getRandomValues(new Uint8Array(32)));
    nextSocket.send(JSON.stringify({
      type: "authHello",
      authVersion: 1,
      connector: "browser-extension",
      clientNonce: authClientNonce,
    }));
    void setStatus("connecting", `Negotiating protocol with 127.0.0.1:${config.port}`);
  };

  nextSocket.onmessage = async (event) => {
    if (!welcomed) {
      preauthInboundFrames += 1;
      if (preauthInboundFrames > AUTH_MAX_INBOUND_FRAMES
        || typeof event.data !== "string"
        || new TextEncoder().encode(event.data).byteLength > AUTH_MAX_FRAME_BYTES) {
        protocolFailure("The server exceeded the pre-authentication frame limits.");
        return;
      }
    }
    let message;
    try { message = JSON.parse(event.data); } catch {
      protocolFailure("The server sent malformed JSON.");
      return;
    }
    if (!authResponseSent) {
      if (authInProgress
        || !exactObjectKeys(message, ["type", "authVersion", "connector", "sessionId", "clientNonce", "serverNonce", "serverProof"])
        || message.type !== "authChallenge"
        || message.authVersion !== 1
        || message.connector !== "browser-extension"
        || message.clientNonce !== authClientNonce
        || typeof message.sessionId !== "string"
        || !/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(message.sessionId)) {
        protocolFailure("The server authentication challenge was invalid.");
        return;
      }
      authInProgress = true;
      try {
        const key = await importAuthKey(config.token);
        decodeBase64Url32(message.serverNonce, "Server nonce");
        const valid = await verifyAuthProof(
          key,
          message.serverProof,
          serverAuthPayload(message.sessionId, authClientNonce, message.serverNonce),
        );
        if (!valid) throw new Error("Server proof mismatch");
        const clientProof = await createAuthProof(
          key,
          clientAuthPayload(message.sessionId, authClientNonce, message.serverNonce),
        );
        if (socket !== nextSocket || nextSocket.readyState !== WebSocket.OPEN) return;
        authSessionId = message.sessionId;
        authServerNonce = message.serverNonce;
        authResponseSent = true;
        nextSocket.send(JSON.stringify({
          type: "authResponse",
          authVersion: 1,
          connector: "browser-extension",
          sessionId: authSessionId,
          clientNonce: authClientNonce,
          serverNonce: authServerNonce,
          clientProof,
        }));
        void setStatus("connecting", `Mutually authenticated with 127.0.0.1:${config.port}`);
      } catch {
        protocolFailure("The local server could not prove knowledge of the extension token.");
      } finally {
        authInProgress = false;
      }
      return;
    }
    if (!welcomed) {
      if (message.type !== "welcome"
        || message.protocolVersion !== PROTOCOL_VERSION
        || message.sessionId !== authSessionId
        || message.serverVersion !== VERSION
        || message.connector !== "browser-extension") {
        protocolFailure("The server welcome did not match protocol version 1.");
        return;
      }
      if (negotiationTimer) clearTimeout(negotiationTimer);
      negotiationTimer = null;
      await initializeControlState();
      const incomingOwner = `server:${message.sessionId}`;
      if (controlLease?.ownerSessionId?.startsWith("server:") && controlLease.ownerSessionId !== incomingOwner) {
        await stopControl("owner_session_changed", { requireExplicitStart: true });
      }
      if (socket !== nextSocket || nextSocket.readyState !== WebSocket.OPEN) return;
      welcomed = true;
      protocolServerSessionId = message.sessionId;
      void setStatus("connecting", `Authenticating extension session with 127.0.0.1:${config.port}`);
      send({
        type: "hello",
        version: VERSION,
        browser: navigator.userAgent.includes("Edg/") ? "Microsoft Edge" : "Google Chrome",
        mode: config.fullAccess ? "full-access" : "safe",
        capabilities: [...COMMANDS],
      });
      negotiationTimer = setTimeout(() => protocolFailure("The server did not acknowledge the extension hello in time."), DEBUGGER_TIMEOUT_MS);
      return;
    }
    if (message.protocolVersion !== PROTOCOL_VERSION || message.sessionId !== protocolServerSessionId) {
      protocolFailure("The server message failed protocol or session validation.");
      return;
    }
    if (!ready) {
      if (message.type !== "helloAck" || message.ok !== true) {
        protocolFailure(typeof message.error === "string" ? message.error : "The server rejected the extension hello.");
        return;
      }
      ready = true;
      protocolSessionReady = true;
      if (negotiationTimer) clearTimeout(negotiationTimer);
      negotiationTimer = null;
      void setStatus("connected", `Connected to 127.0.0.1:${config.port}`);
      if (pingTimer) clearInterval(pingTimer);
      pingTimer = setInterval(() => send({ type: "ping" }), PING_INTERVAL_MS);
      return;
    }
    if (message.type === "pong") return;
    if (message.type === "cancel") {
      if (typeof message.id !== "string"
        || !message.id
        || !Number.isSafeInteger(message.sequence)
        || message.sequence < 0
        || message.sequence > lastCommandSequence) {
        protocolFailure("The server cancel envelope was invalid.");
        return;
      }
      const key = commandKey(protocolServerSessionId, message.id, message.sequence);
      rememberCanceledCommand(key);
      const context = activeCommandContexts.get(key);
      if (context) {
        cancelCommandContext(context, "server_timeout");
        if (context.started && commandUsesControl(context.method) && controlLease) {
          void stopControl("command_canceled", { requireExplicitStart: true });
        } else if (context.started && context.method === "browser.control.start") {
          controlEpoch += 1;
        }
      }
      return;
    }
    if (message.type !== "command" || typeof message.id !== "string" || typeof message.method !== "string" || !Number.isSafeInteger(message.sequence) || message.sequence <= lastCommandSequence) {
      protocolFailure("The server command envelope or monotonic sequence was invalid.");
      return;
    }
    lastCommandSequence = message.sequence;
    const context = {
      key: commandKey(protocolServerSessionId, message.id, message.sequence),
      id: message.id,
      sequence: message.sequence,
      sessionId: protocolServerSessionId,
      method: message.method,
      canceled: false,
      started: false,
      cancelWaiters: new Set(),
    };
    activeCommandContexts.set(context.key, context);
    commandChain = commandChain.then(async () => {
      if (socket !== nextSocket || !ready) {
        activeCommandContexts.delete(context.key);
        return;
      }
      try {
        context.started = true;
        assertCommandActive(context, "command dispatch");
        const result = await dispatch(message.method, message.params ?? {}, false, context);
        assertCommandActive(context, "result delivery");
        send({ id: message.id, type: "result", ok: true, sequence: message.sequence, result });
      } catch (error) {
        if (context.canceled || canceledCommandKeys.has(context.key)) return;
        send({
          id: message.id,
          type: "result",
          ok: false,
          sequence: message.sequence,
          error: { code: error.code ?? error.message?.split(":")[0] ?? "COMMAND_FAILED", message: error.message },
        });
      } finally {
        activeCommandContexts.delete(context.key);
      }
    }).catch(() => protocolFailure("Command dispatch failed outside its result boundary."));
  };

  nextSocket.onerror = () => {};
  nextSocket.onclose = async () => {
    if (negotiationTimer) clearTimeout(negotiationTimer);
    if (socket !== nextSocket) return;
    cancelCommandContextsForSession(protocolServerSessionId, "server_disconnected");
    socket = null;
    protocolServerSessionId = "";
    protocolSessionReady = false;
    if (pingTimer) clearInterval(pingTimer);
    pingTimer = null;
    await stopControl("server_disconnected", { requireExplicitStart: true });
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

function effectiveTabUrl(tab) {
  return String(tab?.pendingUrl || tab?.url || "");
}

function isTrackedBridgeBlank(tab, rawUrl = effectiveTabUrl(tab)) {
  return rawUrl === "about:blank"
    && Number.isInteger(tab?.id)
    && bridgeCreatedTabs.has(tab.id)
    && !Number.isInteger(tab.openerTabId)
    && (!tab.pendingUrl || tab.pendingUrl === "about:blank");
}

function allowedTabVerdict(tab, config, rawUrl = effectiveTabUrl(tab)) {
  return isUrlAllowed(
    rawUrl,
    config.allowedHosts,
    config.port,
    config.fullAccess,
    isTrackedBridgeBlank(tab, rawUrl),
  );
}

async function assertAllowedTab(tab) {
  const config = await settings();
  const verdict = allowedTabVerdict(tab, config);
  if (!verdict.allowed) throw new Error(`SITE_BLOCKED: ${verdict.reason}`);
  return verdict;
}

function pendingDialogError(boundary) {
  const kind = controlLease?.pendingDialog?.type ?? "dialog";
  const error = new Error(`BLOCKED_BY_DIALOG: a JavaScript ${kind} dialog is blocking the controlled page during ${boundary}; resolve it with page.handleDialog first`);
  error.code = "BLOCKED_BY_DIALOG";
  return error;
}

function assertNoPendingDialog(method) {
  if (!DIALOG_TOLERANT_COMMANDS.has(method) && controlLease?.pendingDialog) {
    throw pendingDialogError(method);
  }
}

async function boundedContentOperation(operation, label, authority, commandContext, timeoutMs = CONTENT_TIMEOUT_MS) {
  try {
    return await withTimeout(
      withCommandCancellation(operation, commandContext, label),
      timeoutMs,
      label,
      "CONTENT_TIMEOUT",
    );
  } catch (error) {
    // A dialog that opened mid-request froze the renderer, so the timeout
    // belongs to the dialog: surface the dialog instead of revoking a
    // healthy lease over it.
    if (error.code === "CONTENT_TIMEOUT" && controlLease?.pendingDialog) {
      throw pendingDialogError(label);
    }
    if ((error.code === "CONTENT_TIMEOUT" || error.code === "COMMAND_CANCELED") && authority) {
      await stopControl(`content_interrupted:${label}`, { requireExplicitStart: true });
      throw outcomeUnknownError(label, error);
    }
    throw error;
  }
}

async function contentRequest(tabId, payload, {
  authority = null,
  commandContext = null,
  timeoutMs = CONTENT_TIMEOUT_MS,
  retryAfterTimeout = true,
} = {}) {
  if (authority) assertLeaseAuthority(authority, commandContext, `content ${payload.method} dispatch`);
  else assertCommandActive(commandContext, `content ${payload.method} dispatch`);
  const message = {
    type: "LBB_CONTENT",
    controlSessionId: controlLease?.tabId === tabId ? controlLease.sessionId : null,
    controlEpoch: controlLease?.tabId === tabId ? controlLease.epoch : null,
    ...payload,
  };
  let response;
  try {
    response = await boundedContentOperation(
      chrome.tabs.sendMessage(tabId, message),
      `content ${payload.method}`,
      authority,
      commandContext,
      timeoutMs,
    );
  } catch (firstError) {
    if (firstError.code === "ACTION_OUTCOME_UNKNOWN") throw firstError;
    // A dialog-frozen renderer cannot be reinjected into either; the dialog
    // itself is the actionable failure.
    if (firstError.code === "BLOCKED_BY_DIALOG") throw firstError;
    if (!retryAfterTimeout && firstError.code === "CONTENT_TIMEOUT") throw firstError;
    if (IRREVERSIBLE_CONTENT_METHODS.has(payload.method)) {
      await stopControl(`content_outcome_unknown:${payload.method}`, { requireExplicitStart: true });
      throw outcomeUnknownError(`content ${payload.method}`, firstError);
    }
    try {
      if (authority) assertLeaseAuthority(authority, commandContext, `content ${payload.method} reinjection`);
      else assertCommandActive(commandContext, `content ${payload.method} reinjection`);
      await boundedContentOperation(
        chrome.scripting.executeScript({ target: { tabId }, files: ["content.js"] }),
        `content ${payload.method} reinjection`,
        authority,
        commandContext,
      );
      if (authority) assertLeaseAuthority(authority, commandContext, `content ${payload.method} retry`);
      else assertCommandActive(commandContext, `content ${payload.method} retry`);
      response = await boundedContentOperation(
        chrome.tabs.sendMessage(tabId, message),
        `content ${payload.method} retry`,
        authority,
        commandContext,
        timeoutMs,
      );
    } catch (secondError) {
      if (secondError.code === "ACTION_OUTCOME_UNKNOWN" || secondError.code === "BLOCKED_BY_DIALOG") throw secondError;
      throw new Error(`PAGE_UNAVAILABLE: ${secondError.message || firstError.message}`);
    }
  }
  if (authority) assertLeaseAuthorityAfterDispatch(authority, commandContext, `content ${payload.method} dispatch`);
  else assertCommandActive(commandContext, `content ${payload.method} completion`);
  if (!response?.ok) throw new Error(response?.error ?? "CONTENT_COMMAND_FAILED: Content command failed");
  return response.result;
}

function clamp(value, minimum, maximum) {
  return Math.min(maximum, Math.max(minimum, value));
}

function controlPolicy(config, tab) {
  return {
    allowedHosts: [...config.allowedHosts],
    port: config.port,
    fullAccess: Boolean(config.fullAccess),
    trustedBlank: isTrackedBridgeBlank(tab),
  };
}

function policyUrlVerdict(lease, rawUrl) {
  const policy = lease?.policy;
  if (!policy) return { allowed: false, reason: "The control policy is unavailable" };
  return isUrlAllowed(
    String(rawUrl || ""),
    policy.allowedHosts,
    policy.port,
    policy.fullAccess,
    Boolean(policy.trustedBlank),
  );
}

function comparableDocumentUrl(rawUrl) {
  if (rawUrl === "about:blank") return rawUrl;
  try {
    const url = new URL(rawUrl);
    url.hash = "";
    return url.href;
  } catch {
    return String(rawUrl || "");
  }
}

function sameDocumentUrl(left, right) {
  return comparableDocumentUrl(left) === comparableDocumentUrl(right);
}

function revokeUnexpectedNavigation(reason) {
  if (!controlLease) return;
  void stopControl(reason, { requireExplicitStart: true }).catch(() => {});
}

function acceptTopLevelNavigationSignal({ tabId, url = "", loaderId = "", frameId = "", source, sameDocument = false }) {
  const lease = controlLease;
  if (!lease || lease.tabId !== tabId) return false;
  if (!lease.navigationReady) {
    if (url) lease.documentUrl = url;
    if (loaderId) lease.loaderId = loaderId;
    if (frameId) lease.frameId = frameId;
    return true;
  }
  const pending = lease.pendingNavigation;
  if (!pending) {
    const recentCommit = lease.lastNavigationCommit;
    if (source === "tabs.onUpdated"
      && !loaderId
      && recentCommit
      && Date.now() - recentCommit.at <= 2_000
      && (!url || sameDocumentUrl(recentCommit.url, url))) {
      return true;
    }
    revokeUnexpectedNavigation(`unexpected_top_level_navigation:${source}`);
    return false;
  }
  if (url) {
    const verdict = policyUrlVerdict(lease, url);
    const expectedMatches = !pending.expectedUrl || sameDocumentUrl(pending.expectedUrl, url);
    if (!verdict.allowed || !expectedMatches) {
      revokeUnexpectedNavigation(!verdict.allowed
        ? "navigation_left_allowlist"
        : "navigation_destination_changed");
      return false;
    }
    pending.observedUrl = url;
  }
  pending.lastSignalAt = Date.now();
  if (loaderId) {
    lease.loaderId = loaderId;
    if (frameId) lease.frameId = frameId;
    lease.documentUrl = url || pending.observedUrl || pending.expectedUrl || "";
    lease.pendingNavigation = null;
    lease.lastNavigationCommit = { url: lease.documentUrl, at: Date.now() };
    lease.viewport = null;
    lease.cursor = { ...lease.cursor, visible: false, updatedAt: Date.now() };
    void persistControlState().catch(() => revokeUnexpectedNavigation("navigation_state_persist_failed"));
  } else if (sameDocument) {
    lease.documentUrl = url || pending.observedUrl || pending.expectedUrl || lease.documentUrl;
    lease.pendingNavigation = null;
    lease.lastNavigationCommit = { url: lease.documentUrl, at: Date.now() };
    lease.viewport = null;
    void persistControlState().catch(() => revokeUnexpectedNavigation("navigation_state_persist_failed"));
  }
  return true;
}

async function authorizeTopLevelNavigation(lease, authority, kind, expectedUrl, commandContext) {
  assertLeaseAuthority(authority, commandContext, `${kind} navigation authorization`);
  if (lease.pendingNavigation) throw new Error("NAVIGATION_PENDING: wait for the current top-level navigation to finish");
  if (expectedUrl) {
    const verdict = policyUrlVerdict(lease, expectedUrl);
    if (!verdict.allowed) throw new Error(`SITE_BLOCKED: ${verdict.reason}`);
  }
  lease.documentEpoch = Math.max(0, Number(lease.documentEpoch) || 0) + 1;
  lease.pendingNavigation = {
    kind,
    expectedUrl: expectedUrl || null,
    authorizedAt: Date.now(),
    lastSignalAt: 0,
  };
  lease.viewport = null;
  await persistControlState();
  return captureLeaseAuthority(lease);
}

async function failChangedDocument(lease, boundary, cause) {
  if (lease
    && controlLease?.sessionId === lease.sessionId
    && controlLease?.epoch === lease.epoch) {
    await stopControl(`document_changed:${boundary}`, { requireExplicitStart: true });
  }
  const error = new Error(`DOCUMENT_CHANGED: top-level document identity changed during ${boundary}; output was discarded`);
  error.code = "DOCUMENT_CHANGED";
  error.cause = cause;
  throw error;
}

async function verifyDocumentAuthority(tabId, authority, commandContext, boundary) {
  assertLeaseAuthority(authority, commandContext, `${boundary} document precheck`);
  const lease = controlLease;
  if (lease.pendingNavigation || !lease.navigationReady || !lease.loaderId || !lease.documentUrl) {
    const error = new Error(`NAVIGATION_PENDING: no stable top-level document exists during ${boundary}`);
    error.code = "NAVIGATION_PENDING";
    throw error;
  }
  try {
    const frameTree = await debuggerCommand(
      tabId,
      "Page.getFrameTree",
      {},
      authority,
      commandContext,
    );
    const frame = frameTree?.frameTree?.frame;
    const tab = await getTab(tabId);
    assertLeaseAuthority(authority, commandContext, `${boundary} document postcheck`);
    const tabVerdict = allowedTabVerdict(tab, {
      allowedHosts: lease.policy.allowedHosts,
      port: lease.policy.port,
      fullAccess: lease.policy.fullAccess,
    });
    const frameVerdict = policyUrlVerdict(lease, frame?.url);
    const identityMatches = frame?.loaderId === lease.loaderId
      && sameDocumentUrl(frame?.url, lease.documentUrl)
      && sameDocumentUrl(effectiveTabUrl(tab), frame?.url);
    if (!tabVerdict.allowed || !frameVerdict.allowed || !identityMatches) {
      return failChangedDocument(lease, boundary, new Error(
        !tabVerdict.allowed ? tabVerdict.reason : !frameVerdict.allowed ? frameVerdict.reason : "loader or URL mismatch",
      ));
    }
    return {
      documentEpoch: lease.documentEpoch,
      loaderId: lease.loaderId,
      url: lease.documentUrl,
    };
  } catch (error) {
    if (["DOCUMENT_CHANGED", "CONTROL_CANCELED", "COMMAND_CANCELED"].includes(error.code)) throw error;
    return failChangedDocument(lease, boundary, error);
  }
}

async function verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, boundary) {
  try {
    return await verifyDocumentAuthority(tabId, authority, commandContext, boundary);
  } catch (error) {
    if (["ACTION_OUTCOME_UNKNOWN", "CDP_OUTCOME_UNKNOWN"].includes(error.code)) throw error;
    throw outcomeUnknownError(boundary, error);
  }
}

async function initializeLeaseDocument(lease, tab, config, commandContext = null) {
  const authority = captureLeaseAuthority(lease);
  const frameTree = await debuggerCommand(
    lease.tabId,
    "Page.getFrameTree",
    {},
    authority,
    commandContext,
  );
  const frame = frameTree?.frameTree?.frame;
  const liveTab = tab ?? await getTab(lease.tabId);
  lease.policy = controlPolicy(config, liveTab);
  const tabVerdict = allowedTabVerdict(liveTab, config);
  const frameVerdict = policyUrlVerdict(lease, frame?.url);
  if (!frame?.loaderId
    || !tabVerdict.allowed
    || !frameVerdict.allowed
    || !sameDocumentUrl(effectiveTabUrl(liveTab), frame.url)) {
    throw new Error(`DOCUMENT_UNSAFE: ${tabVerdict.reason || frameVerdict.reason || "top-level document identity is unavailable"}`);
  }
  lease.loaderId = frame.loaderId;
  lease.frameId = frame.id;
  lease.documentUrl = frame.url;
  lease.navigationReady = true;
  lease.pendingNavigation = null;
  return authority;
}

function publicControlState() {
  const cleanups = [...pendingControlCleanups.values()].map((cleanup) => ({ ...cleanup }));
  const cleanup = cleanups[0] ?? null;
  if (!controlLease) {
    return {
      active: false,
      humanPaused: Boolean(humanControlPause?.paused),
      humanPause: humanControlPause ? { ...humanControlPause } : null,
      pausePersistenceUncertain: Boolean(humanControlPauseUncertain?.paused),
      revocationPending: Boolean(cleanup),
      cleanup: cleanup ? { ...cleanup } : null,
      cleanups,
      activeCaptureIds: [],
      revocation: lastControlRevocation ? { ...lastControlRevocation } : null,
    };
  }
  const activeCaptureIds = [...(activeControlCaptures.get(controlLease.tabId) ?? [])];
  return {
    active: true,
    humanPaused: Boolean(humanControlPause?.paused),
    humanPause: humanControlPause ? { ...humanControlPause } : null,
    pausePersistenceUncertain: Boolean(humanControlPauseUncertain?.paused),
    revocationPending: Boolean(cleanup),
    cleanup: cleanup ? { ...cleanup } : null,
    cleanups,
    activeCaptureIds,
    captureDepth: activeCaptureIds.length,
    documentEpoch: controlLease.documentEpoch,
    documentUrl: safeUrlForDisplay(controlLease.documentUrl ?? ""),
    navigationPending: Boolean(controlLease.pendingNavigation),
    sessionId: controlLease.sessionId,
    tabId: controlLease.tabId,
    startedAt: controlLease.startedAt,
    expiresAt: controlLease.expiresAt,
    lastHeartbeatAt: controlLease.lastHeartbeatAt,
    turn: controlLease.turn,
    moveSequence: controlLease.moveSequence,
    epoch: controlLease.epoch,
    cursor: { ...controlLease.cursor },
    owner: controlLease.ownerSessionId?.startsWith("server:") ? "server" : "local",
  };
}

async function persistControlState() {
  await chrome.storage.session.set({
    [CONTROL_STORAGE_KEY]: controlLease,
    [CONTROL_REVOCATION_KEY]: lastControlRevocation,
    [CONTROL_CLEANUPS_KEY]: [...pendingControlCleanups.values()],
    [CONTROL_INPUTS_KEY]: {
      mouse: [...heldMouseInputs].map(([key, record]) => ({ key, ...record })),
      keyboard: [...heldKeyInputs].map(([key, record]) => ({ key, ...record })),
    },
    [CREATED_TABS_KEY]: [...bridgeCreatedTabs],
  });
}

async function persistHeldInputIntent(map, key, record) {
  map.set(key, record);
  try {
    await persistControlState();
  } catch (error) {
    map.delete(key);
    throw error;
  }
}

async function clearHeldInputIntent(map, key, record) {
  map.delete(key);
  try {
    await persistControlState();
  } catch (error) {
    map.set(key, record);
    throw error;
  }
}

function controlCaptureIds(tabId) {
  return [...(activeControlCaptures.get(tabId) ?? [])];
}

function beginControlCapture(tabId, captureId) {
  const captures = activeControlCaptures.get(tabId) ?? new Set();
  captures.add(captureId);
  activeControlCaptures.set(tabId, captures);
  return [...captures];
}

function endControlCapture(tabId, captureId) {
  const captures = activeControlCaptures.get(tabId);
  captures?.delete(captureId);
  if (!captures?.size) activeControlCaptures.delete(tabId);
  return controlCaptureIds(tabId);
}

function controlUiAcknowledged(state, expectedCaptureIds, expectedCursorVisible) {
  if (!state?.hostConnected || !state.popoverOpen) return false;
  const expected = [...expectedCaptureIds].map(String).sort();
  const actual = Array.isArray(state.activeCaptureIds)
    ? state.activeCaptureIds.map(String).sort()
    : [];
  if (expected.length !== actual.length || expected.some((id, index) => id !== actual[index])) return false;
  if (Number(state.captureDepth) !== expected.length) return false;
  if (state.cursorVisible !== Boolean(expectedCursorVisible)) return false;
  if (expected.length > 0) {
    return state.capturing === true && state.pillVisible === false && state.stopVisible === false;
  }
  return state.capturing === false && state.pillVisible === true && state.stopVisible === true;
}

async function failControlUiClosed(lease, phase, cause) {
  if (lease
    && controlLease?.sessionId === lease.sessionId
    && controlLease?.epoch === lease.epoch) {
    await stopControl(`control_ui_failed:${phase}`, { requireExplicitStart: true }).catch(() => {});
  }
  const error = new Error(`CONTROL_UI_RENDER_FAILED: ${phase} was not visibly acknowledged; browser control was revoked`);
  error.code = "CONTROL_UI_RENDER_FAILED";
  error.cause = cause;
  throw error;
}

async function showControlUi(lease = controlLease) {
  if (!lease
    || controlLease?.sessionId !== lease.sessionId
    || controlLease?.epoch !== lease.epoch) return null;
  const authority = captureLeaseAuthority(lease);
  const activeCaptureIds = controlCaptureIds(lease.tabId);
  try {
    const state = await contentRequest(lease.tabId, {
      method: "control.show",
      sessionId: lease.sessionId,
      controlEpoch: lease.epoch,
      expiresAt: lease.expiresAt,
      lastHeartbeatAt: lease.lastHeartbeatAt,
      turn: lease.turn,
      moveSequence: lease.moveSequence,
      activeCaptureIds,
      cursor: { ...lease.cursor, turn: lease.turn, moveSequence: lease.moveSequence },
    }, { authority });
    assertLeaseAuthority(authority, null, "control UI acknowledgement");
    if (!controlUiAcknowledged(state, activeCaptureIds, lease.cursor.visible)) {
      throw new Error("The page did not confirm a painted control indicator");
    }
    return state;
  } catch (error) {
    return failControlUiClosed(lease, "show", error);
  }
}

async function hideControlUi(tabId, sessionId = null) {
  if (!Number.isInteger(tabId)) return;
  await withTimeout(
    contentRequest(tabId, { method: "control.hide", sessionId }),
    1_500,
    "control UI hide",
  ).catch(() => {});
}

async function boundedDebuggerDetach(tabId, label = "debugger detach") {
  if (pendingDebuggerDetaches.has(tabId)) return false;
  const token = crypto.randomUUID();
  const operation = chrome.debugger.detach({ tabId });
  operation.catch(() => {});
  pendingDebuggerDetaches.set(tabId, token);
  void operation.then(() => {
    if (pendingDebuggerDetaches.get(tabId) === token) pendingDebuggerDetaches.delete(tabId);
  }).catch(() => {
    if (pendingDebuggerDetaches.get(tabId) === token) pendingDebuggerDetaches.delete(tabId);
  });
  try {
    await withTimeout(
      operation,
      DEBUGGER_LIFECYCLE_TIMEOUT_MS,
      label,
      "DEBUGGER_DETACH_TIMEOUT",
    );
    return true;
  } catch {
    return false;
  }
}

async function debuggerDetachConfirmed(tabId) {
  const operation = chrome.debugger.getTargets();
  operation.catch(() => {});
  try {
    const targets = await withTimeout(
      operation,
      DEBUGGER_LIFECYCLE_TIMEOUT_MS,
      "debugger detach verification",
      "DEBUGGER_RECOVERY_TIMEOUT",
    );
    return !targets.some((target) => target.attached && target.tabId === tabId);
  } catch {
    return null;
  }
}

function stopHeartbeat() {
  if (controlHeartbeatTimer) clearInterval(controlHeartbeatTimer);
  controlHeartbeatTimer = null;
}

function scheduleHeartbeat() {
  stopHeartbeat();
  if (!controlLease) return;
  controlHeartbeatTimer = setInterval(() => void heartbeatControl(), CONTROL_HEARTBEAT_MS);
}

async function initializeControlState() {
  if (controlStatePromise) return controlStatePromise;
  controlStatePromise = (async () => {
    const [stored, storedPause] = await Promise.all([
      chrome.storage.session.get({
        [CONTROL_STORAGE_KEY]: null,
        [CONTROL_REVOCATION_KEY]: null,
        [CONTROL_CLEANUPS_KEY]: [],
        [CONTROL_INPUTS_KEY]: { mouse: [], keyboard: [] },
        [HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY]: null,
        [CREATED_TABS_KEY]: [],
      }),
      chrome.storage.local.get({ [HUMAN_CONTROL_PAUSE_KEY]: null })
        .then((value) => ({ ok: true, value }))
        .catch((error) => ({ ok: false, error })),
    ]);
    const uncertainPause = stored[HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY];
    const pause = storedPause.ok
      ? storedPause.value[HUMAN_CONTROL_PAUSE_KEY]
      : uncertainPause?.pause;
    humanControlPauseUncertain = uncertainPause?.paused === true
      ? uncertainPause
      : storedPause.ok
        ? null
        : {
            paused: true,
            reason: "pause_storage_read_failed",
            at: Date.now(),
            pause: pause ?? null,
          };
    if (!humanControlPause?.paused) {
      humanControlPause = pause?.paused === true || humanControlPauseUncertain?.paused
        ? {
            paused: true,
            reason: typeof pause?.reason === "string" ? pause.reason : "pause_persistence_uncertain",
            at: Number(pause?.at) || Date.now(),
            tabId: Number.isInteger(pause?.tabId) ? pause.tabId : null,
            sessionId: typeof pause?.sessionId === "string" ? pause.sessionId : null,
          }
        : null;
    }
    lastControlRevocation = stored[CONTROL_REVOCATION_KEY] ?? null;
    pendingControlCleanups.clear();
    for (const cleanup of Array.isArray(stored[CONTROL_CLEANUPS_KEY]) ? stored[CONTROL_CLEANUPS_KEY] : []) {
      if (!Number.isInteger(cleanup?.tabId) || typeof cleanup.reason !== "string") continue;
      pendingControlCleanups.set(cleanup.tabId, {
        tabId: cleanup.tabId,
        sessionId: typeof cleanup.sessionId === "string" ? cleanup.sessionId : null,
        reason: cleanup.reason,
        since: Number(cleanup.since) || Date.now(),
        lastAttemptAt: Number(cleanup.lastAttemptAt) || 0,
        inputsReleased: Boolean(cleanup.inputsReleased),
        phase: typeof cleanup.phase === "string" ? cleanup.phase : "detach_pending",
        attachToken: typeof cleanup.attachToken === "string" ? cleanup.attachToken : null,
        operationSettled: Boolean(cleanup.operationSettled),
        detachConfirmations: Math.max(0, Number(cleanup.detachConfirmations) || 0),
        lastDetachConfirmationAt: Math.max(0, Number(cleanup.lastDetachConfirmationAt) || 0),
      });
    }
    heldMouseInputs.clear();
    heldKeyInputs.clear();
    const storedInputs = stored[CONTROL_INPUTS_KEY] ?? {};
    for (const record of Array.isArray(storedInputs.mouse) ? storedInputs.mouse : []) {
      if (typeof record?.key !== "string"
        || !Number.isInteger(record.tabId)
        || record.releaseMethod !== "Input.dispatchMouseEvent"
        || typeof record.releaseParams !== "object") continue;
      const { key, ...intent } = record;
      heldMouseInputs.set(key, intent);
    }
    for (const record of Array.isArray(storedInputs.keyboard) ? storedInputs.keyboard : []) {
      if (typeof record?.key !== "string"
        || !Number.isInteger(record.tabId)
        || record.releaseMethod !== "Input.dispatchKeyEvent"
        || typeof record.releaseParams !== "object") continue;
      const { key, ...intent } = record;
      heldKeyInputs.set(key, intent);
    }
    const recoveredHeldTabs = new Set(
      [...heldMouseInputs.values(), ...heldKeyInputs.values()].map((record) => record.tabId),
    );
    for (const tabId of recoveredHeldTabs) {
      if (!pendingControlCleanups.has(tabId)) {
        pendingControlCleanups.set(tabId, {
          tabId,
          sessionId: null,
          reason: "recovered_held_input",
          since: Date.now(),
          lastAttemptAt: 0,
          inputsReleased: false,
          phase: "held_input_cleanup",
        });
      }
    }
    if (recoveredHeldTabs.size > 0) {
      const tabId = recoveredHeldTabs.values().next().value;
      lastControlRevocation = {
        tabId,
        reason: "recovered_held_input",
        at: Date.now(),
        requiresExplicitStart: true,
        cleanupPending: true,
      };
    }
    if (lastControlRevocation?.cleanupPending
      && Number.isInteger(lastControlRevocation.tabId)
      && !pendingControlCleanups.has(lastControlRevocation.tabId)) {
      pendingControlCleanups.set(lastControlRevocation.tabId, {
        tabId: lastControlRevocation.tabId,
        sessionId: lastControlRevocation.sessionId ?? null,
        reason: lastControlRevocation.reason,
        since: lastControlRevocation.at,
        lastAttemptAt: 0,
      });
    }
    const storedCreatedTabs = (Array.isArray(stored[CREATED_TABS_KEY])
      ? stored[CREATED_TABS_KEY]
      : []).filter(Number.isInteger);
    bridgeCreatedTabs = new Set(storedCreatedTabs);
    const verifiedBlankTabs = await Promise.all(storedCreatedTabs.map(async (tabId) => {
      try {
        const tab = await withTimeout(
          chrome.tabs.get(tabId),
          1_500,
          "bridge blank provenance recovery",
          "TAB_RECOVERY_TIMEOUT",
        );
        return isTrackedBridgeBlank(tab) ? tabId : null;
      } catch {
        return null;
      }
    }));
    bridgeCreatedTabs = new Set(verifiedBlankTabs.filter(Number.isInteger));
    const candidate = stored[CONTROL_STORAGE_KEY];
    const invalidCandidate = !candidate
      || !Number.isInteger(candidate.tabId)
      || candidate.expiresAt <= Date.now()
      || pendingControlCleanups.size > 0
      || heldMouseInputs.size > 0
      || heldKeyInputs.size > 0
      || !Number.isSafeInteger(candidate.epoch)
      || typeof candidate.ownerSessionId !== "string"
      || !candidate.ownerSessionId
      || Boolean(candidate.pendingNavigation);
    if (invalidCandidate) {
      controlLease = null;
      if (heldMouseInputs.size > 0 || heldKeyInputs.size > 0) {
        await persistControlState();
        await retryPendingControlCleanups();
        return;
      }
      if (Number.isInteger(candidate?.tabId)) {
        await boundedDebuggerDetach(candidate.tabId, "invalid recovered lease detach");
        const detachConfirmed = await debuggerDetachConfirmed(candidate.tabId) === true
          && !pendingDebuggerDetaches.has(candidate.tabId);
        if (detachConfirmed) {
          pendingControlCleanups.delete(candidate.tabId);
        } else if (!pendingControlCleanups.has(candidate.tabId)) {
          pendingControlCleanups.set(candidate.tabId, {
            tabId: candidate.tabId,
            sessionId: candidate.sessionId ?? null,
            reason: "recovered_lease_cleanup",
            since: Date.now(),
            lastAttemptAt: Date.now(),
            inputsReleased: false,
          });
        }
        lastControlRevocation = {
          tabId: candidate.tabId,
          sessionId: candidate.sessionId,
          reason: candidate.expiresAt <= Date.now() ? "lease_expired" : "lease_owner_missing",
          at: Date.now(),
          requiresExplicitStart: true,
          cleanupPending: pendingControlCleanups.size > 0,
        };
      }
      await persistControlState();
      if (pendingControlCleanups.size) void retryPendingControlCleanups();
      return;
    }
    const targetsOperation = chrome.debugger.getTargets();
    targetsOperation.catch(() => {});
    const targets = await withTimeout(
      targetsOperation,
      DEBUGGER_LIFECYCLE_TIMEOUT_MS,
      "debugger target recovery",
      "DEBUGGER_RECOVERY_TIMEOUT",
    ).catch(() => []);
    if (!targets.some((target) => target.attached && target.tabId === candidate.tabId)) {
      controlLease = null;
      await boundedDebuggerDetach(candidate.tabId, "lost recovered lease detach");
      lastControlRevocation = {
        tabId: candidate.tabId,
        reason: "debugger_session_lost",
        at: Date.now(),
        requiresExplicitStart: true,
      };
      await persistControlState();
      return;
    }
    const recoveryConfig = await settings();
    const recoveryTab = await getTab(candidate.tabId);
    controlLease = {
      ...candidate,
      documentEpoch: Math.max(1, Number(candidate.documentEpoch) || 1),
      navigationReady: false,
      pendingNavigation: null,
      policy: controlPolicy(recoveryConfig, recoveryTab),
    };
    controlEpoch = Math.max(controlEpoch, candidate.epoch);
    try {
      await initializeLeaseDocument(controlLease, recoveryTab, recoveryConfig);
      await persistControlState();
      scheduleHeartbeat();
      await showControlUi();
    } catch {
      await stopControl("recovered_document_unverified", { requireExplicitStart: true });
    }
  })();
  return controlStatePromise;
}

function synchronouslyTakeControlLease(reason, requireExplicitStart) {
  controlEpoch += 1;
  const lease = controlLease;
  controlLease = null;
  activeControlCaptures.clear();
  stopHeartbeat();
  if (!lease) return null;
  lastControlRevocation = {
    tabId: lease.tabId,
    sessionId: lease.sessionId,
    reason,
    at: Date.now(),
    requiresExplicitStart: requireExplicitStart,
  };
  return lease;
}

async function bestEffortDebuggerRelease(tabId, method, params, label) {
  const operation = chrome.debugger.sendCommand({ tabId }, method, params);
  operation.catch(() => {});
  try {
    await withTimeout(operation, 1_000, label);
    return true;
  } catch {
    return false;
  }
}

async function releaseHeldMouseInput(key, record, { revokeOnFailure = true } = {}) {
  if (!heldMouseInputs.has(key)) return true;
  const released = await bestEffortDebuggerRelease(
    record.tabId,
    record.releaseMethod,
    record.releaseParams,
    "mouse release cleanup",
  );
  if (released) {
    try {
      await clearHeldInputIntent(heldMouseInputs, key, record);
      return true;
    } catch {
      // The durable intent remains so restart cleanup repeats the idempotent release.
    }
  }
  if (revokeOnFailure) {
    await stopControl("mouse_release_failed", { requireExplicitStart: true });
    const error = new Error("INPUT_RELEASE_FAILED: mouse release was not acknowledged; control was revoked");
    error.code = "INPUT_RELEASE_FAILED";
    throw error;
  }
  return false;
}

async function releaseHeldKeyInput(key, record, { revokeOnFailure = true } = {}) {
  if (!heldKeyInputs.has(key)) return true;
  const released = await bestEffortDebuggerRelease(
    record.tabId,
    record.releaseMethod,
    record.releaseParams,
    "key release cleanup",
  );
  if (released) {
    try {
      await clearHeldInputIntent(heldKeyInputs, key, record);
      return true;
    } catch {
      // The durable intent remains so restart cleanup repeats the idempotent release.
    }
  }
  if (revokeOnFailure) {
    await stopControl("key_release_failed", { requireExplicitStart: true });
    const error = new Error("INPUT_RELEASE_FAILED: key release was not acknowledged; control was revoked");
    error.code = "INPUT_RELEASE_FAILED";
    throw error;
  }
  return false;
}

async function releaseHeldInputs(tabId) {
  const releases = [];
  for (const [key, record] of heldMouseInputs) {
    if (record.tabId === tabId) releases.push(releaseHeldMouseInput(key, record, { revokeOnFailure: false }));
  }
  for (const [key, record] of heldKeyInputs) {
    if (record.tabId === tabId) releases.push(releaseHeldKeyInput(key, record, { revokeOnFailure: false }));
  }
  const results = await Promise.allSettled(releases);
  return results.every((result) => result.status === "fulfilled" && result.value === true);
}

function forgetHeldInputs(tabId) {
  for (const [key, record] of heldMouseInputs) {
    if (record.tabId === tabId) heldMouseInputs.delete(key);
  }
  for (const [key, record] of heldKeyInputs) {
    if (record.tabId === tabId) heldKeyInputs.delete(key);
  }
}

async function stopControl(reason = "released", { requireExplicitStart = false, detach = true } = {}) {
  const pauseLatched = latchHumanControlPause(reason, controlLease);
  let pausePersistError = null;
  const pausePersistPromise = pauseLatched
    ? persistHumanControlPause().catch((error) => { pausePersistError = error; })
    : Promise.resolve();
  let lease = synchronouslyTakeControlLease(reason, requireExplicitStart);
  if (!lease) {
    await initializeControlState();
    lease = synchronouslyTakeControlLease(reason, requireExplicitStart);
  }
  if (!lease) {
    if (pendingControlCleanups.size) await retryPendingControlCleanups();
    await pausePersistPromise;
    if (pausePersistError) {
      await noteHumanPausePersistenceFailure(
        pausePersistError,
        humanControlPause?.tabId,
        reason,
      );
      throw pausePersistError;
    }
    return publicControlState();
  }
  const teardownToken = crypto.randomUUID();
  pendingControlTeardowns.set(lease.tabId, teardownToken);
  try {
    const cleanup = {
      tabId: lease.tabId,
      sessionId: lease.sessionId,
      reason,
      since: Date.now(),
      lastAttemptAt: 0,
      inputsReleased: false,
    };
    pendingControlCleanups.set(lease.tabId, cleanup);
    lastControlRevocation = { ...lastControlRevocation, cleanupPending: true };
    await Promise.all([persistControlState().catch(() => {}), pausePersistPromise]);
    const hidePromise = hideControlUi(lease.tabId, lease.sessionId);
    let inputsReleased = true;
    let detachConfirmed = !detach;
    if (detach) {
      intentionalDetachTabId = lease.tabId;
      try {
        inputsReleased = await releaseHeldInputs(lease.tabId);
        await boundedDebuggerDetach(lease.tabId);
        detachConfirmed = await debuggerDetachConfirmed(lease.tabId) === true
          && !pendingDebuggerDetaches.has(lease.tabId);
      } finally {
        intentionalDetachTabId = null;
      }
    }
    if (detachConfirmed) {
      forgetHeldInputs(lease.tabId);
      pendingControlCleanups.delete(lease.tabId);
      lastControlRevocation = {
        ...lastControlRevocation,
        cleanupPending: false,
        cleanupCompletedAt: Date.now(),
      };
    } else {
      pendingControlCleanups.set(lease.tabId, {
        ...cleanup,
        lastAttemptAt: Date.now(),
        inputsReleased,
      });
      lastControlRevocation = { ...lastControlRevocation, cleanupPending: true };
      setTimeout(() => void retryPendingControlCleanups(), 1_000);
    }
    await Promise.all([hidePromise, persistControlState()]);
    send({ type: "event", name: "browser.control.revoked", data: publicControlState().revocation });
    if (pausePersistError) {
      await noteHumanPausePersistenceFailure(pausePersistError, lease.tabId, reason);
      throw pausePersistError;
    }
    return publicControlState();
  } finally {
    if (pendingControlTeardowns.get(lease.tabId) === teardownToken) {
      pendingControlTeardowns.delete(lease.tabId);
    }
  }
}

async function confirmPendingControlCleanup(tabId, reason = "debugger_detached") {
  if (!pendingControlCleanups.has(tabId)) return false;
  pendingControlCleanups.delete(tabId);
  forgetHeldInputs(tabId);
  if (lastControlRevocation?.tabId === tabId) {
    lastControlRevocation = {
      ...lastControlRevocation,
      cleanupPending: false,
      cleanupCompletedAt: Date.now(),
      cleanupReason: reason,
    };
  }
  await persistControlState();
  send({ type: "event", name: "browser.control.cleanup_completed", data: { tabId, reason } });
  return true;
}

async function retryPendingControlCleanups() {
  for (const [tabId, cleanup] of [...pendingControlCleanups]) {
    if (pendingControlTeardowns.has(tabId) || pendingDebuggerAttaches.has(tabId)) continue;
    const teardownToken = crypto.randomUUID();
    pendingControlTeardowns.set(tabId, teardownToken);
    try {
      const inputsReleased = await releaseHeldInputs(tabId);
      if (!pendingDebuggerDetaches.has(tabId)) {
        intentionalDetachTabId = tabId;
        try {
          await boundedDebuggerDetach(tabId, "pending control cleanup detach");
        } finally {
          intentionalDetachTabId = null;
        }
      }
      const detachConfirmed = await debuggerDetachConfirmed(tabId) === true
        && !pendingDebuggerDetaches.has(tabId);
      if (detachConfirmed) {
        const unresolvedRestoredAttach = cleanup.phase?.startsWith("attach_")
          && !cleanup.operationSettled;
        const now = Date.now();
        const separatedConfirmation = !cleanup.lastDetachConfirmationAt
          || now - cleanup.lastDetachConfirmationAt >= 1_000;
        const detachConfirmations = (cleanup.detachConfirmations || 0)
          + (separatedConfirmation ? 1 : 0);
        if (unresolvedRestoredAttach && detachConfirmations < 2) {
          pendingControlCleanups.set(tabId, {
            ...cleanup,
            lastAttemptAt: now,
            inputsReleased: cleanup.inputsReleased || inputsReleased,
            detachConfirmations,
            lastDetachConfirmationAt: separatedConfirmation
              ? now
              : cleanup.lastDetachConfirmationAt,
          });
          await persistControlState();
          setTimeout(() => void retryPendingControlCleanups(), 1_000);
        } else {
          await confirmPendingControlCleanup(tabId, "cleanup_retry_confirmed");
        }
      } else {
        pendingControlCleanups.set(tabId, {
          ...cleanup,
          lastAttemptAt: Date.now(),
          inputsReleased: cleanup.inputsReleased || inputsReleased,
        });
        if (lastControlRevocation?.tabId === tabId) {
          lastControlRevocation = { ...lastControlRevocation, cleanupPending: true };
        }
        await persistControlState();
      }
    } finally {
      if (pendingControlTeardowns.get(tabId) === teardownToken) {
        pendingControlTeardowns.delete(tabId);
      }
    }
  }
  return publicControlState();
}

async function markUnknownAttachCleanup(tabId, attachToken, reason, { operationSettled = false } = {}) {
  const existing = pendingControlCleanups.get(tabId);
  pendingControlCleanups.set(tabId, {
    tabId,
    sessionId: null,
    reason,
    since: existing?.since ?? Date.now(),
    lastAttemptAt: Date.now(),
    inputsReleased: true,
    phase: "attach_outcome_unknown",
    attachToken,
    operationSettled,
    detachConfirmations: 0,
    lastDetachConfirmationAt: 0,
  });
  lastControlRevocation = {
    tabId,
    reason,
    at: Date.now(),
    requiresExplicitStart: true,
    cleanupPending: true,
  };
  await persistControlState().catch(() => {});
}

async function settleUnknownDebuggerAttach(tabId, attachToken, reason) {
  const activeToken = pendingDebuggerAttaches.get(tabId);
  if (activeToken && activeToken !== attachToken) return false;
  if (activeToken === attachToken) pendingDebuggerAttaches.delete(tabId);
  await markUnknownAttachCleanup(tabId, attachToken, reason, { operationSettled: true });
  await retryPendingControlCleanups();
  return !pendingControlCleanups.has(tabId);
}

async function hardRevokeDetached(tabId, reason) {
  const pauseLatched = latchHumanControlPause(reason, controlLease);
  let pausePersistError = null;
  const pausePersistPromise = pauseLatched
    ? persistHumanControlPause().catch((error) => { pausePersistError = error; })
    : Promise.resolve();
  if (controlLease && controlLease.tabId !== tabId) return;
  let lease = synchronouslyTakeControlLease(reason || "debugger_detached", true);
  if (!lease) {
    await initializeControlState();
    if (controlLease && controlLease.tabId !== tabId) return;
    lease = synchronouslyTakeControlLease(reason || "debugger_detached", true);
  }
  if (!lease) {
    await pausePersistPromise;
    if (pausePersistError) {
      await noteHumanPausePersistenceFailure(pausePersistError, tabId, reason);
    }
    return;
  }
  forgetHeldInputs(tabId);
  await hideControlUi(tabId, lease.sessionId);
  await Promise.all([persistControlState(), pausePersistPromise]);
  send({ type: "event", name: "browser.control.revoked", data: { ...lastControlRevocation } });
  if (pausePersistError) {
    await noteHumanPausePersistenceFailure(pausePersistError, tabId, reason);
  }
}

async function heartbeatControl() {
  await initializeControlState();
  const lease = controlLease;
  if (!lease) return;
  if (lease.expiresAt <= Date.now()) {
    await stopControl("lease_expired", { requireExplicitStart: true });
    return;
  }
  if (lease.pendingNavigation) {
    // A beforeunload dialog legitimately stalls an authorized navigation, so
    // the stall is only fatal once no dialog explains it.
    if (!lease.pendingDialog && Date.now() - lease.pendingNavigation.authorizedAt > 15_000) {
      await stopControl("navigation_timeout", { requireExplicitStart: true });
    }
    return;
  }
  try {
    const authority = captureLeaseAuthority(lease);
    await verifyDocumentAuthority(lease.tabId, authority, null, "control heartbeat");
    if (lease.pendingDialog) {
      // The dialog freezes the renderer: the Runtime.evaluate probe and the
      // overlay refresh would both time out and revoke a healthy lease, so
      // only the browser-side attachment and document identity are checked
      // until the dialog is resolved.
      assertLeaseAuthority(authority, null, "heartbeat commit");
      controlLease.lastHeartbeatAt = Date.now();
      await persistControlState();
      return;
    }
    await debuggerCommand(lease.tabId, "Runtime.evaluate", {
      expression: "void 0",
      silent: true,
      returnByValue: true,
    }, authority);
    assertLeaseAuthority(authority, null, "heartbeat commit");
    controlLease.lastHeartbeatAt = Date.now();
    await persistControlState();
    assertLeaseAuthority(authority, null, "heartbeat UI refresh");
    await showControlUi(controlLease);
  } catch (error) {
    // A dialog racing the heartbeat is not a failed lease.
    if (error?.code === "BLOCKED_BY_DIALOG") return;
    await stopControl("heartbeat_failed", { requireExplicitStart: true });
  }
}

async function startControl(tabId, {
  ttlMs = CONTROL_TTL_DEFAULT_MS,
  explicit = false,
  reason = "browser.control.start",
  ownerSessionId = null,
  commandContext = null,
} = {}) {
  await initializeControlState();
  assertHumanControlAvailable();
  await initializeProtocolIdentity();
  const tab = await getTab(tabId);
  await assertAllowedTab(tab);
  const controlConfig = await settings();
  assertNoPendingControlLifecycle();
  const requestedOwner = ownerSessionId || currentControlOwner();
  const boundedTtl = clamp(Number(ttlMs) || CONTROL_TTL_DEFAULT_MS, CONTROL_TTL_MIN_MS, CONTROL_TTL_MAX_MS);
  if (controlLease?.tabId === tab.id && controlLease.ownerSessionId === requestedOwner) {
    const authority = captureLeaseAuthority();
    assertLeaseAuthority(authority, commandContext, "control lease renewal");
    controlLease.expiresAt = Date.now() + boundedTtl;
    controlLease.lastHeartbeatAt = Date.now();
    if (explicit) lastControlRevocation = null;
    await persistControlState();
    assertLeaseAuthority(authority, commandContext, "control lease renewal commit");
    await showControlUi();
    assertLeaseAuthority(authority, commandContext, "control lease renewal response");
    return publicControlState();
  }
  if (controlLease) {
    const ownerChanged = controlLease.ownerSessionId !== requestedOwner;
    await stopControl(ownerChanged ? "owner_session_changed" : "target_switched", { requireExplicitStart: ownerChanged });
    if (ownerChanged && !explicit) {
      throw new Error("CONTROL_OWNER_MISMATCH: explicitly start a control session owned by the current server session");
    }
    assertNoPendingControlLifecycle();
  }
  if (!explicit && lastControlRevocation?.tabId === tab.id && lastControlRevocation.requiresExplicitStart) {
    throw new Error("CONTROL_REVOKED: Chrome or the user revoked control; explicitly start a new browser control session");
  }
  assertCommandActive(commandContext, "debugger attach intent");
  assertHumanControlAvailable();
  const attachToken = crypto.randomUUID();
  const previousRevocation = lastControlRevocation;
  pendingControlCleanups.set(tab.id, {
    tabId: tab.id,
    sessionId: null,
    reason: "debugger_attach_intent",
    since: Date.now(),
    lastAttemptAt: 0,
    inputsReleased: true,
    phase: "attach_intent",
    attachToken,
  });
  lastControlRevocation = {
    tabId: tab.id,
    reason: "debugger_attach_intent",
    at: Date.now(),
    requiresExplicitStart: true,
    cleanupPending: true,
  };
  try {
    await persistControlState();
    assertCommandActive(commandContext, "debugger attachment");
    assertHumanControlAvailable();
  } catch (error) {
    if (pendingControlCleanups.get(tab.id)?.attachToken === attachToken) {
      pendingControlCleanups.delete(tab.id);
    }
    if (lastControlRevocation?.tabId === tab.id
      && lastControlRevocation.reason === "debugger_attach_intent") {
      lastControlRevocation = previousRevocation;
    }
    await persistControlState().catch(() => {});
    throw error;
  }
  const attachEpoch = controlEpoch;
  const attachOperation = chrome.debugger.attach({ tabId: tab.id }, "1.3");
  attachOperation.catch(() => {});
  pendingDebuggerAttaches.set(tab.id, attachToken);
  try {
    await withTimeout(
      withCommandCancellation(attachOperation, commandContext, "debugger attachment"),
      DEBUGGER_LIFECYCLE_TIMEOUT_MS,
      "debugger attachment",
      "DEBUGGER_ATTACH_TIMEOUT",
    );
  } catch (error) {
    controlEpoch += 1;
    const cleanupReason = error.code === "COMMAND_CANCELED"
      ? "debugger_attach_canceled"
      : "debugger_attach_outcome_unknown";
    await markUnknownAttachCleanup(tab.id, attachToken, cleanupReason);
    if (error.code === "DEBUGGER_ATTACH_TIMEOUT" || error.code === "COMMAND_CANCELED") {
      void attachOperation.then(
        () => settleUnknownDebuggerAttach(tab.id, attachToken, "late_debugger_attach_resolved"),
        () => settleUnknownDebuggerAttach(tab.id, attachToken, "late_debugger_attach_rejected"),
      ).catch(() => {});
    } else {
      await settleUnknownDebuggerAttach(tab.id, attachToken, "debugger_attach_rejected");
    }
    if (error.code === "DEBUGGER_ATTACH_TIMEOUT") {
      const unknown = new Error("DEBUGGER_ATTACH_OUTCOME_UNKNOWN: Chrome did not acknowledge attachment; late attachment will be detached and explicit restart is required");
      unknown.code = "DEBUGGER_ATTACH_OUTCOME_UNKNOWN";
      unknown.cause = error;
      throw unknown;
    }
    throw error;
  }
  if (pendingDebuggerAttaches.get(tab.id) === attachToken) pendingDebuggerAttaches.delete(tab.id);
  let postAttachError = null;
  try {
    assertCommandActive(commandContext, "debugger attachment commit");
    assertHumanControlAvailable();
  } catch (error) {
    postAttachError = error;
  }
  if (controlEpoch !== attachEpoch || postAttachError) {
    await settleUnknownDebuggerAttach(tab.id, attachToken, postAttachError?.code === "HUMAN_CONTROL_PAUSED"
      ? "human_pause_after_debugger_attach"
      : "canceled_debugger_attach_cleanup");
    throw outcomeUnknownError("debugger attachment", postAttachError ?? new Error("browser control changed during attachment"));
  }
  const now = Date.now();
  controlEpoch += 1;
  controlLease = {
    sessionId: crypto.randomUUID(),
    ownerSessionId: requestedOwner,
    epoch: controlEpoch,
    tabId: tab.id,
    startedAt: now,
    expiresAt: now + boundedTtl,
    lastHeartbeatAt: now,
    reason,
    turn: 0,
    moveSequence: 0,
    documentEpoch: 1,
    loaderId: null,
    frameId: null,
    documentUrl: null,
    navigationReady: false,
    pendingNavigation: null,
    pendingDialog: null,
    policy: controlPolicy(controlConfig, tab),
    viewport: null,
    cursor: { x: 0, y: 0, visible: false, updatedAt: now },
  };
  const authority = captureLeaseAuthority();
  try {
    await debuggerCommand(tab.id, "Page.enable", {}, authority, commandContext);
    await debuggerCommand(tab.id, "Runtime.enable", {}, authority, commandContext);
    await initializeLeaseDocument(controlLease, tab, controlConfig, commandContext);
  } catch (error) {
    await stopControl("initialization_failed", { requireExplicitStart: true });
    throw error;
  }
  pendingControlCleanups.delete(tab.id);
  lastControlRevocation = null;
  try {
    await persistControlState();
  } catch (error) {
    await markUnknownAttachCleanup(tab.id, attachToken, "control_commit_persist_failed");
    await stopControl("control_commit_persist_failed", { requireExplicitStart: true });
    throw error;
  }
  assertLeaseAuthority(authority, commandContext, "control initialization commit");
  scheduleHeartbeat();
  await showControlUi();
  assertLeaseAuthority(authority, commandContext, "control start event");
  send({ type: "event", name: "browser.control.started", data: publicControlState() });
  return publicControlState();
}

async function requireControl(tabId, reason, commandContext = null) {
  assertCommandActive(commandContext, "control acquisition");
  await initializeControlState();
  assertHumanControlAvailable();
  await initializeProtocolIdentity();
  if (controlLease?.expiresAt <= Date.now()) {
    await stopControl("lease_expired", { requireExplicitStart: true });
  }
  const requestedOwner = currentControlOwner();
  if (controlLease && controlLease.ownerSessionId !== requestedOwner) {
    await stopControl("owner_session_changed", { requireExplicitStart: true });
    throw new Error("CONTROL_OWNER_MISMATCH: explicitly start a control session owned by the current server session");
  }
  if (controlLease?.tabId === tabId) {
    assertCommandActive(commandContext, "control reuse");
    return controlLease;
  }
  await startControl(tabId, { explicit: false, reason, ownerSessionId: requestedOwner, commandContext });
  assertCommandActive(commandContext, "control acquisition commit");
  return controlLease;
}

function assertControlBinding(params, lease = controlLease) {
  if (!lease) throw new Error("CONTROL_REQUIRED: start browser control before acting");
  if (params.controlSessionId !== undefined && params.controlSessionId !== lease.sessionId) {
    throw new Error("STALE_CONTROL_SESSION: the action belongs to an older browser control session");
  }
  if (params.turn !== undefined && Number(params.turn) !== lease.turn) {
    throw new Error("STALE_CONTROL_TURN: observe the current browser turn before acting");
  }
  if (params.moveSequence !== undefined && Number(params.moveSequence) !== lease.moveSequence) {
    throw new Error("STALE_MOVE_SEQUENCE: pointer state advanced; observe or query control status again");
  }
}

async function captureTab(tab, commandContext = null, existingAuthority = null) {
  await requireControl(tab.id, "page.observe", commandContext);
  const authority = existingAuthority ?? captureLeaseAuthority();
  assertLeaseAuthority(authority, commandContext, "screenshot preparation");
  await verifyDocumentAuthority(tab.id, authority, commandContext, "screenshot preparation");
  const captureId = crypto.randomUUID();
  const lease = controlLease;
  const activeCaptureIds = beginControlCapture(tab.id, captureId);
  try {
    let beginState;
    try {
      beginState = await contentRequest(
        tab.id,
        { method: "control.capture.begin", captureId, activeCaptureIds },
        { authority, commandContext },
      );
    } catch (error) {
      await failControlUiClosed(lease, "capture_begin", error);
    }
    if (!controlUiAcknowledged(beginState, activeCaptureIds, lease.cursor.visible)) {
      await failControlUiClosed(
        lease,
        "capture_begin",
        new Error("The page did not confirm that the control overlay was hidden"),
      );
    }
    assertLeaseAuthority(authority, commandContext, "screenshot capture");
    const capture = await debuggerCommand(tab.id, "Page.captureScreenshot", {
      format: "jpeg",
      quality: 78,
      fromSurface: true,
      captureBeyondViewport: false,
    }, authority, commandContext);
    if (!capture?.data) throw new Error("SCREENSHOT_FAILED: Chrome returned no screenshot data");
    await verifyDocumentAuthority(tab.id, authority, commandContext, "screenshot completion");
    return `data:image/jpeg;base64,${capture.data}`;
  } finally {
    const remainingCaptureIds = endControlCapture(tab.id, captureId);
    let restored = false;
    try {
      const endState = await contentRequest(tab.id, {
        method: "control.capture.end",
        captureId,
        activeCaptureIds: remainingCaptureIds,
        controlSessionId: authority.sessionId,
        controlEpoch: authority.epoch,
      }, { authority, commandContext });
      restored = controlUiAcknowledged(endState, remainingCaptureIds, lease.cursor.visible);
    } catch {
      restored = false;
    }
    const leaseStillActive = controlLease?.sessionId === authority.sessionId
      && controlLease?.epoch === authority.epoch;
    if (leaseStillActive && !restored) {
      try {
        await showControlUi(controlLease);
        restored = true;
      } catch (error) {
        await failControlUiClosed(lease, "capture_restore", error);
      }
    }
  }
}

async function debuggerCommand(tabId, method, params, authority = null, commandContext = null) {
  if (authority) assertLeaseAuthority(authority, commandContext, `CDP ${method} dispatch`);
  else assertCommandActive(commandContext, `CDP ${method} dispatch`);
  const operation = chrome.debugger.sendCommand({ tabId }, method, params);
  operation.catch(() => {});
  let result;
  try {
    result = await withTimeout(operation, DEBUGGER_TIMEOUT_MS, method);
  } catch (error) {
    if (error.code === "DEBUGGER_TIMEOUT") {
      // A pending dialog freezes the renderer, so a CDP timeout under it
      // belongs to the dialog: keep the lease and report the dialog instead.
      if (controlLease?.pendingDialog) throw pendingDialogError(`CDP ${method}`);
      await stopControl(`cdp_timeout:${method}`, { requireExplicitStart: true });
      const unknown = new Error(`CDP_OUTCOME_UNKNOWN: ${method} timed out; control was revoked and automatic retry is unsafe`);
      unknown.code = "CDP_OUTCOME_UNKNOWN";
      unknown.cause = error;
      throw unknown;
    }
    throw error;
  }
  if (authority) assertLeaseAuthorityAfterDispatch(authority, commandContext, `CDP ${method}`);
  else assertCommandActive(commandContext, `CDP ${method} completion`);
  return result;
}

const POINTER_CANDIDATE_COUNT = 20;
const POINTER_PRESENTATION_TIMEOUT_MS = 1_000;

function deterministicUnit(seed, index) {
  let value = (Math.imul((seed + 1) >>> 0, 0x9e3779b1) + Math.imul(index + 1, 0x85ebca6b)) >>> 0;
  value ^= value >>> 16;
  value = Math.imul(value, 0x7feb352d) >>> 0;
  value ^= value >>> 15;
  value = Math.imul(value, 0x846ca68b) >>> 0;
  value ^= value >>> 16;
  return value / 0x1_0000_0000;
}

function cubicPoint(start, first, second, target, progress) {
  const inverse = 1 - progress;
  return {
    x: inverse ** 3 * start.x + 3 * inverse ** 2 * progress * first.x + 3 * inverse * progress ** 2 * second.x + progress ** 3 * target.x,
    y: inverse ** 3 * start.y + 3 * inverse ** 2 * progress * first.y + 3 * inverse * progress ** 2 * second.y + progress ** 3 * target.y,
  };
}

function scorePointerCandidate(candidate, start, target, width, height, directDistance) {
  const samples = [start];
  for (let index = 1; index <= 32; index += 1) samples.push(cubicPoint(start, candidate.first, candidate.second, target, index / 32));
  let length = 0;
  let reversePenalty = 0;
  let boundaryPenalty = 0;
  let curvaturePenalty = 0;
  const direction = { x: (target.x - start.x) / directDistance, y: (target.y - start.y) / directDistance };
  let previousSegment = null;
  for (let index = 1; index < samples.length; index += 1) {
    const previous = samples[index - 1];
    const point = samples[index];
    const segment = { x: point.x - previous.x, y: point.y - previous.y };
    const segmentLength = Math.hypot(segment.x, segment.y);
    length += segmentLength;
    const forward = segment.x * direction.x + segment.y * direction.y;
    if (forward < 0) reversePenalty += -forward;
    if (index < samples.length - 1) {
      const clearance = Math.min(point.x, point.y, width - 1 - point.x, height - 1 - point.y);
      if (clearance < 6) boundaryPenalty += (6 - clearance) ** 2;
    }
    if (previousSegment && segmentLength > 0 && previousSegment.length > 0) {
      const cosine = clamp(
        (segment.x * previousSegment.x + segment.y * previousSegment.y) / (segmentLength * previousSegment.length),
        -1,
        1,
      );
      curvaturePenalty += Math.acos(cosine) ** 2;
    }
    previousSegment = { ...segment, length: segmentLength };
  }
  const lengthRatio = length / directDistance;
  const lengthPenalty = Math.abs(lengthRatio - 1.035) * 42 + Math.max(0, lengthRatio - 1.2) * 180;
  return lengthPenalty + curvaturePenalty * 22 + reversePenalty * 80 + boundaryPenalty * 14;
}

function boundedBezierSpringPath(start, target, viewport, moveSequence) {
  const width = Math.max(1, Number(viewport?.width) || Math.max(start.x, target.x) + 32);
  const height = Math.max(1, Number(viewport?.height) || Math.max(start.y, target.y) + 32);
  const dx = target.x - start.x;
  const dy = target.y - start.y;
  const distance = Math.hypot(dx, dy);
  if (distance < 1) return { points: [{ ...target }], durationMs: 0, distance: 0, candidateCount: 1, score: 0 };
  const direction = { x: dx / distance, y: dy / distance };
  const perpendicular = { x: -direction.y, y: direction.x };
  const candidates = [];
  for (let index = 0; index < POINTER_CANDIDATE_COUNT; index += 1) {
    const sign = index % 2 === 0 ? 1 : -1;
    const bendScale = index === 0 ? 0 : 0.025 + deterministicUnit(moveSequence, index * 5) * 0.13;
    const bend = clamp(distance * bendScale, 0, 64) * sign;
    const firstProgress = 0.22 + deterministicUnit(moveSequence, index * 5 + 1) * 0.2;
    const secondProgress = 0.62 + deterministicUnit(moveSequence, index * 5 + 2) * 0.22;
    const firstBend = bend * (0.7 + deterministicUnit(moveSequence, index * 5 + 3) * 0.45);
    const secondBend = bend * (0.15 + deterministicUnit(moveSequence, index * 5 + 4) * 0.55);
    const first = {
      x: clamp(start.x + dx * firstProgress + perpendicular.x * firstBend, 0, width - 1),
      y: clamp(start.y + dy * firstProgress + perpendicular.y * firstBend, 0, height - 1),
    };
    const second = {
      x: clamp(start.x + dx * secondProgress + perpendicular.x * secondBend, 0, width - 1),
      y: clamp(start.y + dy * secondProgress + perpendicular.y * secondBend, 0, height - 1),
    };
    const candidate = { first, second, index };
    candidate.score = scorePointerCandidate(candidate, start, target, width, height, distance);
    candidates.push(candidate);
  }
  candidates.sort((left, right) => left.score - right.score || left.index - right.index);
  const selected = candidates[0];
  const durationMs = clamp(90 + distance * 0.28, 110, 420);
  const steps = clamp(Math.round(durationMs / 16), 8, 26);
  const springRate = 7.5;
  const springEnd = 1 - (1 + springRate) * Math.exp(-springRate);
  const points = [];
  for (let index = 1; index <= steps; index += 1) {
    const time = index / steps;
    const minimumJerk = 10 * time ** 3 - 15 * time ** 4 + 6 * time ** 5;
    const springArrival = clamp((1 - (1 + springRate * time) * Math.exp(-springRate * time)) / springEnd, 0, 1);
    const progress = clamp(minimumJerk * 0.78 + springArrival * 0.22, 0, 1);
    const point = cubicPoint(start, selected.first, selected.second, target, progress);
    points.push({ x: clamp(point.x, 0, width - 1), y: clamp(point.y, 0, height - 1) });
  }
  points[points.length - 1] = { x: target.x, y: target.y };
  return {
    points,
    durationMs: Math.round(durationMs),
    distance: Math.round(distance),
    candidateCount: POINTER_CANDIDATE_COUNT,
    score: Number(selected.score.toFixed(4)),
  };
}

function pause(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function pointerPresentationState(tabId, authority, commandContext) {
  const read = async (operation, label) => {
    operation.catch(() => {});
    return withTimeout(
      withCommandCancellation(operation, commandContext, label),
      POINTER_PRESENTATION_TIMEOUT_MS,
      label,
      "POINTER_PRESENTATION_TIMEOUT",
    );
  };
  try {
    assertLeaseAuthority(authority, commandContext, "pointer presentation lookup");
    const tab = await read(chrome.tabs.get(tabId), "target tab presentation lookup");
    assertLeaseAuthority(authority, commandContext, "target tab presentation lookup");
    if (!Number.isInteger(tab?.windowId)) {
      return {
        animate: false,
        tabActive: Boolean(tab?.active),
        windowFocused: false,
        windowState: "unknown",
        reason: "window_unavailable",
      };
    }
    const targetWindow = await read(chrome.windows.get(tab.windowId), "target window presentation lookup");
    assertLeaseAuthority(authority, commandContext, "target window presentation lookup");
    const confirmedTab = await read(chrome.tabs.get(tabId), "target tab presentation confirmation");
    assertLeaseAuthority(authority, commandContext, "target tab presentation confirmation");
    if (confirmedTab?.id !== tab.id
      || confirmedTab?.windowId !== tab.windowId
      || confirmedTab?.active !== tab.active) {
      return {
        animate: false,
        tabActive: Boolean(confirmedTab?.active),
        windowFocused: false,
        windowState: "unknown",
        reason: "tab_presentation_changed",
      };
    }
    const confirmedWindow = await read(chrome.windows.get(tab.windowId), "target window presentation confirmation");
    assertLeaseAuthority(authority, commandContext, "target window presentation confirmation");
    if (confirmedWindow?.id !== targetWindow?.id
      || confirmedWindow?.focused !== targetWindow?.focused
      || confirmedWindow?.state !== targetWindow?.state) {
      return {
        animate: false,
        tabActive: Boolean(confirmedTab.active),
        windowFocused: false,
        windowState: "unknown",
        reason: "window_presentation_changed",
      };
    }
    const tabActive = confirmedTab.active === true;
    const windowFocused = confirmedWindow.focused === true;
    const windowState = String(confirmedWindow.state || "unknown");
    const animate = tabActive && windowFocused && windowState === "normal";
    return {
      animate,
      tabActive,
      windowFocused,
      windowState,
      reason: animate
        ? "foreground_visible"
        : !tabActive
          ? "tab_inactive"
          : !windowFocused
            ? "window_unfocused"
            : windowState !== "normal"
              ? `window_${windowState}`
              : "window_unavailable",
    };
  } catch (error) {
    if (["COMMAND_CANCELED", "CONTROL_CANCELED", "DOCUMENT_CHANGED"].includes(error.code)) throw error;
    assertLeaseAuthority(authority, commandContext, "pointer presentation fallback");
    return {
      animate: false,
      tabActive: false,
      windowFocused: false,
      windowState: "unknown",
      reason: error.code === "POINTER_PRESENTATION_TIMEOUT" ? "presentation_timeout" : "presentation_unavailable",
    };
  }
}

function assertPointerArrival(motion) {
  if (!motion || !["arrived", "skipped_background"].includes(motion.arrival)) {
    throw new Error("POINTER_NOT_ARRIVED: trusted input requires an acknowledged final pointer position");
  }
}

async function moveVirtualCursor(tabId, targetX, targetY, commandContext = null, existingAuthority = null) {
  const lease = await requireControl(tabId, "trusted_pointer_input", commandContext);
  const authority = existingAuthority ?? captureLeaseAuthority(lease);
  assertLeaseAuthority(authority, commandContext, "pointer movement preparation");
  const viewport = lease.viewport ?? { width: Math.max(targetX + 32, 1), height: Math.max(targetY + 32, 1) };
  const target = {
    x: clamp(Number(targetX), 0, Math.max(0, Number(viewport.width) - 1)),
    y: clamp(Number(targetY), 0, Math.max(0, Number(viewport.height) - 1)),
  };
  const start = lease.cursor.visible
    ? { x: lease.cursor.x, y: lease.cursor.y }
    : {
        x: clamp(target.x - Math.min(72, Math.max(24, Number(viewport.width) * 0.08)), 0, Math.max(0, Number(viewport.width) - 1)),
        y: clamp(target.y - Math.min(48, Math.max(18, Number(viewport.height) * 0.06)), 0, Math.max(0, Number(viewport.height) - 1)),
      };
  lease.moveSequence += 1;
  const sequence = lease.moveSequence;
  const motion = boundedBezierSpringPath(start, target, viewport, sequence);
  let presentation = await pointerPresentationState(tabId, authority, commandContext);
  const frameDelay = Math.max(4, Math.round(motion.durationMs / motion.points.length));
  let dispatchedPointCount = 0;
  let completedFramePauses = 0;
  let skippedAnimation = !presentation.animate;
  const dispatchPointerPoint = async (point) => {
    assertLeaseAuthority(authority, commandContext, "pointer movement frame");
    await debuggerCommand(
      tabId,
      "Input.dispatchMouseEvent",
      { type: "mouseMoved", x: point.x, y: point.y },
      authority,
      commandContext,
    );
    dispatchedPointCount += 1;
    lease.cursor = { x: point.x, y: point.y, visible: true, updatedAt: Date.now() };
    await contentRequest(tabId, {
      method: "control.cursor",
      cursor: { ...lease.cursor, turn: lease.turn, moveSequence: sequence },
      expiresAt: lease.expiresAt,
    }, { authority, commandContext });
  };
  try {
    if (!presentation.animate) {
      await dispatchPointerPoint(target);
    } else {
      for (let index = 0; index < motion.points.length; index += 1) {
        if (index > 0) {
          presentation = await pointerPresentationState(tabId, authority, commandContext);
          if (!presentation.animate) {
            skippedAnimation = true;
            await dispatchPointerPoint(target);
            break;
          }
        }
        await dispatchPointerPoint(motion.points[index]);
        if (index === motion.points.length - 1) break;
        presentation = await pointerPresentationState(tabId, authority, commandContext);
        if (!presentation.animate) {
          skippedAnimation = true;
          await dispatchPointerPoint(target);
          break;
        }
        await pause(frameDelay);
        completedFramePauses += 1;
      }
    }
    await persistControlState();
    assertLeaseAuthority(authority, commandContext, "pointer movement completion");
  } catch (error) {
    if (dispatchedPointCount > 0
      && !["ACTION_OUTCOME_UNKNOWN", "CDP_OUTCOME_UNKNOWN"].includes(error.code)) {
      throw outcomeUnknownError("pointer movement", error);
    }
    throw error;
  }
  return {
    moveSequence: sequence,
    arrival: skippedAnimation ? "skipped_background" : "arrived",
    status: skippedAnimation ? "skipped_background" : "arrived",
    durationMs: skippedAnimation ? completedFramePauses * frameDelay : motion.durationMs,
    distance: motion.distance,
    points: dispatchedPointCount,
    plannedPoints: motion.points.length,
    candidateCount: motion.candidateCount,
    pathScore: motion.score,
    profile: skippedAnimation ? "background-final-arrival" : "bounded-cubic-minimum-jerk-spring",
    presentation,
  };
}

const POINTER_MODIFIER_BITS = { Alt: 1, Control: 2, Meta: 4, Shift: 8 };

function pointerModifierMask(modifiers) {
  let mask = 0;
  for (const modifier of modifiers) {
    const bit = POINTER_MODIFIER_BITS[modifier];
    if (!bit) throw new Error("BAD_MODIFIER: click modifiers must be a subset of Shift, Control, Alt, and Meta");
    mask |= bit;
  }
  return mask;
}

async function trustedClick(tabId, description, ref, generation, pointer, commandContext = null, existingAuthority = null) {
  const button = String(pointer?.button ?? "left");
  const clickCount = Number(pointer?.clickCount) || 1;
  const modifiers = Number(pointer?.modifiers) || 0;
  if (!["left", "middle", "right"].includes(button)) throw new Error("BAD_BUTTON: button must be left, middle, or right");
  if (!Number.isInteger(clickCount) || clickCount < 1 || clickCount > 3) throw new Error("BAD_CLICK_COUNT: clickCount must be an integer from 1 to 3");
  const authority = existingAuthority ?? captureLeaseAuthority();
  await verifyDocumentAuthority(tabId, authority, commandContext, "trusted click preparation");
  const x = description.bounds.x + description.bounds.width / 2;
  const y = description.bounds.y + description.bounds.height / 2;
  const motion = await moveVirtualCursor(tabId, x, y, commandContext, authority);
  assertPointerArrival(motion);
  assertLeaseAuthority(authority, commandContext, "click target commit");
  await contentRequest(
    tabId,
    { method: "commitClick", ref, generation, proof: description.proof },
    { authority, commandContext },
  );
  await verifyDocumentAuthority(tabId, authority, commandContext, "trusted click commit");
  const heldKey = crypto.randomUUID();
  const held = {
    tabId,
    sessionId: authority.sessionId,
    epoch: authority.epoch,
    releaseMethod: "Input.dispatchMouseEvent",
    releaseParams: { type: "mouseReleased", x, y, button, clickCount, modifiers },
  };
  await persistHeldInputIntent(heldMouseInputs, heldKey, held);
  try {
    await verifyDocumentAuthority(tabId, authority, commandContext, "mouse press");
    assertLeaseAuthority(authority, commandContext, "mouse press");
    await debuggerCommand(
      tabId,
      "Input.dispatchMouseEvent",
      { type: "mousePressed", x, y, button, clickCount, modifiers },
      authority,
      commandContext,
    );
    await verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, "mouse release");
    assertLeaseAuthority(authority, commandContext, "mouse release");
    await debuggerCommand(
      tabId,
      "Input.dispatchMouseEvent",
      { type: "mouseReleased", x, y, button, clickCount, modifiers },
      authority,
      commandContext,
    );
    await clearHeldInputIntent(heldMouseInputs, heldKey, held);
  } finally {
    await releaseHeldMouseInput(heldKey, held);
  }
  await verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, "trusted click completion");
  return { clicked: true, trusted: true, button, clickCount, modifiers, control: publicControlState(), motion };
}

async function trustedHover(tabId, description, ref, generation, commandContext = null, existingAuthority = null) {
  const authority = existingAuthority ?? captureLeaseAuthority();
  await verifyDocumentAuthority(tabId, authority, commandContext, "trusted hover preparation");
  const x = description.bounds.x + description.bounds.width / 2;
  const y = description.bounds.y + description.bounds.height / 2;
  // Hover is pointer arrival only: the trusted cursor settles on the target
  // center and no press or release event is ever dispatched.
  const motion = await moveVirtualCursor(tabId, x, y, commandContext, authority);
  assertPointerArrival(motion);
  await verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, "trusted hover completion");
  return { hovered: true, trusted: true, ref, generation, x, y, control: publicControlState(), motion };
}

async function trustedClickAt(tabId, x, y, button = "left", clickCount = 1, targetProof, commandContext = null, existingAuthority = null) {
  if (!["left", "middle", "right"].includes(button)) throw new Error("BAD_BUTTON: button must be left, middle, or right");
  if (!Number.isInteger(clickCount) || clickCount < 1 || clickCount > 3) throw new Error("BAD_CLICK_COUNT: clickCount must be an integer from 1 to 3");
  const authority = existingAuthority ?? captureLeaseAuthority();
  await verifyDocumentAuthority(tabId, authority, commandContext, "trusted point click preparation");
  const motion = await moveVirtualCursor(tabId, x, y, commandContext, authority);
  assertPointerArrival(motion);
  assertLeaseAuthority(authority, commandContext, "point target commit");
  await contentRequest(
    tabId,
    { method: "commitPoint", x, y, generation: targetProof.generation, token: targetProof.token, proof: targetProof.proof },
    { authority, commandContext },
  );
  await verifyDocumentAuthority(tabId, authority, commandContext, "trusted point click commit");
  const heldKey = crypto.randomUUID();
  const held = {
    tabId,
    sessionId: authority.sessionId,
    epoch: authority.epoch,
    releaseMethod: "Input.dispatchMouseEvent",
    releaseParams: { type: "mouseReleased", x, y, button, clickCount },
  };
  await persistHeldInputIntent(heldMouseInputs, heldKey, held);
  try {
    await verifyDocumentAuthority(tabId, authority, commandContext, "point mouse press");
    assertLeaseAuthority(authority, commandContext, "mouse press");
    await debuggerCommand(
      tabId,
      "Input.dispatchMouseEvent",
      { type: "mousePressed", x, y, button, clickCount },
      authority,
      commandContext,
    );
    await verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, "point mouse release");
    assertLeaseAuthority(authority, commandContext, "mouse release");
    await debuggerCommand(
      tabId,
      "Input.dispatchMouseEvent",
      { type: "mouseReleased", x, y, button, clickCount },
      authority,
      commandContext,
    );
    await clearHeldInputIntent(heldMouseInputs, heldKey, held);
  } finally {
    await releaseHeldMouseInput(heldKey, held);
  }
  await verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, "trusted point click completion");
  return { clicked: true, trusted: true, x, y, button, clickCount, control: publicControlState(), motion };
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

async function trustedKey(tabId, chord, fullAccess, commandContext = null, existingAuthority = null) {
  if (!fullAccess && !allowedKey(chord)) throw new Error("BAD_KEY: key is not allowlisted in Safe mode");
  await requireControl(tabId, "page.key", commandContext);
  const authority = existingAuthority ?? captureLeaseAuthority();
  const { key, code, keyCode, modifiers } = parseKeyChord(chord);
  const params = { key, code, modifiers, windowsVirtualKeyCode: keyCode, nativeVirtualKeyCode: keyCode };
  const heldKey = crypto.randomUUID();
  const held = {
    tabId,
    sessionId: authority.sessionId,
    epoch: authority.epoch,
    releaseMethod: "Input.dispatchKeyEvent",
    releaseParams: { type: "keyUp", ...params },
  };
  await verifyDocumentAuthority(tabId, authority, commandContext, "key down");
  await persistHeldInputIntent(heldKeyInputs, heldKey, held);
  try {
    await verifyDocumentAuthority(tabId, authority, commandContext, "key down dispatch");
    assertLeaseAuthority(authority, commandContext, "key down");
    await debuggerCommand(tabId, "Input.dispatchKeyEvent", { type: "rawKeyDown", ...params }, authority, commandContext);
    await verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, "key up");
    assertLeaseAuthority(authority, commandContext, "key up");
    await debuggerCommand(tabId, "Input.dispatchKeyEvent", { type: "keyUp", ...params }, authority, commandContext);
    await clearHeldInputIntent(heldKeyInputs, heldKey, held);
  } finally {
    await releaseHeldKeyInput(heldKey, held);
  }
  await verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, "key completion");
  return { pressed: chord, control: publicControlState() };
}

async function insertText(tabId, value, commandContext = null, existingAuthority = null) {
  await requireControl(tabId, "page.typeText", commandContext);
  const authority = existingAuthority ?? captureLeaseAuthority();
  await verifyDocumentAuthority(tabId, authority, commandContext, "text insertion");
  await debuggerCommand(tabId, "Input.insertText", { text: value }, authority, commandContext);
  await verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, "text insertion completion");
  return { typed: true, length: value.length, control: publicControlState() };
}

async function evaluateJavaScript(tabId, expression, commandContext = null, existingAuthority = null) {
  await requireControl(tabId, "page.evaluate", commandContext);
  const authority = existingAuthority ?? captureLeaseAuthority();
  await verifyDocumentAuthority(tabId, authority, commandContext, "JavaScript evaluation");
  const response = await debuggerCommand(tabId, "Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
    userGesture: true,
    timeout: 12_000,
  }, authority, commandContext);
  await verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, "JavaScript evaluation completion");
  if (response.exceptionDetails) {
    const detail = response.exceptionDetails.exception?.description || response.exceptionDetails.text || "JavaScript evaluation failed";
    throw new Error(`EVALUATION_FAILED: ${detail}`);
  }
  return {
    type: response.result?.type ?? "undefined",
    value: Object.hasOwn(response.result ?? {}, "value")
      ? response.result.value
      : (response.result?.unserializableValue ?? response.result?.description),
    control: publicControlState(),
  };
}

async function queueApproval(method, params, tabId, description, risk, commandContext = null) {
  const pending = {
    id: crypto.randomUUID(), method, params, tabId, ref: params.ref, label: description.name || description.role,
    risk, createdAt: Date.now(), expiresAt: Date.now() + 120_000,
  };
  await commandSideEffect(commandContext, "approval creation", () => chrome.storage.local.set({ pendingApproval: pending }));
  await commandSideEffect(commandContext, "approval badge color", () => chrome.action.setBadgeBackgroundColor({ color: "#f3bd4e" }));
  await commandSideEffect(commandContext, "approval badge text", () => chrome.action.setBadgeText({ text: "?" }));
  return { status: "approval_required", approvalId: pending.id, risk, label: pending.label, expiresAt: pending.expiresAt };
}

// Runs the sanitized page.batch sub-actions strictly in order, delegating
// every step to dispatchAction so each method re-runs its full existing
// pre-dispatch proof against the same generation. Freshness stays strict: a
// step that invalidates the snapshot (fill, select, scroll, or any page
// reaction to an earlier step) makes the next snapshot-bound step fail
// STALE_SNAPSHOT, the loop stops at the first failure, and no later step is
// ever dispatched.
async function runBatchActions(actions, dispatchAction) {
  const perStep = [];
  let completed = 0;
  let failedIndex = null;
  let failedError = null;
  for (let index = 0; index < actions.length; index += 1) {
    const action = actions[index] ?? {};
    const method = String(action.method ?? "");
    // A dialog opened by an earlier step freezes the renderer, so the next
    // step would only dispatch into a frozen page; the batch aborts at this
    // exact index instead.
    if (controlLease?.pendingDialog) {
      failedIndex = index;
      failedError = "BLOCKED_BY_DIALOG: a JavaScript dialog opened mid-batch; resolve it with page.handleDialog before continuing";
      perStep.push({ method, ok: false, error: failedError });
      break;
    }
    if (!BATCH_SUBMETHODS.has(method)) {
      failedIndex = index;
      failedError = "BAD_REQUEST: page.batch sub-actions may only use page.click, page.fill, page.select, page.key, or page.scroll";
      perStep.push({ method, ok: false, error: failedError });
      break;
    }
    const subParams = { ...action };
    delete subParams.method;
    try {
      await dispatchAction(method, subParams);
    } catch (error) {
      failedIndex = index;
      failedError = String(error?.message ?? error ?? "COMMAND_FAILED");
      perStep.push({ method, ok: false, error: failedError });
      break;
    }
    completed += 1;
    perStep.push({ method, ok: true });
  }
  const result = { completed, total: actions.length, perStep };
  if (failedIndex !== null) {
    result.failedIndex = failedIndex;
    result.failedError = failedError;
  }
  return result;
}

async function groupBridgeCreatedTab(tab, commandContext = null) {
  await initializeControlState();
  if (!Number.isInteger(tab?.id)) throw new Error("BAD_TAB: Chrome did not return a new tab identifier");
  const groupId = await commandSideEffect(commandContext, "tab grouping", () => chrome.tabs.group({ tabIds: tab.id }));
  await commandSideEffect(commandContext, "tab group labeling", () => chrome.tabGroups.update(groupId, {
    title: "Local Browser Bridge",
    color: "green",
    collapsed: false,
  }));
  assertCommandActive(commandContext, "bridge-created tab registration");
  bridgeCreatedTabs.add(tab.id);
  await persistControlState();
  assertCommandActive(commandContext, "bridge-created tab registration commit");
  return groupId;
}

async function dispatch(method, params, approved, commandContext = null, { batched = false } = {}) {
  assertCommandActive(commandContext, "command validation");
  if (!COMMANDS.has(method)) throw new Error("UNKNOWN_COMMAND: method is not supported");
  await initializeControlState();
  if (!PAUSE_ALLOWED_COMMANDS.has(method)) {
    assertHumanControlAvailable();
  }
  // A pending JavaScript dialog freezes the renderer main thread, so any
  // renderer-touching command fails fast here, before any content or CDP
  // dispatch could time out and revoke the lease.
  assertNoPendingDialog(method);
  switch (method) {
    case "status": {
      await initializeControlState();
      const config = await settings();
      assertCommandActive(commandContext, "status response");
      return { connected: socket?.readyState === WebSocket.OPEN, enabled: config.enabled, fullAccess: config.fullAccess, allowedHosts: config.allowedHosts, control: publicControlState() };
    }
    case "browser.control.start": {
      return startControl(params.tabId, {
        ttlMs: params.ttlMs,
        explicit: true,
        commandContext,
      });
    }
    case "browser.control.status": {
      await initializeControlState();
      assertCommandActive(commandContext, "control status");
      if (controlLease?.expiresAt <= Date.now()) {
        await stopControl("lease_expired", { requireExplicitStart: true });
      }
      return publicControlState();
    }
    case "browser.control.stop": {
      await initializeControlState();
      assertCommandActive(commandContext, "control release");
      if (params.sessionId && controlLease && params.sessionId !== controlLease.sessionId) {
        throw new Error("STALE_CONTROL_SESSION: the requested browser control session is no longer active");
      }
      return stopControl("released_by_client", { requireExplicitStart: true });
    }
    case "tabs.list": {
      await initializeControlState();
      const config = await settings();
      const tabs = await chrome.tabs.query({});
      assertCommandActive(commandContext, "tab listing");
      const allowedTabs = tabs.filter((tab) => allowedTabVerdict(tab, config).allowed);
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
      await commandSideEffect(commandContext, "tab activation", () => chrome.tabs.update(tab.id, { active: true }));
      await commandSideEffect(commandContext, "window focus", () => chrome.windows.update(tab.windowId, { focused: true }));
      return { tabId: tab.id, active: true };
    }
    case "tabs.new": {
      const tab = await commandSideEffect(
        commandContext,
        "tab creation",
        () => chrome.tabs.create({ url: "about:blank", active: true }),
      );
      const groupId = await groupBridgeCreatedTab(tab, commandContext);
      return { tabId: tab.id, groupId, bridgeCreated: true };
    }
    case "tabs.close": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const config = await settings();
      if (!config.fullAccess && !approved) {
        return queueApproval(
          method,
          params,
          tab.id,
          { name: tab.title || "Untitled tab", role: "tab" },
          "close a browser tab",
          commandContext,
        );
      }
      await commandSideEffect(commandContext, "tab close", () => chrome.tabs.remove(tab.id));
      return { closed: true, tabId: params.tabId };
    }
    case "page.navigate": {
      const tab = await getTab(params.tabId);
      const config = await settings();
      let destination;
      try { destination = new URL(String(params.url)); } catch { throw new Error("BAD_URL: enter a complete HTTP or HTTPS URL"); }
      const verdict = isUrlAllowed(destination.href, config.allowedHosts, config.port, config.fullAccess);
      if (!verdict.allowed) throw new Error(`SITE_BLOCKED: ${verdict.reason}`);
      await requireControl(tab.id, "page.navigate", commandContext);
      assertControlBinding(params);
      const initialAuthority = captureLeaseAuthority();
      const authority = await authorizeTopLevelNavigation(
        controlLease,
        initialAuthority,
        "navigate",
        verdict.url,
        commandContext,
      );
      await leaseSideEffect(
        authority,
        commandContext,
        "page navigation",
        () => chrome.tabs.update(tab.id, { url: verdict.url, active: true }),
      );
      return { tabId: tab.id, url: safeUrlForDisplay(verdict.url), control: publicControlState() };
    }
    case "page.back": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await requireControl(tab.id, "page.back", commandContext);
      assertControlBinding(params);
      const initialAuthority = captureLeaseAuthority();
      const history = await debuggerCommand(tab.id, "Page.getNavigationHistory", {}, initialAuthority, commandContext);
      const expectedUrl = history.entries?.[history.currentIndex - 1]?.url;
      if (!expectedUrl) throw new Error("NO_NAVIGATION_ENTRY: there is no previous document");
      const authority = await authorizeTopLevelNavigation(controlLease, initialAuthority, "back", expectedUrl, commandContext);
      await leaseSideEffect(authority, commandContext, "back navigation", () => chrome.tabs.goBack(tab.id));
      return { navigated: "back", control: publicControlState() };
    }
    case "page.forward": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await requireControl(tab.id, "page.forward", commandContext);
      assertControlBinding(params);
      const initialAuthority = captureLeaseAuthority();
      const history = await debuggerCommand(tab.id, "Page.getNavigationHistory", {}, initialAuthority, commandContext);
      const expectedUrl = history.entries?.[history.currentIndex + 1]?.url;
      if (!expectedUrl) throw new Error("NO_NAVIGATION_ENTRY: there is no next document");
      const authority = await authorizeTopLevelNavigation(controlLease, initialAuthority, "forward", expectedUrl, commandContext);
      await leaseSideEffect(authority, commandContext, "forward navigation", () => chrome.tabs.goForward(tab.id));
      return { navigated: "forward", control: publicControlState() };
    }
    case "page.reload": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await requireControl(tab.id, "page.reload", commandContext);
      assertControlBinding(params);
      const initialAuthority = captureLeaseAuthority();
      const authority = await authorizeTopLevelNavigation(
        controlLease,
        initialAuthority,
        "reload",
        controlLease.documentUrl,
        commandContext,
      );
      await leaseSideEffect(authority, commandContext, "page reload", () => chrome.tabs.reload(tab.id));
      return { reloaded: true, control: publicControlState() };
    }
    case "page.observe": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const lease = await requireControl(tab.id, "page.observe", commandContext);
      const authority = captureLeaseAuthority(lease);
      assertLeaseAuthority(authority, commandContext, "observation turn");
      await verifyDocumentAuthority(tab.id, authority, commandContext, "observation snapshot");
      lease.turn += 1;
      await showControlUi(lease);
      assertLeaseAuthority(authority, commandContext, "page snapshot");
      const snapshot = await contentRequest(
        tab.id,
        { method: "snapshot" },
        { authority, commandContext },
      );
      await verifyDocumentAuthority(tab.id, authority, commandContext, "observation snapshot completion");
      lease.viewport = snapshot.viewport;
      await persistControlState();
      assertLeaseAuthority(authority, commandContext, "screenshot preparation");
      const screenshot = await captureTab(tab, commandContext, authority);
      for (const element of snapshot.elements ?? []) element.risk = classifyRisk(element);
      return { snapshot, screenshot, control: publicControlState() };
    }
    case "page.waitFor": {
      // Read-only condition wait: the same tab permission checks as
      // page.observe, but without a control lease, snapshot binding, or
      // any trusted input.
      await initializeControlState();
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const timeoutMs = clamp(Number(params.timeoutMs) || WAIT_TIMEOUT_DEFAULT_MS, WAIT_TIMEOUT_MIN_MS, WAIT_TIMEOUT_MAX_MS);
      const payload = { method: "wait", timeoutMs };
      for (const name of ["text", "textGone", "urlPrefix"]) {
        if (typeof params[name] === "string") payload[name] = params[name];
      }
      if (Number.isFinite(params.mutationQuietMs)) {
        payload.mutationQuietMs = clamp(Number(params.mutationQuietMs), WAIT_MUTATION_QUIET_MIN_MS, WAIT_MUTATION_QUIET_MAX_MS);
      }
      // The content call outlives the clamped wait but never the server's
      // 15s command deadline, and a timed-out wait is never re-dispatched.
      return contentRequest(tab.id, payload, {
        commandContext,
        timeoutMs: timeoutMs + WAIT_CONTENT_TIMEOUT_MARGIN_MS,
        retryAfterTimeout: false,
      });
    }
    case "page.click": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await requireControl(tab.id, "page.click", commandContext);
      assertControlBinding(params);
      const button = String(params.button ?? "left");
      const clickCount = Number(params.clickCount) || 1;
      const modifierNames = Array.isArray(params.modifiers) ? params.modifiers.map(String) : [];
      const modifierMask = pointerModifierMask(modifierNames);
      const authority = captureLeaseAuthority();
      await verifyDocumentAuthority(tab.id, authority, commandContext, "click target preparation");
      const description = await contentRequest(
        tab.id,
        { method: "prepareClick", ref: params.ref, generation: params.generation },
        { authority, commandContext },
      );
      await verifyDocumentAuthority(tab.id, authority, commandContext, "click target prepared");
      if (description.disabled) throw new Error("ELEMENT_DISABLED: target cannot be clicked");
      // Safe mode treats any non-default pointer verb like a risky click.
      const pointerRisk = button !== "left" || clickCount > 1 || modifierMask !== 0
        ? "perform a modified pointer click"
        : null;
      const risk = classifyRisk(description) ?? pointerRisk;
      const config = await settings();
      assertLeaseAuthority(authority, commandContext, "click authorization");
      if (!config.fullAccess && risk && !approved) {
        if (batched) {
          // A pending approval cannot be queued from inside page.batch: the
          // batch fails at this step and the human runs the risky click as
          // its own command instead.
          throw new Error(`APPROVAL_REQUIRED: ${risk}; run this step as its own command so a human can approve it`);
        }
        return queueApproval(method, params, tab.id, description, risk, commandContext);
      }
      return trustedClick(
        tab.id,
        description,
        params.ref,
        params.generation,
        { button, clickCount, modifiers: modifierMask },
        commandContext,
        authority,
      );
    }
    case "page.hover": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await requireControl(tab.id, "page.hover", commandContext);
      assertControlBinding(params);
      const authority = captureLeaseAuthority();
      await verifyDocumentAuthority(tab.id, authority, commandContext, "hover target preparation");
      const description = await contentRequest(
        tab.id,
        { method: "prepareClick", ref: params.ref, generation: params.generation },
        { authority, commandContext },
      );
      await verifyDocumentAuthority(tab.id, authority, commandContext, "hover target prepared");
      if (description.disabled) throw new Error("ELEMENT_DISABLED: target cannot be hovered");
      return trustedHover(tab.id, description, params.ref, params.generation, commandContext, authority);
    }
    case "page.fill": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await requireControl(tab.id, "page.fill", commandContext);
      assertControlBinding(params);
      const authority = captureLeaseAuthority();
      await verifyDocumentAuthority(tab.id, authority, commandContext, "fill target preparation");
      const description = await contentRequest(
        tab.id,
        { method: "describe", ref: params.ref, generation: params.generation },
        { authority, commandContext },
      );
      const config = await settings();
      if (!config.fullAccess && (isSensitiveField(description) || description.sensitive)) throw new Error("SENSITIVE_FIELD: enter passwords, payment data, and one-time codes manually");
      const result = await contentRequest(tab.id, {
        method: "fill", ref: params.ref, generation: params.generation, text: String(params.text ?? ""), allowSensitive: config.fullAccess,
      }, { authority, commandContext });
      await verifyDocumentAuthorityAfterDispatch(tab.id, authority, commandContext, "fill completion");
      return { ...result, control: publicControlState() };
    }
    case "page.select": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await requireControl(tab.id, "page.select", commandContext);
      assertControlBinding(params);
      const authority = captureLeaseAuthority();
      await verifyDocumentAuthority(tab.id, authority, commandContext, "select preparation");
      const description = await contentRequest(
        tab.id,
        { method: "describe", ref: params.ref, generation: params.generation },
        { authority, commandContext },
      );
      const config = await settings();
      if (!config.fullAccess && (isSensitiveField(description) || description.sensitive)) throw new Error("SENSITIVE_FIELD: select payment-card and authentication values manually");
      const result = await contentRequest(
        tab.id,
        {
          method: "select",
          ref: params.ref,
          generation: params.generation,
          value: String(params.value ?? ""),
          allowSensitive: config.fullAccess,
        },
        { authority, commandContext },
      );
      await verifyDocumentAuthorityAfterDispatch(tab.id, authority, commandContext, "select completion");
      return { ...result, control: publicControlState() };
    }
    case "page.key": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const config = await settings();
      await requireControl(tab.id, "page.key", commandContext);
      assertControlBinding(params);
      const authority = captureLeaseAuthority();
      await verifyDocumentAuthority(tab.id, authority, commandContext, "key target preparation");
      await contentRequest(
        tab.id,
        { method: "assertGeneration", generation: params.generation },
        { authority, commandContext },
      );
      return trustedKey(tab.id, String(params.key), config.fullAccess, commandContext, authority);
    }
    case "page.scroll": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await requireControl(tab.id, "page.scroll", commandContext);
      assertControlBinding(params);
      const authority = captureLeaseAuthority();
      await verifyDocumentAuthority(tab.id, authority, commandContext, "scroll preparation");
      const result = await contentRequest(
        tab.id,
        { method: "scroll", generation: params.generation, deltaX: Number(params.deltaX) || 0, deltaY: Number(params.deltaY) || 0 },
        { authority, commandContext },
      );
      await verifyDocumentAuthorityAfterDispatch(tab.id, authority, commandContext, "scroll completion");
      return { ...result, control: publicControlState() };
    }
    case "page.batch": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await requireControl(tab.id, "page.batch", commandContext);
      assertControlBinding(params);
      const actions = Array.isArray(params.actions) ? params.actions : [];
      if (actions.length < 1 || actions.length > BATCH_MAX_ACTIONS) {
        throw new Error(`BAD_REQUEST: page.batch needs between 1 and ${BATCH_MAX_ACTIONS} sub-actions`);
      }
      // Every sub-action re-enters dispatch so its full existing per-method
      // proof (tab policy, lease, freshness, sensitivity) runs unchanged;
      // batched mode only forbids queueing a pending approval mid-batch.
      const batch = await runBatchActions(
        actions,
        (subMethod, subParams) => dispatch(subMethod, subParams, false, commandContext, { batched: true }),
      );
      return { ...batch, control: publicControlState() };
    }
    case "page.handleDialog": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      await initializeControlState();
      if (controlLease?.tabId !== tab.id || !controlLease.pendingDialog) {
        throw new Error("NO_PENDING_DIALOG: no JavaScript dialog is open on the controlled tab");
      }
      await requireControl(tab.id, "page.handleDialog", commandContext);
      assertControlBinding(params);
      const authority = captureLeaseAuthority();
      const dialog = { ...controlLease.pendingDialog };
      const accept = params.accept === true;
      const handleParams = { accept };
      if (accept && dialog.hasPrompt && typeof params.promptText === "string") {
        handleParams.promptText = params.promptText.slice(0, DIALOG_PROMPT_TEXT_MAX_CHARS);
      }
      // No document identity precheck here: a beforeunload dialog
      // legitimately holds the document in a pending-navigation state, so
      // only the lease authority guards this CDP call.
      await debuggerCommand(tab.id, "Page.handleJavaScriptDialog", handleParams, authority, commandContext);
      if (controlLease?.sessionId === authority.sessionId && controlLease?.epoch === authority.epoch) {
        controlLease.pendingDialog = null;
        // The dialog was already handled, so a failed persistence must not
        // misreport the outcome; the dialogClosed event re-clears durably.
        await persistControlState().catch(() => {});
      }
      return { handled: true, accept, type: dialog.type, control: publicControlState() };
    }
    case "page.clickAt": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const config = await settings();
      if (!config.fullAccess) throw new Error("FULL_ACCESS_REQUIRED: enable Full Access in the extension popup");
      await requireControl(tab.id, "page.clickAt", commandContext);
      assertControlBinding(params);
      const authority = captureLeaseAuthority();
      await verifyDocumentAuthority(tab.id, authority, commandContext, "point target preparation");
      const x = Number(params.x);
      const y = Number(params.y);
      const prepared = await contentRequest(
        tab.id,
        { method: "preparePoint", generation: params.generation, x, y },
        { authority, commandContext },
      );
      await verifyDocumentAuthority(tab.id, authority, commandContext, "point target prepared");
      return trustedClickAt(
        tab.id,
        x,
        y,
        String(params.button ?? "left"),
        Number(params.clickCount) || 1,
        { ...prepared, generation: params.generation },
        commandContext,
        authority,
      );
    }
    case "page.typeText": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const config = await settings();
      if (!config.fullAccess) throw new Error("FULL_ACCESS_REQUIRED: enable Full Access in the extension popup");
      await requireControl(tab.id, "page.typeText", commandContext);
      assertControlBinding(params);
      const authority = captureLeaseAuthority();
      await verifyDocumentAuthority(tab.id, authority, commandContext, "text target preparation");
      await contentRequest(
        tab.id,
        { method: "assertGeneration", generation: params.generation },
        { authority, commandContext },
      );
      return insertText(tab.id, String(params.text ?? ""), commandContext, authority);
    }
    case "page.evaluate": {
      const tab = await getTab(params.tabId);
      await assertAllowedTab(tab);
      const config = await settings();
      if (!config.fullAccess) throw new Error("FULL_ACCESS_REQUIRED: enable Full Access in the extension popup");
      await requireControl(tab.id, "page.evaluate", commandContext);
      assertControlBinding(params);
      const authority = captureLeaseAuthority();
      await verifyDocumentAuthority(tab.id, authority, commandContext, "evaluation target preparation");
      return evaluateJavaScript(tab.id, String(params.expression ?? ""), commandContext, authority);
    }
    default:
      throw new Error("UNKNOWN_COMMAND");
  }
}

async function popupState() {
  await initializeControlState();
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
    currentTabId: Number.isInteger(tab?.id) ? tab.id : null,
    currentHostAllowed: currentHost ? isUrlAllowed(`https://${currentHost}/`, config.allowedHosts, config.port, config.fullAccess).allowed : false,
    pendingApproval: pending ? { id: pending.id, label: pending.label, risk: pending.risk, expiresAt: pending.expiresAt } : null,
    control: publicControlState(),
  };
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (sender.id === chrome.runtime.id && message?.type === "LBB_CONTROL_UI") {
    (async () => {
      await initializeControlState();
      if (!Number.isInteger(sender.tab?.id)) throw new Error("Invalid control UI request");
      if (message.action === "reconcile") {
        if (controlLease?.expiresAt <= Date.now()) await stopControl("lease_expired", { requireExplicitStart: true });
        return controlLease?.tabId === sender.tab.id ? publicControlState() : { active: false };
      }
      if (message.action !== "stop") throw new Error("Invalid control UI request");
      if (!controlLease || controlLease.tabId !== sender.tab.id) return publicControlState();
      return stopControl("released_by_user", { requireExplicitStart: true });
    })().then((result) => sendResponse({ ok: true, result })).catch((error) => sendResponse({ ok: false, error: error.message }));
    return true;
  }
  if (sender.id !== chrome.runtime.id || message?.type !== "LBB_POPUP") return undefined;
  (async () => {
    switch (message.action) {
      case "getState": return popupState();
      case "startControlCurrent": {
        const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
        if (!Number.isInteger(tab?.id)) throw new Error("The current tab is unavailable");
        await startControl(tab.id, { explicit: true, reason: "popup" });
        return popupState();
      }
      case "resumeRemoteControl": {
        if (sender.url !== chrome.runtime.getURL("popup.html")) {
          throw new Error("TRUSTED_POPUP_REQUIRED: only the extension popup can resume browser control");
        }
        await resumeHumanControlFromPopup();
        return popupState();
      }
      case "releaseControl":
        await stopControl("released_by_user", { requireExplicitStart: true });
        return popupState();
      case "saveConnection": {
        const port = Number(message.port);
        if (!Number.isInteger(port) || port < 1 || port > 65_535) throw new Error("Port must be between 1 and 65535");
        const updates = { port };
        const token = String(message.token ?? "").trim();
        if (token) {
          decodeBase64Url32(token, "Extension token");
          updates.token = token;
        }
        await updateSecuritySettings(updates, "connection_settings_changed");
        return popupState();
      }
      case "toggleEnabled":
        await updateSecuritySettings({ enabled: Boolean(message.enabled) }, message.enabled ? "bridge_resumed" : "bridge_paused");
        return popupState();
      case "toggleFullAccess":
        await updateSecuritySettings({ fullAccess: Boolean(message.fullAccess), pendingApproval: null }, "access_mode_changed");
        return popupState();
      case "allowCurrent": {
        const state = await popupState();
        const host = normalizeAllowedHost(state.currentHost);
        if (!host) throw new Error("The current tab is not an HTTP or HTTPS site");
        const config = await settings();
        await updateSecuritySettings({ allowedHosts: [...new Set([...config.allowedHosts, host])] }, "site_permissions_changed");
        return popupState();
      }
      case "removeHost": {
        const host = normalizeAllowedHost(message.host);
        const config = await settings();
        await updateSecuritySettings({ allowedHosts: config.allowedHosts.filter((item) => item !== host) }, "site_permissions_changed");
        return popupState();
      }
      case "addHost": {
        const host = normalizeAllowedHost(message.host);
        if (!host) throw new Error("Enter a hostname such as example.com or *.example.com");
        const config = await settings();
        await updateSecuritySettings({ allowedHosts: [...new Set([...config.allowedHosts, host])] }, "site_permissions_changed");
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
  void chrome.storage.local.get(DEFAULTS).then((stored) => updateSecuritySettings({
    enabled: stored.enabled,
    fullAccess: stored.fullAccess,
    port: stored.port,
    allowedHosts: stored.allowedHosts,
    connectionStatus: stored.connectionStatus,
    connectionDetail: stored.connectionDetail,
    pendingApproval: stored.pendingApproval,
  }, "extension_lifecycle_changed"));
});
chrome.runtime.onStartup.addListener(() => void connect());
chrome.alarms.create("local-browser-bridge-reconnect", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "local-browser-bridge-reconnect" && socket?.readyState !== WebSocket.OPEN) void connect();
  if (alarm.name === "local-browser-bridge-reconnect") void heartbeatControl();
  if (alarm.name === "local-browser-bridge-reconnect" && pendingControlCleanups.size) {
    void retryPendingControlCleanups();
  }
});
chrome.debugger.onDetach.addListener((source, reason) => {
  if (Number.isInteger(source.tabId)) {
    pendingDebuggerAttaches.delete(source.tabId);
    pendingDebuggerDetaches.delete(source.tabId);
    void confirmPendingControlCleanup(source.tabId, reason);
  }
  if (Number.isInteger(source.tabId) && source.tabId !== intentionalDetachTabId) {
    void hardRevokeDetached(source.tabId, reason);
  }
});
chrome.debugger.onEvent.addListener((source, method, params) => {
  if (!Number.isInteger(source.tabId) || controlLease?.tabId !== source.tabId) return;
  if (method === "Page.frameNavigated" && params?.frame && !params.frame.parentId) {
    acceptTopLevelNavigationSignal({
      tabId: source.tabId,
      url: params.frame.url,
      loaderId: params.frame.loaderId,
      frameId: params.frame.id,
      source: "Page.frameNavigated",
    });
  }
  if (method === "Page.navigatedWithinDocument"
    && params?.frameId === controlLease?.frameId) {
    acceptTopLevelNavigationSignal({
      tabId: source.tabId,
      url: params.url,
      frameId: params.frameId,
      source: "Page.navigatedWithinDocument",
      sameDocument: true,
    });
  }
  // The Page domain is enabled for the whole lease, so JavaScript dialog
  // lifecycle events arrive here for exactly the controlled tab.
  if (method === "Page.javascriptDialogOpening") {
    const dialogType = ["alert", "confirm", "prompt", "beforeunload"].includes(params?.type)
      ? params.type
      : "dialog";
    const pendingDialog = {
      type: dialogType,
      message: String(params?.message ?? "").slice(0, DIALOG_MESSAGE_MAX_CHARS),
      hasPrompt: dialogType === "prompt",
      at: Date.now(),
    };
    controlLease.pendingDialog = pendingDialog;
    void persistControlState().catch(() => {});
    send({ type: "event", name: "page.dialogOpened", data: { tabId: source.tabId, ...pendingDialog } });
  }
  if (method === "Page.javascriptDialogClosed") {
    controlLease.pendingDialog = null;
    void persistControlState().catch(() => {});
    send({ type: "event", name: "page.dialogClosed", data: { tabId: source.tabId, accepted: params?.result === true } });
  }
});
chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (bridgeCreatedTabs.has(tabId)
    && typeof changeInfo.url === "string"
    && changeInfo.url !== "about:blank") {
    bridgeCreatedTabs.delete(tabId);
    void persistControlState().catch(() => {});
  }
  if (controlLease?.tabId !== tabId) return;
  if ((changeInfo.status === "loading" || typeof changeInfo.url === "string")
    && !acceptTopLevelNavigationSignal({
      tabId,
      url: changeInfo.url,
      source: "tabs.onUpdated",
    })) return;
  if (changeInfo.status === "loading") {
    controlLease.viewport = null;
    controlLease.cursor = { ...controlLease.cursor, visible: false, updatedAt: Date.now() };
    void persistControlState();
  }
  if (changeInfo.status === "complete") void showControlUi().catch(() => {});
});
chrome.tabs.onRemoved.addListener((tabId) => {
  void initializeControlState().then(async () => {
    activeControlCaptures.delete(tabId);
    await confirmPendingControlCleanup(tabId, "target_closed");
    if (bridgeCreatedTabs.delete(tabId)) await persistControlState();
    if (controlLease?.tabId === tabId) await hardRevokeDetached(tabId, "target_closed");
  });
});
chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  const relevant = Object.keys(changes).some((key) => SECURITY_SETTING_KEYS.has(key));
  if (!relevant || consumeInternalSettings(changes)) return;
  void queueTransportRotation("external_security_settings_changed");
});

void initializeControlState();
void connect();
