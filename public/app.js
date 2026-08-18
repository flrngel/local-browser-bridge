const ui = Object.fromEntries(
  [
    "connection", "connection-text", "refresh-tabs", "tabs", "new-tab", "navigate-form", "navigate-url",
    "observe", "target-meta", "screenshot", "screenshot-empty", "click-form", "click-ref", "fill-form",
    "fill-ref", "fill-text", "select-form", "select-ref", "select-value", "selected-element", "elements",
    "elements-empty", "element-count", "selected-text", "page-text", "activity", "revision", "toast",
    "coordinates-form", "coordinate-x", "coordinate-y", "type-text-form", "type-text", "custom-key-form", "custom-key",
    "evaluate-form", "expression", "evaluation-result",
    "release-panel", "current-version", "update-status", "update-detail", "check-update", "update-link",
  ].map((id) => [id, document.getElementById(id)]),
);

let csrfToken = "";
let currentState = null;
let busy = false;
let toastTimer = null;
let lastObservationGeneration = "";

function setBusy(value) {
  busy = value;
  for (const button of document.querySelectorAll("button")) button.disabled = value;
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

function formatTime(iso) {
  try { return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }); }
  catch { return ""; }
}

function renderConnection(state) {
  ui.connection.classList.toggle("online", state.connected);
  ui.connection.classList.toggle("offline", !state.connected);
  if (!state.connected) {
    ui["connection-text"].textContent = "Extension offline — open its popup to connect";
    return;
  }
  const mode = state.extension?.mode === "full-access" ? "FULL ACCESS" : "SAFE MODE";
  const versionMismatch = state.extension && state.update?.currentVersion && state.extension.version !== state.update.currentVersion;
  const versionLabel = versionMismatch ? `${state.extension.version} · VERSION MISMATCH` : state.extension?.version;
  const detail = state.extension ? `${state.extension.browser} · extension ${versionLabel} · ${mode}` : "handshake pending";
  ui["connection-text"].textContent = `Connected · ${detail}`;
}

function renderUpdate(state) {
  const update = state.update ?? {};
  const labels = {
    checking: "Checking official release metadata…",
    up_to_date: "Up to date",
    available: `Update available: ${update.latestVersion ?? "new version"}`,
    error: "Update status unavailable",
    disabled: "Automatic check disabled",
  };
  ui["current-version"].textContent = `version ${update.currentVersion ?? "unknown"}`;
  ui["update-status"].textContent = labels[update.status] ?? "Update status unavailable";
  ui["update-detail"].textContent = update.message
    ? `${update.message} The checker never downloads or installs files and sends no telemetry.`
    : "The checker contacts only GitHub release metadata. It never downloads or installs files and sends no telemetry.";
  ui["release-panel"].className = `release-panel panel update-${update.status ?? "error"}`;
  ui["check-update"].textContent = update.status === "checking" ? "Checking…" : "Check again";
  ui["update-link"].href = update.releaseUrl || "https://github.com/flrngel/local-browser-bridge/releases";
}

function renderTabs(state) {
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
    ui.screenshot.src = `${observation.screenshotUrl}&cache=${Date.now()}`;
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

function render(state) {
  currentState = state;
  renderConnection(state);
  renderUpdate(state);
  renderTabs(state);
  renderObservation(state);
  renderActivity(state);
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
  try {
    const payload = await request("/api/action", {
      method: "POST",
      body: JSON.stringify({ method, params }),
    });
    render(payload.state);
    if (payload.result?.status === "approval_required") {
      showToast(`${payload.result.risk}. Ask the human to approve this in the extension popup.`, "approval");
    } else {
      showToast(`${method} completed`);
    }
    return payload.result;
  } catch (error) {
    showToast(`${error.code ? `${error.code}: ` : ""}${error.message}`, "error");
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
  button.addEventListener("click", () => runAction("page.key", { key: button.dataset.key }));
}
ui["coordinates-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.clickAt", { x: Number(ui["coordinate-x"].value), y: Number(ui["coordinate-y"].value), button: "left", clickCount: 1 });
});
ui["type-text-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.typeText", { text: ui["type-text"].value });
});
ui["custom-key-form"].addEventListener("submit", (event) => {
  event.preventDefault();
  runAction("page.key", { key: ui["custom-key"].value.trim() });
});
ui["evaluate-form"].addEventListener("submit", async (event) => {
  event.preventDefault();
  const result = await runAction("page.evaluate", { expression: ui.expression.value });
  if (!result) return;
  ui["evaluation-result"].textContent = JSON.stringify(result, null, 2);
  ui["evaluation-result"].hidden = false;
});

async function boot() {
  const session = await request("/api/session");
  csrfToken = session.csrfToken;
  await loadState();
  const events = new EventSource("/api/events");
  let lastRefresh = 0;
  events.addEventListener("state", () => {
    const now = Date.now();
    if (now - lastRefresh < 200 || busy) return;
    lastRefresh = now;
    void loadState();
  });
  for (const name of ["connection", "hello", "tabs", "observation", "approval", "warning", "error", "update"]) {
    events.addEventListener(name, () => {
      if (!busy) void loadState();
    });
  }
  events.onerror = () => setTimeout(() => { if (!busy) void loadState(); }, 2_000);
}

boot().catch((error) => showToast(error.message, "error"));
