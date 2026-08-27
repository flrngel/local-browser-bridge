#!/usr/bin/env node

import { createHash } from "node:crypto";
import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { isAbsolute, join, resolve } from "node:path";

const PRODUCT_VERSION = "0.12.39";
const SCHEMA_VERSION = 2;
const OPERATOR_DIRECTORY = "operator";
const REQUEST_FILE = "macos-app-share-concurrency-handoff-request.json";
const START_FILE = "macos-app-share-concurrency-handoff-start.json";
const COMPLETE_FILE = "macos-app-share-concurrency-handoff-complete.json";
const REQUEST_KIND = "macos-app-share-concurrency-handoff-request";
const START_KIND = "macos-app-share-concurrency-handoff-start";
const COMPLETE_KIND = "macos-app-share-concurrency-handoff-complete";
const BUNDLE_IDENTIFIER = "dev.flrngel.local-browser-bridge.acceptance.app-share";
const WINDOW_TITLE = "LBB macOS Acceptance App Share";
const BUTTON_TEXT = "START APP-SHARE CHECK";
const BUTTON_IDENTIFIER = "lbb-app-share-start";
const MAX_MARKER_BYTES = 16_384;
const MAX_REQUEST_LIFETIME_MS = 300_000;
const MAX_ACTION_TO_COMPLETE_MS = 10_000;
const FUTURE_TOLERANCE_MS = 1_000;
const FILE_TIME_TOLERANCE_MS = 5_000;
const QUIET_SEAT_MAXIMUM_WAIT_MS = 30 * 60_000;
const PRODUCER_PRE_REQUEST_WORK_BUDGET_MS = 30 * 60_000;
const REQUEST_PUBLICATION_MAXIMUM_WAIT_MS =
  QUIET_SEAT_MAXIMUM_WAIT_MS + PRODUCER_PRE_REQUEST_WORK_BUDGET_MS;
const EVIDENCE_DIRECTORY_WAIT_TIMEOUT_MS = 60_000;
const POLL_MS = 100;
const MAX_PID = 2_147_483_647;

const REQUEST_FIELDS = [
  "schemaVersion", "kind", "productVersion", "requestId", "createdAt", "expiresAt",
  "runnerPid", "promptPid", "expectedBundleIdentifier", "expectedWindowTitle",
  "expectedButtonText", "expectedButtonAccessibilityIdentifier",
  "expectedButtonEnabledAfterDelivery", "exactAppObserved", "exactWindowObserved",
  "requestDelivered", "panelOnScreen", "panelNonactivating", "notificationOnly",
  "exactAppShareRequired", "physicalHumanProvenanceRequired",
  "acceptedAsProductAuthority",
];

const START_FIELDS = [
  "acceptedAsAuthority", "buttonAccepted", "buttonActionObserved", "createdAt",
  "cryptographicToolIdentityClaimed", "kind", "physicalHumanProvenanceClaimed",
  "productVersion", "promptPid", "requestId", "requestSha256", "schemaVersion",
];

const COMPLETE_FIELDS = [
  "acceptedAsAuthority", "buttonRemainedDisabledDuringProductAction", "createdAt",
  "cryptographicToolIdentityClaimed", "handoffStateSequenceBound", "kind",
  "physicalHumanProvenanceClaimed", "productActionCompletedAt", "productActionStartedAt",
  "productVersion", "promptPid", "requestId", "requestSha256", "schemaVersion",
  "startReceiptSha256",
];

class WatcherError extends Error {}

function fail(message) {
  throw new WatcherError(message);
}

function exactKeys(value, expected, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be one JSON object.`);
  }
  const actual = Object.keys(value);
  if (actual.length !== expected.length || actual.some((field, index) => field !== expected[index])) {
    fail(`${label} fields are not in exact canonical order.`);
  }
}

function exactInteger(value, minimum, maximum, label) {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    fail(`${label} must be an integer from ${minimum} through ${maximum}.`);
  }
}

function exactBoolean(value, expected, label) {
  if (typeof value !== "boolean" || value !== expected) fail(`${label} must be ${expected}.`);
}

function canonicalTimestamp(value, label) {
  if (
    typeof value !== "string" ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(value)
  ) {
    fail(`${label} must be a canonical UTC timestamp.`);
  }
  const milliseconds = Date.parse(value);
  if (!Number.isFinite(milliseconds) || new Date(milliseconds).toISOString() !== value) {
    fail(`${label} must be a real canonical UTC timestamp.`);
  }
  return milliseconds;
}

function parseExactJson(text, fields, label) {
  if (
    typeof text !== "string" || text.length === 0 ||
    Buffer.byteLength(text, "utf8") > MAX_MARKER_BYTES || text.includes("\0") ||
    text.charCodeAt(0) === 0xfeff || !text.endsWith("\n")
  ) {
    fail(`${label} has invalid bytes or size.`);
  }
  let marker;
  try {
    marker = JSON.parse(text);
  } catch {
    fail(`${label} is not one complete JSON object.`);
  }
  exactKeys(marker, fields, label);
  if (`${JSON.stringify(marker)}\n` !== text) fail(`${label} is not canonical compact JSON.`);
  return marker;
}

function sha256(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

function validateFileTimestamp(createdAtMs, fileMtimeMs, nowMs, label) {
  if (createdAtMs > nowMs + FUTURE_TOLERANCE_MS) fail(`${label} is future-dated.`);
  if (!Number.isFinite(fileMtimeMs) || Math.abs(fileMtimeMs - createdAtMs) > FILE_TIME_TOLERANCE_MS) {
    fail(`${label} file timestamp is not bound to createdAt.`);
  }
}

function validateRequest(record, expectedRunnerPid, nowMs) {
  const marker = parseExactJson(record.text, REQUEST_FIELDS, "request marker");
  exactInteger(marker.schemaVersion, SCHEMA_VERSION, SCHEMA_VERSION, "request schemaVersion");
  if (marker.kind !== REQUEST_KIND || marker.productVersion !== PRODUCT_VERSION) {
    fail("request marker kind or productVersion is invalid.");
  }
  if (!/^[0-9a-f]{32}$/.test(marker.requestId)) fail("requestId is not canonical.");
  const createdAtMs = canonicalTimestamp(marker.createdAt, "request createdAt");
  const expiresAtMs = canonicalTimestamp(marker.expiresAt, "request expiresAt");
  if (
    expiresAtMs <= createdAtMs || expiresAtMs - createdAtMs > MAX_REQUEST_LIFETIME_MS ||
    nowMs > expiresAtMs + FUTURE_TOLERANCE_MS
  ) {
    fail("request marker is expired or has an invalid lifetime.");
  }
  validateFileTimestamp(createdAtMs, record.mtimeMs, nowMs, "request marker");
  exactInteger(marker.runnerPid, 1, MAX_PID, "request runnerPid");
  exactInteger(marker.promptPid, 1, MAX_PID, "request promptPid");
  if (marker.runnerPid !== expectedRunnerPid || marker.promptPid === marker.runnerPid) {
    fail("request process binding is invalid.");
  }
  if (
    marker.expectedBundleIdentifier !== BUNDLE_IDENTIFIER ||
    marker.expectedWindowTitle !== WINDOW_TITLE || marker.expectedButtonText !== BUTTON_TEXT ||
    marker.expectedButtonAccessibilityIdentifier !== BUTTON_IDENTIFIER
  ) {
    fail("request exact-app surface binding is invalid.");
  }
  for (const [field, expected] of [
    ["expectedButtonEnabledAfterDelivery", true], ["exactAppObserved", true],
    ["exactWindowObserved", true], ["requestDelivered", true], ["panelOnScreen", true],
    ["panelNonactivating", true], ["notificationOnly", false],
    ["exactAppShareRequired", true], ["physicalHumanProvenanceRequired", false],
    ["acceptedAsProductAuthority", false],
  ]) exactBoolean(marker[field], expected, `request ${field}`);
  return { marker, createdAtMs, expiresAtMs, sha256: sha256(record.text) };
}

function validateStart(record, request, nowMs) {
  const marker = parseExactJson(record.text, START_FIELDS, "start receipt");
  exactInteger(marker.schemaVersion, SCHEMA_VERSION, SCHEMA_VERSION, "start schemaVersion");
  if (
    marker.kind !== START_KIND || marker.productVersion !== PRODUCT_VERSION ||
    marker.requestId !== request.marker.requestId || marker.requestSha256 !== request.sha256 ||
    marker.promptPid !== request.marker.promptPid
  ) {
    fail("start receipt request binding is invalid.");
  }
  for (const [field, expected] of [
    ["acceptedAsAuthority", false], ["buttonAccepted", true],
    ["buttonActionObserved", true], ["cryptographicToolIdentityClaimed", false],
    ["physicalHumanProvenanceClaimed", false],
  ]) exactBoolean(marker[field], expected, `start ${field}`);
  const createdAtMs = canonicalTimestamp(marker.createdAt, "start createdAt");
  if (createdAtMs < request.createdAtMs || createdAtMs > request.expiresAtMs) {
    fail("start receipt is outside its request interval.");
  }
  validateFileTimestamp(createdAtMs, record.mtimeMs, nowMs, "start receipt");
  return { marker, createdAtMs, sha256: sha256(record.text) };
}

function validateComplete(record, request, start, nowMs) {
  const marker = parseExactJson(record.text, COMPLETE_FIELDS, "complete receipt");
  exactInteger(marker.schemaVersion, SCHEMA_VERSION, SCHEMA_VERSION, "complete schemaVersion");
  if (
    marker.kind !== COMPLETE_KIND || marker.productVersion !== PRODUCT_VERSION ||
    marker.requestId !== request.marker.requestId || marker.requestSha256 !== request.sha256 ||
    marker.startReceiptSha256 !== start.sha256 || marker.promptPid !== request.marker.promptPid
  ) {
    fail("complete receipt request/start binding is invalid.");
  }
  for (const [field, expected] of [
    ["acceptedAsAuthority", false], ["buttonRemainedDisabledDuringProductAction", true],
    ["cryptographicToolIdentityClaimed", false], ["handoffStateSequenceBound", true],
    ["physicalHumanProvenanceClaimed", false],
  ]) exactBoolean(marker[field], expected, `complete ${field}`);
  const startedAtMs = canonicalTimestamp(marker.productActionStartedAt, "product action start");
  const completedAtMs = canonicalTimestamp(marker.productActionCompletedAt, "product action completion");
  const createdAtMs = canonicalTimestamp(marker.createdAt, "complete createdAt");
  if (
    startedAtMs < start.createdAtMs || completedAtMs < startedAtMs ||
    startedAtMs - start.createdAtMs > MAX_ACTION_TO_COMPLETE_MS ||
    completedAtMs - startedAtMs > MAX_ACTION_TO_COMPLETE_MS ||
    createdAtMs < completedAtMs || createdAtMs - startedAtMs > MAX_ACTION_TO_COMPLETE_MS ||
    createdAtMs - start.createdAtMs > MAX_ACTION_TO_COMPLETE_MS
  ) {
    fail("complete receipt timestamps are outside the bound product-action interval.");
  }
  validateFileTimestamp(createdAtMs, record.mtimeMs, nowMs, "complete receipt");
  return marker;
}

function sameFile(before, after) {
  return before.dev === after.dev && before.ino === after.ino && before.size === after.size &&
    before.mtimeNs === after.mtimeNs && before.nlink === after.nlink;
}

function ordinaryMarker(stats, label) {
  if (!stats.isFile() || stats.isSymbolicLink() || stats.nlink !== 1n) {
    fail(`${label} must be one ordinary, singly linked file.`);
  }
  if ((stats.mode & 0o077n) !== 0n) fail(`${label} must be owner-private.`);
  if (typeof process.getuid === "function" && stats.uid !== BigInt(process.getuid())) {
    fail(`${label} must be owned by the watcher user.`);
  }
  if (stats.size < 1n || stats.size > BigInt(MAX_MARKER_BYTES)) fail(`${label} has invalid size.`);
}

async function readStableMarker(path, label) {
  let handle;
  try {
    handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW);
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    fail(`${label} could not be opened read-only without following links.`);
  }
  try {
    const before = await handle.stat({ bigint: true });
    ordinaryMarker(before, label);
    const bytes = await handle.readFile();
    const after = await handle.stat({ bigint: true });
    const pathAfter = await lstat(path, { bigint: true });
    ordinaryMarker(after, label);
    ordinaryMarker(pathAfter, label);
    if (!sameFile(before, after) || !sameFile(after, pathAfter) || BigInt(bytes.length) !== after.size) {
      fail(`${label} changed while read.`);
    }
    let text;
    try {
      text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } catch {
      fail(`${label} is not strict UTF-8.`);
    }
    return {
      text,
      mtimeMs: Number(after.mtimeNs / 1_000_000n),
      identity: `${after.dev}:${after.ino}:${after.size}:${after.mtimeNs}`,
    };
  } finally {
    try { await handle.close(); } catch { fail(`${label} handle did not close cleanly.`); }
  }
}

async function readEvidenceMarker(evidenceDir, name, label) {
  const operatorPath = join(evidenceDir, OPERATOR_DIRECTORY);
  let before;
  try {
    before = await lstat(operatorPath, { bigint: true });
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    fail("operator directory could not be inspected safely.");
  }
  if (!before.isDirectory() || before.isSymbolicLink() || (before.mode & 0o077n) !== 0n) {
    fail("operator directory must be ordinary and owner-private.");
  }
  if (typeof process.getuid === "function" && before.uid !== BigInt(process.getuid())) {
    fail("operator directory owner is invalid.");
  }
  if ((await realpath(operatorPath)) !== operatorPath) fail("operator directory is not canonical.");
  const record = await readStableMarker(join(operatorPath, name), label);
  const after = await lstat(operatorPath, { bigint: true });
  if (
    !after.isDirectory() || after.isSymbolicLink() || before.dev !== after.dev ||
    before.ino !== after.ino || before.mode !== after.mode || before.uid !== after.uid
  ) fail("operator directory changed while read.");
  return record;
}

function processAlive(pid) {
  try { process.kill(pid, 0); return true; } catch (error) {
    if (error?.code === "EPERM") return true;
    if (error?.code === "ESRCH") return false;
    throw error;
  }
}

const sleep = (milliseconds) => new Promise((resolveSleep) => setTimeout(resolveSleep, milliseconds));

async function watchHandoff({ runnerPid, loadMarker, isAlive, now, pause, emit,
  timeoutMs = REQUEST_PUBLICATION_MAXIMUM_WAIT_MS }) {
  const requestDeadline = now() + timeoutMs;
  let requestRecord;
  let request;
  while (now() <= requestDeadline) {
    requestRecord = await loadMarker(REQUEST_FILE, "request marker");
    if (requestRecord) {
      request = validateRequest(requestRecord, runnerPid, now());
      break;
    }
    if ((await loadMarker(START_FILE, "start receipt")) ||
        (await loadMarker(COMPLETE_FILE, "complete receipt"))) {
      fail("an app-share receipt appeared without its request marker.");
    }
    if (!isAlive(runnerPid)) fail("the watched macOS acceptance runner is not alive.");
    await pause(POLL_MS);
  }
  if (!request) fail("timed out waiting for a fresh macOS app-share request marker.");

  let startRecord = await loadMarker(START_FILE, "start receipt");
  let start = startRecord ? validateStart(startRecord, request, now()) : null;
  if (!start) {
    if (!isAlive(runnerPid) || !isAlive(request.marker.promptPid)) {
      fail("the bound runner or app exited before the exact-app-share action.");
    }
    emit(`ACTION REQUIRED: In the exact app share for ${BUNDLE_IDENTIFIER}, press ${BUTTON_TEXT} exactly once. Do not use the shared desktop or retry.`);
  }

  while (!start && now() <= request.expiresAtMs + FUTURE_TOLERANCE_MS) {
    const currentRequest = await loadMarker(REQUEST_FILE, "request marker");
    if (!currentRequest || currentRequest.text !== requestRecord.text ||
        currentRequest.identity !== requestRecord.identity) {
      fail("the request marker disappeared or changed after notification.");
    }
    startRecord = await loadMarker(START_FILE, "start receipt");
    if (startRecord) {
      start = validateStart(startRecord, request, now());
      break;
    }
    if ((await loadMarker(COMPLETE_FILE, "complete receipt")) !== null) {
      fail("a complete receipt appeared before its start receipt.");
    }
    if (!isAlive(runnerPid) || !isAlive(request.marker.promptPid)) {
      fail("the bound runner or app exited before the exact-app-share action.");
    }
    await pause(POLL_MS);
  }
  if (!start) fail("timed out waiting for the exact-app-share start receipt.");
  emit("START RECEIVED: The bound button action was recorded. Stop all UI use while the product action finishes.");

  const completionDeadline = start.createdAtMs + MAX_ACTION_TO_COMPLETE_MS + FUTURE_TOLERANCE_MS;
  while (now() <= completionDeadline) {
    const currentRequest = await loadMarker(REQUEST_FILE, "request marker");
    const currentStart = await loadMarker(START_FILE, "start receipt");
    if (!currentRequest || !currentStart || currentRequest.text !== requestRecord.text ||
        currentRequest.identity !== requestRecord.identity || currentStart.text !== startRecord.text ||
        currentStart.identity !== startRecord.identity) {
      fail("the bound request/start chain disappeared or changed before completion.");
    }
    const completeRecord = await loadMarker(COMPLETE_FILE, "complete receipt");
    if (completeRecord) {
      validateComplete(completeRecord, request, start, now());
      emit("COMPLETE: Exact-app-share orchestration and the quiet shared-seat product boundary are ready for independent result verification.");
      return;
    }
    if (!isAlive(runnerPid) || !isAlive(request.marker.promptPid)) {
      fail("the bound runner or app exited before completion.");
    }
    await pause(POLL_MS);
  }
  fail("timed out waiting for the bound macOS app-share completion receipt.");
}

function parseArguments(argv) {
  const options = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const name = argv[index];
    const value = argv[index + 1];
    if (!name?.startsWith("--") || value === undefined || value.startsWith("--")) {
      fail("arguments must be exact --name value pairs.");
    }
    if (options.has(name)) fail(`duplicate argument: ${name}`);
    options.set(name, value);
  }
  const allowed = new Set(["--mode", "--evidence-dir", "--runner-pid"]);
  for (const name of options.keys()) if (!allowed.has(name)) fail(`unknown argument: ${name}`);
  const mode = options.get("--mode");
  if (mode === "self-test" && options.size === 1) return { mode };
  if (mode !== "watch" || options.size !== 3) {
    fail("usage: --mode watch --evidence-dir <absolute-path> --runner-pid <pid>");
  }
  const evidenceDir = options.get("--evidence-dir");
  if (!isAbsolute(evidenceDir)) fail("--evidence-dir must be absolute.");
  const runnerPidText = options.get("--runner-pid");
  if (!/^[1-9][0-9]*$/.test(runnerPidText)) fail("--runner-pid must be canonical.");
  const runnerPid = Number(runnerPidText);
  exactInteger(runnerPid, 1, MAX_PID, "--runner-pid");
  return { mode, evidenceDir: resolve(evidenceDir), runnerPid };
}

async function inspectEvidenceDirectory(path) {
  let stats;
  try { stats = await lstat(path, { bigint: true }); } catch (error) {
    if (error?.code === "ENOENT") return false;
    fail("--evidence-dir could not be inspected safely.");
  }
  if (!stats.isDirectory() || stats.isSymbolicLink() || (stats.mode & 0o077n) !== 0n) {
    fail("--evidence-dir must be an ordinary owner-private directory.");
  }
  if ((await realpath(path)) !== path) fail("--evidence-dir must be canonical.");
  return true;
}

async function waitForEvidenceDirectory({ runnerPid, inspect, isAlive, now, pause,
  timeoutMs = EVIDENCE_DIRECTORY_WAIT_TIMEOUT_MS }) {
  const deadline = now() + timeoutMs;
  while (now() <= deadline) {
    if (await inspect()) return;
    if (!isAlive(runnerPid)) fail("runner exited before creating its evidence directory.");
    await pause(POLL_MS);
  }
  fail("timed out waiting for the runner-created evidence directory.");
}

function requestMarker(createdAt, expiresAt, runnerPid, promptPid) {
  return {
    schemaVersion: 2,
    kind: REQUEST_KIND,
    productVersion: PRODUCT_VERSION,
    requestId: "0123456789abcdef0123456789abcdef",
    createdAt,
    expiresAt,
    runnerPid,
    promptPid,
    expectedBundleIdentifier: BUNDLE_IDENTIFIER,
    expectedWindowTitle: WINDOW_TITLE,
    expectedButtonText: BUTTON_TEXT,
    expectedButtonAccessibilityIdentifier: BUTTON_IDENTIFIER,
    expectedButtonEnabledAfterDelivery: true,
    exactAppObserved: true,
    exactWindowObserved: true,
    requestDelivered: true,
    panelOnScreen: true,
    panelNonactivating: true,
    notificationOnly: false,
    exactAppShareRequired: true,
    physicalHumanProvenanceRequired: false,
    acceptedAsProductAuthority: false,
  };
}

function record(marker, mtimeMs) {
  const text = `${JSON.stringify(marker)}\n`;
  return { text, mtimeMs, identity: `self:${mtimeMs}:${sha256(text)}` };
}

async function expectFailure(action, text) {
  try { await action(); } catch (error) {
    if (error instanceof Error && error.message.includes(text)) return;
    throw error;
  }
  fail(`self-test expected failure containing: ${text}`);
}

async function selfTest() {
  const nowMs = Date.parse("2026-08-24T12:00:00.000Z");
  const runnerPid = 41001;
  const promptPid = 41002;
  const requestRecord = record(requestMarker(
    new Date(nowMs - 1_000).toISOString(),
    new Date(nowMs + 299_000).toISOString(),
    runnerPid,
    promptPid,
  ), nowMs - 1_000);
  const request = validateRequest(requestRecord, runnerPid, nowMs);
  const startRecord = record({
    acceptedAsAuthority: false,
    buttonAccepted: true,
    buttonActionObserved: true,
    createdAt: new Date(nowMs).toISOString(),
    cryptographicToolIdentityClaimed: false,
    kind: START_KIND,
    physicalHumanProvenanceClaimed: false,
    productVersion: PRODUCT_VERSION,
    promptPid,
    requestId: request.marker.requestId,
    requestSha256: request.sha256,
    schemaVersion: 2,
  }, nowMs);
  const start = validateStart(startRecord, request, nowMs);
  const completeRecord = record({
    acceptedAsAuthority: false,
    buttonRemainedDisabledDuringProductAction: true,
    createdAt: new Date(nowMs + 2_100).toISOString(),
    cryptographicToolIdentityClaimed: false,
    handoffStateSequenceBound: true,
    kind: COMPLETE_KIND,
    physicalHumanProvenanceClaimed: false,
    productActionCompletedAt: new Date(nowMs + 2_000).toISOString(),
    productActionStartedAt: new Date(nowMs + 100).toISOString(),
    productVersion: PRODUCT_VERSION,
    promptPid,
    requestId: request.marker.requestId,
    requestSha256: request.sha256,
    schemaVersion: 2,
    startReceiptSha256: start.sha256,
  }, nowMs + 2_100);
  validateComplete(completeRecord, request, start, nowMs + 2_100);
  await expectFailure(
    () => Promise.resolve(validateComplete(
      completeRecord,
      request,
      { ...start, createdAtMs: nowMs - MAX_ACTION_TO_COMPLETE_MS },
      nowMs + 2_100,
    )),
    "bound product-action interval",
  );

  const emissions = [];
  let actionSent = false;
  let startSent = false;
  let clock = nowMs;
  await watchHandoff({
    runnerPid,
    loadMarker: async (name) => {
      if (name === REQUEST_FILE) return requestRecord;
      if (name === START_FILE) return actionSent ? startRecord : null;
      return startSent ? completeRecord : null;
    },
    isAlive: (pid) => pid === runnerPid || pid === promptPid,
    now: () => clock,
    pause: async () => {},
    emit: (message) => {
      emissions.push(message);
      if (message.startsWith("ACTION REQUIRED:")) actionSent = true;
      if (message.startsWith("START RECEIVED:")) {
        startSent = true;
        clock = nowMs + 2_100;
      }
    },
    timeoutMs: 1_000,
  });
  if (
    emissions.length !== 3 || !emissions[0].includes(BUNDLE_IDENTIFIER) ||
    !emissions[0].includes(BUTTON_TEXT) || !emissions[2].startsWith("COMPLETE:")
  ) fail("self-test did not emit the exact action/start/complete sequence.");

  const catchUp = [];
  await watchHandoff({
    runnerPid,
    loadMarker: async (name) => name === REQUEST_FILE ? requestRecord
      : name === START_FILE ? startRecord : completeRecord,
    isAlive: () => false,
    now: () => nowMs + 2_100,
    pause: async () => {},
    emit: (message) => catchUp.push(message),
    timeoutMs: 1_000,
  });
  if (catchUp.some((message) => message.startsWith("ACTION REQUIRED:")) ||
      !catchUp.at(-1)?.startsWith("COMPLETE:")) {
    fail("self-test catch-up requested a duplicate action or missed completion.");
  }

  await expectFailure(
    () => Promise.resolve(validateStart(record({
      ...JSON.parse(startRecord.text), requestSha256: "f".repeat(64),
    }, nowMs), request, nowMs)),
    "request binding",
  );
  await expectFailure(
    () => Promise.resolve(validateComplete(record({
      ...JSON.parse(completeRecord.text), startReceiptSha256: "e".repeat(64),
    }, nowMs + 2_100), request, start, nowMs + 2_100)),
    "request/start binding",
  );
  const expiredRecord = record(requestMarker(
    new Date(nowMs - 302_000).toISOString(),
    new Date(nowMs - 2_000).toISOString(),
    runnerPid,
    promptPid,
  ), nowMs - 302_000);
  await expectFailure(() => Promise.resolve(validateRequest(expiredRecord, runnerPid, nowMs)), "expired");
  await expectFailure(
    () => Promise.resolve(validateRequest(record({
      ...JSON.parse(requestRecord.text), unexpected: true,
    }, nowMs - 1_000), runnerPid, nowMs)),
    "canonical order",
  );
  console.log("macOS app-share-concurrency handoff watcher self-test passed.");
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (options.mode === "self-test") return await selfTest();
  if (process.platform !== "darwin") fail("watch mode is supported only on macOS.");
  await waitForEvidenceDirectory({
    runnerPid: options.runnerPid,
    inspect: () => inspectEvidenceDirectory(options.evidenceDir),
    isAlive: processAlive,
    now: Date.now,
    pause: sleep,
  });
  await watchHandoff({
    runnerPid: options.runnerPid,
    loadMarker: (name, label) => readEvidenceMarker(options.evidenceDir, name, label),
    isAlive: processAlive,
    now: Date.now,
    pause: sleep,
    emit: (message) => console.log(message),
  });
}

main().catch((error) => {
  console.error(`macOS app-share-concurrency handoff watcher failed: ${
    error instanceof WatcherError ? error.message : "unexpected internal failure"
  }`);
  process.exitCode = 1;
});
