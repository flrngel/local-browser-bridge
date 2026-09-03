const ui = Object.fromEntries(
  [
    "connection", "connection-text", "refresh-tabs", "tabs", "new-tab", "navigate-form", "navigate-url",
    "observe", "target-meta", "screenshot", "screenshot-empty", "click-form", "click-ref", "fill-form",
    "fill-ref", "fill-text", "select-form", "select-ref", "select-value", "selected-element", "elements",
    "elements-empty", "element-count", "selected-text", "page-text", "activity", "revision", "toast",
    "coordinates-form", "coordinate-x", "coordinate-y", "type-text-form", "type-text", "custom-key-form", "custom-key",
    "evaluate-form", "expression", "evaluation-result",
    "release-panel", "current-version", "update-status", "update-detail", "check-update", "update-link",
    "agent-fetch-url", "copy-agent-fetch-url", "shell-status",
    "browser-control-panel", "browser-control-summary", "browser-control-badge", "browser-control-details",
    "browser-control-form", "browser-control-ttl", "browser-control-start", "browser-control-status", "browser-control-stop",
    "computer-connection", "computer-connection-text", "computer-meta", "computer-status", "computer-observe", "computer-window",
    "computer-screenshot", "computer-screenshot-empty", "computer-frame-meta", "computer-click-form",
    "computer-x", "computer-y", "computer-move-duration", "computer-move", "computer-double-click",
    "computer-share-form", "computer-share-fps", "computer-share-start", "computer-share-stop", "computer-share-status",
    "computer-scroll-form", "computer-scroll-x", "computer-scroll-y", "computer-drag-form",
    "computer-drag-from-x", "computer-drag-from-y", "computer-drag-to-x", "computer-drag-to-y", "computer-drag-duration",
    "computer-drag-use-start", "computer-drag-use-end", "computer-type-form", "computer-type-text",
    "computer-key-form", "computer-key", "computer-pointer-details", "computer-session-details",
    "computer-elements", "computer-elements-empty", "computer-element-count", "computer-semantic-status",
    "computer-action-effect", "computer-action-summary", "computer-action-details", "computer-action-evidence",
    "auth-dialog", "auth-form", "auth-token", "auth-error",
    "home-view", "advanced-view", "show-advanced", "show-home",
    "home-hero-headline", "home-hero-action",
    "status-app-pill", "status-browser-pill", "status-browser-action",
    "status-desktop-toggle", "status-desktop-pill", "status-desktop-progress", "status-desktop-retry",
    "status-shell-toggle", "status-shell-pill",
    "home-browser-setup", "extension-folder-path-row", "extension-folder-path",
    "extension-folder-unknown", "copy-extension-path",
    "copy-ai-instructions", "copy-ai-link",
  ].map((id) => [id, document.getElementById(id)]),
);

let csrfToken = "";
let sessionToken = "";
let bridgeToken = "";
let currentState = null;
let busy = false;
let toastTimer = null;
let lastObservationGeneration = "";
let lastRenderedObservationKey = "";
let lastTabsSignature = "";
let lastComputerWindow = "";
let lastComputerAction = null;
let lastRenderedComputerAction;
let lastComputerElementsSignature = "";
let lastComputerScreenshotUrl = "";
const imageObjectUrls = new WeakMap();
const imageLoadStates = new WeakMap();

const COMPUTER_MUTATION_METHODS = new Set([
  "computer.move", "computer.click", "computer.drag", "computer.scroll", "computer.typeText",
  "computer.key", "computer.invoke", "computer.setValue",
]);
const SEMANTIC_INVOKE_ACTIONS = new Set(["press", "showMenu", "pick", "confirm", "cancel", "open"]);

function setBusy(value) {
  busy = value;
  for (const button of document.querySelectorAll("button")) button.disabled = value;
  if (!value) {
    syncBrowserAvailability();
    syncComputerAvailability();
  }
}

function showToast(message, tone = "normal") {
  clearTimeout(toastTimer);
  ui.toast.textContent = message;
  ui.toast.className = `toast ${tone === "normal" ? "" : tone}`.trim();
  ui.toast.hidden = false;
  toastTimer = setTimeout(() => { ui.toast.hidden = true; }, tone === "approval" ? 9_000 : 5_000);
}

async function request(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: {
      ...(options.body ? { "Content-Type": "application/json" } : {}),
      ...(options.method === "POST" ? { "X-CSRF-Token": csrfToken } : {}),
      ...(sessionToken ? { Authorization: `Session ${sessionToken}` } : {}),
      ...options.headers,
    },
  });
  const payload = await response.json();
  if (!response.ok || !payload.ok) {
    const error = new Error(payload.error?.message ?? `Request failed (${response.status})`);
    error.code = payload.error?.code;
    throw error;
  }
  return payload;
}

function browserImageKey(observation) {
  if (!observation?.screenshotUrl) return "";
  return `browser:${observation.tabId}:${observation.generation}:${observation.screenshotUrl}`;
}

function computerImageKey(observation) {
  if (!observation?.screenshotUrl) return "";
  return `computer:${observation.windowId}:${observation.frameId}:${observation.screenshotUrl}`;
}

function protectedImageReady(element, expectedKey) {
  const state = imageLoadStates.get(element);
  return Boolean(expectedKey && state?.expectedKey === expectedKey && state.loadedKey === expectedKey);
}

function clearProtectedImage(element) {
  imageLoadStates.get(element)?.controller.abort();
  imageLoadStates.delete(element);
  const objectUrl = imageObjectUrls.get(element);
  if (objectUrl) URL.revokeObjectURL(objectUrl);
  imageObjectUrls.delete(element);
  element.removeAttribute("src");
  element.removeAttribute("aria-busy");
}

async function loadProtectedImage(element, path, expectedKey) {
  imageLoadStates.get(element)?.controller.abort();
  const controller = new AbortController();
  const requestState = { controller, expectedKey, loadedKey: "", pendingUrl: "" };
  imageLoadStates.set(element, requestState);
  element.setAttribute("aria-busy", "true");
  if (element === ui["computer-screenshot"]) syncComputerAvailability();

  try {
    const response = await fetch(path, {
      headers: sessionToken ? { Authorization: `Session ${sessionToken}` } : {},
      cache: "no-store",
      signal: controller.signal,
    });
    if (!response.ok) throw new Error(`Screenshot request failed (${response.status})`);
    const blob = await response.blob();
    if (imageLoadStates.get(element) !== requestState) return false;
    const nextUrl = URL.createObjectURL(blob);
    if (imageLoadStates.get(element) !== requestState) {
      URL.revokeObjectURL(nextUrl);
      return false;
    }
    const previousUrl = imageObjectUrls.get(element);
    requestState.pendingUrl = nextUrl;
    element.src = nextUrl;
    try {
      await element.decode();
    } catch (error) {
      if (imageLoadStates.get(element) !== requestState) {
        URL.revokeObjectURL(nextUrl);
        return false;
      }
      throw error;
    }
    if (imageLoadStates.get(element) !== requestState) {
      URL.revokeObjectURL(nextUrl);
      return false;
    }
    imageObjectUrls.set(element, nextUrl);
    requestState.loadedKey = expectedKey;
    requestState.pendingUrl = "";
    element.removeAttribute("aria-busy");
    if (previousUrl) URL.revokeObjectURL(previousUrl);
    if (element === ui["computer-screenshot"]) syncComputerAvailability();
    return true;
  } catch (error) {
    if (requestState.pendingUrl) {
      URL.revokeObjectURL(requestState.pendingUrl);
      requestState.pendingUrl = "";
    }
    if (error.name === "AbortError") return false;
    if (imageLoadStates.get(element) === requestState) {
      imageLoadStates.delete(element);
      element.removeAttribute("aria-busy");
      if (element === ui["computer-screenshot"]) syncComputerAvailability();
    }
    throw error;
  }
}

function tokenFromFragment() {
  const fragment = new URLSearchParams(window.location.hash.slice(1));
  const token = fragment.get("token")?.trim() ?? "";
  if (window.location.hash) {
    window.history.replaceState(null, "", `${window.location.pathname}${window.location.search}`);
  }
  return token;
}

async function createDashboardSession(masterToken = "", existingSession = "") {
  const authorization = masterToken
    ? `Bearer ${masterToken}`
    : existingSession
      ? `Session ${existingSession}`
      : "";
  const session = await request("/api/session", {
    headers: authorization ? { Authorization: authorization } : {},
  });
  sessionToken = session.sessionToken;
  window.sessionStorage.setItem("lbbDashboardSession", sessionToken);
  // Kept only in memory (never written to storage) so a one-click extension
  // Connect can relay it this page load; see connectExtension() below.
  if (masterToken) bridgeToken = masterToken;
  return session;
}

function requestDashboardToken(message = "") {
  ui["auth-error"].textContent = message;
  ui["auth-error"].hidden = !message;
  ui["auth-dialog"].showModal();
  ui["auth-dialog"].oncancel = (event) => event.preventDefault();
  queueMicrotask(() => ui["auth-token"].focus());
  return new Promise((resolve) => {
    const submit = async (event) => {
      event.preventDefault();
      const token = ui["auth-token"].value.trim();
      if (!token) return;
      const button = ui["auth-form"].querySelector("button");
      button.disabled = true;
      try {
        const session = await createDashboardSession(token);
        ui["auth-token"].value = "";
        ui["auth-dialog"].close();
        ui["auth-form"].removeEventListener("submit", submit);
        resolve(session);
      } catch (error) {
        ui["auth-error"].textContent = error.message;
        ui["auth-error"].hidden = false;
      } finally {
        button.disabled = false;
      }
    };
    ui["auth-form"].addEventListener("submit", submit);
  });
}

async function bootstrapDashboardSession() {
  const fragmentToken = tokenFromFragment();
  if (fragmentToken) {
    try {
      return await createDashboardSession(fragmentToken);
    } catch (error) {
      return requestDashboardToken(error.message);
    }
  }
  const existingSession = window.sessionStorage.getItem("lbbDashboardSession") ?? "";
  if (existingSession) {
    try {
      return await createDashboardSession("", existingSession);
    } catch {
      window.sessionStorage.removeItem("lbbDashboardSession");
      sessionToken = "";
    }
  }
  try {
    return await createDashboardSession();
  } catch (error) {
    return requestDashboardToken(error.message);
  }
}

function text(tag, value, className) {
  const node = document.createElement(tag);
  node.textContent = value;
  if (className) node.className = className;
  return node;
}

function actionButton(label, className, onClick) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `button compact ${className}`;
  button.textContent = label;
  button.disabled = busy;
  button.addEventListener("click", onClick);
  return button;
}

function shortId(value) {
  const raw = String(value ?? "");
  if (!raw) return "—";
  return raw.length > 18 ? `${raw.slice(0, 8)}…${raw.slice(-6)}` : raw;
}

function formatEpoch(value) {
  if (value === null || value === undefined || !Number.isFinite(Number(value))) return "—";
  try { return new Date(Number(value)).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }); }
  catch { return "—"; }
}

function formatNumber(value, digits = 1) {
  return Number.isFinite(Number(value)) ? Number(value).toFixed(digits) : "—";
}

function titleCase(value) {
  return String(value ?? "unknown")
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/[_-]+/g, " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function metadataEntry(term, value) {
  const row = document.createElement("div");
  row.append(text("dt", term), text("dd", value));
  return row;
}

function renderMetadata(container, entries) {
  container.replaceChildren(...entries.map(([term, value]) => metadataEntry(term, String(value ?? "—"))));
}

function setStateBadge(element, label, tone) {
  setTextIfChanged(element, label);
  element.className = `state-badge ${tone}`;
}

function setTextIfChanged(element, value) {
  if (element.textContent !== value) element.textContent = value;
}

function formatTime(iso) {
  try { return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }); }
  catch { return ""; }
}

function renderConnection(state) {
  ui.connection.classList.toggle("online", state.connected);
  ui.connection.classList.toggle("offline", !state.connected);
  if (!state.connected) {
    setTextIfChanged(ui["connection-text"], "Extension offline — open its popup to connect");
    return;
  }
  const mode = state.extension?.mode === "full-access" ? "FULL ACCESS" : "SAFE MODE";
  const versionMismatch = state.extension && state.update?.currentVersion && state.extension.version !== state.update.currentVersion;
  const versionLabel = versionMismatch ? `${state.extension.version} · VERSION MISMATCH` : state.extension?.version;
  const detail = state.extension ? `${state.extension.browser} · extension ${versionLabel} · ${mode}` : "handshake pending";
  setTextIfChanged(ui["connection-text"], `Connected · ${detail}`);
}

function syncBrowserAvailability() {
  if (!currentState) return;
  const compatible = Boolean(currentState.connected && currentState.extension?.compatible);
  const targetReady = Number.isInteger(currentState.targetTabId);
  const active = currentState.browserControl?.active === true;
  const humanPaused = currentState.browserControl?.humanPaused === true;
  ui["browser-control-start"].disabled = busy || !compatible || !targetReady || humanPaused;
  ui["browser-control-status"].disabled = busy || !compatible;
  ui["browser-control-stop"].disabled = busy || !compatible || !active;
  ui["browser-control-ttl"].disabled = busy || !compatible;
}

function renderBrowserControl(state) {
  const control = state.browserControl ?? {};
  const browserName = typeof state.extension?.browser === "string"
    ? state.extension.browser
    : "Browser";
  const active = control.active === true;
  const humanPaused = control.humanPaused === true;
  const target = state.tabs?.find((tab) => tab.id === control.tabId)
    ?? state.tabs?.find((tab) => tab.id === state.targetTabId);
  const pointer = control.cursor ?? {};
  const sessionTab = active ? control.tabId : state.targetTabId;
  ui["browser-control-panel"].classList.toggle("control-active", active);

  if (!state.connected) {
    setTextIfChanged(ui["browser-control-summary"], "The extension is offline. Open its popup and connect it to this server.");
    setStateBadge(ui["browser-control-badge"], "Offline", "danger-state");
  } else if (!state.extension?.compatible) {
    setTextIfChanged(ui["browser-control-summary"], "The extension handshake is incompatible. Install the release that matches this server.");
    setStateBadge(ui["browser-control-badge"], "Blocked", "danger-state");
  } else if (humanPaused) {
    setTextIfChanged(ui["browser-control-summary"], "Remote browser control was paused by a person. Only Resume remote control in the extension popup can authorize it again.");
    setStateBadge(ui["browser-control-badge"], "Paused by you", "warning-state");
  } else if (active) {
    setTextIfChanged(ui["browser-control-summary"], `${browserName} control is attached to tab ${control.tabId}. The browser's native debugging notice should remain visible, and the tab is in a named "Local Browser Bridge" group.`);
    setStateBadge(ui["browser-control-badge"], "Control active", "active");
  } else if (control.revocation?.requiresExplicitStart) {
    setTextIfChanged(ui["browser-control-summary"], `Control was revoked (${titleCase(control.revocation.reason)}). Select Start control to create a new explicit lease.`);
    setStateBadge(ui["browser-control-badge"], "Revoked", "warning-state");
  } else {
    setTextIfChanged(
      ui["browser-control-summary"],
      Number.isInteger(state.targetTabId)
        ? `Tab ${state.targetTabId} is selected. Start control to attach ${browserName}'s visible debugger-backed lease.`
        : "Select a browser tab before starting control.",
    );
    setStateBadge(ui["browser-control-badge"], "Inactive", "inactive");
  }

  const pointerLabel = active && pointer.visible
    ? `${formatNumber(pointer.x, 0)}, ${formatNumber(pointer.y, 0)} · move ${control.moveSequence ?? 0}`
    : "Not visible";
  renderMetadata(ui["browser-control-details"], [
    ["Session", active ? shortId(control.sessionId) : "Not started"],
    ["Target", sessionTab ? `Tab ${sessionTab} · ${target?.title || "title unavailable"}` : "No tab selected"],
    ["Lease", active ? `${formatEpoch(control.startedAt)} → ${formatEpoch(control.expiresAt)}` : "Inactive"],
    ["Turn", active ? control.turn ?? 0 : "—"],
    ["Pointer", pointerLabel],
    ["Protocol", state.extension ? `v${state.extension.protocolVersion} · ${shortId(state.extension.sessionId)}` : "Handshake pending"],
  ]);
  ui["browser-control-start"].textContent = humanPaused
    ? "Resume in extension popup"
    : active && control.tabId === state.targetTabId
      ? "Renew control"
      : "Start control";
  syncBrowserAvailability();
}

function syncComputerAvailability() {
  if (!currentState) return;
  const connected = Boolean(currentState.computerConnected);
  const compatible = Boolean(currentState.computer?.compatible);
  const observation = currentState.computerObservation;
  const frameReady = Boolean(
    observation?.frameId
      && protectedImageReady(ui["computer-screenshot"], computerImageKey(observation)),
  );
  const inputReady = Boolean(currentState.computer?.inputReady && compatible);
  const semanticReady = Boolean(currentState.computer?.semanticReady && compatible);
  const windowReady = Boolean(ui["computer-window"].value);
  const shareActive = currentState.computer?.share?.active === true;
  ui["computer-status"].disabled = busy || !connected || !compatible;
  ui["computer-observe"].disabled = busy || !connected || !compatible || !windowReady;
  ui["computer-window"].disabled = busy || !connected || !compatible || shareActive || !(currentState.computer?.windows?.length);
  ui["computer-share-fps"].disabled = busy || !connected || !compatible || shareActive;
  ui["computer-share-start"].disabled = busy || !connected || !compatible || !windowReady || shareActive;
  ui["computer-share-stop"].disabled = busy || !connected || !shareActive;
  for (const control of document.querySelectorAll("#computer-click-form button, .computer-scroll, #computer-scroll-form button, #computer-drag-form button, #computer-type-form button, #computer-key-form button")) {
    control.disabled = busy || !connected || !frameReady || !inputReady;
  }
  for (const control of document.querySelectorAll(".computer-semantic-action")) {
    const semanticRequired = control.dataset.requiresSemantic === "true";
    control.disabled = busy || !connected || !frameReady || (semanticRequired && !semanticReady) || control.dataset.elementDisabled === "true";
  }
}

function renderComputer(state) {
  const connected = Boolean(state.computerConnected);
  const computer = state.computer;
  const observation = state.computerObservation;
  const priorWindow = ui["computer-window"].value;
  const knownWindows = computer?.windows ?? [];
  const preferredWindow = knownWindows.some((window) => window.id === priorWindow)
    ? priorWindow
    : computer?.share?.windowId || observation?.windowId || knownWindows[0]?.id;
  ui["computer-window"].replaceChildren();
  if (!knownWindows.length) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = connected ? "No capturable windows reported" : "Connect the helper first";
    ui["computer-window"].append(option);
  }
  for (const window of knownWindows) {
    const option = document.createElement("option");
    option.value = window.id;
    option.textContent = `${window.appName} — ${window.title || "Untitled window"} (${window.width}×${window.height})`;
    option.selected = window.id === preferredWindow;
    ui["computer-window"].append(option);
  }
  ui["computer-connection"].classList.toggle("online", connected);
  ui["computer-connection"].classList.toggle("offline", !connected);
  if (!connected) {
    setTextIfChanged(ui["computer-connection-text"], "Computer helper offline — start the separate native app");
    ui["computer-meta"].textContent = "Start Local Computer Helper to enable desktop control.";
  } else if (!computer) {
    setTextIfChanged(ui["computer-connection-text"], "Computer helper connected · handshake pending");
    ui["computer-meta"].textContent = "Waiting for helper capability metadata.";
  } else if (!computer.compatible) {
    ui["computer-connection"].classList.remove("online");
    ui["computer-connection"].classList.add("offline");
    setTextIfChanged(ui["computer-connection-text"], `Computer version mismatch · helper ${computer.version}`);
    ui["computer-meta"].textContent = `Install helper ${state.update?.currentVersion ?? "matching the server"}; native actions are blocked.`;
  } else {
    const input = computer.inputReady ? "input ready" : "native input route unavailable";
    const semantic = computer.semanticReady ? "semantic ready" : "semantic permission required";
    setTextIfChanged(ui["computer-connection-text"], `Computer connected · ${computer.platform} ${computer.architecture} · ${input} · ${semantic}`);
    ui["computer-meta"].textContent = `${computer.sessionMode} · ${computer.isolation} · ${computer.backend} · helper ${computer.version} · ${input} · ${semantic}`;
  }

  const share = computer?.share ?? {};
  const activationMode = computer?.invariants?.targetActivationMode;
  const activation = activationMode === "may-use-transient-ax-frontmost-focus-lease"
    ? "May use a transient target AXFrontmost lease · OS foreground checked before/after"
    : activationMode === "no-explicit-target-activation-api"
      ? "No explicit target-activation API · foreground/focus checked before/after"
      : "Activation behavior unavailable";
  if (share.active) {
    ui["computer-share-fps"].value = String(share.fps);
    const indicator = share.systemIndicator ? " The operating system controls capture indication." : "";
    setTextIfChanged(ui["computer-share-status"], `Persistent exact-window stream ${shortId(share.windowId)} · ${share.fps} FPS maximum. Input checks compare foreground, focus, desktop, and pointer before and after each action; matching samples cannot rule out a shorter transient change.${indicator}`);
    ui["computer-share-start"].textContent = "Persistent share active";
  } else {
    const stopped = share.stopped ? ` Last stop: ${titleCase(share.reason)}.` : "";
    setTextIfChanged(ui["computer-share-status"], `Persistent exact-window share is inactive.${stopped}`);
    ui["computer-share-start"].textContent = "Start persistent share";
  }

  renderMetadata(ui["computer-session-details"], [
    ["Session", computer ? shortId(computer.sessionId) : "Helper offline"],
    ["Protocol", computer ? `v${computer.protocolVersion} · helper ${computer.version}` : "—"],
    ["Backend", computer ? `${computer.platform} ${computer.architecture} · ${computer.backend}` : "—"],
    ["Delivery", observation?.deliveryMode || computer?.sessionMode || "—"],
    ["Activation", computer ? activation : "—"],
    ["Share", share.active ? `${shortId(share.id)} · ${share.captureBackend || "native stream"} · ${share.fps} FPS max · source ${share.sourceSequence ?? 0} / transport ${share.sequence ?? 0}` : "Inactive"],
    ["Isolation", share.active ? "Shared login session · exact-window capture and target-routed input · not a VM" : "—"],
    ["Dropped", share.active ? `${share.sourceDroppedFrames ?? 0} source · ${share.transportDroppedFrames ?? share.droppedFrames ?? 0} transport` : "—"],
  ]);

  if (!observation) {
    clearProtectedImage(ui["computer-screenshot"]);
    lastComputerScreenshotUrl = "";
    ui["computer-screenshot"].hidden = true;
    ui["computer-screenshot-empty"].hidden = false;
    ui["computer-frame-meta"].textContent = "No frame is available. Input stays disabled until a fresh frame is captured.";
    renderMetadata(ui["computer-pointer-details"], [["State", "No pointer frame"]]);
  } else {
    if (lastComputerWindow !== observation.windowId || !ui["computer-x"].value || !ui["computer-y"].value) {
      ui["computer-x"].value = String(Math.floor(observation.imageWidth / 2));
      ui["computer-y"].value = String(Math.floor(observation.imageHeight / 2));
      lastComputerWindow = observation.windowId;
    }
    if (observation.screenshotUrl && observation.screenshotUrl !== lastComputerScreenshotUrl) {
      lastComputerScreenshotUrl = observation.screenshotUrl;
      const expectedKey = computerImageKey(observation);
      void loadProtectedImage(
        ui["computer-screenshot"],
        `${observation.screenshotUrl}&cache=${Date.now()}`,
        expectedKey,
      ).catch((error) => showToast(error.message, "error"));
    }
    ui["computer-screenshot"].hidden = false;
    ui["computer-screenshot-empty"].hidden = true;
    ui["computer-frame-meta"].textContent = `${observation.appName} — ${observation.windowTitle} · ${observation.deliveryMode} · image ${observation.imageWidth}×${observation.imageHeight} · window ${observation.screenWidth}×${observation.screenHeight} · scale ${formatNumber(observation.transportScaleX, 3)}×${formatNumber(observation.transportScaleY, 3)} · frame ${observation.frameId}`;
    for (const id of ["computer-x", "computer-drag-from-x", "computer-drag-to-x"]) {
      ui[id].max = String(observation.imageWidth - 1);
      if (ui[id].value !== "") ui[id].value = String(Math.min(Number(ui[id].value), observation.imageWidth - 1));
    }
    for (const id of ["computer-y", "computer-drag-from-y", "computer-drag-to-y"]) {
      ui[id].max = String(observation.imageHeight - 1);
      if (ui[id].value !== "") ui[id].value = String(Math.min(Number(ui[id].value), observation.imageHeight - 1));
    }
    const pointer = observation.pointer ?? {};
    const position = pointer.visible
      ? `${formatNumber(pointer.imageX, 1)}, ${formatNumber(pointer.imageY, 1)} ${pointer.coordinateSpace || "image-pixels"}`
      : "Hidden for this window";
    renderMetadata(ui["computer-pointer-details"], [
      ["Pointer", shortId(pointer.id)],
      ["Position", position],
      ["Action", `${titleCase(pointer.action)}${pointer.pressed ? " · pressed" : ""}`],
      ["Motion", `heading ${formatNumber(pointer.headingDegrees, 1)}° · sequence ${pointer.sequence ?? 0}`],
      ["Style", pointer.style?.theme ? `${pointer.style.theme} · ${pointer.style.logicalSize ?? "?"} px` : "Composited cursor"],
      ["Updated", pointer.updatedAt || "—"],
    ]);
  }
  renderComputerElements(observation);
  renderComputerActionEvidence();
  syncComputerAvailability();
}

function setSelectedComputerPoint(x, y, announce = true) {
  ui["computer-x"].value = String(Math.round(x));
  ui["computer-y"].value = String(Math.round(y));
  if (announce) showToast(`Selected window point ${Math.round(x)}, ${Math.round(y)}`);
}

function renderComputerElements(observation) {
  const elements = observation?.elements ?? [];
  ui["computer-element-count"].textContent = `${elements.length} element${elements.length === 1 ? "" : "s"}`;
  if (!observation) {
    clearProtectedImage(ui.screenshot);
    ui["computer-semantic-status"].textContent = "Observe a window to load accessibility-backed elements.";
  } else if (!observation.semanticAvailable) {
    ui["computer-semantic-status"].textContent = observation.semanticError
      ? `Semantic control unavailable: ${observation.semanticError}`
      : "Semantic control is unavailable for this window.";
  } else {
    ui["computer-semantic-status"].textContent = `${observation.semanticMode} reported ${elements.length} actionable or readable elements. Re-observe after a UI change because refs are frame-bound.`;
  }

  const signature = JSON.stringify({
    available: observation?.semanticAvailable ?? false,
    mode: observation?.semanticMode ?? "",
    elements: elements.map((element) => ({
      ref: element.ref,
      role: element.role,
      name: element.name,
      value: element.value,
      sensitive: element.sensitive,
      valueRedacted: element.valueRedacted,
      enabled: element.enabled,
      actions: element.actions,
      bounds: element.bounds,
    })),
  });
  if (signature === lastComputerElementsSignature) return;
  lastComputerElementsSignature = signature;
  ui["computer-elements"].replaceChildren();
  ui["computer-elements-empty"].hidden = elements.length > 0;

  for (const element of elements) {
    const row = document.createElement("tr");
    const refCell = document.createElement("td");
    refCell.append(text("span", element.ref, "ref"));
    const nameCell = document.createElement("td");
    nameCell.append(text("div", element.name || "Unnamed", "element-name"));
    nameCell.append(text("div", `${element.role} · ${element.enabled === false ? "disabled" : "enabled"}`, "element-detail"));
    const valueCell = document.createElement("td");
    const valueLabel = element.sensitive || element.valueRedacted
      ? "Sensitive value redacted"
      : element.value || "No exposed value";
    valueCell.append(text("div", valueLabel, "semantic-value"));
    if (element.bounds) {
      valueCell.append(text("div", `${formatNumber(element.bounds.x, 0)}, ${formatNumber(element.bounds.y, 0)} · ${formatNumber(element.bounds.width, 0)}×${formatNumber(element.bounds.height, 0)} ${element.coordinateSpace || "image-pixels"}`, "element-detail"));
    }
    const actionsCell = document.createElement("td");
    actionsCell.className = "semantic-actions";
    if (element.bounds) {
      const centerButton = actionButton("Use center", "ghost", () => {
        setSelectedComputerPoint(element.bounds.x + element.bounds.width / 2, element.bounds.y + element.bounds.height / 2);
      });
      centerButton.classList.add("computer-semantic-action");
      centerButton.dataset.elementDisabled = "false";
      centerButton.dataset.requiresSemantic = "false";
      centerButton.setAttribute("aria-label", `Use the center of ${element.name || element.ref} as the selected point`);
      actionsCell.append(centerButton);
    }
    for (const action of element.actions ?? []) {
      if (!SEMANTIC_INVOKE_ACTIONS.has(action)) continue;
      const button = actionButton(titleCase(action), "primary", () => {
        const params = computerFrameParams({ elementRef: element.ref, action });
        if (params) void runAction("computer.invoke", params);
      });
      button.classList.add("computer-semantic-action");
      button.dataset.elementDisabled = String(element.enabled === false);
      button.dataset.requiresSemantic = "true";
      button.setAttribute("aria-label", `${titleCase(action)} ${element.name || element.ref}`);
      actionsCell.append(button);
    }
    if (!element.sensitive && !element.valueRedacted && (element.actions ?? []).includes("setValue")) {
      const valueControl = document.createElement("div");
      valueControl.className = "semantic-value-control";
      const input = document.createElement("input");
      input.type = "text";
      input.placeholder = "New value";
      input.setAttribute("aria-label", `New value for ${element.name || element.ref}`);
      const button = actionButton("Set value", "secondary", () => {
        if (!input.value) {
          showToast("Enter a semantic value first.", "error");
          input.focus();
          return;
        }
        const params = computerFrameParams({ elementRef: element.ref, value: input.value });
        if (params) void runAction("computer.setValue", params);
      });
      button.classList.add("computer-semantic-action");
      button.dataset.elementDisabled = String(element.enabled === false);
      button.dataset.requiresSemantic = "true";
      valueControl.append(input, button);
      actionsCell.append(valueControl);
    }
    if (!actionsCell.childElementCount) actionsCell.textContent = "No supported action";
    row.append(refCell, nameCell, valueCell, actionsCell);
    ui["computer-elements"].append(row);
  }
}

function renderComputerActionEvidence() {
  if (lastComputerAction === lastRenderedComputerAction) return;
  lastRenderedComputerAction = lastComputerAction;
  ui["computer-action-details"].replaceChildren();
  ui["computer-action-evidence"].replaceChildren();
  if (!lastComputerAction) {
    setStateBadge(ui["computer-action-effect"], "No action", "inactive");
    ui["computer-action-summary"].textContent = "Run a native action to inspect its delivery invariants and any verified target-side postcondition.";
    return;
  }
  const { method } = lastComputerAction;
  if (lastComputerAction.failed) {
    const error = lastComputerAction.error ?? {};
    const code = error.code || "REQUEST_FAILED";
    const message = error.message || "No error detail was returned.";
    setStateBadge(ui["computer-action-effect"], "Failed", "danger-state");
    ui["computer-action-summary"].textContent = `${method} failed · ${code}: ${message}`;
    renderMetadata(ui["computer-action-details"], [
      ["Outcome", "Failed before a successful native result was returned"],
      ["Error code", code],
      ["Previous result", "Cleared"],
    ]);
    ui["computer-action-evidence"].append(text("li", "No successful native result or target-side evidence is being shown for this failed request.", "evidence-missing"));
    return;
  }
  const result = lastComputerAction.result ?? {};
  const effect = result.effect || "Unreported";
  const tone = effect === "Confirmed" ? "active" : effect === "Refused" || effect === "SuspectedNoop" ? "danger-state" : "warning-state";
  setStateBadge(ui["computer-action-effect"], titleCase(effect), tone);
  const confirmation = effect === "Confirmed"
    ? "A target-side semantic postcondition was observed."
    : "Delivery completed, but no target-side postcondition confirmed the requested effect.";
  ui["computer-action-summary"].textContent = `${method} · ${confirmation}`;
  renderMetadata(ui["computer-action-details"], [
    ["Action ID", shortId(result.actionId)],
    ["Frame", shortId(result.frameId)],
    ["Delivery", result.deliveryMode || "—"],
    ["Total", result.timings?.totalMs !== undefined ? `${formatNumber(result.timings.totalMs, 2)} ms` : "—"],
    ["Pointer", result.pointer ? `${shortId(result.pointer.id)} · sequence ${result.pointer.sequence ?? 0}` : "Not applicable"],
    ["Motion", result.motion?.curve || result.motion?.gesture?.curve || "Not applicable"],
  ]);
  for (const item of result.evidence ?? []) {
    const row = document.createElement("li");
    row.className = item.observed ? "evidence-observed" : "evidence-missing";
    row.append(text("strong", `${item.observed ? "Observed" : "Not observed"} · ${titleCase(item.claim)}`));
    row.append(text("span", `${titleCase(item.kind)}${item.supportsConfirmation ? " · confirms effect" : " · delivery evidence only"}`));
    row.append(text("span", item.detail || "No detail reported"));
    ui["computer-action-evidence"].append(row);
  }
  if (!(result.evidence ?? []).length) {
    ui["computer-action-evidence"].append(text("li", "This action returned no structured evidence items.", "evidence-missing"));
  }
}

function renderUpdate(state) {
  const update = state.update ?? {};
  const labels = {
    checking: "Checking official release metadata…",
    up_to_date: "Up to date",
    development: `Development build ahead of stable ${update.latestVersion ?? "release"}`,
    available: `Update available: ${update.latestVersion ?? "new version"}`,
    error: "Update status unavailable",
    disabled: "Automatic check disabled",
  };
  ui["current-version"].textContent = `version ${update.currentVersion ?? "unknown"}`;
  setTextIfChanged(ui["update-status"], labels[update.status] ?? "Update status unavailable");
  ui["update-detail"].textContent = update.message
    ? `${update.message} The checker never downloads or installs files and sends no telemetry.`
    : "The checker contacts only GitHub release metadata. It never downloads or installs files and sends no telemetry.";
  ui["release-panel"].className = `release-panel panel update-${update.status ?? "error"}`;
  ui["check-update"].textContent = update.status === "checking"
    ? "Checking…"
    : update.status === "development" ? "Recheck stable release" : "Check again";
  ui["update-link"].textContent = update.status === "development" ? "View latest stable release" : "View verified release";
  ui["update-link"].href = update.releaseUrl || "https://github.com/flrngel/local-browser-bridge/releases";
}

function renderAgentAccess(state) {
  ui["agent-fetch-url"].value = state.agentFetch?.baseUrl ?? "";
  const shell = state.shell ?? {};
  ui["shell-status"].textContent = shell.enabled
    ? `Shell enabled · ${shell.defaultShell} · full current-user command access`
    : "Shell disabled · restart or reinstall with --enable-shell only when you intend to grant full current-user command access";
}

function renderTabs(state) {
  const signature = JSON.stringify({ connected: state.connected, targetTabId: state.targetTabId, tabs: state.tabs });
  if (signature === lastTabsSignature) return;
  lastTabsSignature = signature;
  ui.tabs.replaceChildren();
  if (!state.tabs.length) {
    ui.tabs.append(text("p", state.connected ? "No controllable tabs reported." : "Connect the extension first.", "empty-state"));
    return;
  }
  for (const tab of state.tabs) {
    const card = document.createElement("article");
    card.className = `tab-card${tab.id === state.targetTabId ? " selected" : ""}`;
    card.append(text("div", tab.title || "Untitled tab", "tab-title"));
    card.append(text("div", tab.url || "about:blank", "tab-url"));
    const actions = document.createElement("div");
    actions.className = "tab-actions";
    actions.append(actionButton(tab.id === state.targetTabId ? "Selected" : "Select", "ghost", () => runAction("tabs.activate", { tabId: tab.id })));
    actions.append(actionButton("Close", "ghost", () => runAction("tabs.close", { tabId: tab.id })));
    card.append(actions);
    ui.tabs.append(card);
  }
}

function selectRef(element) {
  ui["click-ref"].value = element.ref;
  ui["fill-ref"].value = element.ref;
  ui["select-ref"].value = element.ref;
  ui["selected-element"].textContent = `${element.ref} · ${element.role} · ${element.name || "unnamed"}`;
}

function renderObservation(state) {
  const observation = state.observation;
  const renderKey = observation ? `${observation.tabId}:${observation.generation}:${observation.screenshotUrl ?? ""}` : "none";
  if (renderKey === lastRenderedObservationKey) return;
  lastRenderedObservationKey = renderKey;
  if (!observation) {
    ui["target-meta"].textContent = "No observation yet.";
    ui.screenshot.hidden = true;
    ui["screenshot-empty"].hidden = false;
    ui.elements.replaceChildren();
    ui["elements-empty"].hidden = false;
    ui["element-count"].textContent = "0 elements";
    ui["selected-text"].hidden = true;
    ui["page-text"].textContent = "No page text captured.";
    return;
  }

  if (lastObservationGeneration && observation.generation !== lastObservationGeneration) {
    ui["click-ref"].value = "";
    ui["fill-ref"].value = "";
    ui["select-ref"].value = "";
    ui["selected-element"].textContent = "Observation changed; select a fresh element ref.";
  }
  lastObservationGeneration = observation.generation;

  ui["target-meta"].textContent = `Tab ${observation.tabId} · ${observation.title || "Untitled"} · ${observation.url}`;
  if (observation.screenshotUrl) {
    const expectedKey = browserImageKey(observation);
    void loadProtectedImage(
      ui.screenshot,
      `${observation.screenshotUrl}&cache=${Date.now()}`,
      expectedKey,
    ).catch((error) => showToast(error.message, "error"));
    ui.screenshot.hidden = false;
    ui["screenshot-empty"].hidden = true;
  } else {
    ui.screenshot.hidden = true;
    ui["screenshot-empty"].hidden = false;
  }

  const elements = observation.elements ?? [];
  ui.elements.replaceChildren();
  ui["elements-empty"].hidden = elements.length > 0;
  ui["element-count"].textContent = `${elements.length} element${elements.length === 1 ? "" : "s"}`;
  for (const element of elements) {
    const row = document.createElement("tr");
    const refCell = document.createElement("td");
    refCell.append(text("span", element.ref, "ref"));
    const nameCell = document.createElement("td");
    nameCell.append(text("div", element.name || "Unnamed", "element-name"));
    nameCell.append(text("div", `${element.role}${element.type ? ` · ${element.type}` : ""}`, "element-detail"));
    const stateCell = document.createElement("td");
    const stateLabels = [element.disabled ? "disabled" : "enabled", element.inViewport ? "in viewport" : "offscreen"];
    if (element.risk) stateLabels.push(`risk: ${element.risk}`);
    stateCell.textContent = stateLabels.join(" · ");
    const useCell = document.createElement("td");
    useCell.append(actionButton("Use ref", "ghost", () => selectRef(element)));
    useCell.append(document.createTextNode(" "));
    useCell.append(actionButton("Click", "primary", () => runAction("page.click", {
      ref: element.ref,
      generation: observation.generation,
    })));
    row.append(refCell, nameCell, stateCell, useCell);
    ui.elements.append(row);
  }
  ui["selected-text"].hidden = !observation.selectedText;
  ui["selected-text"].textContent = observation.selectedText ? `Selected text: ${observation.selectedText}` : "";
  ui["page-text"].textContent = observation.bodyText || "No rendered body text was available.";
}

function renderActivity(state) {
  ui.activity.replaceChildren();
  for (const item of state.activity ?? []) {
    const row = document.createElement("li");
    row.className = "activity-item";
    row.append(text("span", formatTime(item.at), "activity-time"));
    row.append(text("span", item.method, "activity-method"));
    row.append(text("span", `${item.status.toUpperCase()} · ${item.message}`, `activity-status-${item.status}`));
    ui.activity.append(row);
  }
  ui.revision.textContent = `revision ${state.revision}`;
}

function shouldAcceptStateRevision(state, previousState) {
  const nextRevision = Number(state?.revision);
  const currentRevision = Number(previousState?.revision);
  return !(previousState
    && Number.isFinite(nextRevision)
    && Number.isFinite(currentRevision)
    && nextRevision < currentRevision);
}

function render(state) {
  if (!shouldAcceptStateRevision(state, currentState)) return false;
  currentState = state;
  renderConnection(state);
  renderBrowserControl(state);
  renderComputer(state);
  renderUpdate(state);
  renderAgentAccess(state);
  renderTabs(state);
  renderObservation(state);
  renderActivity(state);
  renderHome(state);
  return true;
}

function computerFrameParams(extra = {}) {
  const observation = currentState?.computerObservation;
  if (!observation?.frameId) {
    showToast("Observe the desktop first so input can be bound to a fresh frame.", "error");
    return null;
  }
  if (!protectedImageReady(ui["computer-screenshot"], computerImageKey(observation))) {
    showToast("Wait for the exact current frame image to finish loading before using it for input.", "error");
    return null;
  }
  return { frameId: observation.frameId, ...extra };
}

function selectedComputerPoint() {
  return {
    x: Number(ui["computer-x"].value),
    y: Number(ui["computer-y"].value),
  };
}

async function loadState() {
  try {
    const payload = await request("/api/state");
    render(payload.state);
  } catch (error) {
    showToast(error.message, "error");
  }
}

async function runAction(method, params = {}) {
  if (busy) return;
  setBusy(true);
  let responseReceived = false;
  try {
    const payload = await request("/api/action", {
      method: "POST",
      body: JSON.stringify({ method, params }),
    });
    responseReceived = true;
    if (COMPUTER_MUTATION_METHODS.has(method)) {
      lastComputerAction = { method, result: payload.result ?? {} };
    }
    render(payload.state);
    if (payload.result?.status === "approval_required") {
      showToast(`${payload.result.risk}. Ask the human to approve this in the extension popup.`, "approval");
    } else {
      showToast(`${method} completed`);
    }
    return payload.result;
  } catch (error) {
    const errorCode = error?.code || "REQUEST_FAILED";
    const errorMessage = error?.message || "The request failed without an error detail.";
    if (COMPUTER_MUTATION_METHODS.has(method) && !responseReceived) {
      lastComputerAction = {
        method,
        failed: true,
        error: {
          code: errorCode,
          message: errorMessage,
        },
      };
      renderComputerActionEvidence();
    }
    showToast(`${errorCode}: ${errorMessage}`, "error");
    await loadState();
    return null;
  } finally {
    setBusy(false);
  }
}

async function checkForUpdate() {
  if (busy) return;
  setBusy(true);
  try {
    const payload = await request("/api/update/check", { method: "POST" });
    render(payload.state);
    showToast(payload.update.message, payload.update.status === "error" ? "error" : "normal");
  } catch (error) {
    showToast(error.message, "error");
  } finally {
    setBusy(false);
  }
}

ui["refresh-tabs"].addEventListener("click", () => runAction("tabs.list"));
ui["check-update"].addEventListener("click", () => void checkForUpdate());
ui["browser-control-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  void runAction("browser.control.start", { ttlMs: Number(ui["browser-control-ttl"].value) });
});
ui["browser-control-status"].addEventListener("click", () => void runAction("browser.control.status"));
ui["browser-control-stop"].addEventListener("click", () => {
  const sessionId = currentState?.browserControl?.sessionId;
  void runAction("browser.control.stop", sessionId ? { sessionId } : {});
});
ui["computer-status"].addEventListener("click", () => runAction("computer.status"));
ui["computer-observe"].addEventListener("click", () => runAction("computer.observe", { windowId: ui["computer-window"].value }));
ui["computer-share-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  void runAction("computer.share.start", {
    windowId: ui["computer-window"].value,
    fps: Number(ui["computer-share-fps"].value),
  });
});
ui["computer-share-stop"].addEventListener("click", () => void runAction("computer.share.stop"));
ui["computer-screenshot"].addEventListener("click", (event) => {
  const observation = currentState?.computerObservation;
  if (!observation) return;
  if (!protectedImageReady(ui["computer-screenshot"], computerImageKey(observation))) {
    showToast("Wait for the exact current frame image to finish loading before selecting coordinates.", "error");
    return;
  }
  const bounds = event.currentTarget.getBoundingClientRect();
  const x = Math.max(0, Math.min(observation.imageWidth - 1, Math.floor((event.clientX - bounds.left) * observation.imageWidth / bounds.width)));
  const y = Math.max(0, Math.min(observation.imageHeight - 1, Math.floor((event.clientY - bounds.top) * observation.imageHeight / bounds.height)));
  setSelectedComputerPoint(x, y);
});
ui["computer-click-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  const params = computerFrameParams({
    ...selectedComputerPoint(),
    button: "left",
    clickCount: 1,
    durationMs: Number(ui["computer-move-duration"].value),
  });
  if (params) runAction("computer.click", params);
});
ui["computer-move"].addEventListener("click", () => {
  const params = computerFrameParams({
    ...selectedComputerPoint(),
    durationMs: Number(ui["computer-move-duration"].value),
  });
  if (params) void runAction("computer.move", params);
});
ui["computer-double-click"].addEventListener("click", () => {
  const params = computerFrameParams({
    ...selectedComputerPoint(),
    button: "left",
    clickCount: 2,
    durationMs: Number(ui["computer-move-duration"].value),
  });
  if (params) runAction("computer.click", params);
});
for (const button of document.querySelectorAll(".computer-scroll")) {
  button.addEventListener("click", () => {
    const params = computerFrameParams({
      ...selectedComputerPoint(),
      deltaX: Number(button.dataset.x),
      deltaY: Number(button.dataset.y),
    });
    if (params) runAction("computer.scroll", params);
  });
}
ui["computer-scroll-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  const params = computerFrameParams({
    ...selectedComputerPoint(),
    deltaX: Number(ui["computer-scroll-x"].value),
    deltaY: Number(ui["computer-scroll-y"].value),
  });
  if (params) void runAction("computer.scroll", params);
});
ui["computer-drag-use-start"].addEventListener("click", () => {
  const point = selectedComputerPoint();
  ui["computer-drag-from-x"].value = String(point.x);
  ui["computer-drag-from-y"].value = String(point.y);
  showToast("Selected point copied to the drag start.");
});
ui["computer-drag-use-end"].addEventListener("click", () => {
  const point = selectedComputerPoint();
  ui["computer-drag-to-x"].value = String(point.x);
  ui["computer-drag-to-y"].value = String(point.y);
  showToast("Selected point copied to the drag end.");
});
ui["computer-drag-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  const params = computerFrameParams({
    fromX: Number(ui["computer-drag-from-x"].value),
    fromY: Number(ui["computer-drag-from-y"].value),
    toX: Number(ui["computer-drag-to-x"].value),
    toY: Number(ui["computer-drag-to-y"].value),
    durationMs: Number(ui["computer-drag-duration"].value),
  });
  if (params) void runAction("computer.drag", params);
});
ui["computer-type-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  const params = computerFrameParams({ text: ui["computer-type-text"].value });
  if (params) runAction("computer.typeText", params);
});
ui["computer-key-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  const params = computerFrameParams({ key: ui["computer-key"].value.trim() });
  if (params) runAction("computer.key", params);
});
ui["new-tab"].addEventListener("click", () => runAction("tabs.new"));
ui.observe.addEventListener("click", () => runAction("page.observe"));
ui["navigate-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.navigate", { url: ui["navigate-url"].value });
});
ui["click-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.click", { ref: ui["click-ref"].value.trim(), generation: currentState?.observation?.generation ?? "" });
});
ui["fill-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.fill", {
    ref: ui["fill-ref"].value.trim(),
    generation: currentState?.observation?.generation ?? "",
    text: ui["fill-text"].value,
  });
});
ui["select-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.select", {
    ref: ui["select-ref"].value.trim(),
    generation: currentState?.observation?.generation ?? "",
    value: ui["select-value"].value,
  });
});
for (const button of document.querySelectorAll(".page-command")) {
  button.addEventListener("click", () => runAction(button.dataset.method));
}
for (const button of document.querySelectorAll(".scroll-command")) {
  button.addEventListener("click", () => runAction("page.scroll", { deltaY: Number(button.dataset.y), deltaX: 0 }));
}
for (const button of document.querySelectorAll(".key-command")) {
  button.addEventListener("click", () => runAction("page.key", { key: button.dataset.key, generation: currentState?.observation?.generation ?? "" }));
}
ui["coordinates-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.clickAt", { x: Number(ui["coordinate-x"].value), y: Number(ui["coordinate-y"].value), button: "left", clickCount: 1, generation: currentState?.observation?.generation ?? "" });
});
ui["type-text-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.typeText", { text: ui["type-text"].value, generation: currentState?.observation?.generation ?? "" });
});
ui["custom-key-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.key", { key: ui["custom-key"].value.trim(), generation: currentState?.observation?.generation ?? "" });
});
ui["evaluate-form"].addEventListener("submit", async (event) => {
  event.preventDefault();
  const result = await runAction("page.evaluate", { expression: ui.expression.value });
  if (!result) return;
  ui["evaluation-result"].textContent = JSON.stringify(result, null, 2);
  ui["evaluation-result"].hidden = false;
});
ui["copy-agent-fetch-url"].addEventListener("click", async () => {
  const value = ui["agent-fetch-url"].value;
  if (!value) return;
  await navigator.clipboard.writeText(value);
  showToast("Private Agent Fetch base URL copied.");
});

// The pinned Chrome extension ID (matches extension/lib.js's EXTENSION_ID).
const EXTENSION_ID = "gjaniambdhcnffbapkknllilikeoopdg";

function renderExtensionFolderPath(state) {
  const path = state?.setup?.extensionPath;
  const known = typeof path === "string" && path.length > 0;
  ui["extension-folder-path-row"].hidden = !known;
  ui["extension-folder-unknown"].hidden = known;
  if (known) ui["extension-folder-path"].textContent = path;
}

// Self-contained copy/paste block for an AI assistant. Uses the exact Agent
// Fetch shape and method names from docs/API_REFERENCE.md; kept out of the
// Home-jargon-checked region below because it must use the real parameter
// names ("ref", "generation") the API actually requires.
function buildAgentInstructions(state) {
  const base = state?.agentFetch?.baseUrl ?? "";
  return [
    "Local Browser Bridge lets an AI assistant see and control a browser tab (and, if turned on, a desktop window and a local shell) on this computer. Only use it if the user asked you to.",
    "",
    `Base URL: ${base}`,
    "That's a normal address, shaped like http://127.0.0.1:<port>/api/v1/fetch/<key>. Every call below is a plain HTTP GET to it -- no headers, no request body.",
    "",
    "Add \"&callId=<a-unique-id-you-pick>\" to every call except status and tabs.list.",
    "",
    "List open tabs:",
    `GET ${base}/tabs.list`,
    "",
    "Take control of a tab (tabId is optional; omit it to use the current tab):",
    `GET ${base}/browser.control.start?callId=ctl-1&tabId=<id>`,
    "",
    "Open a page:",
    `GET ${base}/page.navigate?callId=nav-1&tabId=<id>&url=<page-url>`,
    "",
    "Read it (returns a screenshot, the page text, and clickable elements, each with a \"ref\"):",
    `GET ${base}/page.observe?callId=obs-1&tabId=<id>`,
    "",
    "Click something, using the \"ref\" and \"generation\" from that read:",
    `GET ${base}/page.click?callId=click-1&tabId=<id>&ref=<ref>&generation=<generation>`,
    "",
    "This URL is like a password: anyone who has it can control this computer. Keep it private.",
  ].join("\n");
}

// HOME:STRINGS:START
function setView(view) {
  const advanced = view === "advanced";
  ui["home-view"].hidden = advanced;
  ui["advanced-view"].hidden = !advanced;
  try { localStorage.setItem("lbbView", view); } catch {}
}

async function postSettings(body) {
  try {
    const payload = await request("/api/settings", { method: "POST", body: JSON.stringify(body) });
    render(payload.state);
  } catch (error) {
    showToast(error.message, "error");
    await loadState();
  }
}

let connectAttemptFailed = false;

function failConnectAttempt() {
  connectAttemptFailed = true;
  if (currentState) renderHome(currentState);
  ui["home-browser-setup"].scrollIntoView({ behavior: "smooth", block: "start" });
}

// Runs the browser extension one-click pairing. On a first visit (loaded
// with a URL fragment) bridgeToken is already in memory; on a return visit
// the session is restored from sessionStorage and bridgeToken is empty, so
// this asks the same-origin, session-authenticated /api/pairing endpoint for
// the values it needs -- fetched only now, on click, never on page load, and
// never persisted anywhere.
async function connectExtension() {
  if (typeof chrome === "undefined" || !chrome.runtime?.sendMessage) {
    failConnectAttempt();
    return;
  }
  let port = currentState?.setup?.port;
  let pairToken = bridgeToken;
  if (!pairToken || !port) {
    try {
      const pairing = await request("/api/pairing", { method: "POST" });
      port = pairing.port;
      pairToken = pairing.token;
      bridgeToken = pairToken;
    } catch {
      failConnectAttempt();
      return;
    }
  }
  if (!port) {
    failConnectAttempt();
    return;
  }
  try {
    const response = await new Promise((resolve, reject) => {
      chrome.runtime.sendMessage(
        EXTENSION_ID,
        { type: "lbb.pair", port, token: pairToken },
        (result) => {
          if (chrome.runtime.lastError) reject(new Error(chrome.runtime.lastError.message));
          else resolve(result);
        },
      );
    });
    if (response?.ok && response?.pending) {
      showToast("Check the new browser tab to finish connecting.");
      return;
    }
    failConnectAttempt();
  } catch {
    // Fall through to the manual steps below; never leave a dead button.
    failConnectAttempt();
  }
}

function renderHome(state) {
  const setup = state.setup ?? {};

  setStateBadge(ui["status-app-pill"], "Running", "active");

  const browserConnected = Boolean(setup.browserConnected);
  setStateBadge(ui["status-browser-pill"], browserConnected ? "Connected" : "Not connected", browserConnected ? "active" : "warning-state");
  ui["status-browser-action"].hidden = browserConnected;
  // Only show the manual add-on steps once a Connect attempt has actually
  // failed or found no way to reach the extension -- not on every render
  // while the user simply has not clicked Connect yet.
  if (browserConnected) connectAttemptFailed = false;
  ui["home-browser-setup"].hidden = browserConnected || !connectAttemptFailed;
  renderExtensionFolderPath(state);

  const desktopEnabled = Boolean(setup.desktopControlEnabled);
  const desktopConnected = Boolean(setup.desktopControlConnected);
  const desktopSetup = setup.desktopControlSetup ?? {};
  ui["status-desktop-toggle"].checked = desktopEnabled;
  if (!desktopEnabled) {
    setStateBadge(ui["status-desktop-pill"], "Off", "inactive");
    ui["status-desktop-progress"].hidden = true;
    ui["status-desktop-retry"].hidden = true;
  } else if (desktopConnected) {
    setStateBadge(ui["status-desktop-pill"], "On", "active");
    ui["status-desktop-progress"].hidden = true;
    ui["status-desktop-retry"].hidden = true;
  } else if (desktopSetup.state === "failed") {
    setStateBadge(ui["status-desktop-pill"], "Setup failed", "danger-state");
    setTextIfChanged(ui["status-desktop-progress"], desktopSetup.message || "Setup failed.");
    ui["status-desktop-progress"].hidden = false;
    ui["status-desktop-retry"].hidden = false;
  } else if (desktopSetup.state === "downloading") {
    setStateBadge(ui["status-desktop-pill"], "Setting up…", "warning-state");
    setTextIfChanged(ui["status-desktop-progress"], `${desktopSetup.message || "Getting ready"}… ${desktopSetup.percent ?? 0}%`);
    ui["status-desktop-progress"].hidden = false;
    ui["status-desktop-retry"].hidden = true;
  } else {
    // Nothing is downloading (macOS never downloads a helper at all, and a
    // Windows install may simply not have started it yet) -- so no percent
    // is shown here; that would claim progress that is not happening.
    setStateBadge(ui["status-desktop-pill"], "Not running yet", "warning-state");
    setTextIfChanged(ui["status-desktop-progress"], "Screen control is on, but its helper app isn't running yet. Start Local Computer Helper to finish connecting.");
    ui["status-desktop-progress"].hidden = false;
    ui["status-desktop-retry"].hidden = true;
  }

  const shellEnabled = Boolean(setup.shellEnabled);
  ui["status-shell-toggle"].checked = shellEnabled;
  setStateBadge(ui["status-shell-pill"], shellEnabled ? "On" : "Off", shellEnabled ? "active" : "inactive");

  ui["home-hero-action"].onclick = null;
  if (setup.ready) {
    setTextIfChanged(ui["home-hero-headline"], "Ready to use");
    ui["home-hero-action"].hidden = true;
  } else if (!browserConnected) {
    setTextIfChanged(ui["home-hero-headline"], "Connect the browser add-on to get started");
    ui["home-hero-action"].hidden = false;
    ui["home-hero-action"].textContent = "Connect";
    ui["home-hero-action"].onclick = () => void connectExtension();
  } else if (desktopSetup.state === "failed") {
    setTextIfChanged(ui["home-hero-headline"], "Screen control setup failed");
    ui["home-hero-action"].hidden = false;
    ui["home-hero-action"].textContent = "Retry";
    ui["home-hero-action"].onclick = () => void postSettings({ desktopControlEnabled: true });
  } else if (desktopSetup.state === "downloading") {
    setTextIfChanged(ui["home-hero-headline"], "Setting up screen control…");
    ui["home-hero-action"].hidden = true;
  } else {
    setTextIfChanged(ui["home-hero-headline"], "Waiting for the screen control helper to start…");
    ui["home-hero-action"].hidden = true;
  }
}

ui["show-advanced"].addEventListener("click", () => setView("advanced"));
ui["show-home"].addEventListener("click", () => setView("home"));
ui["status-desktop-toggle"].addEventListener("change", (event) => void postSettings({ desktopControlEnabled: event.target.checked }));
ui["status-shell-toggle"].addEventListener("change", (event) => void postSettings({ shellEnabled: event.target.checked }));
ui["status-desktop-retry"].addEventListener("click", () => void postSettings({ desktopControlEnabled: true }));
ui["status-browser-action"].addEventListener("click", () => void connectExtension());
ui["copy-ai-instructions"].addEventListener("click", async () => {
  await navigator.clipboard.writeText(buildAgentInstructions(currentState));
  showToast("Instructions copied. Paste them into your AI assistant.");
});
ui["copy-ai-link"].addEventListener("click", async () => {
  const url = currentState?.agentFetch?.baseUrl ?? "";
  if (!url) return;
  await navigator.clipboard.writeText(url);
  showToast("Link copied.");
});
ui["copy-extension-path"].addEventListener("click", async () => {
  await navigator.clipboard.writeText(ui["extension-folder-path"].textContent);
  showToast("Folder path copied.");
});
(() => {
  let savedView = "";
  try { savedView = localStorage.getItem("lbbView") ?? ""; } catch {}
  setView(savedView === "advanced" ? "advanced" : "home");
})();
// HOME:STRINGS:END

async function streamEvents(onEvent) {
  for (;;) {
    try {
      const response = await fetch("/api/events", {
        headers: { Authorization: `Session ${sessionToken}` },
        cache: "no-store",
      });
      if (response.status === 401) {
        window.sessionStorage.removeItem("lbbDashboardSession");
        window.location.reload();
        return;
      }
      if (!response.ok || !response.body) throw new Error(`Event stream failed (${response.status})`);
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true }).replaceAll("\r\n", "\n");
        let boundary;
        while ((boundary = buffer.indexOf("\n\n")) >= 0) {
          const block = buffer.slice(0, boundary);
          buffer = buffer.slice(boundary + 2);
          const eventName = block
            .split("\n")
            .find((line) => line.startsWith("event:"))
            ?.slice(6)
            .trim();
          if (eventName) onEvent(eventName);
        }
      }
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 2_000));
  }
}

async function boot() {
  const session = await bootstrapDashboardSession();
  csrfToken = session.csrfToken;
  await loadState();
  let lastRefresh = 0;
  let refreshTimer = null;
  const refreshFromEvent = () => {
    if (busy) return;
    const now = Date.now();
    const remaining = Math.max(0, 200 - (now - lastRefresh));
    if (remaining === 0) {
      lastRefresh = now;
      void loadState();
      return;
    }
    if (refreshTimer) return;
    refreshTimer = setTimeout(() => {
      refreshTimer = null;
      if (busy) return;
      lastRefresh = Date.now();
      void loadState();
    }, remaining);
  };
  const eventNames = new Set([
    "state", "connection", "hello", "tabs", "observation", "approval", "warning", "error", "update", "settings",
    "browser-control", "computer-connection", "computer-hello", "computer-observation", "computer-status",
    "computer-share", "computer-share-frame", "computer-share-error", "computer-warning", "computer-error",
  ]);
  void streamEvents((name) => {
    if (eventNames.has(name)) refreshFromEvent();
  });
}

boot().catch((error) => showToast(error.message, "error"));
