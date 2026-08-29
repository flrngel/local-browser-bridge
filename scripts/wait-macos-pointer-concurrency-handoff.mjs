#!/usr/bin/env node

import { constants } from "node:fs";
import { lstat, open, realpath } from "node:fs/promises";
import { isAbsolute, join, resolve } from "node:path";

const PRODUCT_VERSION = "0.12.51";
const SCHEMA_VERSION = 1;
const OPERATOR_DIRECTORY = "operator";
const REQUEST_FILE = "macos-pointer-concurrency-handoff-request.json";
const COMPLETE_FILE = "macos-pointer-concurrency-handoff-complete.json";
const REQUEST_KIND = "macos-pointer-concurrency-handoff-request";
const COMPLETE_KIND = "macos-pointer-concurrency-handoff-complete";
const MAX_MARKER_BYTES = 16_384;
const MAX_MARKER_AGE_MS = 30_000;
const FUTURE_TOLERANCE_MS = 5_000;
const FILE_TIME_TOLERANCE_MS = 5_000;
const MAX_MOTION_SPAN_MS = 300_000;
const MAX_REQUEST_TO_COMPLETE_MS = 310_000;
const QUIET_SEAT_MAXIMUM_WAIT_MS = 30 * 60_000;
const PRODUCER_PRE_REQUEST_WORK_BUDGET_MS = 30 * 60_000;
const REQUEST_PUBLICATION_MAXIMUM_WAIT_MS =
  QUIET_SEAT_MAXIMUM_WAIT_MS + PRODUCER_PRE_REQUEST_WORK_BUDGET_MS;
const EVIDENCE_DIRECTORY_WAIT_TIMEOUT_MS = 60_000;
const POLL_MS = 100;
const MAX_PID = 2_147_483_647;

const REQUEST_FIELDS = [
  "schemaVersion",
  "kind",
  "productVersion",
  "requestId",
  "createdAt",
  "runnerPid",
  "promptPid",
  "requestDelivered",
  "panelOnScreen",
  "panelNonactivating",
  "notificationOnly",
  "acceptedAsAuthority",
];

const COMPLETE_FIELDS = [
  ...REQUEST_FIELDS,
  "sustainedMotionSamples",
  "sustainedMotionSpanMilliseconds",
  "productBoundaryContaminated",
  "independentBoundaryContaminated",
  "clickFreeMotionObserved",
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
  if (
    actual.length !== expected.length ||
    actual.some((field, index) => field !== expected[index])
  ) {
    fail(`${label} fields are not in exact canonical order.`);
  }
}

function exactInteger(value, minimum, maximum, label) {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    fail(`${label} must be an integer from ${minimum} through ${maximum}.`);
  }
}

function exactBoolean(value, expected, label) {
  if (typeof value !== "boolean" || value !== expected) {
    fail(`${label} must be ${expected}.`);
  }
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
    typeof text !== "string" ||
    text.length === 0 ||
    Buffer.byteLength(text, "utf8") > MAX_MARKER_BYTES ||
    text.includes("\0") ||
    text.charCodeAt(0) === 0xfeff
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
  for (const field of fields) {
    const count = text.match(new RegExp(`"${field}"\\s*:`, "g"))?.length ?? 0;
    if (count !== 1) {
      fail(`${label} must contain each canonical field exactly once.`);
    }
  }
  return marker;
}

function validateBaseMarker(marker, expectedKind, expectedRunnerPid, label) {
  exactInteger(marker.schemaVersion, SCHEMA_VERSION, SCHEMA_VERSION, `${label} schemaVersion`);
  if (marker.kind !== expectedKind) fail(`${label} kind is invalid.`);
  if (marker.productVersion !== PRODUCT_VERSION) {
    fail(`${label} productVersion must be ${PRODUCT_VERSION}.`);
  }
  if (typeof marker.requestId !== "string" || !/^[0-9a-f]{32}$/.test(marker.requestId)) {
    fail(`${label} requestId must be 32 lowercase hexadecimal characters.`);
  }
  const createdAtMs = canonicalTimestamp(marker.createdAt, `${label} createdAt`);
  exactInteger(marker.runnerPid, 1, MAX_PID, `${label} runnerPid`);
  exactInteger(marker.promptPid, 1, MAX_PID, `${label} promptPid`);
  if (marker.runnerPid !== expectedRunnerPid) fail(`${label} runnerPid does not match the watched runner.`);
  if (marker.promptPid === marker.runnerPid) fail(`${label} promptPid must identify a separate process.`);
  exactBoolean(marker.requestDelivered, true, `${label} requestDelivered`);
  exactBoolean(marker.panelOnScreen, true, `${label} panelOnScreen`);
  exactBoolean(marker.panelNonactivating, true, `${label} panelNonactivating`);
  exactBoolean(marker.notificationOnly, true, `${label} notificationOnly`);
  exactBoolean(marker.acceptedAsAuthority, false, `${label} acceptedAsAuthority`);
  return createdAtMs;
}

function validateFreshness(createdAtMs, fileMtimeMs, nowMs, label) {
  if (createdAtMs > nowMs + FUTURE_TOLERANCE_MS) fail(`${label} is future-dated.`);
  if (nowMs - createdAtMs > MAX_MARKER_AGE_MS) fail(`${label} is stale.`);
  if (!Number.isFinite(fileMtimeMs) || Math.abs(fileMtimeMs - createdAtMs) > FILE_TIME_TOLERANCE_MS) {
    fail(`${label} file timestamp is not bound to its createdAt value.`);
  }
}

function validateRequest(record, expectedRunnerPid, nowMs) {
  const marker = parseExactJson(record.text, REQUEST_FIELDS, "request marker");
  const createdAtMs = validateBaseMarker(marker, REQUEST_KIND, expectedRunnerPid, "request marker");
  validateFreshness(createdAtMs, record.mtimeMs, nowMs, "request marker");
  return { marker, createdAtMs };
}

function validateComplete(record, request, expectedRunnerPid, nowMs) {
  const marker = parseExactJson(record.text, COMPLETE_FIELDS, "complete marker");
  const createdAtMs = validateBaseMarker(marker, COMPLETE_KIND, expectedRunnerPid, "complete marker");
  validateFreshness(createdAtMs, record.mtimeMs, nowMs, "complete marker");
  for (const field of ["requestId", "runnerPid", "promptPid"]) {
    if (marker[field] !== request.marker[field]) {
      fail(`complete marker ${field} does not match the request marker.`);
    }
  }
  if (
    createdAtMs < request.createdAtMs ||
    createdAtMs - request.createdAtMs > MAX_REQUEST_TO_COMPLETE_MS
  ) {
    fail("complete marker createdAt is outside the request-bound handoff interval.");
  }
  exactInteger(marker.sustainedMotionSamples, 3, 1_000_000, "complete marker sustainedMotionSamples");
  exactInteger(
    marker.sustainedMotionSpanMilliseconds,
    500,
    MAX_MOTION_SPAN_MS,
    "complete marker sustainedMotionSpanMilliseconds",
  );
  exactBoolean(
    marker.productBoundaryContaminated,
    true,
    "complete marker productBoundaryContaminated",
  );
  exactBoolean(
    marker.independentBoundaryContaminated,
    true,
    "complete marker independentBoundaryContaminated",
  );
  exactBoolean(marker.clickFreeMotionObserved, true, "complete marker clickFreeMotionObserved");
  return marker;
}

function sameFile(before, after) {
  return (
    before.dev === after.dev &&
    before.ino === after.ino &&
    before.size === after.size &&
    before.mtimeNs === after.mtimeNs &&
    before.nlink === after.nlink
  );
}

function ordinaryMarker(stats, label) {
  if (!stats.isFile() || stats.isSymbolicLink() || stats.nlink !== 1n) {
    fail(`${label} must be one ordinary, singly linked file.`);
  }
  if ((stats.mode & 0o077n) !== 0n) {
    fail(`${label} must not grant group or other filesystem access.`);
  }
  if (typeof process.getuid === "function" && stats.uid !== BigInt(process.getuid())) {
    fail(`${label} must be owned by the watcher user.`);
  }
  if (stats.size < 1n || stats.size > BigInt(MAX_MARKER_BYTES)) {
    fail(`${label} has invalid size.`);
  }
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
      fail(`${label} changed while it was read.`);
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
  } catch (error) {
    if (error instanceof WatcherError) throw error;
    fail(`${label} could not be read as one stable ordinary file.`);
  } finally {
    try {
      await handle.close();
    } catch {
      fail(`${label} read handle did not close cleanly.`);
    }
  }
}

function sameIdentity(before, after) {
  return before.dev === after.dev && before.ino === after.ino;
}

async function readEvidenceMarker(evidenceDir, name, label) {
  const operatorPath = join(evidenceDir, OPERATOR_DIRECTORY);
  let directoryBefore;
  try {
    directoryBefore = await lstat(operatorPath, { bigint: true });
  } catch (error) {
    if (error?.code === "ENOENT") return null;
    fail("The operator marker directory could not be inspected safely.");
  }
  if (!directoryBefore.isDirectory() || directoryBefore.isSymbolicLink()) {
    fail("The operator marker directory must be one ordinary directory, not a link.");
  }
  if ((directoryBefore.mode & 0o077n) !== 0n) {
    fail("The operator marker directory must not grant group or other filesystem access.");
  }
  if (
    typeof process.getuid === "function" &&
    directoryBefore.uid !== BigInt(process.getuid())
  ) {
    fail("The operator marker directory must be owned by the watcher user.");
  }
  try {
    if ((await realpath(operatorPath)) !== operatorPath) {
      fail("The operator marker directory must be its canonical real path.");
    }
  } catch (error) {
    if (error instanceof WatcherError) {
      throw error;
    }
    fail("The operator marker directory could not be resolved safely.");
  }
  const record = await readStableMarker(join(operatorPath, name), label);
  let directoryAfter;
  try {
    directoryAfter = await lstat(operatorPath, { bigint: true });
  } catch {
    fail("The operator marker directory disappeared while it was read.");
  }
  if (
    !directoryAfter.isDirectory() ||
    directoryAfter.isSymbolicLink() ||
    !sameIdentity(directoryBefore, directoryAfter) ||
    (directoryAfter.mode & 0o077n) !== 0n ||
    (typeof process.getuid === "function" &&
      directoryAfter.uid !== BigInt(process.getuid()))
  ) {
    fail("The operator marker directory changed while it was read.");
  }
  return record;
}

function processAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    if (error?.code === "EPERM") return true;
    if (error?.code === "ESRCH") return false;
    throw error;
  }
}

function sleep(milliseconds) {
  return new Promise((resolveSleep) => setTimeout(resolveSleep, milliseconds));
}

async function watchHandoff({
  runnerPid,
  loadMarker,
  isAlive,
  now,
  pause,
  emit,
  timeoutMs = REQUEST_PUBLICATION_MAXIMUM_WAIT_MS,
}) {
  const deadline = now() + timeoutMs;
  let requestRecord;
  let request;
  while (now() <= deadline) {
    requestRecord = await loadMarker(REQUEST_FILE, "request marker");
    if (requestRecord !== null) {
      request = validateRequest(requestRecord, runnerPid, now());
      const completeRecord = await loadMarker(COMPLETE_FILE, "complete marker");
      if (completeRecord !== null) {
        validateComplete(completeRecord, request, runnerPid, now());
        emit("COMPLETE: Both boundaries observed sustained click-free shared-pointer movement.");
        return;
      }
      if (!isAlive(runnerPid)) fail("The watched macOS acceptance runner is not alive.");
      if (!isAlive(request.marker.promptPid)) fail("The nonactivating pointer prompt is not alive.");
      break;
    }
    if ((await loadMarker(COMPLETE_FILE, "complete marker")) !== null) {
      fail("A complete marker appeared without its request marker.");
    }
    if (!isAlive(runnerPid)) fail("The watched macOS acceptance runner is not alive.");
    await pause(POLL_MS);
  }
  if (!request) fail("Timed out waiting for a fresh macOS pointer-concurrency request marker.");

  emit("ACTION REQUIRED: Continuously move the shared pointer without clicking; keep moving until COMPLETE.");

  const completionDeadline =
    request.createdAtMs + MAX_REQUEST_TO_COMPLETE_MS + FUTURE_TOLERANCE_MS;
  while (now() <= completionDeadline) {
    const currentRequest = await loadMarker(REQUEST_FILE, "request marker");
    if (
      currentRequest === null ||
      currentRequest.text !== requestRecord.text ||
      currentRequest.identity !== requestRecord.identity
    ) {
      fail("The request marker disappeared or changed after notification.");
    }
    const completeRecord = await loadMarker(COMPLETE_FILE, "complete marker");
    if (completeRecord !== null) {
      validateComplete(completeRecord, request, runnerPid, now());
      emit("COMPLETE: Both boundaries observed sustained click-free shared-pointer movement.");
      return;
    }
    if (!isAlive(runnerPid)) fail("The watched macOS acceptance runner exited before completion.");
    if (!isAlive(request.marker.promptPid)) {
      fail("The nonactivating pointer prompt exited before completion.");
    }
    await pause(POLL_MS);
  }
  fail("Timed out waiting for a fresh macOS pointer-concurrency complete marker.");
}

function parseArguments(argv) {
  const options = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const name = argv[index];
    const value = argv[index + 1];
    if (!name?.startsWith("--") || value === undefined || value.startsWith("--")) {
      fail("Arguments must be exact --name value pairs.");
    }
    if (options.has(name)) fail(`Duplicate argument: ${name}`);
    options.set(name, value);
  }
  const allowed = new Set(["--mode", "--evidence-dir", "--runner-pid"]);
  for (const name of options.keys()) {
    if (!allowed.has(name)) fail(`Unknown argument: ${name}`);
  }
  const mode = options.get("--mode");
  if (mode === "self-test") {
    if (options.size !== 1) fail("Self-test mode accepts only --mode self-test.");
    return { mode };
  }
  if (mode !== "watch" || options.size !== 3) {
    fail("Usage: --mode watch --evidence-dir <absolute-path> --runner-pid <pid>");
  }
  const evidenceDir = options.get("--evidence-dir");
  if (!isAbsolute(evidenceDir)) fail("--evidence-dir must be an absolute path.");
  const runnerPidText = options.get("--runner-pid");
  if (!/^[1-9][0-9]*$/.test(runnerPidText)) fail("--runner-pid must be a canonical positive integer.");
  const runnerPid = Number(runnerPidText);
  exactInteger(runnerPid, 1, MAX_PID, "--runner-pid");
  return { mode, evidenceDir: resolve(evidenceDir), runnerPid };
}

async function inspectEvidenceDirectory(path) {
  let stats;
  try {
    stats = await lstat(path, { bigint: true });
  } catch (error) {
    if (error?.code === "ENOENT") return false;
    fail("--evidence-dir could not be inspected safely.");
  }
  if (!stats.isDirectory() || stats.isSymbolicLink() || stats.nlink < 1n) {
    fail("--evidence-dir must be one ordinary directory, not a link.");
  }
  try {
    if ((await realpath(path)) !== path) {
      fail("--evidence-dir must already be its canonical real path.");
    }
  } catch (error) {
    if (error instanceof WatcherError) throw error;
    fail("--evidence-dir could not be resolved safely.");
  }
  return true;
}

async function waitForEvidenceDirectory({
  runnerPid,
  inspect,
  isAlive,
  now,
  pause,
  timeoutMs = EVIDENCE_DIRECTORY_WAIT_TIMEOUT_MS,
}) {
  const deadline = now() + timeoutMs;
  while (now() <= deadline) {
    if (await inspect()) return;
    if (!isAlive(runnerPid)) {
      fail("The watched macOS acceptance runner exited before creating its evidence directory.");
    }
    await pause(POLL_MS);
  }
  fail("Timed out waiting for the runner-created macOS evidence directory.");
}

function makeMarker(kind, createdAt, runnerPid, promptPid) {
  return {
    schemaVersion: SCHEMA_VERSION,
    kind,
    productVersion: PRODUCT_VERSION,
    requestId: "0123456789abcdef0123456789abcdef",
    createdAt,
    runnerPid,
    promptPid,
    requestDelivered: true,
    panelOnScreen: true,
    panelNonactivating: true,
    notificationOnly: true,
    acceptedAsAuthority: false,
  };
}

function record(marker, mtimeMs) {
  const text = `${JSON.stringify(marker, null, 2)}\n`;
  return { text, mtimeMs, identity: `self-test:${mtimeMs}:${text.length}` };
}

async function expectFailure(action, expectedText) {
  try {
    await action();
  } catch (error) {
    if (error instanceof Error && error.message.includes(expectedText)) return;
    throw error;
  }
  fail(`Self-test expected failure containing: ${expectedText}`);
}

async function selfTest() {
  const nowMs = Date.parse("2026-08-23T12:00:00.000Z");
  const runnerPid = 41001;
  const promptPid = 41002;
  const requestMarker = makeMarker(
    REQUEST_KIND,
    new Date(nowMs - 1_000).toISOString(),
    runnerPid,
    promptPid,
  );
  const completeMarker = {
    ...makeMarker(
      COMPLETE_KIND,
      new Date(nowMs - 100).toISOString(),
      runnerPid,
      promptPid,
    ),
    sustainedMotionSamples: 3,
    sustainedMotionSpanMilliseconds: 500,
    productBoundaryContaminated: true,
    independentBoundaryContaminated: true,
    clickFreeMotionObserved: true,
  };
  const requestRecord = record(requestMarker, nowMs - 1_000);
  const completeRecord = record(completeMarker, nowMs - 100);
  const emissions = [];
  let normalActionEmitted = false;
  await watchHandoff({
    runnerPid,
    loadMarker: async (name) => {
      if (name === REQUEST_FILE) return requestRecord;
      return normalActionEmitted ? completeRecord : null;
    },
    isAlive: (pid) => pid === runnerPid || pid === promptPid,
    now: () => nowMs,
    pause: async () => {},
    emit: (message) => {
      emissions.push(message);
      if (message.startsWith("ACTION REQUIRED:")) normalActionEmitted = true;
    },
    timeoutMs: 1_000,
  });
  if (
    emissions.length !== 2 ||
    !emissions[0].startsWith("ACTION REQUIRED:") ||
    !emissions[1].startsWith("COMPLETE:")
  ) {
    fail("Self-test did not emit one exact action and completion notification.");
  }

  let directoryInspectionCount = 0;
  await waitForEvidenceDirectory({
    runnerPid,
    inspect: async () => {
      directoryInspectionCount += 1;
      return directoryInspectionCount === 3;
    },
    isAlive: (pid) => pid === runnerPid,
    now: () => nowMs,
    pause: async () => {},
    timeoutMs: 1_000,
  });
  if (directoryInspectionCount !== 3) {
    fail("Self-test did not wait for the runner-created evidence directory.");
  }
  await expectFailure(
    async () => waitForEvidenceDirectory({
      runnerPid,
      inspect: async () => false,
      isAlive: () => false,
      now: () => nowMs,
      pause: async () => {},
      timeoutMs: 1_000,
    }),
    "exited before creating its evidence directory",
  );

  let delayedNowMs = nowMs;
  const delayedRequestAtMs = nowMs + 11 * 60_000;
  const delayedRequest = record(
    makeMarker(
      REQUEST_KIND,
      new Date(delayedRequestAtMs - 1_000).toISOString(),
      runnerPid,
      promptPid,
    ),
    delayedRequestAtMs - 1_000,
  );
  const delayedComplete = record(
    {
      ...makeMarker(
        COMPLETE_KIND,
        new Date(delayedRequestAtMs - 100).toISOString(),
        runnerPid,
        promptPid,
      ),
      sustainedMotionSamples: 3,
      sustainedMotionSpanMilliseconds: 500,
      productBoundaryContaminated: true,
      independentBoundaryContaminated: true,
      clickFreeMotionObserved: true,
    },
    delayedRequestAtMs - 100,
  );
  const delayedEmissions = [];
  let delayedActionEmitted = false;
  await watchHandoff({
    runnerPid,
    loadMarker: async (name) => {
      if (delayedNowMs < delayedRequestAtMs) return null;
      if (name === REQUEST_FILE) return delayedRequest;
      return delayedActionEmitted ? delayedComplete : null;
    },
    isAlive: (pid) => pid === runnerPid || pid === promptPid,
    now: () => delayedNowMs,
    pause: async () => {
      delayedNowMs += 60_000;
    },
    emit: (message) => {
      delayedEmissions.push(message);
      if (message.startsWith("ACTION REQUIRED:")) delayedActionEmitted = true;
    },
  });
  if (
    delayedNowMs - nowMs <= 10 * 60_000 ||
    delayedEmissions.length !== 2 ||
    !delayedEmissions[1].startsWith("COMPLETE:")
  ) {
    fail("Self-test did not permit a producer-bound request after the old ten-minute limit.");
  }

  const exitedRunnerEmissions = [];
  let exitedRunnerLivenessChecks = 0;
  let exitedRunnerActionEmitted = false;
  await watchHandoff({
    runnerPid,
    loadMarker: async (name) => {
      if (name === REQUEST_FILE) return requestRecord;
      return exitedRunnerActionEmitted ? completeRecord : null;
    },
    isAlive: (pid) => {
      if (pid === promptPid) return true;
      exitedRunnerLivenessChecks += 1;
      return exitedRunnerLivenessChecks === 1;
    },
    now: () => nowMs,
    pause: async () => {},
    emit: (message) => {
      exitedRunnerEmissions.push(message);
      if (message.startsWith("ACTION REQUIRED:")) exitedRunnerActionEmitted = true;
    },
    timeoutMs: 1_000,
  });
  if (exitedRunnerEmissions.length !== 2) {
    fail("Self-test rejected a valid bound completion after runner exit.");
  }
  const catchUpEmissions = [];
  await watchHandoff({
    runnerPid,
    loadMarker: async (name) => name === REQUEST_FILE ? requestRecord : completeRecord,
    isAlive: () => false,
    now: () => nowMs,
    pause: async () => {},
    emit: (message) => catchUpEmissions.push(message),
    timeoutMs: 1_000,
  });
  if (
    catchUpEmissions.length !== 1 ||
    !catchUpEmissions[0].startsWith("COMPLETE:")
  ) {
    fail("Self-test rejected valid bound catch-up markers after runner and prompt exit.");
  }
  let missingCompletionRunnerChecks = 0;
  await expectFailure(
    async () => watchHandoff({
      runnerPid,
      loadMarker: async (name) => name === REQUEST_FILE ? requestRecord : null,
      isAlive: (pid) => {
        if (pid === promptPid) return true;
        missingCompletionRunnerChecks += 1;
        return missingCompletionRunnerChecks === 1;
      },
      now: () => nowMs,
      pause: async () => {},
      emit: () => {},
      timeoutMs: 1_000,
    }),
    "exited before completion",
  );
  const invalidComplete = record(
    { ...completeMarker, requestId: "fedcba9876543210fedcba9876543210" },
    nowMs - 100,
  );
  let invalidCompletionRunnerChecks = 0;
  await expectFailure(
    async () => watchHandoff({
      runnerPid,
      loadMarker: async (name) => name === REQUEST_FILE ? requestRecord : invalidComplete,
      isAlive: (pid) => {
        if (pid === promptPid) return true;
        invalidCompletionRunnerChecks += 1;
        return invalidCompletionRunnerChecks === 1;
      },
      now: () => nowMs,
      pause: async () => {},
      emit: () => {},
      timeoutMs: 1_000,
    }),
    "requestId",
  );

  await expectFailure(
    async () => validateRequest(record(requestMarker, nowMs - 31_001), runnerPid, nowMs + 30_001),
    "stale",
  );
  const futureRequest = makeMarker(
    REQUEST_KIND,
    new Date(nowMs + FUTURE_TOLERANCE_MS + 1).toISOString(),
    runnerPid,
    promptPid,
  );
  await expectFailure(
    async () => validateRequest(record(futureRequest, nowMs + FUTURE_TOLERANCE_MS + 1), runnerPid, nowMs),
    "future-dated",
  );
  await expectFailure(
    async () => validateRequest(record({ ...requestMarker, unexpected: true }, nowMs - 1_000), runnerPid, nowMs),
    "canonical order",
  );
  const authorityClaimingRequest = { ...requestMarker };
  authorityClaimingRequest.acceptedAsAuthority = !requestMarker.acceptedAsAuthority;
  await expectFailure(
    async () => validateRequest(record(authorityClaimingRequest, nowMs - 1_000), runnerPid, nowMs),
    "acceptedAsAuthority",
  );
  await expectFailure(
    async () => validateComplete(
      record({ ...completeMarker, requestId: "fedcba9876543210fedcba9876543210" }, nowMs - 100),
      validateRequest(requestRecord, runnerPid, nowMs),
      runnerPid,
      nowMs,
    ),
    "requestId",
  );
  await expectFailure(
    async () => validateComplete(
      record({ ...completeMarker, sustainedMotionSamples: 2 }, nowMs - 100),
      validateRequest(requestRecord, runnerPid, nowMs),
      runnerPid,
      nowMs,
    ),
    "sustainedMotionSamples",
  );
  const uncorroboratedComplete = { ...completeMarker };
  uncorroboratedComplete.productBoundaryContaminated =
    !completeMarker.productBoundaryContaminated;
  await expectFailure(
    async () => validateComplete(
      record(uncorroboratedComplete, nowMs - 100),
      validateRequest(requestRecord, runnerPid, nowMs),
      runnerPid,
      nowMs,
    ),
    "productBoundaryContaminated",
  );
  const clickedComplete = { ...completeMarker, clickFreeMotionObserved: false };
  await expectFailure(
    async () => validateComplete(
      record(clickedComplete, nowMs - 100),
      validateRequest(requestRecord, runnerPid, nowMs),
      runnerPid,
      nowMs,
    ),
    "clickFreeMotionObserved",
  );
  await expectFailure(
    async () => watchHandoff({
      runnerPid,
      loadMarker: async (name) => name === REQUEST_FILE ? requestRecord : null,
      isAlive: (pid) => pid === runnerPid,
      now: () => nowMs,
      pause: async () => {},
      emit: () => {},
      timeoutMs: 1_000,
    }),
    "prompt is not alive",
  );
  let requestReadCount = 0;
  const changedRequest = record({ ...requestMarker, createdAt: new Date(nowMs - 900).toISOString() }, nowMs - 900);
  await expectFailure(
    async () => watchHandoff({
      runnerPid,
      loadMarker: async (name) => {
        if (name !== REQUEST_FILE) return null;
        requestReadCount += 1;
        return requestReadCount === 1 ? requestRecord : changedRequest;
      },
      isAlive: (pid) => pid === runnerPid || pid === promptPid,
      now: () => nowMs,
      pause: async () => {},
      emit: () => {},
      timeoutMs: 1_000,
    }),
    "disappeared or changed",
  );
  await expectFailure(
    async () => watchHandoff({
      runnerPid,
      loadMarker: async () => null,
      isAlive: () => false,
      now: () => nowMs,
      pause: async () => {},
      emit: () => {},
      timeoutMs: 1_000,
    }),
    "not alive",
  );
  console.log("macOS pointer-concurrency handoff watcher self-test passed.");
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (options.mode === "self-test") {
    await selfTest();
    return;
  }
  if (process.platform !== "darwin") fail("Watch mode is supported only on macOS.");
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
  const message = error instanceof WatcherError
    ? error.message
    : "unexpected internal failure";
  console.error(`macOS pointer-concurrency handoff watcher failed: ${message}`);
  process.exitCode = 1;
});
