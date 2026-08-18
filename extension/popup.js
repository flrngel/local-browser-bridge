import { VERSION } from "./lib.js";

const byId = (id) => document.getElementById(id);
const ui = {
  version: byId("version"), status: byId("status"), enabled: byId("enabled"), fullAccess: byId("full-access"), modeStatus: byId("mode-status"),
  connectionForm: byId("connection-form"), port: byId("port"),
  token: byId("token"), approvalSection: byId("approval-section"), approvalDetail: byId("approval-detail"),
  approve: byId("approve"), reject: byId("reject"), currentSite: byId("current-site"), allowCurrent: byId("allow-current"),
  hostForm: byId("host-form"), host: byId("host"), hosts: byId("hosts"), allowedSitesSection: byId("allowed-sites-section"), message: byId("message"),
};
let state = null;
ui.version.textContent = `Version ${VERSION}`;

function call(action, extra = {}) {
  return chrome.runtime.sendMessage({ type: "LBB_POPUP", action, ...extra }).then((response) => {
    if (!response?.ok) throw new Error(response?.error ?? "Extension request failed");
    return response.result;
  });
}

function showMessage(message) {
  ui.message.textContent = message;
  ui.message.hidden = false;
  setTimeout(() => { ui.message.hidden = true; }, 4_000);
}

function render(next) {
  state = next;
  ui.status.textContent = `${next.connectionStatus}: ${next.connectionDetail}`;
  ui.enabled.checked = next.enabled;
  ui.fullAccess.checked = next.fullAccess;
  ui.modeStatus.textContent = next.fullAccess
    ? "ON — safety interlocks are bypassed. The allowlist below is ignored."
    : "OFF — allowlist, sensitive-field blocks, and one-time approvals are enforced.";
  ui.modeStatus.classList.toggle("active", next.fullAccess);
  ui.allowedSitesSection.classList.toggle("inactive", next.fullAccess);
  ui.port.value = next.port;
  ui.token.placeholder = next.tokenConfigured ? "Saved — leave blank to keep it" : "Paste the server token";
  ui.currentSite.textContent = `Current site: ${next.currentHost || "unavailable"}${next.currentHostAllowed ? " (allowed)" : ""}`;
  ui.allowCurrent.disabled = !next.currentHost || next.currentHostAllowed;
  ui.hosts.replaceChildren();
  for (const host of next.allowedHosts) {
    const item = document.createElement("li");
    const label = document.createElement("span");
    label.textContent = host;
    const remove = document.createElement("button");
    remove.type = "button";
    remove.textContent = "Remove";
    remove.addEventListener("click", () => update("removeHost", { host }));
    item.append(label, remove);
    ui.hosts.append(item);
  }
  ui.approvalSection.hidden = !next.pendingApproval;
  if (next.pendingApproval) {
    ui.approvalDetail.textContent = `${next.pendingApproval.risk}: “${next.pendingApproval.label}”. Review the target page, then approve once or reject.`;
  }
}

async function update(action, extra = {}) {
  try { render(await call(action, extra)); }
  catch (error) { showMessage(error.message); }
}

ui.connectionForm.addEventListener("submit", (event) => {
  event.preventDefault();
  void update("saveConnection", { port: Number(ui.port.value), token: ui.token.value });
  ui.token.value = "";
});
ui.enabled.addEventListener("change", () => void update("toggleEnabled", { enabled: ui.enabled.checked }));
ui.fullAccess.addEventListener("change", () => void update("toggleFullAccess", { fullAccess: ui.fullAccess.checked }));
ui.allowCurrent.addEventListener("click", () => void update("allowCurrent"));
ui.hostForm.addEventListener("submit", (event) => {
  event.preventDefault();
  void update("addHost", { host: ui.host.value });
  ui.host.value = "";
});
ui.approve.addEventListener("click", () => state?.pendingApproval && void update("approve", { id: state.pendingApproval.id }));
ui.reject.addEventListener("click", () => state?.pendingApproval && void update("reject", { id: state.pendingApproval.id }));

update("getState");
