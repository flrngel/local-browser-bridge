const byId = (id) => document.getElementById(id);
const ui = {
  request: byId("request"),
  expired: byId("expired"),
  origin: byId("origin-text"),
  port: byId("port-text"),
  connect: byId("connect"),
  cancel: byId("cancel"),
  message: byId("message"),
};

// This tab was opened for exactly one pairing request, identified by the id
// in its own URL. Every message back to the background page carries that id
// so a superseded tab (a newer lbb.pair message replaced the pending slot
// after this tab was opened) can never be served, or complete, someone
// else's request - the background rejects a mismatched id instead of
// silently acting on whatever is currently pending.
const requestId = new URLSearchParams(location.search).get("request") ?? "";

function call(action) {
  return chrome.runtime.sendMessage({ type: "LBB_PAIR", action, requestId }).then((response) => {
    if (!response?.ok) throw new Error(response?.error ?? "Pairing request failed");
    return response.result;
  });
}

function showExpired() {
  ui.request.hidden = true;
  ui.expired.hidden = false;
}

function showMessage(text) {
  ui.message.textContent = text;
  ui.message.hidden = false;
}

async function closeThisTab() {
  const tab = await chrome.tabs.getCurrent();
  if (Number.isInteger(tab?.id)) await chrome.tabs.remove(tab.id);
}

async function init() {
  try {
    const pending = await call("getPending");
    if (!pending) {
      showExpired();
      return;
    }
    ui.origin.textContent = pending.origin;
    ui.port.textContent = String(pending.port);
  } catch {
    showExpired();
  }
}

ui.connect.addEventListener("click", () => {
  ui.connect.disabled = true;
  ui.cancel.disabled = true;
  call("connect")
    .then(() => {
      ui.request.hidden = true;
      showMessage("Connected. This tab will close automatically.");
      setTimeout(() => void closeThisTab(), 1_200);
    })
    .catch((error) => {
      ui.connect.disabled = false;
      ui.cancel.disabled = false;
      showMessage(error.message);
    });
});

ui.cancel.addEventListener("click", () => {
  call("cancel").catch(() => {}).then(() => closeThisTab());
});

void init();
