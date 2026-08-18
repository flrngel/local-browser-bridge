import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import WebSocket from "ws";

const port = Number(process.env.LBB_PORT || 17_373);
const tokenPath = process.env.LBB_TOKEN_PATH || join(homedir(), ".local-browser-bridge", "token");
const token = process.env.LBB_TOKEN || (await readFile(tokenPath, "utf8")).trim();
const pixel = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
let generation = "mock-1";
let displayName = "";
let selectedColor = "green";

const socket = new WebSocket(`ws://127.0.0.1:${port}/bridge?token=${encodeURIComponent(token)}`, {
  headers: { Origin: "chrome-extension://local-browser-bridge-mock" },
});

function observation() {
  generation = `mock-${Date.now()}`;
  return {
    snapshot: {
      generation,
      title: "Mock target tab",
      url: "http://127.0.0.1:9000/demo",
      viewport: { width: 1280, height: 720, devicePixelRatio: 1 },
      scroll: { x: 0, y: 0, maxY: 400 },
      bodyText: `Browser Bridge mock target. Display name: ${displayName || "empty"}. Favorite color: ${selectedColor}.`,
      elements: [
        { ref: "e1", role: "textbox", name: "Display name", type: "text", disabled: false, inViewport: true },
        { ref: "e2", role: "select", name: "Favorite color", type: "", disabled: false, inViewport: true },
        { ref: "e3", role: "button", name: "Show greeting", type: "submit", disabled: false, inViewport: true },
      ],
    },
    screenshot: pixel,
  };
}

function handle(method, params) {
  switch (method) {
    case "status": return { connected: true, mock: true, fullAccess: true };
    case "tabs.list": return { activeTabId: 101, tabs: [{ id: 101, title: "Mock target tab", url: "http://127.0.0.1:9000/demo", active: true }] };
    case "tabs.activate": return { tabId: params.tabId, active: true };
    case "tabs.new": return { tabId: 102 };
    case "tabs.close": return { closed: true, tabId: params.tabId };
    case "page.observe": return observation();
    case "page.fill": displayName = String(params.text ?? ""); return { filled: true };
    case "page.select": selectedColor = String(params.value ?? ""); return { selected: selectedColor };
    case "page.click": return { clicked: true, trusted: true };
    case "page.navigate": return { tabId: params.tabId, url: params.url };
    case "page.back": case "page.forward": case "page.reload": return { ok: true };
    case "page.key": return { pressed: params.key };
    case "page.scroll": return { x: params.deltaX, y: params.deltaY };
    case "page.clickAt": return { clicked: true, trusted: true, x: params.x, y: params.y, button: params.button, clickCount: params.clickCount };
    case "page.typeText": return { typed: true, length: String(params.text ?? "").length };
    case "page.evaluate": return { type: "string", value: `mock:${params.expression}` };
    default: throw new Error(`Unsupported mock command: ${method}`);
  }
}

socket.on("open", () => {
  socket.send(JSON.stringify({
    type: "hello",
    version: "0.2.0-mock",
    browser: "Mock Chromium",
    mode: "full-access",
    capabilities: ["tabs.list", "page.observe", "page.click", "page.fill", "page.select", "page.clickAt", "page.typeText", "page.evaluate"],
  }));
  console.log(`Mock extension connected to ws://127.0.0.1:${port}/bridge`);
});

socket.on("message", (raw) => {
  const message = JSON.parse(raw.toString());
  if (message.type === "pong") return;
  if (message.type !== "command") return;
  try {
    const result = handle(message.method, message.params ?? {});
    socket.send(JSON.stringify({ id: message.id, type: "result", ok: true, result }));
  } catch (error) {
    socket.send(JSON.stringify({ id: message.id, type: "result", ok: false, error: { code: "MOCK_ERROR", message: error.message } }));
  }
});

socket.on("error", (error) => {
  console.error(error.message);
  process.exitCode = 1;
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => socket.close(1000, "Mock stopped"));
}
