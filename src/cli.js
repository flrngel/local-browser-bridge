#!/usr/bin/env node
import { homedir } from "node:os";
import { join } from "node:path";
import { createBridgeServer } from "./server.js";
import { loadOrCreateToken } from "./token.js";

function parsePort(raw) {
  const port = Number(raw ?? 17_373);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) throw new Error("LBB_PORT must be an integer between 1 and 65535");
  return port;
}

const port = parsePort(process.env.LBB_PORT);
const tokenPath = process.env.LBB_TOKEN_PATH || join(homedir(), ".local-browser-bridge", "token");
const token = process.env.LBB_TOKEN || (await loadOrCreateToken(tokenPath));
const bridge = createBridgeServer({ port, token });

await bridge.listen();

console.log(`Local Browser Bridge 0.2.0`);
console.log(`Control surface: http://127.0.0.1:${port}`);
console.log(`Extension token: ${token}`);
console.log(process.env.LBB_TOKEN ? "Token source: LBB_TOKEN" : `Token file: ${tokenPath}`);
console.log("The server is loopback-only. Press Ctrl+C to stop.");

let stopping = false;
async function stop(signal) {
  if (stopping) return;
  stopping = true;
  console.log(`Stopping after ${signal}...`);
  await bridge.close();
  process.exitCode = 0;
}

process.on("SIGINT", () => void stop("SIGINT"));
process.on("SIGTERM", () => void stop("SIGTERM"));
