import { randomUUID } from "node:crypto";
import { EventEmitter } from "node:events";
import { WebSocket, WebSocketServer } from "ws";
import { tokensEqual } from "./token.js";

export class ExtensionHub extends EventEmitter {
  constructor(httpServer, { token, callTimeoutMs = 15_000, maxPayload = 8 * 1024 * 1024 }) {
    super();
    this.token = token;
    this.callTimeoutMs = callTimeoutMs;
    this.socket = null;
    this.pending = new Map();
    this.lastHello = null;
    this.wss = new WebSocketServer({ noServer: true, maxPayload });
    this.upgradeHandler = (request, socket, head) => this.#upgrade(request, socket, head);
    httpServer.on("upgrade", this.upgradeHandler);
  }

  get connected() {
    return this.socket?.readyState === WebSocket.OPEN;
  }

  #rejectUpgrade(socket, status, message) {
    socket.write(`HTTP/1.1 ${status} ${message}\r\nConnection: close\r\n\r\n`);
    socket.destroy();
  }

  #upgrade(request, socket, head) {
    let url;
    try {
      url = new URL(request.url ?? "/", "http://127.0.0.1");
    } catch {
      this.#rejectUpgrade(socket, 400, "Bad Request");
      return;
    }

    if (url.pathname !== "/bridge") {
      this.#rejectUpgrade(socket, 404, "Not Found");
      return;
    }

    const origin = request.headers.origin ?? "";
    if (!origin.startsWith("chrome-extension://")) {
      this.#rejectUpgrade(socket, 403, "Forbidden");
      return;
    }

    if (!tokensEqual(url.searchParams.get("token") ?? "", this.token)) {
      this.#rejectUpgrade(socket, 401, "Unauthorized");
      return;
    }

    this.wss.handleUpgrade(request, socket, head, (webSocket) => {
      this.wss.emit("connection", webSocket, request);
      this.#attach(webSocket, origin);
    });
  }

  #attach(socket, origin) {
    const previous = this.socket;
    this.socket = socket;
    if (previous?.readyState === WebSocket.OPEN) previous.close(4000, "Replaced by a newer extension connection");
    this.emit("connection", true);

    socket.on("message", (raw) => {
      let message;
      try {
        message = JSON.parse(raw.toString());
      } catch {
        return;
      }

      if (message.type === "ping") {
        if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({ type: "pong" }));
        return;
      }

      if (message.type === "hello") {
        this.lastHello = {
          version: String(message.version ?? "unknown"),
          browser: String(message.browser ?? "Chromium"),
          capabilities: Array.isArray(message.capabilities) ? message.capabilities.map(String) : [],
          connectedAt: new Date().toISOString(),
          origin,
        };
        this.emit("hello", this.lastHello);
        return;
      }

      if (message.type === "event") {
        this.emit("extensionEvent", {
          name: String(message.name ?? "unknown"),
          data: message.data && typeof message.data === "object" ? message.data : {},
        });
        return;
      }

      if (typeof message.id !== "string") return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      clearTimeout(pending.timer);
      if (message.ok) pending.resolve(message.result);
      else {
        const error = new Error(message.error?.message ?? message.error ?? "Unknown extension error");
        error.code = message.error?.code ?? "EXTENSION_ERROR";
        error.details = message.error?.details;
        pending.reject(error);
      }
    });

    socket.on("close", () => {
      for (const [id, pending] of this.pending) {
        if (pending.socket !== socket) continue;
        clearTimeout(pending.timer);
        this.pending.delete(id);
        pending.reject(Object.assign(new Error("Browser extension disconnected"), { code: "EXTENSION_DISCONNECTED" }));
      }
      if (this.socket === socket) {
        this.socket = null;
        this.emit("connection", false);
      }
    });

    socket.on("error", (error) => this.emit("socketError", error));
  }

  call(method, params = {}, timeoutMs = this.callTimeoutMs) {
    if (!this.connected) {
      return Promise.reject(Object.assign(new Error("Browser extension is not connected"), { code: "EXTENSION_OFFLINE" }));
    }

    const id = randomUUID();
    const socket = this.socket;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(Object.assign(new Error(`Extension command timed out: ${method}`), { code: "COMMAND_TIMEOUT" }));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer, socket });
      try {
        socket.send(JSON.stringify({ id, type: "command", method, params }));
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(error);
      }
    });
  }

  close() {
    for (const [id, pending] of this.pending) {
      clearTimeout(pending.timer);
      this.pending.delete(id);
      pending.reject(Object.assign(new Error("Bridge server stopped"), { code: "SERVER_STOPPED" }));
    }
    if (this.socket?.readyState === WebSocket.OPEN) this.socket.close(1001, "Server stopped");
    this.wss.close();
  }
}
