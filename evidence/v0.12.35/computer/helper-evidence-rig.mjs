#!/usr/bin/env node

import { spawn, spawnSync } from "node:child_process";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import {
  access,
  lstat,
  link,
  mkdir,
  mkdtemp,
  open,
  readFile,
  rename,
  rm,
  stat,
  symlink,
  unlink,
  writeFile,
} from "node:fs/promises";
import { constants as fsConstants } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve, sep } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";

const EXPECTED_VERSION = "0.12.35";
const EXPECTED_ARCHIVE = `local-browser-bridge-v${EXPECTED_VERSION}-macos-universal.tar.gz`;
const CANONICAL_RELEASE_ASSETS = [
  `local-browser-bridge-v${EXPECTED_VERSION}-windows-x86_64.exe`,
  `local-computer-helper-v${EXPECTED_VERSION}-windows-x86_64.exe`,
  EXPECTED_ARCHIVE,
  `local-browser-bridge-extension-v${EXPECTED_VERSION}.zip`,
];
const MAX_COMPRESSED_MACOS_ARCHIVE_BYTES = 256 * 1024 * 1024;
const MAX_UNCOMPRESSED_MACOS_ARCHIVE_BYTES = 512 * 1024 * 1024;
const MAX_CHECKSUM_MANIFEST_BYTES = 16 * 1024;
const EXPECTED_MACOS_ARCHIVE_ENTRIES = [
  { name: "local-browser-bridge", kind: "file", mode: 0o755, maximumBytes: 200 * 1024 * 1024 },
  { name: "Local Computer Helper.app", kind: "directory", mode: 0o755, maximumBytes: 0 },
  { name: "Local Computer Helper.app/Contents", kind: "directory", mode: 0o755, maximumBytes: 0 },
  { name: "Local Computer Helper.app/Contents/Info.plist", kind: "file", mode: 0o644, maximumBytes: 1024 * 1024 },
  { name: "Local Computer Helper.app/Contents/MacOS", kind: "directory", mode: 0o755, maximumBytes: 0 },
  { name: "Local Computer Helper.app/Contents/MacOS/local-computer-helper", kind: "file", mode: 0o755, maximumBytes: 200 * 1024 * 1024 },
  { name: "Local Computer Helper.app/Contents/_CodeSignature", kind: "directory", mode: 0o755, maximumBytes: 0 },
  { name: "Local Computer Helper.app/Contents/_CodeSignature/CodeResources", kind: "file", mode: 0o644, maximumBytes: 16 * 1024 * 1024 },
  { name: "LICENSE", kind: "file", mode: 0o644, maximumBytes: 1024 * 1024 },
  { name: "THIRD_PARTY_LICENSES.txt", kind: "file", mode: 0o644, maximumBytes: 16 * 1024 * 1024 },
];
const FIXTURE_TITLE = "LBB v0.12.35 Persistent SCStream Evidence";
const SIBLING_FIXTURE_TITLE = "LBB v0.12.35 Same-PID Sibling Receiver";
const SEMANTIC_VALUE = "v0.12.35-semantic-value";
const STATUS_BACKEND = "background-window/ax+skylight+screencapturekit-stream";
const CAPTURE_BACKEND = "macos-screencapturekit-scstream";
const SELECTION_MODE = "programmatic-exact-window";
const WAIT_STEP_MS = 100;
const SHARE_FPS = 10;
const LONG_PIXEL_ACTION_MS = 900;
const POST_RESIZE_PIXEL_ACTION_MS = 120;
const APP_SHARE_CONCURRENCY_ACTION_MS = 2_000;
const APP_SHARE_HANDOFF_WAIT_MS = 300_000;
const APP_SHARE_HANDOFF_COMPLETION_GRACE_MS = 18_000;
const APP_SHARE_HANDOFF_MINIMUM_ACTION_BUDGET_MS = 8_000;
const APP_SHARE_HANDOFF_COMPLETION_RESERVE_MS = 3_000;
const APP_SHARE_ACTION_AUTHORITY_REFRESH_MAXIMUM_WAIT_MS = 5_000;
const APP_SHARE_ACTION_FRAME_MAX_ESTIMATED_AGE_MS = 1_000;
const SHARE_ACTION_AUTHORITY_REJECTION_KEYS = [
  "replayOrNonAdvancing",
  "shareMismatch",
  "targetMismatch",
  "geometryMismatch",
  "invalidMetadata",
  "staleAge",
];
// Retained only for the source-level optional physical-pointer adversarial
// classifier self-tests; release-candidate execution never enters that path.
const POINTER_HANDOFF_COMPLETION_GRACE_MS = APP_SHARE_HANDOFF_COMPLETION_GRACE_MS;
const DELIBERATE_MOTION_REQUIRED_SAMPLES = 3;
const DELIBERATE_MOTION_MINIMUM_SPAN_MS = 500;
const DELIBERATE_MOTION_SAMPLE_MS = 250;
const APP_SHARE_HANDOFF_MARKER_SCHEMA = 2;
const POINTER_HANDOFF_WAITING_STATE = "WAITING";
const APP_SHARE_HANDOFF_READY_STATE = "READY";
const APP_SHARE_HANDOFF_ARMED_STATE = "ARMED";
const POINTER_HANDOFF_MOVE_STATE = "MOVE";
const POINTER_HANDOFF_ACTION_STATE = "ACTION";
const POINTER_HANDOFF_COMPLETE_STATE = "COMPLETE";
const APP_SHARE_HANDOFF_REQUEST_FILE = "macos-app-share-concurrency-handoff-request.json";
const APP_SHARE_HANDOFF_START_FILE = "macos-app-share-concurrency-handoff-start.json";
const APP_SHARE_HANDOFF_COMPLETE_FILE = "macos-app-share-concurrency-handoff-complete.json";
const APP_SHARE_BUNDLE_IDENTIFIER = "dev.flrngel.local-browser-bridge.acceptance.app-share";
const APP_SHARE_WINDOW_TITLE = "LBB macOS Acceptance App Share";
const APP_SHARE_READY_BUTTON_TEXT = "START APP-SHARE CHECK";
const CANCELED_MOVE_DURATION_MS = 2_000;
const CANCELLATION_DISPATCH_PROOF_TIMEOUT_MS = 10_000;
const TARGET_CLOSE_SETTLE_FRAME_PERIODS = 3;
const TARGET_CLOSE_CAPTURE_CODES = new Set(["COMPUTER_NO_WINDOW", "COMPUTER_CAPTURE_FAILED"]);
const POINTER_EVIDENCE_LANES = new Set(["quiet", "deliberate-concurrency"]);
const HID_POINTER_COUNTER_FIELDS = [
  "leftMouseDown",
  "leftMouseUp",
  "rightMouseDown",
  "rightMouseUp",
  "mouseMoved",
  "leftMouseDragged",
  "rightMouseDragged",
  "scrollWheel",
  "otherMouseDragged",
  "otherMouseDown",
  "otherMouseUp",
  "tabletPointer",
  "tabletProximity",
];
const HID_KEYBOARD_COUNTER_FIELDS = ["keyDown", "keyUp", "flagsChanged"];
const MAX_HID_POINTER_COUNTER_ADVANCE = 1_000_000;
const SUBPROCESS_TIMEOUT_MS = 60_000;
const SYSTEM_PROBE_TIMEOUT_MS = 5_000;
const QUIET_SEAT_REQUIRED_STABLE_MS = 30_000;
const QUIET_SEAT_MAXIMUM_WAIT_MS = 30 * 60_000;
const PRODUCER_PRE_REQUEST_WORK_BUDGET_MS = 30 * 60_000;
const REQUEST_PUBLICATION_MAXIMUM_WAIT_MS =
  QUIET_SEAT_MAXIMUM_WAIT_MS + PRODUCER_PRE_REQUEST_WORK_BUDGET_MS;
const QUIET_SEAT_SAMPLE_INTERVAL_MS = 500;
const QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS = 60;
const MARKER_PUBLISH_TIMEOUT_MS = 5_000;
const NATIVE_TEXT_SUFFIX = `-native-${randomBytes(6).toString("hex")}`;
const GENERATED_OUTPUT_NAMES = [
  "helper-results.json",
  "helper-rig.log",
  "computer-01-exact-window-observe.png",
  "computer-02-semantic-set-value.png",
  "computer-03-semantic-invoke.png",
  "computer-04-persistent-scstream-start.png",
  "computer-05-live-share-pixel-action.png",
  "computer-06-persistent-share-resize.png",
  "operator",
];
const bearerToken = randomBytes(32).toString("base64url");

const rigArguments = process.argv.slice(2);
if (rigArguments.length === 1 && rigArguments[0] === "--self-test") {
  await runRigSelfTest();
  process.exit(0);
}

const preparePackageMode = rigArguments[0] === "--prepare-package";
const preparePackageArguments = preparePackageMode ? rigArguments.slice(1) : [];
if (preparePackageMode && preparePackageArguments.length !== 4) {
  console.error(
    "Usage: node helper-evidence-rig.mjs --prepare-package <macos-archive> <SHA256SUMS.txt> <expected-SHA256SUMS-sha256> <fresh-package-output-dir>",
  );
  process.exit(2);
}

const [
  prepareArchiveInput,
  prepareSumsInput,
  prepareExpectedManifestSha256,
  prepareOutputInput,
] = preparePackageArguments;

const [
  serverInput,
  helperInput,
  outputInput,
  scratchParentInput,
  archiveInput,
  sumsInput,
  expectedManifestSha256,
  expectedSourceSha,
  expectedWorkflowRunId,
  expectedWorkflowRunAttempt,
  expectedArtifactId,
  expectedArtifactZipSha256,
  pointerEvidenceLane = "quiet",
] = preparePackageMode ? [] : rigArguments;
if (!preparePackageMode && (
  !serverInput || !helperInput || !outputInput || !scratchParentInput ||
  !archiveInput || !sumsInput || !expectedManifestSha256 || !expectedSourceSha ||
  !expectedWorkflowRunId || !expectedWorkflowRunAttempt ||
  !expectedArtifactId || !expectedArtifactZipSha256
)) {
  console.error(
    "Usage: node helper-evidence-rig.mjs <server> <helper> <fresh-output-dir> <scratch-parent> <macos-archive> <SHA256SUMS.txt> <expected-SHA256SUMS-sha256> <expected-source-sha> <expected-workflow-run-id> <expected-workflow-run-attempt> <expected-artifact-id> <expected-artifact-zip-sha256> [quiet|deliberate-concurrency]",
  );
  process.exit(2);
}
for (const [label, value, pattern] of preparePackageMode ? [] : [
  ["manifest SHA-256", expectedManifestSha256, /^[0-9a-f]{64}$/],
  ["source SHA", expectedSourceSha, /^[0-9a-f]{40}$/],
  ["workflow run ID", expectedWorkflowRunId, /^[1-9][0-9]*$/],
  ["workflow run attempt", expectedWorkflowRunAttempt, /^[1-9][0-9]*$/],
  ["artifact ID", expectedArtifactId, /^[1-9][0-9]*$/],
  ["artifact ZIP SHA-256", expectedArtifactZipSha256, /^[0-9a-f]{64}$/],
]) {
  if (!pattern.test(value)) {
    console.error(`Expected ${label} has an invalid canonical form.`);
    process.exit(2);
  }
}
if (!preparePackageMode && !POINTER_EVIDENCE_LANES.has(pointerEvidenceLane)) {
  console.error("Pointer evidence lane must be quiet or deliberate-concurrency.");
  process.exit(2);
}

const serverPath = preparePackageMode ? null : resolve(serverInput);
const helperPath = preparePackageMode ? null : resolve(helperInput);
const outputDir = resolve(preparePackageMode ? prepareOutputInput : outputInput);
const scratchParent = preparePackageMode ? null : resolve(scratchParentInput);
const archivePath = resolve(preparePackageMode ? prepareArchiveInput : archiveInput);
const sumsPath = resolve(preparePackageMode ? prepareSumsInput : sumsInput);
const rigSourcePath = fileURLToPath(import.meta.url);
const rigSourceDirectory = dirname(rigSourcePath);
const fixtureSource = resolve(rigSourceDirectory, "HelperEvidenceFixture.swift");
const systemProbeSource = resolve(rigSourceDirectory, "SystemProbe.swift");
const pointerHandoffSource = resolve(rigSourceDirectory, "AppShareHandoff.swift");
const physicalPointerHandoffSource = resolve(
  rigSourceDirectory,
  "PhysicalPointerHandoff.swift",
);
const acceptanceFinalizerSource = resolve(
  rigSourceDirectory,
  "../../../scripts/finalize-macos-acceptance.mjs",
);
const resultsPath = join(outputDir, "helper-results.json");
const logPath = join(outputDir, "helper-rig.log");
const operatorDirectory = join(outputDir, "operator");
const pointerHandoffRequestPath = join(operatorDirectory, APP_SHARE_HANDOFF_REQUEST_FILE);
const pointerHandoffStartPath = join(operatorDirectory, APP_SHARE_HANDOFF_START_FILE);
const pointerHandoffCompletePath = join(operatorDirectory, APP_SHARE_HANDOFF_COMPLETE_FILE);

const logLines = [];
const checks = [];
const screenshots = [];
const shareSamples = [];
let scratchDir;
let serverProcess;
let helperProcess;
let fixtureProcess;
let pointerHandoffProcess;
let port;
let helperSpawnCount = 0;
let successfulResultWritten = false;
let outputReserved = false;
let laneStartedAt = null;
let systemProbeBinary;
let pointerHandoffBinary;
let pointerHandoffControlPath;
let pointerHandoffAppPath;
let fixtureStatePath;
let failureProbeBaseline;
let fixtureTargetPid;
let fixtureSiblingWindowId;
let nativeTextPayloadMayBeVisible = false;
let pointerHandoffRequestId = null;
let pointerHandoffRequestCreatedAt = null;
let pointerHandoffRequestSha256 = null;
let pointerHandoffStartReceiptSha256 = null;
let pointerHandoffStartReceiptCreatedAt = null;
let pointerHandoffProductActionStartedAt = null;
let pointerHandoffProductActionCompletedAt = null;
let pointerHandoffRequestPublicationDeadlineMilliseconds = null;
let pointerHandoffArmDeadlineMilliseconds = null;
let pointerHandoffHardDeadlineMilliseconds = null;
let pointerHandoffActionDeadlineMilliseconds = null;
let pointerHandoffRequestPublicationAcknowledged = false;
let pointerHandoffCompletePublicationAcknowledged = false;
let pointerHandoffPromptClosed = false;
let pointerHandoffAcceptanceButtonActionObserved = false;
let pointerHandoffExactAppBundleObserved = false;
let pointerHandoffExactWindowObserved = false;
let pointerHandoffExactButtonObserved = false;
let pointerHandoffButtonDisabledAfterAction = false;
let pointerHandoffSurfaceObservedAtProductBoundaries = false;
let pointerHandoffSharedHidInputObserved = null;
let pointerHandoffSampledSharedContextUnchanged = false;
let pointerHandoffAuthorityRefreshedAfterReceipt = false;
let pointerHandoffAuthorityFreshAtDispatch = false;
let pointerHandoffActionDispatched = false;
let pointerHandoffProductBoundaryQuiet = false;
let pointerHandoffIndependentBoundaryQuiet = false;
let pointerHandoffTargetPostconditionObserved = false;
let quietSeatStabilization = {
  required: true,
  completed: false,
  requiredStableMilliseconds: QUIET_SEAT_REQUIRED_STABLE_MS,
  maximumWaitMilliseconds: QUIET_SEAT_MAXIMUM_WAIT_MS,
  sampleIntervalMilliseconds: QUIET_SEAT_SAMPLE_INTERVAL_MS,
  requiredStableTransitions: QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS,
  stableDurationMilliseconds: 0,
  observedSamples: 0,
  stableTransitions: 0,
  resetCount: 0,
  monitoringUnknown: false,
  completedBeforeCandidateExecution: false,
  rawPointerDataRetained: false,
};
const pointerEvidenceObserved = {
  quiet: false,
  concurrentSharedSeatActivity: false,
  unknown: false,
};
const capabilityBinding = {
  inputDeliveryProvenanceV1: false,
  pointerActivityMonitorV1: false,
};
const manifestBinding = {
  file: basename(sumsPath),
  expectedSha256: expectedManifestSha256,
  actualSha256: null,
  expectedSha256Matched: false,
  exactCanonicalAssetSet: false,
  canonicalEntryCount: 0,
  archiveFile: basename(archivePath),
  archiveSha256: null,
  archiveEntryMatched: false,
};
const releaseCandidateBinding = {
  schemaVersion: 3,
  version: EXPECTED_VERSION,
  releaseTag: `v${EXPECTED_VERSION}`,
  repository: "flrngel/local-browser-bridge",
  sourceSha: expectedSourceSha,
  workflowRunId: expectedWorkflowRunId,
  workflowRunAttempt: expectedWorkflowRunAttempt,
  workflowEvent: "workflow_dispatch",
  workflowRef: "refs/heads/main",
  workflowPath: ".github/workflows/deploy.yml",
  artifactId: expectedArtifactId,
  artifactZipSha256: expectedArtifactZipSha256,
  checksumManifestSha256: expectedManifestSha256,
};
const harnessSourceBinding = {
  sourceSha: null,
  detachedHead: false,
  cleanTrackedAndUntracked: false,
  fsckPassed: false,
  exactTrackedHarnessBlobs: false,
};

function childEnvironment(overrides = {}) {
  const environment = {};
  for (const name of ["PATH", "HOME", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE", "__CF_USER_TEXT_ENCODING"]) {
    if (typeof process.env[name] === "string" && process.env[name].length > 0) {
      environment[name] = process.env[name];
    }
  }
  return { ...environment, ...overrides };
}

function sanitizePathDetail(value) {
  let text = String(value);
  if (text.includes(bearerToken)) return "Sensitive failure detail withheld";
  text = text.split(NATIVE_TEXT_SUFFIX).join("[NATIVE_TEXT_PAYLOAD]");
  for (const [path, replacement] of [
    [scratchDir, "[SCRATCH]"],
    [serverPath, serverPath ? basename(serverPath) : "[SERVER]"],
    [helperPath, helperPath ? basename(helperPath) : "[HELPER]"],
    [archivePath, basename(archivePath)],
    [sumsPath, basename(sumsPath)],
    [outputDir, "[EVIDENCE]"],
  ]) {
    if (path) text = text.split(path).join(replacement);
  }
  return text;
}

function assertNoToken(text, label) {
  if (text.includes(bearerToken)) {
    throw new Error(`refusing to persist the bearer token in ${label}`);
  }
}

function assertNoRetainedNativeTextPayload(text, label) {
  if (text.includes(NATIVE_TEXT_SUFFIX)) {
    throw new Error(`refusing to retain the native text payload in ${label}`);
  }
}

function assertNoRetainedPointerRawData(text, label) {
  for (const rawField of [
    "\"cursorX\":",
    "\"cursorY\":",
    "\"hidPointerCounters\":",
    "\"hidKeyboardCounters\":",
  ]) {
    if (text.includes(rawField)) {
      throw new Error(`refusing to retain raw shared-pointer probe data in ${label}`);
    }
  }
}

function log(message) {
  const safeMessage = sanitizePathDetail(message);
  assertNoToken(safeMessage, "log");
  const line = `${new Date().toISOString()} ${safeMessage}`;
  logLines.push(line);
  console.log(line);
}

function check(name, passed, detail) {
  const safeDetail = sanitizePathDetail(detail);
  const item = { name, passed: Boolean(passed), detail: safeDetail };
  checks.push(item);
  log(`${item.passed ? "PASS" : "FAIL"} ${name}: ${safeDetail}`);
  return item.passed;
}

function requireCheck(name, passed, detail) {
  if (!check(name, passed, detail)) throw new Error(`${name}: ${detail}`);
}

const delay = (milliseconds) => new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));

async function waitFor(description, predicate, timeoutMs = 15_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const value = await predicate();
      if (value) return value;
    } catch (error) {
      lastError = error;
    }
    await delay(WAIT_STEP_MS);
  }
  const suffix = lastError ? `: ${sanitizePathDetail(lastError.message)}` : "";
  throw new Error(`${description} timed out${suffix}`);
}

async function freePort() {
  return await new Promise((resolvePort, reject) => {
    const listener = createServer();
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", () => {
      const address = listener.address();
      listener.close((error) => {
        if (error) reject(error);
        else resolvePort(address.port);
      });
    });
  });
}

function run(commandName, args, timeoutMs = SUBPROCESS_TIMEOUT_MS) {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1) {
    throw new Error("subprocess timeout must be a positive integer");
  }
  const result = spawnSync(commandName, args, {
    encoding: "utf8",
    maxBuffer: 4 * 1024 * 1024,
    timeout: timeoutMs,
    killSignal: "SIGKILL",
  });
  if (result.error?.code === "ETIMEDOUT" || result.signal === "SIGKILL") {
    const error = new Error(`${basename(commandName)} exceeded its bounded execution deadline`);
    error.code = "SUBPROCESS_TIMEOUT";
    throw error;
  }
  if (result.error) {
    throw new Error(`${basename(commandName)} could not be executed`);
  }
  if (result.status !== 0) {
    const detail = sanitizePathDetail((result.stderr || result.stdout || "no diagnostic output").trim());
    throw new Error(`${basename(commandName)} failed: ${detail}`);
  }
  return result.stdout.trim();
}

function runExactLine(commandName, args, expectedLine, timeoutMs = SUBPROCESS_TIMEOUT_MS) {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1) {
    throw new Error("subprocess timeout must be a positive integer");
  }
  const result = spawnSync(commandName, args, {
    encoding: "utf8",
    maxBuffer: 4 * 1024 * 1024,
    timeout: timeoutMs,
    killSignal: "SIGKILL",
  });
  if (result.error?.code === "ETIMEDOUT" || result.signal === "SIGKILL") {
    const error = new Error(`${basename(commandName)} exceeded its bounded execution deadline`);
    error.code = "SUBPROCESS_TIMEOUT";
    throw error;
  }
  if (result.error) {
    throw new Error(`${basename(commandName)} could not be executed`);
  }
  if (result.status !== 0) {
    const detail = sanitizePathDetail((result.stderr || result.stdout || "no diagnostic output").trim());
    throw new Error(`${basename(commandName)} failed: ${detail}`);
  }
  return result.stderr === "" && result.stdout === `${expectedLine}\n`;
}

function extractCandidateArchiveBounded(path, destination, expectedArchiveSha256) {
  const extractor = String.raw`
import gzip
import hashlib
import json
import os
import re
import stat
import sys

archive_path, destination, expected_sha256, expected_json, maximum_compressed, maximum_total = sys.argv[1:]
expected_entries = json.loads(expected_json)
maximum_compressed = int(maximum_compressed)
maximum_total = int(maximum_total)
maximum_padding = 1024 * 1024

if not re.fullmatch(r"[0-9a-f]{64}", expected_sha256):
    raise SystemExit("expected macOS archive SHA-256 is not canonical")
expected = {entry["name"]: entry for entry in expected_entries}
if len(expected) != 10 or len(expected) != len(expected_entries):
    raise SystemExit("internal macOS archive inventory is not exactly ten unique entries")

destination_absolute = os.path.abspath(destination)
destination_state = os.lstat(destination_absolute)
if not stat.S_ISDIR(destination_state.st_mode) or stat.S_ISLNK(destination_state.st_mode):
    raise SystemExit("macOS archive extraction root is not an ordinary directory")
if os.listdir(destination_absolute):
    raise SystemExit("macOS archive extraction root is not empty")

flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
archive_descriptor = os.open(archive_path, flags)
try:
    before = os.fstat(archive_descriptor)
    path_before = os.lstat(archive_path)
    if (
        not stat.S_ISREG(before.st_mode)
        or stat.S_ISLNK(path_before.st_mode)
        or before.st_nlink != 1
        or before.st_dev != path_before.st_dev
        or before.st_ino != path_before.st_ino
        or before.st_size != path_before.st_size
        or before.st_size < 1
        or before.st_size > maximum_compressed
        or before.st_uid != os.geteuid()
    ):
        raise SystemExit("macOS archive is not one bounded current-user-owned ordinary file")

    digest = hashlib.sha256()
    compressed_bytes = 0
    while True:
        chunk = os.read(archive_descriptor, 1024 * 1024)
        if not chunk:
            break
        compressed_bytes += len(chunk)
        if compressed_bytes > maximum_compressed:
            raise SystemExit("macOS archive exceeds the compressed byte bound")
        digest.update(chunk)
    archive_sha256 = digest.hexdigest()
    if archive_sha256 != expected_sha256:
        raise SystemExit("macOS archive does not match its canonical checksum-manifest entry")
    os.lseek(archive_descriptor, 0, os.SEEK_SET)

    for entry in sorted(
        (item for item in expected_entries if item["kind"] == "directory"),
        key=lambda item: (item["name"].count("/"), item["name"]),
    ):
        target = os.path.join(destination_absolute, *entry["name"].split("/"))
        os.mkdir(target, 0o700)
        target_state = os.lstat(target)
        if not stat.S_ISDIR(target_state.st_mode) or stat.S_ISLNK(target_state.st_mode):
            raise SystemExit(f"macOS archive directory could not be materialized safely: {entry['name']}")

    def read_exact(stream, count, label):
        chunks = []
        remaining = count
        while remaining:
            chunk = stream.read(min(1024 * 1024, remaining))
            if not chunk:
                raise SystemExit(f"truncated macOS tar while reading {label}")
            chunks.append(chunk)
            remaining -= len(chunk)
        return b"".join(chunks)

    def parse_octal(field, label):
        value = field.strip(b" \0")
        if not value or re.fullmatch(rb"[0-7]+", value) is None:
            raise SystemExit(f"macOS tar has a noncanonical octal {label}")
        return int(value, 8)

    def parse_name(field):
        terminator = field.find(b"\0")
        if terminator < 1 or any(field[terminator + 1:]):
            raise SystemExit("macOS tar has a noncanonical name field")
        try:
            return field[:terminator].decode("ascii")
        except UnicodeDecodeError:
            raise SystemExit("macOS tar member name is not ASCII")

    seen = set()
    total_payload_bytes = 0
    with os.fdopen(os.dup(archive_descriptor), "rb", closefd=True) as archive_stream:
        with gzip.GzipFile(fileobj=archive_stream, mode="rb") as tar_stream:
            zero_blocks = 0
            while True:
                header = read_exact(tar_stream, 512, "a member header")
                if header == bytes(512):
                    zero_blocks += 1
                    if zero_blocks == 2:
                        break
                    continue
                if zero_blocks:
                    raise SystemExit("macOS tar has a noncanonical single zero block")
                stored_checksum = parse_octal(header[148:156], "header checksum")
                checksum_header = bytearray(header)
                checksum_header[148:156] = b"        "
                if sum(checksum_header) != stored_checksum:
                    raise SystemExit("macOS tar member header checksum mismatch")
                if header[257:263] != b"ustar\0" or header[263:265] != b"00":
                    raise SystemExit("macOS tar is not the expected PAX-free ustar format")
                if any(header[345:500]):
                    raise SystemExit("macOS tar uses an unexpected prefix or extended-name field")
                if any(header[157:257]):
                    raise SystemExit("macOS tar member contains a link target")

                raw_name = parse_name(header[0:100])
                type_flag = header[156:157]
                if type_flag == b"5":
                    logical_name = raw_name[:-1] if raw_name.endswith("/") else raw_name
                    kind = "directory"
                elif type_flag in (b"0", b"\0"):
                    logical_name = raw_name
                    kind = "file"
                else:
                    raise SystemExit("macOS tar contains a link, PAX record, or unsupported member type")
                if (
                    not logical_name
                    or logical_name.startswith("/")
                    or "\\" in logical_name
                    or "//" in logical_name
                    or any(part in ("", ".", "..") for part in logical_name.split("/"))
                    or (kind == "file" and raw_name.endswith("/"))
                    or (kind == "directory" and raw_name not in (logical_name, logical_name + "/"))
                ):
                    raise SystemExit("macOS tar contains a noncanonical or traversal-capable path")
                if logical_name in seen:
                    raise SystemExit(f"macOS tar contains a duplicate member: {logical_name}")
                entry = expected.get(logical_name)
                if entry is None or entry["kind"] != kind:
                    raise SystemExit(f"macOS tar inventory is unexpected: {logical_name}")
                seen.add(logical_name)

                mode = parse_octal(header[100:108], "member mode") & 0o7777
                size = parse_octal(header[124:136], "member size")
                if mode != entry["mode"]:
                    raise SystemExit(f"macOS tar member mode is not exact: {logical_name}")
                if kind == "directory":
                    if size != 0:
                        raise SystemExit(f"macOS tar directory has a payload: {logical_name}")
                    continue
                if size < 1 or size > entry["maximumBytes"]:
                    raise SystemExit(f"macOS tar member exceeds its byte bound: {logical_name}")
                total_payload_bytes += size
                if total_payload_bytes > maximum_total:
                    raise SystemExit("macOS tar exceeds the total uncompressed payload bound")

                target = os.path.join(destination_absolute, *logical_name.split("/"))
                parent = os.path.dirname(target)
                parent_state = os.lstat(parent)
                if not stat.S_ISDIR(parent_state.st_mode) or stat.S_ISLNK(parent_state.st_mode):
                    raise SystemExit(f"macOS tar member parent is unsafe: {logical_name}")
                output_flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
                output_descriptor = os.open(target, output_flags, 0o600)
                try:
                    remaining = size
                    while remaining:
                        chunk = read_exact(tar_stream, min(1024 * 1024, remaining), logical_name)
                        view = memoryview(chunk)
                        while view:
                            written = os.write(output_descriptor, view)
                            if written < 1:
                                raise SystemExit(f"macOS tar member write stalled: {logical_name}")
                            view = view[written:]
                        remaining -= len(chunk)
                    os.fchmod(output_descriptor, entry["mode"])
                    os.fsync(output_descriptor)
                finally:
                    os.close(output_descriptor)
                padding = (-size) % 512
                if padding and any(read_exact(tar_stream, padding, f"{logical_name} padding")):
                    raise SystemExit(f"macOS tar member has nonzero padding: {logical_name}")

            trailing_bytes = 0
            while True:
                chunk = tar_stream.read(1024 * 1024)
                if not chunk:
                    break
                trailing_bytes += len(chunk)
                if trailing_bytes > maximum_padding:
                    raise SystemExit("macOS tar contains excessive terminal padding")
                if any(chunk):
                    raise SystemExit("macOS tar contains data after its terminal zero blocks")
    if seen != set(expected):
        missing = sorted(set(expected) - seen)
        raise SystemExit(f"macOS tar omits exact required entries: {','.join(missing)}")

    for entry in sorted(
        (item for item in expected_entries if item["kind"] == "directory"),
        key=lambda item: (item["name"].count("/"), item["name"]),
        reverse=True,
    ):
        target = os.path.join(destination_absolute, *entry["name"].split("/"))
        directory_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0)
        directory_descriptor = os.open(target, directory_flags)
        try:
            directory_state = os.fstat(directory_descriptor)
            if not stat.S_ISDIR(directory_state.st_mode):
                raise SystemExit(f"macOS archive directory changed before mode finalization: {entry['name']}")
            os.fchmod(directory_descriptor, entry["mode"])
            os.fsync(directory_descriptor)
        finally:
            os.close(directory_descriptor)

    after = os.fstat(archive_descriptor)
    path_after = os.lstat(archive_path)
    stable_fields = ("st_dev", "st_ino", "st_size", "st_mtime_ns", "st_ctime_ns", "st_nlink")
    if any(getattr(before, field) != getattr(after, field) for field in stable_fields) or any(
        getattr(after, field) != getattr(path_after, field) for field in stable_fields
    ):
        raise SystemExit("macOS archive changed during bounded extraction")
finally:
    os.close(archive_descriptor)

print(json.dumps({
    "archiveSha256": archive_sha256,
    "compressedBytes": compressed_bytes,
    "uncompressedPayloadBytes": total_payload_bytes,
    "entryCount": len(seen),
}, separators=(",", ":")))
`;
  const result = spawnSync(
    "python3",
    [
      "-",
      path,
      destination,
      expectedArchiveSha256,
      JSON.stringify(EXPECTED_MACOS_ARCHIVE_ENTRIES),
      String(MAX_COMPRESSED_MACOS_ARCHIVE_BYTES),
      String(MAX_UNCOMPRESSED_MACOS_ARCHIVE_BYTES),
    ],
    {
      input: extractor,
      encoding: "utf8",
      maxBuffer: 1024 * 1024,
      timeout: SUBPROCESS_TIMEOUT_MS,
      killSignal: "SIGKILL",
      env: childEnvironment(),
    },
  );
  if (result.error?.code === "ETIMEDOUT" || result.signal === "SIGKILL") {
    throw new Error("bounded macOS archive extraction exceeded its execution deadline");
  }
  if (result.error) throw new Error("bounded macOS archive extractor could not be executed");
  if (result.status !== 0) {
    const detail = sanitizePathDetail((result.stderr || "no diagnostic output").trim());
    throw new Error(`bounded macOS archive extraction failed: ${detail}`);
  }
  let summary;
  try {
    summary = JSON.parse(result.stdout);
  } catch {
    throw new Error("bounded macOS archive extractor returned an invalid summary");
  }
  requireCheck(
    "macOS archive bounded exact inventory",
    summary.archiveSha256 === expectedArchiveSha256 &&
      summary.entryCount === EXPECTED_MACOS_ARCHIVE_ENTRIES.length &&
      Number.isSafeInteger(summary.compressedBytes) && summary.compressedBytes > 0 &&
      summary.compressedBytes <= MAX_COMPRESSED_MACOS_ARCHIVE_BYTES &&
      Number.isSafeInteger(summary.uncompressedPayloadBytes) &&
      summary.uncompressedPayloadBytes > 0 &&
      summary.uncompressedPayloadBytes <= MAX_UNCOMPRESSED_MACOS_ARCHIVE_BYTES,
    "ten PAX-free regular-file/directory entries passed bounded streaming extraction",
  );
  return summary;
}

async function sha256(path) {
  return createHash("sha256").update(await readFile(path)).digest("hex");
}

function verifyHarnessSourceBinding(stage) {
  const sourceRoot = run("git", ["-C", rigSourceDirectory, "rev-parse", "--show-toplevel"]);
  const sourceSha = run("git", ["-C", sourceRoot, "rev-parse", "HEAD"]);
  const branchName = run("git", ["-C", sourceRoot, "rev-parse", "--abbrev-ref", "HEAD"]);
  const status = run("git", [
    "-C", sourceRoot, "status", "--porcelain=v2", "--untracked-files=all",
  ]);
  run("git", ["-C", sourceRoot, "diff", "--quiet", "HEAD", "--"]);
  run("git", ["-C", sourceRoot, "diff", "--cached", "--quiet"]);
  const deleted = run("git", ["-C", sourceRoot, "ls-files", "--deleted"]);
  const untracked = run("git", [
    "-C", sourceRoot, "ls-files", "--others", "--exclude-standard",
  ]);
  run("git", ["-C", sourceRoot, "fsck", "--full"]);

  const harnessPaths = [
    rigSourcePath,
    fixtureSource,
    systemProbeSource,
    pointerHandoffSource,
    physicalPointerHandoffSource,
    acceptanceFinalizerSource,
  ];
  let exactTrackedHarnessBlobs = true;
  for (const sourcePath of harnessPaths) {
    const trackedPath = relative(sourceRoot, sourcePath);
    if (
      trackedPath.length === 0 || trackedPath === ".." ||
      trackedPath.startsWith(`..${sep}`) || resolve(sourceRoot, trackedPath) !== sourcePath
    ) {
      exactTrackedHarnessBlobs = false;
      break;
    }
    const headBlob = run("git", ["-C", sourceRoot, "rev-parse", `HEAD:${trackedPath}`]);
    const worktreeBlob = run("git", ["-C", sourceRoot, "hash-object", "--", sourcePath]);
    if (headBlob !== worktreeBlob) {
      exactTrackedHarnessBlobs = false;
      break;
    }
  }

  requireCheck(`${stage} harness source commit matches coordinator binding`,
    sourceSha === expectedSourceSha,
    sourceSha === expectedSourceSha ? "exact source SHA matched" : "source SHA mismatch");
  requireCheck(`${stage} harness checkout is detached`, branchName === "HEAD",
    branchName === "HEAD" ? "detached HEAD" : "checkout is attached to a branch");
  requireCheck(`${stage} harness checkout is clean`,
    status === "" && deleted === "" && untracked === "",
    status === "" && deleted === "" && untracked === ""
      ? "tracked, staged, deleted, and untracked sets are empty"
      : "checkout contains a worktree, index, deleted, or untracked change");
  requireCheck(`${stage} harness source repository passes git fsck`, true,
    "git fsck --full passed");
  requireCheck(`${stage} harness files are exact tracked blobs`, exactTrackedHarnessBlobs,
    exactTrackedHarnessBlobs
      ? "runner, native probes, and acceptance finalizer match HEAD blobs"
      : "harness blob mismatch");

  harnessSourceBinding.sourceSha = sourceSha;
  harnessSourceBinding.detachedHead = branchName === "HEAD";
  harnessSourceBinding.cleanTrackedAndUntracked = status === "" && deleted === "" && untracked === "";
  harnessSourceBinding.fsckPassed = true;
  harnessSourceBinding.exactTrackedHarnessBlobs = exactTrackedHarnessBlobs;
}

async function pathExists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}

async function deadlineCheckedFilesystemStep(deadlineMilliseconds, description, operation) {
  if (deadlineMilliseconds - Date.now() <= 0) {
    throw new Error(`${description} exceeded its absolute filesystem deadline`);
  }
  const result = await operation();
  if (deadlineMilliseconds - Date.now() <= 0) {
    throw new Error(`${description} completed after its absolute filesystem deadline`);
  }
  return result;
}

function hasExactBigIntFileIdentity(state) {
  return typeof state?.dev === "bigint" && state.dev > 0n &&
    typeof state.ino === "bigint" && state.ino > 0n &&
    typeof state.size === "bigint" && state.size >= 0n &&
    typeof state.mode === "bigint" &&
    typeof state.uid === "bigint" && typeof state.gid === "bigint" &&
    typeof state.nlink === "bigint" && state.nlink > 0n &&
    typeof state.mtimeNs === "bigint" && typeof state.ctimeNs === "bigint" &&
    typeof state.birthtimeNs === "bigint";
}

function samePersistentFileObjectIdentity(left, right) {
  return hasExactBigIntFileIdentity(left) && hasExactBigIntFileIdentity(right) &&
    left.dev === right.dev && left.ino === right.ino && left.size === right.size &&
    left.mode === right.mode && left.uid === right.uid && left.gid === right.gid &&
    left.birthtimeNs === right.birthtimeNs;
}

function sameCoreFileIdentity(left, right) {
  return samePersistentFileObjectIdentity(left, right) &&
    left.mtimeNs === right.mtimeNs;
}

function sameOrdinaryFileIdentity(left, right) {
  return sameCoreFileIdentity(left, right) &&
    left.ctimeNs === right.ctimeNs &&
    left.nlink === right.nlink;
}

function samePublishedFileAcrossWriterClose(left, right, platform = process.platform) {
  if (!samePersistentFileObjectIdentity(left, right) || left.nlink !== right.nlink) return false;
  if (platform === "win32") {
    // Windows can defer LastWriteTime and ChangeTime finalization until the
    // last writable handle closes. Permit only those two timestamp fields to
    // settle across that exact close boundary. The volume/file ID, creation
    // time, size, metadata projection, ownership projection, and link count
    // remain exact; the post-close descriptor/path/read sequence below again
    // requires every compared timestamp to be stable.
    return true;
  }
  return left.mtimeNs === right.mtimeNs && left.ctimeNs === right.ctimeNs;
}

function hasPlatformPrivateMarkerMetadata(
  state,
  platform = process.platform,
  currentUid = typeof process.getuid === "function" ? BigInt(process.getuid()) : null,
) {
  if (!hasExactBigIntFileIdentity(state)) return false;
  if (platform === "win32") {
    // libuv cannot expose a Windows DACL through Stats. It deliberately
    // projects a regular file as 0444 or 0666 and reports POSIX uid/gid as
    // zero, regardless of the 0600 creation request. Accept only that bounded
    // projection here; file authority still comes from exact volume/file ID,
    // timestamps, size, and link-count equality, never from these mode bits.
    const projectedPermissions = state.mode & 0o777n;
    return (projectedPermissions === 0o444n || projectedPermissions === 0o666n) &&
      state.uid === 0n && state.gid === 0n;
  }
  return currentUid !== null && state.uid === currentUid &&
    (state.mode & 0o077n) === 0n;
}

async function syncMarkerDirectory(path, deadlineMilliseconds, platform = process.platform) {
  if (platform === "win32") {
    // Node maps FileHandle.sync() to FlushFileBuffers on Windows. Windows has
    // no supported directory-fsync equivalent through that API, so the exact
    // file is synced before publication and rebound by handle after linking;
    // no unsupported directory flush is treated as a successful durability
    // claim.
    return false;
  }
  const directoryHandle = await deadlineCheckedFilesystemStep(
    deadlineMilliseconds,
    "operator marker directory open",
    () => open(
      dirname(path),
      fsConstants.O_RDONLY |
        (fsConstants.O_DIRECTORY ?? 0) |
        (fsConstants.O_NOFOLLOW ?? 0) |
        (fsConstants.O_NONBLOCK ?? 0),
    ),
  );
  try {
    await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker directory sync",
      () => directoryHandle.sync(),
    );
  } finally {
    await directoryHandle.close();
  }
  return true;
}

async function publishAtomicMarkerOnce(path, marker, timeoutMs = MARKER_PUBLISH_TIMEOUT_MS) {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1) {
    throw new Error("operator marker publication timeout is invalid");
  }
  const deadlineMilliseconds = Date.now() + timeoutMs;
  const serialized = `${JSON.stringify(marker)}\n`;
  assertNoToken(serialized, "operator marker");
  assertNoRetainedNativeTextPayload(serialized, "operator marker");
  assertNoRetainedPointerRawData(serialized, "operator marker");
  const temporaryPath = join(
    dirname(path),
    `.pointer-handoff-${randomBytes(12).toString("hex")}.tmp`,
  );
  let handle;
  let temporaryPresent = false;
  try {
    handle = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker temporary-file open",
      () => open(
        temporaryPath,
        fsConstants.O_WRONLY |
          fsConstants.O_CREAT |
          fsConstants.O_EXCL |
          (fsConstants.O_NOFOLLOW ?? 0) |
          (fsConstants.O_NONBLOCK ?? 0),
        0o600,
      ),
    );
    temporaryPresent = true;
    await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker write",
      () => handle.writeFile(serialized, "utf8"),
    );
    await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker file sync",
      () => handle.sync(),
    );
    const descriptorState = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker descriptor inspection",
      () => handle.stat({ bigint: true }),
    );
    const temporaryState = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker path inspection",
      () => lstat(temporaryPath, { bigint: true }),
    );
    if (
      !descriptorState.isFile() || !temporaryState.isFile() || temporaryState.isSymbolicLink() ||
      descriptorState.nlink !== 1n ||
      descriptorState.size !== BigInt(Buffer.byteLength(serialized, "utf8")) ||
      !hasPlatformPrivateMarkerMetadata(descriptorState) ||
      !sameOrdinaryFileIdentity(descriptorState, temporaryState)
    ) {
      throw new Error("operator marker temporary file failed its stable ordinary-file binding");
    }
    await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker create-once link",
      () => link(temporaryPath, path),
    );
    const linkedDescriptorState = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker linked descriptor inspection",
      () => handle.stat({ bigint: true }),
    );
    const linkedTemporaryState = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker linked source inspection",
      () => lstat(temporaryPath, { bigint: true }),
    );
    const destinationState = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker destination inspection",
      () => lstat(path, { bigint: true }),
    );
    if (
      linkedDescriptorState.nlink !== 2n || linkedTemporaryState.nlink !== 2n ||
      destinationState.nlink !== 2n ||
      !sameCoreFileIdentity(descriptorState, linkedDescriptorState) ||
      !sameOrdinaryFileIdentity(linkedDescriptorState, linkedTemporaryState) ||
      !sameOrdinaryFileIdentity(linkedTemporaryState, destinationState)
    ) {
      throw new Error("operator marker destination did not bind to the exact temporary inode");
    }
    await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker temporary-name removal",
      () => unlink(temporaryPath),
    );
    temporaryPresent = false;
    const publishedDescriptorState = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker published descriptor inspection",
      () => handle.stat({ bigint: true }),
    );
    const publishedState = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker published identity inspection",
      () => lstat(path, { bigint: true }),
    );
    if (
      publishedDescriptorState.nlink !== 1n || publishedState.nlink !== 1n ||
      !publishedDescriptorState.isFile() || !publishedState.isFile() ||
      publishedState.isSymbolicLink() ||
      !hasPlatformPrivateMarkerMetadata(publishedDescriptorState) ||
      !sameCoreFileIdentity(descriptorState, publishedDescriptorState) ||
      !sameOrdinaryFileIdentity(publishedDescriptorState, publishedState)
    ) {
      throw new Error("operator marker published inode changed after temporary-name removal");
    }
    await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker descriptor close",
      () => handle.close(),
    );
    handle = null;
    const settledPublishedState = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker post-close identity inspection",
      () => lstat(path, { bigint: true }),
    );
    if (
      settledPublishedState.nlink !== 1n || !settledPublishedState.isFile() ||
      settledPublishedState.isSymbolicLink() ||
      !hasPlatformPrivateMarkerMetadata(settledPublishedState) ||
      !samePublishedFileAcrossWriterClose(publishedState, settledPublishedState)
    ) {
      throw new Error("operator marker identity changed while its writer closed");
    }
    const publishedHandle = await deadlineCheckedFilesystemStep(
      deadlineMilliseconds,
      "operator marker published-file open",
      () => open(
        path,
        fsConstants.O_RDONLY |
          (fsConstants.O_NOFOLLOW ?? 0) |
          (fsConstants.O_NONBLOCK ?? 0),
      ),
    );
    try {
      const reboundDescriptorState = await deadlineCheckedFilesystemStep(
        deadlineMilliseconds,
        "operator marker published-file descriptor inspection",
        () => publishedHandle.stat({ bigint: true }),
      );
      const publishedBytes = await deadlineCheckedFilesystemStep(
        deadlineMilliseconds,
        "operator marker published-file content inspection",
        () => publishedHandle.readFile(),
      );
      const reboundDescriptorAfterRead = await deadlineCheckedFilesystemStep(
        deadlineMilliseconds,
        "operator marker published-file post-read inspection",
        () => publishedHandle.stat({ bigint: true }),
      );
      const reboundPublishedState = await deadlineCheckedFilesystemStep(
        deadlineMilliseconds,
        "operator marker published-file path reinspection",
        () => lstat(path, { bigint: true }),
      );
      if (
        reboundDescriptorState.nlink !== 1n || reboundDescriptorAfterRead.nlink !== 1n ||
        reboundPublishedState.nlink !== 1n ||
        !reboundDescriptorState.isFile() || !reboundDescriptorAfterRead.isFile() ||
        !reboundPublishedState.isFile() ||
        reboundPublishedState.isSymbolicLink() ||
        !hasPlatformPrivateMarkerMetadata(reboundDescriptorState) ||
        !publishedBytes.equals(Buffer.from(serialized, "utf8")) ||
        !sameOrdinaryFileIdentity(settledPublishedState, reboundDescriptorState) ||
        !sameOrdinaryFileIdentity(reboundDescriptorState, reboundDescriptorAfterRead) ||
        !sameOrdinaryFileIdentity(reboundDescriptorAfterRead, reboundPublishedState)
      ) {
        throw new Error("operator marker published path lost its stable ordinary-file binding");
      }
    } finally {
      await publishedHandle.close();
    }
    await syncMarkerDirectory(path, deadlineMilliseconds);
  } finally {
    if (handle) {
      try { await handle.close(); } catch {}
    }
    if (temporaryPresent) {
      try {
        await unlink(temporaryPath);
      } catch (error) {
        if (error?.code !== "ENOENT") throw error;
      }
    }
  }
}

async function writePointerHandoffState(state, details = {}) {
  let lines;
  if (state === APP_SHARE_HANDOFF_READY_STATE) {
    if (!/^[0-9a-f]{64}$/.test(details.requestSha256 || "")) {
      throw new Error("refusing an unbound app-share READY state");
    }
    lines = [state, details.requestSha256];
  } else if (state === POINTER_HANDOFF_ACTION_STATE) {
    if (
      !/^[0-9a-f]{64}$/.test(details.requestSha256 || "") ||
      !/^[0-9a-f]{64}$/.test(details.startReceiptSha256 || "") ||
      typeof details.productActionStartedAt !== "string"
    ) {
      throw new Error("refusing an unbound app-share ACTION state");
    }
    lines = [
      state,
      details.requestSha256,
      details.startReceiptSha256,
      details.productActionStartedAt,
    ];
  } else if (state === POINTER_HANDOFF_COMPLETE_STATE) {
    if (
      !/^[0-9a-f]{64}$/.test(details.requestSha256 || "") ||
      !/^[0-9a-f]{64}$/.test(details.startReceiptSha256 || "") ||
      typeof details.productActionStartedAt !== "string" ||
      typeof details.productActionCompletedAt !== "string" ||
      details.productActionStartedAt > details.productActionCompletedAt
    ) {
      throw new Error("refusing an unbound app-share COMPLETE state");
    }
    lines = [
      state,
      details.requestSha256,
      details.startReceiptSha256,
      details.productActionStartedAt,
      details.productActionCompletedAt,
    ];
  } else {
    throw new Error("refusing an invalid app-share handoff state");
  }
  const temporaryPath = `${pointerHandoffControlPath}.${randomBytes(8).toString("hex")}.tmp`;
  await writeFile(temporaryPath, `${lines.join("\n")}\n`, {
    encoding: "utf8",
    flag: "wx",
    mode: 0o600,
  });
  await rename(temporaryPath, pointerHandoffControlPath);
}

function pointerHandoffSummary() {
  return {
    requested: pointerEvidenceLane === "deliberate-concurrency",
    requestPublicationAcknowledged: pointerHandoffRequestPublicationAcknowledged,
    startReceiptAcknowledged: pointerHandoffStartReceiptSha256 !== null,
    completePublicationAcknowledged: pointerHandoffCompletePublicationAcknowledged,
    promptClosed: pointerHandoffPromptClosed,
    exactAppBundleObserved: pointerHandoffExactAppBundleObserved,
    exactWindowObserved: pointerHandoffExactWindowObserved,
    exactButtonObserved: pointerHandoffExactButtonObserved,
    buttonDisabledAfterAction: pointerHandoffButtonDisabledAfterAction,
    acceptanceButtonActionObserved: pointerHandoffAcceptanceButtonActionObserved,
    appShareSurfaceObservedAtProductBoundaries: pointerHandoffSurfaceObservedAtProductBoundaries,
    sharedHidInputObserved: pointerHandoffSharedHidInputObserved,
    sampledSharedContextUnchanged: pointerHandoffSampledSharedContextUnchanged,
    authorityRefreshedAfterReceipt: pointerHandoffAuthorityRefreshedAfterReceipt,
    authorityFreshAtDispatch: pointerHandoffAuthorityFreshAtDispatch,
    actionDispatched: pointerHandoffActionDispatched,
    targetPostconditionObserved: pointerHandoffTargetPostconditionObserved,
    productBoundaryQuiet: pointerHandoffProductBoundaryQuiet,
    independentBoundaryQuiet: pointerHandoffIndependentBoundaryQuiet,
    physicalHumanProvenanceClaimed: false,
    cryptographicToolIdentityClaimed: false,
    orchestrationNotProductControl: true,
    markerNotificationOnly: false,
    markerAcceptedAsProductAuthority: false,
    rawAppIdentityRetainedInResult: false,
    rawPointerDataRetained: false,
  };
}

function pointerHandoffRequestMarker(promptPid) {
  pointerHandoffRequestCreatedAt = new Date().toISOString();
  return {
    schemaVersion: APP_SHARE_HANDOFF_MARKER_SCHEMA,
    kind: "macos-app-share-concurrency-handoff-request",
    productVersion: EXPECTED_VERSION,
    requestId: pointerHandoffRequestId,
    createdAt: pointerHandoffRequestCreatedAt,
    expiresAt: new Date(pointerHandoffArmDeadlineMilliseconds).toISOString(),
    runnerPid: process.pid,
    promptPid,
    expectedBundleIdentifier: APP_SHARE_BUNDLE_IDENTIFIER,
    expectedWindowTitle: APP_SHARE_WINDOW_TITLE,
    expectedButtonText: APP_SHARE_READY_BUTTON_TEXT,
    expectedButtonAccessibilityIdentifier: "lbb-app-share-start",
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

function canonicalFlatJsonRecord(record) {
  return `${JSON.stringify(record, Object.keys(record).sort())}\n`;
}

async function readBoundAppShareReceipt(path, expectedKeys, validate, description) {
  let handle;
  let bytes;
  try {
    handle = await open(
      path,
      fsConstants.O_RDONLY |
        (fsConstants.O_NOFOLLOW ?? 0) |
        (fsConstants.O_NONBLOCK ?? 0),
    );
    const descriptorBefore = await handle.stat({ bigint: true });
    const pathBefore = await lstat(path, { bigint: true });
    if (
      !descriptorBefore.isFile() || !pathBefore.isFile() || pathBefore.isSymbolicLink() ||
      descriptorBefore.nlink !== 1n ||
      !sameOrdinaryFileIdentity(descriptorBefore, pathBefore) ||
      descriptorBefore.size < 1n || descriptorBefore.size > 16n * 1024n ||
      !hasPlatformPrivateMarkerMetadata(descriptorBefore)
    ) {
      throw new Error(`${description} is not one owner-private ordinary single-link file`);
    }
    bytes = await handle.readFile();
    const descriptorAfter = await handle.stat({ bigint: true });
    const pathAfter = await lstat(path, { bigint: true });
    if (
      BigInt(bytes.length) !== descriptorBefore.size ||
      !descriptorAfter.isFile() || !pathAfter.isFile() || pathAfter.isSymbolicLink() ||
      descriptorAfter.nlink !== 1n ||
      !hasPlatformPrivateMarkerMetadata(descriptorAfter) ||
      !sameOrdinaryFileIdentity(descriptorBefore, descriptorAfter) ||
      !sameOrdinaryFileIdentity(descriptorAfter, pathAfter)
    ) {
      throw new Error(`${description} changed while it was read`);
    }
  } finally {
    await handle?.close();
  }
  let raw;
  try {
    raw = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error(`${description} is not strict UTF-8`);
  }
  assertNoToken(raw, description);
  assertNoRetainedNativeTextPayload(raw, description);
  assertNoRetainedPointerRawData(raw, description);
  let record;
  try {
    record = JSON.parse(raw);
  } catch {
    throw new Error(`${description} is not valid JSON`);
  }
  if (
    record == null || Array.isArray(record) || typeof record !== "object" ||
    Object.keys(record).sort().join("\n") !== [...expectedKeys].sort().join("\n") ||
    raw !== canonicalFlatJsonRecord(record) ||
    !validate(record)
  ) {
    throw new Error(`${description} failed its exact bound schema`);
  }
  return {
    record,
    sha256: createHash("sha256").update(raw, "utf8").digest("hex"),
  };
}

async function readAppShareStartReceipt(promptPid) {
  return await readBoundAppShareReceipt(
    pointerHandoffStartPath,
    [
      "acceptedAsAuthority", "buttonAccepted", "buttonActionObserved", "createdAt",
      "cryptographicToolIdentityClaimed", "kind", "physicalHumanProvenanceClaimed",
      "productVersion", "promptPid", "requestId", "requestSha256", "schemaVersion",
    ],
    (record) => {
      const created = Date.parse(record.createdAt);
      const requestCreated = Date.parse(pointerHandoffRequestCreatedAt);
      return record.schemaVersion === APP_SHARE_HANDOFF_MARKER_SCHEMA &&
        record.kind === "macos-app-share-concurrency-handoff-start" &&
        record.productVersion === EXPECTED_VERSION &&
        record.requestId === pointerHandoffRequestId &&
        record.requestSha256 === pointerHandoffRequestSha256 &&
        record.promptPid === promptPid &&
        record.buttonAccepted === true &&
        record.buttonActionObserved === true &&
        record.cryptographicToolIdentityClaimed === false &&
        record.physicalHumanProvenanceClaimed === false &&
        record.acceptedAsAuthority === false &&
        Number.isFinite(created) && Number.isFinite(requestCreated) &&
        created >= requestCreated && created <= Date.now() + 1_000;
    },
    "app-share start receipt",
  );
}

async function readAppShareCompleteReceipt(promptPid) {
  return await readBoundAppShareReceipt(
    pointerHandoffCompletePath,
    [
      "acceptedAsAuthority",
      "buttonRemainedDisabledDuringProductAction", "createdAt", "kind",
      "cryptographicToolIdentityClaimed", "handoffStateSequenceBound",
      "physicalHumanProvenanceClaimed", "productActionCompletedAt", "productActionStartedAt",
      "productVersion", "promptPid", "requestId", "requestSha256", "schemaVersion",
      "startReceiptSha256",
    ],
    (record) => {
      const created = Date.parse(record.createdAt);
      const startCreated = Date.parse(pointerHandoffStartReceiptCreatedAt);
      const productStarted = Date.parse(record.productActionStartedAt);
      const productCompleted = Date.parse(record.productActionCompletedAt);
      return record.schemaVersion === APP_SHARE_HANDOFF_MARKER_SCHEMA &&
        record.kind === "macos-app-share-concurrency-handoff-complete" &&
        record.productVersion === EXPECTED_VERSION &&
        record.requestId === pointerHandoffRequestId &&
        record.requestSha256 === pointerHandoffRequestSha256 &&
        record.startReceiptSha256 === pointerHandoffStartReceiptSha256 &&
        record.promptPid === promptPid &&
        record.productActionStartedAt === pointerHandoffProductActionStartedAt &&
        record.productActionCompletedAt === pointerHandoffProductActionCompletedAt &&
        record.handoffStateSequenceBound === true &&
        record.buttonRemainedDisabledDuringProductAction === true &&
        record.cryptographicToolIdentityClaimed === false &&
        record.physicalHumanProvenanceClaimed === false &&
        record.acceptedAsAuthority === false &&
        Number.isFinite(created) && Number.isFinite(startCreated) &&
        Number.isFinite(productStarted) && Number.isFinite(productCompleted) &&
        productStarted >= startCreated && productStarted - startCreated <= 10_000 &&
        productCompleted >= productStarted && productCompleted - productStarted <= 10_000 &&
        created >= productCompleted && created - startCreated <= 10_000 &&
        created <= Date.now() + 1_000;
    },
    "app-share completion receipt",
  );
}

function canonicalChecksumEntries(contents) {
  if (contents.includes("\r") || !contents.endsWith("\n")) return null;
  const lines = contents.slice(0, -1).split("\n");
  if (lines.length !== CANONICAL_RELEASE_ASSETS.length) return null;
  const entries = lines.map((line) => {
    const match = /^([0-9a-f]{64})  ([^\s/]+)$/.exec(line);
    return match ? { sha256: match[1], file: match[2] } : null;
  });
  if (entries.some((entry) => entry === null)) return null;
  if (new Set(entries.map((entry) => entry.file)).size !== entries.length) return null;
  return entries.every((entry, index) => entry.file === CANONICAL_RELEASE_ASSETS[index])
    ? entries
    : null;
}

async function bindCanonicalChecksumManifest(expectedSha256) {
  requireCheck(
    "expected checksum-manifest hash format",
    /^[0-9a-f]{64}$/.test(expectedSha256),
    "64 lowercase hexadecimal characters",
  );
  requireCheck(
    "canonical checksum-manifest name",
    basename(sumsPath) === "SHA256SUMS.txt",
    basename(sumsPath),
  );
  requireCheck("exact archive name", basename(archivePath) === EXPECTED_ARCHIVE, basename(archivePath));

  let manifestHandle;
  let bytes;
  try {
    manifestHandle = await open(
      sumsPath,
      fsConstants.O_RDONLY | (fsConstants.O_NOFOLLOW ?? 0),
    );
    const descriptorBefore = await manifestHandle.stat({ bigint: true });
    const pathBefore = await lstat(sumsPath, { bigint: true });
    requireCheck(
      "checksum manifest is one bounded current-user-owned ordinary file",
      descriptorBefore.isFile() && pathBefore.isFile() && !pathBefore.isSymbolicLink() &&
        descriptorBefore.nlink === 1n && sameOrdinaryFileIdentity(descriptorBefore, pathBefore) &&
        descriptorBefore.size > 0n &&
        descriptorBefore.size <= BigInt(MAX_CHECKSUM_MANIFEST_BYTES) &&
        (descriptorBefore.mode & 0o022n) === 0n &&
        (typeof process.getuid !== "function" ||
          descriptorBefore.uid === BigInt(process.getuid())),
      "ordinary, singly linked, owner-controlled, and within the manifest byte bound",
    );
    bytes = await manifestHandle.readFile();
    const descriptorAfter = await manifestHandle.stat({ bigint: true });
    const pathAfter = await lstat(sumsPath, { bigint: true });
    requireCheck(
      "checksum manifest stayed stable while it was bound",
      BigInt(bytes.length) === descriptorBefore.size &&
        sameOrdinaryFileIdentity(descriptorBefore, descriptorAfter) &&
        sameOrdinaryFileIdentity(descriptorAfter, pathAfter),
      "path and descriptor identity, size, links, and timestamps remained unchanged",
    );
  } finally {
    await manifestHandle?.close();
  }

  const manifestSha256 = createHash("sha256").update(bytes).digest("hex");
  manifestBinding.actualSha256 = manifestSha256;
  manifestBinding.expectedSha256 = expectedSha256;
  manifestBinding.expectedSha256Matched = manifestSha256 === expectedSha256;
  requireCheck(
    "out-of-band checksum-manifest hash matches",
    manifestBinding.expectedSha256Matched,
    manifestSha256,
  );

  let sums;
  try {
    sums = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new Error("checksum manifest is not canonical UTF-8");
  }
  const canonicalEntries = canonicalChecksumEntries(sums);
  manifestBinding.exactCanonicalAssetSet = canonicalEntries !== null;
  manifestBinding.canonicalEntryCount = canonicalEntries?.length ?? 0;
  requireCheck(
    "checksum manifest has the exact canonical four-entry set",
    manifestBinding.exactCanonicalAssetSet,
    canonicalEntries ? CANONICAL_RELEASE_ASSETS.join(",") : "non-canonical manifest refused",
  );
  const manifestEntry = canonicalEntries?.find((entry) => entry.file === EXPECTED_ARCHIVE);
  requireCheck(
    "canonical manifest contains the macOS archive checksum",
    typeof manifestEntry?.sha256 === "string" && /^[0-9a-f]{64}$/.test(manifestEntry.sha256),
    manifestEntry?.sha256 || "missing archive checksum",
  );
  return { manifestSha256, canonicalEntries, archiveSha256: manifestEntry.sha256 };
}

function pngDimensions(data) {
  if (data.length < 24 || data.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") {
    throw new Error("captured screenshot is not a PNG");
  }
  return { width: data.readUInt32BE(16), height: data.readUInt32BE(20) };
}

function exactVersion(path, executableName) {
  return run(path, ["--version"]).replace(`${executableName} `, "");
}

function architectures(path) {
  return run("lipo", ["-archs", path]).split(/\s+/).filter(Boolean).sort();
}

function processProbe(
  path,
  targetPid = null,
  pointerPrompt = null,
  timeoutMs = SYSTEM_PROBE_TIMEOUT_MS,
) {
  const hasTarget = Number.isSafeInteger(targetPid) && targetPid > 0;
  const hasPointerPrompt =
    Number.isSafeInteger(pointerPrompt?.pid) && pointerPrompt.pid > 0 &&
    [
      POINTER_HANDOFF_WAITING_STATE,
      APP_SHARE_HANDOFF_READY_STATE,
      APP_SHARE_HANDOFF_ARMED_STATE,
      POINTER_HANDOFF_ACTION_STATE,
      POINTER_HANDOFF_COMPLETE_STATE,
    ].includes(pointerPrompt?.state);
  const args = hasPointerPrompt
    ? [hasTarget ? String(targetPid) : "0", "0", "0", String(pointerPrompt.pid), pointerPrompt.state]
    : hasTarget ? [String(targetPid)] : [];
  return JSON.parse(run(path, args, timeoutMs));
}

function pointerPromptDeliveryObserved(sample) {
  return (
    sample?.appSharePromptProbeRequested === true &&
    sample?.appSharePromptBundleMatched === true &&
    sample?.appSharePromptOwnerMatched === true &&
    sample?.appSharePromptTitleMatched === true &&
    sample?.appSharePromptButtonMatched === true &&
    sample?.appSharePromptButtonEnabledMatched === true &&
    sample?.appSharePromptOnScreen === true &&
    sample?.appSharePromptNonactivating === true
  );
}

async function processProbeWaitingForActive(path, targetPid, targetWindowId, timeoutMs) {
  return await new Promise((resolveProbe, rejectProbe) => {
    const child = spawn(path, [String(targetPid), String(targetWindowId), String(timeoutMs)], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      if (stdout.length > 1024 * 1024) child.kill("SIGKILL");
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      if (stderr.length > 1024 * 1024) child.kill("SIGKILL");
    });
    child.once("error", rejectProbe);
    child.once("close", (code) => {
      if (code !== 0) {
        rejectProbe(new Error(`system probe failed: ${stderr.trim() || `exit ${code}`}`));
        return;
      }
      try {
        resolveProbe(JSON.parse(stdout));
      } catch {
        rejectProbe(new Error("system probe returned malformed JSON"));
      }
    });
  });
}

function hidPointerCounterProgress(before, after) {
  if (
    before?.pointerActivityMonitorHealthy !== true ||
    after?.pointerActivityMonitorHealthy !== true ||
    before?.hidPointerCounters == null ||
    after?.hidPointerCounters == null
  ) {
    return "unknown";
  }
  let advanced = false;
  for (const field of HID_POINTER_COUNTER_FIELDS) {
    const beforeValue = before.hidPointerCounters[field];
    const afterValue = after.hidPointerCounters[field];
    if (
      !Number.isSafeInteger(beforeValue) || beforeValue < 0 || beforeValue > 0xffffffff ||
      !Number.isSafeInteger(afterValue) || afterValue < 0 || afterValue > 0xffffffff
    ) {
      return "unknown";
    }
    const delta = (afterValue - beforeValue) >>> 0;
    if (delta > MAX_HID_POINTER_COUNTER_ADVANCE) return "unknown";
    if (delta > 0) advanced = true;
  }
  return advanced ? "advanced" : "stable";
}

function hidKeyboardCounterProgress(before, after) {
  if (
    before?.pointerActivityMonitorHealthy !== true ||
    after?.pointerActivityMonitorHealthy !== true ||
    before?.hidKeyboardCounters == null ||
    after?.hidKeyboardCounters == null
  ) {
    return "unknown";
  }
  let advanced = false;
  for (const field of HID_KEYBOARD_COUNTER_FIELDS) {
    const beforeValue = before.hidKeyboardCounters[field];
    const afterValue = after.hidKeyboardCounters[field];
    if (
      !Number.isSafeInteger(beforeValue) || beforeValue < 0 || beforeValue > 0xffffffff ||
      !Number.isSafeInteger(afterValue) || afterValue < 0 || afterValue > 0xffffffff
    ) {
      return "unknown";
    }
    const delta = (afterValue - beforeValue) >>> 0;
    if (delta > MAX_HID_POINTER_COUNTER_ADVANCE) return "unknown";
    if (delta > 0) advanced = true;
  }
  return advanced ? "advanced" : "stable";
}

function clickFreePointerMotionProgress(before, after) {
  if (
    before?.pointerActivityMonitorHealthy !== true ||
    after?.pointerActivityMonitorHealthy !== true ||
    before?.hidPointerCounters == null ||
    after?.hidPointerCounters == null
  ) {
    return "unknown";
  }
  let mouseMoved = false;
  for (const field of HID_POINTER_COUNTER_FIELDS) {
    const beforeValue = before.hidPointerCounters[field];
    const afterValue = after.hidPointerCounters[field];
    if (
      !Number.isSafeInteger(beforeValue) || beforeValue < 0 || beforeValue > 0xffffffff ||
      !Number.isSafeInteger(afterValue) || afterValue < 0 || afterValue > 0xffffffff
    ) {
      return "unknown";
    }
    const delta = (afterValue - beforeValue) >>> 0;
    if (delta > MAX_HID_POINTER_COUNTER_ADVANCE) return "unknown";
    if (delta === 0) continue;
    if (field !== "mouseMoved") return "disallowed";
    mouseMoved = true;
  }
  return mouseMoved ? "advanced" : "stable";
}

function quietSeatSampleMonitoringState(sample) {
  const counters = sample?.hidPointerCounters;
  const keyboardCounters = sample?.hidKeyboardCounters;
  const pointerReadable =
    sample?.pointerActivityMonitorHealthy === true &&
    Number.isFinite(sample?.cursorX) && Number.isFinite(sample?.cursorY) &&
    counters != null && HID_POINTER_COUNTER_FIELDS.every((field) =>
      Number.isSafeInteger(counters[field]) && counters[field] >= 0 && counters[field] <= 0xffffffff
    );
  const keyboardReadable =
    sample?.pointerActivityMonitorHealthy === true &&
    keyboardCounters != null && HID_KEYBOARD_COUNTER_FIELDS.every((field) =>
      Number.isSafeInteger(keyboardCounters[field]) && keyboardCounters[field] >= 0 &&
      keyboardCounters[field] <= 0xffffffff
    );
  if (
    !pointerReadable || !keyboardReadable ||
    sample?.activeSpaceProbeHealthy !== true ||
    !Number.isSafeInteger(sample?.activeSpace) || sample.activeSpace < 1 ||
    sample?.foregroundProbeHealthy !== true
  ) {
    return "unknown";
  }
  if (sample?.foregroundTransitionObserved === true) return "changed";
  if (
    sample?.foregroundIdentityStable !== true ||
    sample?.rawForegroundIdentityStable !== true ||
    sample?.foregroundAXProbeHealthy !== true ||
    !Number.isSafeInteger(sample?.foregroundPID) || sample.foregroundPID < 1 ||
    !Number.isSafeInteger(sample?.rawForegroundPID) ||
    sample.rawForegroundPID !== sample.foregroundPID ||
    typeof sample?.rawForegroundPSN !== "string" || sample.rawForegroundPSN.length !== 16 ||
    !Number.isSafeInteger(sample?.frontWindowID) || sample.frontWindowID < 1 ||
    !Number.isSafeInteger(sample?.foregroundAXFocusedWindowID) ||
    sample.foregroundAXFocusedWindowID < 1 ||
    sample.foregroundAXFocusedWindowID !== sample.foregroundAXMainWindowID ||
    sample?.foregroundAXFrontmost !== true
  ) {
    return "unknown";
  }
  return "readable";
}

function quietSeatTransitionDisposition(before, after) {
  const beforeState = quietSeatSampleMonitoringState(before);
  const afterState = quietSeatSampleMonitoringState(after);
  if (beforeState === "unknown" || afterState === "unknown") return "unknown";
  if (beforeState === "changed" || afterState === "changed") return "reset";

  const invariants = systemInvariants(before, after);
  const pointerState = independentPointerLaneState(invariants);
  if (pointerState === "unknown") return "unknown";
  if (
    pointerState === "concurrent-shared-seat-activity" ||
    invariants.foregroundUnchanged !== true ||
    invariants.userFocusUnchanged !== true ||
    invariants.spaceUnchanged !== true
  ) {
    return "reset";
  }
  return pointerState === "quiet" ? "stable" : "unknown";
}

function quietSeatTimeoutError() {
  return new Error("timed out waiting for one continuous native quiet-seat stabilization epoch");
}

function quietSeatUnknownError() {
  return new Error("native quiet-seat monitoring became unknown or unhealthy before product execution");
}

function attachQuietSeatSummary(error, summary) {
  error.quietSeatStabilization = { ...summary };
  return error;
}

async function runNativeQuietSeatStabilization({
  baseline,
  waitDeadlineMilliseconds,
  requiredStableMilliseconds = QUIET_SEAT_REQUIRED_STABLE_MS,
  sampleIntervalMilliseconds = QUIET_SEAT_SAMPLE_INTERVAL_MS,
  requiredStableTransitions = QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS,
  nowMilliseconds = () => Date.now(),
  maximumWaitMilliseconds = waitDeadlineMilliseconds - nowMilliseconds(),
  probe,
  pause = delay,
  report = log,
  onSummary = () => {},
}) {
  if (
    !Number.isSafeInteger(waitDeadlineMilliseconds) ||
    !Number.isSafeInteger(requiredStableMilliseconds) || requiredStableMilliseconds < 1 ||
    !Number.isSafeInteger(sampleIntervalMilliseconds) || sampleIntervalMilliseconds < 1 ||
    !Number.isSafeInteger(requiredStableTransitions) || requiredStableTransitions < 1 ||
    typeof probe !== "function"
  ) {
    throw new Error("the native quiet-seat stabilization policy is incomplete");
  }

  const startedAtMilliseconds = nowMilliseconds();
  const remainingWaitMilliseconds = waitDeadlineMilliseconds - startedAtMilliseconds;
  if (
    !Number.isSafeInteger(maximumWaitMilliseconds) || maximumWaitMilliseconds < 1 ||
    !Number.isSafeInteger(remainingWaitMilliseconds) || remainingWaitMilliseconds < 1 ||
    remainingWaitMilliseconds > maximumWaitMilliseconds
  ) {
    throw quietSeatTimeoutError();
  }
  const summary = {
    required: true,
    completed: false,
    requiredStableMilliseconds,
    maximumWaitMilliseconds,
    sampleIntervalMilliseconds,
    requiredStableTransitions,
    stableDurationMilliseconds: 0,
    observedSamples: 1,
    stableTransitions: 0,
    resetCount: 0,
    monitoringUnknown: false,
    completedBeforeCandidateExecution: false,
    rawPointerDataRetained: false,
  };
  const publishSummary = () => onSummary({ ...summary });
  const failUnknown = () => {
    summary.monitoringUnknown = true;
    publishSummary();
    throw attachQuietSeatSummary(quietSeatUnknownError(), summary);
  };

  const baselineState = quietSeatSampleMonitoringState(baseline);
  if (baselineState === "unknown") failUnknown();
  let previous = baseline;
  let stableEpochStartedAt = startedAtMilliseconds;
  if (baselineState === "changed") {
    summary.resetCount = 1;
    report("Observed pre-execution user-context activity; starting a fresh native quiet-seat epoch.");
  }
  publishSummary();

  while (nowMilliseconds() < waitDeadlineMilliseconds) {
    const remainingProbeBudgetMilliseconds = waitDeadlineMilliseconds - nowMilliseconds();
    if (!Number.isSafeInteger(remainingProbeBudgetMilliseconds) || remainingProbeBudgetMilliseconds < 1) {
      break;
    }
    let sample;
    try {
      sample = await probe(Math.min(SYSTEM_PROBE_TIMEOUT_MS, remainingProbeBudgetMilliseconds));
    } catch (error) {
      if (armProbeExpired(error, waitDeadlineMilliseconds, nowMilliseconds())) break;
      failUnknown();
    }
    if (nowMilliseconds() >= waitDeadlineMilliseconds) break;
    summary.observedSamples += 1;
    const disposition = quietSeatTransitionDisposition(previous, sample);
    if (disposition === "unknown") failUnknown();

    const observedAtMilliseconds = nowMilliseconds();
    if (disposition === "reset") {
      summary.resetCount += 1;
      summary.stableDurationMilliseconds = 0;
      summary.stableTransitions = 0;
      stableEpochStartedAt = observedAtMilliseconds;
      report("Pointer, foreground, focus, or Space activity reset the native quiet-seat epoch; no candidate process has started.");
    } else {
      summary.stableTransitions += 1;
      summary.stableDurationMilliseconds = Math.max(
        0,
        observedAtMilliseconds - stableEpochStartedAt,
      );
      if (
        summary.stableDurationMilliseconds >= requiredStableMilliseconds &&
        summary.stableTransitions >= requiredStableTransitions
      ) {
        summary.completed = true;
        summary.completedBeforeCandidateExecution = true;
        publishSummary();
        return { finalSample: sample, summary: { ...summary } };
      }
    }
    previous = sample;
    publishSummary();
    const remainingPauseBudgetMilliseconds = waitDeadlineMilliseconds - nowMilliseconds();
    if (remainingPauseBudgetMilliseconds < 1) break;
    await pause(Math.min(sampleIntervalMilliseconds, remainingPauseBudgetMilliseconds));
  }

  publishSummary();
  throw attachQuietSeatSummary(quietSeatTimeoutError(), summary);
}

function preDispatchPointerTransitionDisposition(progress, invariants) {
  if (!["stable", "advanced", "disallowed"].includes(progress)) return "unknown";
  if (
    progress === "disallowed" ||
    invariants?.foregroundUnchanged !== true ||
    invariants?.userFocusUnchanged !== true ||
    invariants?.spaceUnchanged !== true
  ) {
    return "rearm";
  }
  return "accept";
}

function armProbeExpired(error, deadlineMilliseconds, nowMilliseconds = Date.now()) {
  return error?.code === "SUBPROCESS_TIMEOUT" && nowMilliseconds >= deadlineMilliseconds;
}

function deliberatePointerTimeoutError() {
  return new Error(
    "separately authorized deliberate shared-pointer activity timed out at stage waitDeliberatePointerActivity",
  );
}

async function runPreDispatchPointerArmStateMachine({
  baseline,
  armDeadlineMilliseconds,
  hardDeadlineMilliseconds,
  nowMilliseconds = () => Date.now(),
  promptAlive,
  probePrompt,
  transitionPrompt,
  waitForPrompt,
  setActionDeadlineMilliseconds = () => {},
  pause = delay,
  report = log,
}) {
  if (
    !Number.isSafeInteger(armDeadlineMilliseconds) ||
    !Number.isSafeInteger(hardDeadlineMilliseconds) ||
    hardDeadlineMilliseconds < armDeadlineMilliseconds
  ) {
    throw new Error("the deliberate pointer handoff has no shared absolute deadline");
  }
  if (
    typeof promptAlive !== "function" ||
    typeof probePrompt !== "function" ||
    typeof transitionPrompt !== "function" ||
    typeof waitForPrompt !== "function"
  ) {
    throw new Error("the deliberate pointer handoff state machine is incomplete");
  }

  let previous = baseline;
  let consecutive = 0;
  let firstAdvanceAt = 0;

  const resetCleanMotionEpoch = (sample, message) => {
    report(message);
    previous = sample;
    consecutive = 0;
    firstAdvanceAt = 0;
  };
  const returnActionToMove = async (message) => {
    report(message);
    setActionDeadlineMilliseconds(null);
    await transitionPrompt(POINTER_HANDOFF_MOVE_STATE);
    if (nowMilliseconds() >= armDeadlineMilliseconds) {
      throw deliberatePointerTimeoutError();
    }
    previous = await waitForPrompt(
      POINTER_HANDOFF_MOVE_STATE,
      armDeadlineMilliseconds,
    );
    if (!pointerPromptDeliveryObserved(previous)) {
      throw new Error("the independently probed pointer handoff panel did not return to MOVE while re-arming");
    }
    consecutive = 0;
    firstAdvanceAt = 0;
    await pause(DELIBERATE_MOTION_SAMPLE_MS);
  };

  while (nowMilliseconds() < armDeadlineMilliseconds) {
    if (!promptAlive()) {
      throw new Error("the pointer handoff prompt exited while operator motion was required");
    }
    const armProbeBudgetMs = armDeadlineMilliseconds - nowMilliseconds();
    if (!Number.isSafeInteger(armProbeBudgetMs) || armProbeBudgetMs < 1) break;
    let sample;
    try {
      sample = await probePrompt(
        POINTER_HANDOFF_MOVE_STATE,
        Math.min(SYSTEM_PROBE_TIMEOUT_MS, armProbeBudgetMs),
      );
    } catch (error) {
      if (armProbeExpired(error, armDeadlineMilliseconds, nowMilliseconds())) break;
      throw error;
    }
    if (nowMilliseconds() >= armDeadlineMilliseconds) break;
    if (!pointerPromptDeliveryObserved(sample)) {
      throw new Error("the independently probed pointer handoff panel was not visible and nonactivating while arming");
    }

    const progress = clickFreePointerMotionProgress(previous, sample);
    const moveInvariants = systemInvariants(previous, sample);
    const moveDisposition = preDispatchPointerTransitionDisposition(
      progress,
      moveInvariants,
    );
    if (moveDisposition === "unknown") {
      throw new Error("the independent HID pointer monitor became unreadable while arming");
    }
    if (moveDisposition === "rearm") {
      resetCleanMotionEpoch(
        sample,
        "Pre-dispatch input or user-context activity reset the MOVE clean-motion arm; no product action was sent.",
      );
      await pause(DELIBERATE_MOTION_SAMPLE_MS);
      continue;
    }

    const observedAt = nowMilliseconds();
    if (progress === "advanced") {
      if (consecutive === 0) firstAdvanceAt = observedAt;
      consecutive += 1;
      const span = observedAt - firstAdvanceAt;
      if (
        consecutive >= DELIBERATE_MOTION_REQUIRED_SAMPLES &&
        span >= DELIBERATE_MOTION_MINIMUM_SPAN_MS
      ) {
        if (nowMilliseconds() >= armDeadlineMilliseconds) break;
        const actionDeadlineMilliseconds = Math.min(
          hardDeadlineMilliseconds,
          nowMilliseconds() + POINTER_HANDOFF_COMPLETION_GRACE_MS,
        );
        setActionDeadlineMilliseconds(actionDeadlineMilliseconds);
        report("Sustained shared-pointer activity observed; keep moving while the action runs.");
        await transitionPrompt(POINTER_HANDOFF_ACTION_STATE);
        const actionPromptSample = await waitForPrompt(
          POINTER_HANDOFF_ACTION_STATE,
          actionDeadlineMilliseconds,
        );
        if (!pointerPromptDeliveryObserved(actionPromptSample)) {
          throw new Error("the independently probed pointer handoff panel was not visible and nonactivating during the ACTION transition");
        }
        const actionTransitionProgress = clickFreePointerMotionProgress(
          sample,
          actionPromptSample,
        );
        const actionTransitionInvariants = systemInvariants(sample, actionPromptSample);
        const actionTransitionDisposition = preDispatchPointerTransitionDisposition(
          actionTransitionProgress,
          actionTransitionInvariants,
        );
        if (actionTransitionDisposition === "unknown") {
          throw new Error("the independent HID pointer monitor became unreadable during the ACTION transition");
        }
        if (actionTransitionDisposition === "rearm") {
          await returnActionToMove(
            "Pre-dispatch input or user-context activity reset the ACTION transition; no product action was sent.",
          );
          continue;
        }

        const finalProbeBudgetMs = actionDeadlineMilliseconds - nowMilliseconds();
        if (!Number.isSafeInteger(finalProbeBudgetMs) || finalProbeBudgetMs < 1) {
          throw new Error("the pointer handoff pre-dispatch deadline elapsed");
        }
        const dispatchBaseline = await probePrompt(
          POINTER_HANDOFF_ACTION_STATE,
          Math.min(SYSTEM_PROBE_TIMEOUT_MS, finalProbeBudgetMs),
        );
        if (!pointerPromptDeliveryObserved(dispatchBaseline)) {
          throw new Error("the pointer handoff panel was not independently visible immediately before dispatch");
        }
        const dispatchProgress = clickFreePointerMotionProgress(
          actionPromptSample,
          dispatchBaseline,
        );
        const dispatchInvariants = systemInvariants(
          actionPromptSample,
          dispatchBaseline,
        );
        const dispatchDisposition = preDispatchPointerTransitionDisposition(
          dispatchProgress,
          dispatchInvariants,
        );
        if (dispatchDisposition === "unknown") {
          throw new Error("the independent HID pointer monitor became unreadable before dispatch");
        }
        if (dispatchDisposition === "rearm") {
          await returnActionToMove(
            "Pre-dispatch input or user-context activity reset the final ACTION boundary; no product action was sent.",
          );
          continue;
        }
        return {
          actionDeadlineMilliseconds,
          actionTransitionInvariants,
          dispatchBaseline,
          dispatchInvariants,
          sustainedMotionSamples: consecutive,
          sustainedMotionSpanMilliseconds: span,
        };
      }
    } else {
      consecutive = 0;
      firstAdvanceAt = 0;
    }
    previous = sample;
    await pause(DELIBERATE_MOTION_SAMPLE_MS);
  }
  throw deliberatePointerTimeoutError();
}

function rigSelfTestPointerSample({
  mouseMoved = 0,
  leftMouseDown = 0,
  keyDown = 0,
  identity = 1,
} = {}) {
  const counters = Object.fromEntries(HID_POINTER_COUNTER_FIELDS.map((field) => [field, 0]));
  const keyboardCounters = Object.fromEntries(
    HID_KEYBOARD_COUNTER_FIELDS.map((field) => [field, 0]),
  );
  counters.mouseMoved = mouseMoved;
  counters.leftMouseDown = leftMouseDown;
  keyboardCounters.keyDown = keyDown;
  const foregroundPID = 500 + identity;
  const frontWindowID = 700 + identity;
  return {
    pointerActivityMonitorHealthy: true,
    hidPointerCounters: counters,
    hidKeyboardCounters: keyboardCounters,
    pointerBoundaryActivityObserved: false,
    keyboardBoundaryActivityObserved: false,
    cursorX: 100,
    cursorY: 100,
    foregroundPID,
    foregroundProbeHealthy: true,
    foregroundTransitionObserved: false,
    foregroundIdentityStable: true,
    rawForegroundIdentityStable: true,
    rawForegroundPID: foregroundPID,
    rawForegroundPSN: identity.toString(16).padStart(16, "0"),
    frontWindowID,
    foregroundAXFocusedWindowID: frontWindowID,
    foregroundAXMainWindowID: frontWindowID,
    foregroundAXFrontmost: true,
    foregroundAXProbeHealthy: true,
    activeSpace: 1,
    activeSpaceProbeHealthy: true,
    appSharePromptProbeRequested: true,
    appSharePromptBundleMatched: true,
    appSharePromptOwnerMatched: true,
    appSharePromptTitleMatched: true,
    appSharePromptButtonMatched: true,
    appSharePromptButtonEnabledMatched: true,
    appSharePromptOnScreen: true,
    appSharePromptNonactivating: true,
  };
}

function rigSelfTestAssert(condition, message) {
  if (!condition) throw new Error(message);
}

async function runQuietSeatExecutionSelfTests() {
  {
    const clock = { value: 0 };
    const reports = [];
    const result = await runNativeQuietSeatStabilization({
      baseline: rigSelfTestPointerSample(),
      waitDeadlineMilliseconds: 10_000,
      requiredStableMilliseconds: 1_500,
      sampleIntervalMilliseconds: 500,
      requiredStableTransitions: 3,
      nowMilliseconds: () => clock.value,
      probe: async () => rigSelfTestPointerSample(),
      pause: async (milliseconds) => {
        clock.value += milliseconds;
      },
      report: (message) => reports.push(message),
    });
    rigSelfTestAssert(
      result.summary.completed === true &&
        result.summary.completedBeforeCandidateExecution === true &&
        result.summary.stableDurationMilliseconds === 1_500 &&
        result.summary.stableTransitions === 4 &&
        result.summary.resetCount === 0 &&
        result.summary.monitoringUnknown === false &&
        reports.length === 0,
      "a continuous readable quiet-seat epoch did not complete exactly once",
    );
  }

  {
    const clock = { value: 0 };
    const samples = [
      rigSelfTestPointerSample({ mouseMoved: 1 }),
      rigSelfTestPointerSample({ mouseMoved: 1 }),
      rigSelfTestPointerSample({ mouseMoved: 1 }),
    ];
    const reports = [];
    const result = await runNativeQuietSeatStabilization({
      baseline: rigSelfTestPointerSample(),
      waitDeadlineMilliseconds: 10_000,
      requiredStableMilliseconds: 1_000,
      sampleIntervalMilliseconds: 500,
      requiredStableTransitions: 2,
      nowMilliseconds: () => clock.value,
      probe: async () => {
        const sample = samples.shift();
        rigSelfTestAssert(sample != null, "quiet-seat reset test exhausted samples");
        return sample;
      },
      pause: async (milliseconds) => {
        clock.value += milliseconds;
      },
      report: (message) => reports.push(message),
    });
    rigSelfTestAssert(
      samples.length === 0 &&
        result.summary.resetCount === 1 &&
        result.summary.stableDurationMilliseconds === 1_000 &&
        result.summary.stableTransitions === 2 &&
        reports.some((message) => message.includes("reset the native quiet-seat epoch")),
      "pointer contamination did not reset the complete quiet-seat epoch",
    );
  }

  {
    let failure = null;
    try {
      await runNativeQuietSeatStabilization({
        baseline: rigSelfTestPointerSample(),
        waitDeadlineMilliseconds: 10_000,
        requiredStableMilliseconds: 1_000,
        sampleIntervalMilliseconds: 500,
        requiredStableTransitions: 2,
        nowMilliseconds: () => 0,
        probe: async () => ({
          ...rigSelfTestPointerSample(),
          pointerActivityMonitorHealthy: false,
        }),
        pause: async () => {},
        report: () => {},
      });
    } catch (error) {
      failure = error;
    }
    rigSelfTestAssert(
      failure?.message === quietSeatUnknownError().message &&
        failure?.quietSeatStabilization?.monitoringUnknown === true &&
        failure?.quietSeatStabilization?.completed === false,
      "unknown quiet-seat monitoring did not fail closed immediately",
    );
  }

  {
    const clock = { value: 0 };
    let movement = 0;
    let failure = null;
    try {
      await runNativeQuietSeatStabilization({
        baseline: rigSelfTestPointerSample(),
        waitDeadlineMilliseconds: 1_000,
        requiredStableMilliseconds: 5_000,
        sampleIntervalMilliseconds: 500,
        requiredStableTransitions: 10,
        nowMilliseconds: () => clock.value,
        probe: async () => rigSelfTestPointerSample({ mouseMoved: ++movement }),
        pause: async (milliseconds) => {
          clock.value += milliseconds;
        },
        report: () => {},
      });
    } catch (error) {
      failure = error;
    }
    rigSelfTestAssert(
      clock.value === 1_000 &&
        failure?.message === quietSeatTimeoutError().message &&
        failure?.quietSeatStabilization?.resetCount === 2 &&
        failure?.quietSeatStabilization?.completed === false,
      "quiet-seat contamination extended or bypassed the immutable wait deadline",
    );
  }
  console.log(
    "macOS quiet-seat execution regressions passed: stable completion, contamination reset, unknown refusal, immutable timeout.",
  );
}

async function runPointerArmExecutionSelfTests() {
  {
    const clock = { value: 9_990 };
    const timeout = Object.assign(new Error("bounded timeout"), { code: "SUBPROCESS_TIMEOUT" });
    let probeCalls = 0;
    let failure = null;
    try {
      await runPreDispatchPointerArmStateMachine({
        baseline: rigSelfTestPointerSample(),
        armDeadlineMilliseconds: 10_000,
        hardDeadlineMilliseconds: 20_000,
        nowMilliseconds: () => clock.value,
        promptAlive: () => true,
        probePrompt: (_state, timeoutMilliseconds) => {
          rigSelfTestAssert(timeoutMilliseconds === 10, "deadline test did not use the remaining absolute budget");
          probeCalls += 1;
          clock.value = 10_000;
          throw timeout;
        },
        transitionPrompt: async () => {
          throw new Error("deadline test transitioned the prompt");
        },
        waitForPrompt: async () => {
          throw new Error("deadline test waited for a prompt transition");
        },
        pause: async () => {},
        report: () => {},
      });
    } catch (error) {
      failure = error;
    }
    rigSelfTestAssert(
      probeCalls === 1 && failure?.message === deliberatePointerTimeoutError().message,
      "deadline-expired process-probe execution did not become canonical arm expiry",
    );
  }

  {
    const clock = { value: 1_000 };
    const armDeadlineMilliseconds = 10_000;
    const moveSamples = [
      rigSelfTestPointerSample({ mouseMoved: 1, identity: 2 }),
      rigSelfTestPointerSample({ mouseMoved: 2, identity: 2 }),
      rigSelfTestPointerSample({ mouseMoved: 3, identity: 2 }),
      rigSelfTestPointerSample({ mouseMoved: 4, identity: 2 }),
    ];
    const actionProbeSamples = [rigSelfTestPointerSample({ mouseMoved: 6, identity: 2 })];
    const transitions = [];
    const events = [];
    const result = await runPreDispatchPointerArmStateMachine({
      baseline: rigSelfTestPointerSample({ identity: 1 }),
      armDeadlineMilliseconds,
      hardDeadlineMilliseconds: 20_000,
      nowMilliseconds: () => clock.value,
      promptAlive: () => true,
      probePrompt: (state) => {
        const sample = state === POINTER_HANDOFF_MOVE_STATE
          ? moveSamples.shift()
          : actionProbeSamples.shift();
        rigSelfTestAssert(sample != null, `MOVE contamination test exhausted ${state} samples`);
        return sample;
      },
      transitionPrompt: async (state) => transitions.push(state),
      waitForPrompt: async (state, deadlineMilliseconds) => {
        rigSelfTestAssert(
          state === POINTER_HANDOFF_ACTION_STATE && deadlineMilliseconds > armDeadlineMilliseconds,
          "MOVE contamination test lost its bounded ACTION transition",
        );
        return rigSelfTestPointerSample({ mouseMoved: 5, identity: 2 });
      },
      pause: async (milliseconds) => {
        clock.value += milliseconds;
      },
      report: (message) => events.push(message),
    });
    rigSelfTestAssert(
      moveSamples.length === 0 &&
        actionProbeSamples.length === 0 &&
        transitions.join(",") === POINTER_HANDOFF_ACTION_STATE &&
        result.sustainedMotionSamples === 3 &&
        result.sustainedMotionSpanMilliseconds === 500 &&
        events.some((message) => message.includes("reset the MOVE clean-motion arm")),
      "foreground-contaminated MOVE samples were absorbed into the clean arm",
    );
  }

  {
    const clock = { value: 2_000 };
    const armDeadlineMilliseconds = 20_000;
    const moveSamples = [1, 2, 3, 6, 7, 8].map((mouseMoved) =>
      rigSelfTestPointerSample({ mouseMoved, leftMouseDown: mouseMoved >= 6 ? 1 : 0 })
    );
    const actionWaitSamples = [
      rigSelfTestPointerSample({ mouseMoved: 4 }),
      rigSelfTestPointerSample({ mouseMoved: 9, leftMouseDown: 1 }),
    ];
    const actionProbeSamples = [
      rigSelfTestPointerSample({ mouseMoved: 5, leftMouseDown: 1 }),
      rigSelfTestPointerSample({ mouseMoved: 10, leftMouseDown: 1 }),
    ];
    const moveWaitSamples = [rigSelfTestPointerSample({ mouseMoved: 5, leftMouseDown: 1 })];
    const transitions = [];
    const actionDeadlines = [];
    const observedArmDeadlines = [];
    const result = await runPreDispatchPointerArmStateMachine({
      baseline: rigSelfTestPointerSample(),
      armDeadlineMilliseconds,
      hardDeadlineMilliseconds: 30_000,
      nowMilliseconds: () => clock.value,
      promptAlive: () => true,
      probePrompt: (state) => {
        const sample = state === POINTER_HANDOFF_MOVE_STATE
          ? moveSamples.shift()
          : actionProbeSamples.shift();
        rigSelfTestAssert(sample != null, `ACTION contamination test exhausted ${state} samples`);
        return sample;
      },
      transitionPrompt: async (state) => transitions.push(state),
      waitForPrompt: async (state, deadlineMilliseconds) => {
        if (state === POINTER_HANDOFF_MOVE_STATE) {
          observedArmDeadlines.push(deadlineMilliseconds);
          return moveWaitSamples.shift();
        }
        return actionWaitSamples.shift();
      },
      setActionDeadlineMilliseconds: (deadlineMilliseconds) => {
        actionDeadlines.push(deadlineMilliseconds);
      },
      pause: async (milliseconds) => {
        clock.value += milliseconds;
      },
      report: () => {},
    });
    rigSelfTestAssert(
      moveSamples.length === 0 &&
        actionWaitSamples.length === 0 &&
        actionProbeSamples.length === 0 &&
        moveWaitSamples.length === 0 &&
        transitions.join(",") === [
          POINTER_HANDOFF_ACTION_STATE,
          POINTER_HANDOFF_MOVE_STATE,
          POINTER_HANDOFF_ACTION_STATE,
        ].join(",") &&
        actionDeadlines.length === 3 &&
        actionDeadlines[1] === null &&
        observedArmDeadlines.length === 1 &&
        observedArmDeadlines[0] === armDeadlineMilliseconds &&
        result.sustainedMotionSamples === 3 &&
        result.sustainedMotionSpanMilliseconds === 500,
      "final ACTION contamination did not return through MOVE to a fresh absolute-deadline arm",
    );
  }
  console.log(
    "macOS pointer-arm execution regressions passed: deadline expiry, MOVE re-arm, final ACTION re-arm.",
  );
}

async function runAppShareFilesystemSelfTests() {
  const root = await mkdtemp(join(tmpdir(), "lbb-app-share-fs-self-test-"));
  try {
    const identityState = {
      dev: 17n,
      ino: 23n,
      size: 41n,
      mode: 0o100600n,
      uid: 501n,
      gid: 20n,
      nlink: 1n,
      mtimeNs: 101n,
      ctimeNs: 102n,
      birthtimeNs: 100n,
    };
    const windowsIdentityState = {
      ...identityState,
      mode: 0o100666n,
      uid: 0n,
      gid: 0n,
    };
    const windowsWriterClosedState = {
      ...windowsIdentityState,
      mtimeNs: 201n,
      ctimeNs: 202n,
    };
    rigSelfTestAssert(
      sameOrdinaryFileIdentity(identityState, { ...identityState }) &&
        sameCoreFileIdentity(identityState, { ...identityState, ctimeNs: 202n, nlink: 2n }) &&
        samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsIdentityState, mtimeNs: 201n },
          "win32",
        ) &&
        samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsIdentityState, ctimeNs: 202n },
          "win32",
        ) &&
        samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          windowsWriterClosedState,
          "win32",
        ) &&
        sameOrdinaryFileIdentity(
          windowsWriterClosedState,
          { ...windowsWriterClosedState },
        ) &&
        hasPlatformPrivateMarkerMetadata(identityState, "darwin", 501n) &&
        hasPlatformPrivateMarkerMetadata(windowsIdentityState, "win32", null),
      "cross-platform exact marker metadata was rejected",
    );
    rigSelfTestAssert(
      !sameOrdinaryFileIdentity(identityState, { ...identityState, ino: 24n }) &&
        !sameOrdinaryFileIdentity(identityState, { ...identityState, ctimeNs: 103n }) &&
        !sameOrdinaryFileIdentity(identityState, { ...identityState, nlink: 2n }) &&
        !sameOrdinaryFileIdentity(identityState, { ...identityState, ino: Number(23n) }) &&
        !sameCoreFileIdentity(identityState, { ...identityState, mtimeNs: 201n }) &&
        !sameOrdinaryFileIdentity(
          windowsWriterClosedState,
          { ...windowsWriterClosedState, mtimeNs: 203n },
        ) &&
        !sameOrdinaryFileIdentity(
          windowsWriterClosedState,
          { ...windowsWriterClosedState, ctimeNs: 203n },
        ) &&
        !samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsWriterClosedState, ino: 24n },
          "win32",
        ) &&
        !samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsWriterClosedState, dev: 18n },
          "win32",
        ) &&
        !samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsWriterClosedState, size: 42n },
          "win32",
        ) &&
        !samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsWriterClosedState, mode: 0o100444n },
          "win32",
        ) &&
        !samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsWriterClosedState, uid: 1n },
          "win32",
        ) &&
        !samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsWriterClosedState, gid: 1n },
          "win32",
        ) &&
        !samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsWriterClosedState, nlink: 2n },
          "win32",
        ) &&
        !samePublishedFileAcrossWriterClose(
          windowsIdentityState,
          { ...windowsWriterClosedState, birthtimeNs: 99n },
          "win32",
        ) &&
        !samePublishedFileAcrossWriterClose(
          identityState,
          { ...identityState, mtimeNs: 201n },
          "darwin",
        ) &&
        !samePublishedFileAcrossWriterClose(
          identityState,
          { ...identityState, ctimeNs: 202n },
          "darwin",
        ) &&
        !samePublishedFileAcrossWriterClose(
          identityState,
          { ...identityState, mtimeNs: 201n, ctimeNs: 202n },
          "darwin",
        ) &&
        !hasPlatformPrivateMarkerMetadata(
          { ...windowsIdentityState, mode: 0o100600n },
          "win32",
          null,
        ) &&
        !hasPlatformPrivateMarkerMetadata(
          { ...identityState, mode: 0o100666n },
          "darwin",
          501n,
        ) &&
        !hasPlatformPrivateMarkerMetadata(identityState, "darwin", 502n),
      "cross-platform marker identity or POSIX privacy policy accepted unstable metadata",
    );
    rigSelfTestAssert(
      await syncMarkerDirectory(root, Date.now() + MARKER_PUBLISH_TIMEOUT_MS, "win32") === false,
      "Windows directory-sync policy made an unsupported durability claim",
    );

    const markerPath = join(root, "marker.json");
    await publishAtomicMarkerOnce(markerPath, { alpha: true });
    const marker = await readBoundAppShareReceipt(
      markerPath,
      ["alpha"],
      (record) => record.alpha === true,
      "self-test app-share receipt",
    );
    rigSelfTestAssert(marker.record.alpha === true, "stable receipt reader changed canonical data");

    let duplicateRejected = false;
    try {
      await publishAtomicMarkerOnce(markerPath, { alpha: false });
    } catch {
      duplicateRejected = true;
    }
    rigSelfTestAssert(duplicateRejected, "create-once marker publication accepted a duplicate");

    const symlinkPath = join(root, "symlink.json");
    try {
      await symlink(markerPath, symlinkPath);
      let symlinkRejected = false;
      try {
        await readBoundAppShareReceipt(
          symlinkPath,
          ["alpha"],
          () => true,
          "self-test symlink receipt",
        );
      } catch {
        symlinkRejected = true;
      }
      rigSelfTestAssert(symlinkRejected, "receipt reader followed a symlink");
    } catch (error) {
      if (
        process.platform !== "win32" ||
        (error?.code !== "EPERM" && error?.code !== "EACCES")
      ) {
        throw error;
      }
    }

    const hardlinkPath = join(root, "hardlink.json");
    await link(markerPath, hardlinkPath);
    let hardlinkRejected = false;
    try {
      await readBoundAppShareReceipt(
        markerPath,
        ["alpha"],
        () => true,
        "self-test multiply linked receipt",
      );
    } catch {
      hardlinkRejected = true;
    }
    rigSelfTestAssert(hardlinkRejected, "receipt reader accepted a multiply linked file");
    await unlink(hardlinkPath);

    if (process.platform !== "win32") {
      const fifoPath = join(root, "fifo.json");
      const fifo = spawnSync("/usr/bin/mkfifo", [fifoPath], {
        encoding: "utf8",
        timeout: 2_000,
        windowsHide: true,
      });
      rigSelfTestAssert(fifo.status === 0, "receipt FIFO self-test setup failed");
      const startedAt = Date.now();
      let fifoRejected = false;
      try {
        await readBoundAppShareReceipt(
          fifoPath,
          ["alpha"],
          () => true,
          "self-test FIFO receipt",
        );
      } catch {
        fifoRejected = true;
      }
      rigSelfTestAssert(
        fifoRejected && Date.now() - startedAt < 1_000,
        "receipt reader did not reject a FIFO without blocking",
      );
    }

    let expiredDeadlineRejected = false;
    try {
      await deadlineCheckedFilesystemStep(
        Date.now() - 1,
        "self-test expired operation",
        async () => {},
      );
    } catch (error) {
      expiredDeadlineRejected = error instanceof Error && error.message.includes("deadline");
    }
    rigSelfTestAssert(expiredDeadlineRejected, "expired filesystem deadline was not rejected");
    console.log(
      "Cross-platform marker identity regressions passed: exact BigInt binding, Windows metadata projection, POSIX private mode.",
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

async function runRigSelfTest() {
  rigSelfTestAssert(
    runExactLine(
      process.execPath,
      ["-e", "process.stdout.write('self-test-token\\n')"],
      "self-test-token",
    ),
    "exact one-line subprocess output was refused",
  );
  rigSelfTestAssert(
    !runExactLine(
      process.execPath,
      ["-e", "process.stdout.write('diagnostic\\nself-test-token\\n')"],
      "self-test-token",
    ),
    "extra subprocess stdout was accepted",
  );
  rigSelfTestAssert(
    !runExactLine(
      process.execPath,
      ["-e", "process.stdout.write('self-test-token')"],
      "self-test-token",
    ),
    "subprocess stdout without its exact trailing LF was accepted",
  );
  rigSelfTestAssert(
    !runExactLine(
      process.execPath,
      ["-e", "process.stdout.write('self-test-token\\n'); process.stderr.write('diagnostic\\n')"],
      "self-test-token",
    ),
    "subprocess stderr was accepted",
  );
  let nonzeroSubprocessRejected = false;
  try {
    runExactLine(process.execPath, ["-e", "process.exit(7)"], "self-test-token");
  } catch {
    nonzeroSubprocessRejected = true;
  }
  rigSelfTestAssert(nonzeroSubprocessRejected, "nonzero subprocess exit was accepted");
  let requestPublicationDeadlineRejected = false;
  try {
    remainingRequestPublicationTime(10_000, "self-test", 10_000);
  } catch (error) {
    requestPublicationDeadlineRejected =
      error instanceof Error && error.message.includes("deadline elapsed");
  }
  if (
    REQUEST_PUBLICATION_MAXIMUM_WAIT_MS !== 60 * 60_000 ||
    remainingRequestPublicationTime(10_000, "self-test", 9_999) !== 1 ||
    !requestPublicationDeadlineRejected
  ) {
    throw new Error("request-publication absolute deadline self-test failed");
  }
  const counters = Object.fromEntries(HID_POINTER_COUNTER_FIELDS.map((field) => [field, 100]));
  const keyboardCounters = Object.fromEntries(
    HID_KEYBOARD_COUNTER_FIELDS.map((field) => [field, 100]),
  );
  const sample = {
    pointerActivityMonitorHealthy: true,
    hidPointerCounters: counters,
    hidKeyboardCounters: keyboardCounters,
  };
  const changed = (field, value = 101) => ({
    pointerActivityMonitorHealthy: true,
    hidPointerCounters: { ...counters, [field]: value },
  });
  if (
    clickFreePointerMotionProgress(sample, sample) !== "stable" ||
    clickFreePointerMotionProgress(sample, changed("mouseMoved")) !== "advanced"
  ) {
    throw new Error("click-free mouse-movement self-test failed");
  }
  for (const field of HID_POINTER_COUNTER_FIELDS.filter((field) => field !== "mouseMoved")) {
    if (clickFreePointerMotionProgress(sample, changed(field)) !== "disallowed") {
      throw new Error("non-motion HID pointer activity was not rejected");
    }
  }
  if (
    clickFreePointerMotionProgress(
      sample,
      { ...changed("mouseMoved"), pointerActivityMonitorHealthy: false },
    ) !== "unknown" ||
    clickFreePointerMotionProgress(sample, changed("mouseMoved", 1_000_101)) !== "unknown"
  ) {
    throw new Error("unreadable HID pointer activity was not rejected");
  }
  const preservedContext = {
    foregroundUnchanged: true,
    userFocusUnchanged: true,
    spaceUnchanged: true,
  };
  if (
    preDispatchPointerTransitionDisposition("advanced", preservedContext) !== "accept" ||
    preDispatchPointerTransitionDisposition("stable", preservedContext) !== "accept" ||
    preDispatchPointerTransitionDisposition("disallowed", preservedContext) !== "rearm" ||
    preDispatchPointerTransitionDisposition("advanced", {
      ...preservedContext,
      foregroundUnchanged: false,
    }) !== "rearm" ||
    preDispatchPointerTransitionDisposition("advanced", {
      ...preservedContext,
      userFocusUnchanged: false,
    }) !== "rearm" ||
    preDispatchPointerTransitionDisposition("advanced", {
      ...preservedContext,
      spaceUnchanged: false,
    }) !== "rearm" ||
    preDispatchPointerTransitionDisposition("unknown", preservedContext) !== "unknown"
  ) {
    throw new Error("pre-dispatch pointer re-arm self-test failed");
  }
  const timeout = Object.assign(new Error("bounded timeout"), { code: "SUBPROCESS_TIMEOUT" });
  if (
    armProbeExpired(timeout, 10_000, 9_999) ||
    !armProbeExpired(timeout, 10_000, 10_000) ||
    armProbeExpired(new Error("different failure"), 10_000, 10_001)
  ) {
    throw new Error("pointer-arm deadline classification self-test failed");
  }
  const quietBaseline = rigSelfTestPointerSample();
  const focusChanged = {
    ...quietBaseline,
    frontWindowID: quietBaseline.frontWindowID + 1,
    foregroundAXFocusedWindowID: quietBaseline.foregroundAXFocusedWindowID + 1,
    foregroundAXMainWindowID: quietBaseline.foregroundAXMainWindowID + 1,
  };
  if (
    quietSeatTransitionDisposition(quietBaseline, quietBaseline) !== "stable" ||
    quietSeatTransitionDisposition(
      quietBaseline,
      rigSelfTestPointerSample({ mouseMoved: 1 }),
    ) !== "reset" ||
    quietSeatTransitionDisposition(
      quietBaseline,
      rigSelfTestPointerSample({ keyDown: 1 }),
    ) !== "reset" ||
    quietSeatTransitionDisposition(
      quietBaseline,
      rigSelfTestPointerSample({ identity: 2 }),
    ) !== "reset" ||
    quietSeatTransitionDisposition(quietBaseline, focusChanged) !== "reset" ||
    quietSeatTransitionDisposition(
      quietBaseline,
      { ...quietBaseline, activeSpace: 2 },
    ) !== "reset" ||
    quietSeatTransitionDisposition(
      quietBaseline,
      { ...quietBaseline, pointerActivityMonitorHealthy: false },
    ) !== "unknown"
  ) {
    throw new Error("native quiet-seat pointer/context classification self-test failed");
  }
  const quietActionPointerInvariants = {
    cursorPositionUnchanged: true,
    sharedPointerActivityObserved: false,
    hidSystemPointerActivityObserved: false,
    rawInputPointerActivityObserved: false,
    injectedPointerActivityObserved: false,
    pointerActivityMonitorHealthy: true,
    sharedPointerBoundaryCorroborated: true,
    sharedPointerBoundaryState: "corroborated",
    sharedPointerActivityState: "quiet",
  };
  const quietIndependentPointerInvariants = {
    ...quietActionPointerInvariants,
    hidSystemKeyboardActivityObserved: false,
    keyboardActivityMonitorHealthy: true,
    sharedKeyboardBoundaryCorroborated: true,
    sharedKeyboardBoundaryState: "corroborated",
    sharedKeyboardActivityState: "quiet",
    sharedInputSeatActivityObserved: false,
  };
  const contaminatedActionPointerInvariants = {
    ...quietActionPointerInvariants,
    cursorPositionUnchanged: false,
    sharedPointerActivityObserved: true,
    hidSystemPointerActivityObserved: true,
    sharedPointerActivityState: "contaminated",
  };
  const malformedActionPointerInvariants = { ...quietActionPointerInvariants };
  delete malformedActionPointerInvariants.rawInputPointerActivityObserved;
  if (
    actionPointerLaneState(quietActionPointerInvariants) !== "quiet" ||
    independentPointerLaneState(quietActionPointerInvariants) !== "unknown" ||
    independentPointerLaneState(quietIndependentPointerInvariants) !== "quiet" ||
    actionPointerLaneState(contaminatedActionPointerInvariants) !==
      "concurrent-shared-seat-activity" ||
    actionPointerLaneState(malformedActionPointerInvariants) !== "unknown" ||
    actionPointerLaneState({
      ...quietActionPointerInvariants,
      rawInputPointerActivityObserved: "false",
    }) !== "unknown" ||
    actionPointerLaneState({
      ...quietActionPointerInvariants,
      pointerActivityMonitorHealthy: false,
    }) !== "unknown" ||
    actionPointerLaneState({
      ...quietActionPointerInvariants,
      sharedPointerBoundaryCorroborated: false,
      sharedPointerBoundaryState: "unknown",
    }) !== "unknown" ||
    actionPointerLaneState({
      ...quietActionPointerInvariants,
      sharedPointerActivityState: "unknown",
    }) !== "unknown" ||
    actionPointerLaneState({
      ...quietActionPointerInvariants,
      rawInputPointerActivityObserved: true,
    }) !== "unknown" ||
    actionPointerLaneState({
      ...quietActionPointerInvariants,
      sharedPointerActivityState: "contaminated",
    }) !== "unknown" ||
    independentPointerLaneState({
      ...quietIndependentPointerInvariants,
      hidSystemKeyboardActivityObserved: true,
      sharedKeyboardActivityState: "contaminated",
      sharedInputSeatActivityObserved: true,
    }) !== "concurrent-shared-seat-activity"
  ) {
    throw new Error("action and independent pointer classifier separation self-test failed");
  }
  const shareAuthorityBaseline = shareActionAuthority({
    frameId: "frame-1",
    capturedAt: "2026-08-24T00:00:00.000Z",
    captureAgeMs: 25,
    shareId: "share-1",
    sourceSequence: 40,
    windowId: "window-1",
    pid: 101,
    windowTitle: FIXTURE_TITLE,
  }, {
    shareId: "share-1",
    sourceSequence: 40,
    sequence: 11,
    windowWidth: 820,
    windowHeight: 552,
    imageWidth: 1218,
    imageHeight: 820,
  });
  if (
    shareAuthorityBaseline?.capturedAt !== "2026-08-24T00:00:00.000Z" ||
    shareAuthorityBaseline?.captureAgeMs !== 25 ||
    shareAuthorityBaseline?.topLevelShareId !== "share-1" ||
    shareAuthorityBaseline?.topLevelSourceSequence !== 40
  ) {
    throw new Error("share action authority mapper self-test failed");
  }
  const shareAuthoritySuccessor = {
    ...shareAuthorityBaseline,
    frameId: "frame-2",
    capturedAt: "2026-08-24T00:00:00.500Z",
    captureAgeMs: 20,
    sourceSequence: 41,
    topLevelSourceSequence: 41,
    sequence: 12,
  };
  const successorPublishedMilliseconds = Date.parse(shareAuthoritySuccessor.capturedAt);
  const refreshDecisionCases = [
    ["fresh exact successor", shareAuthoritySuccessor, successorPublishedMilliseconds + 500, true],
    ["replayed authority", shareAuthorityBaseline, successorPublishedMilliseconds + 500, false],
    ["replayed frame", { ...shareAuthoritySuccessor, frameId: shareAuthorityBaseline.frameId }, successorPublishedMilliseconds + 500, false],
    ["cross-share nested identity", { ...shareAuthoritySuccessor, shareId: "share-2" }, successorPublishedMilliseconds + 500, false],
    ["cross-share top-level identity", { ...shareAuthoritySuccessor, topLevelShareId: "share-2" }, successorPublishedMilliseconds + 500, false],
    ["stale source", { ...shareAuthoritySuccessor, sourceSequence: shareAuthorityBaseline.sourceSequence }, successorPublishedMilliseconds + 500, false],
    ["cross-source projection", { ...shareAuthoritySuccessor, topLevelSourceSequence: shareAuthorityBaseline.sourceSequence }, successorPublishedMilliseconds + 500, false],
    ["stale transport", { ...shareAuthoritySuccessor, sequence: shareAuthorityBaseline.sequence }, successorPublishedMilliseconds + 500, false],
    ["cross-process target", { ...shareAuthoritySuccessor, pid: shareAuthorityBaseline.pid + 1 }, successorPublishedMilliseconds + 500, false],
    ["cross-window target", { ...shareAuthoritySuccessor, windowId: "window-2" }, successorPublishedMilliseconds + 500, false],
    ["retitled target", { ...shareAuthoritySuccessor, windowTitle: `${FIXTURE_TITLE} changed` }, successorPublishedMilliseconds + 500, false],
    ["window geometry race", { ...shareAuthoritySuccessor, windowWidth: shareAuthorityBaseline.windowWidth + 1 }, successorPublishedMilliseconds + 500, false],
    ["image geometry race", { ...shareAuthoritySuccessor, imageHeight: shareAuthorityBaseline.imageHeight + 1 }, successorPublishedMilliseconds + 500, false],
    ["1001ms-age publication", shareAuthoritySuccessor, successorPublishedMilliseconds + 981, false],
    ["future publication", shareAuthoritySuccessor, successorPublishedMilliseconds - 1, false],
    ["invalid timestamp", { ...shareAuthoritySuccessor, capturedAt: "not-an-instant" }, successorPublishedMilliseconds + 500, false],
    ["invalid native age", { ...shareAuthoritySuccessor, captureAgeMs: -1 }, successorPublishedMilliseconds + 500, false],
  ];
  for (const [name, candidate, nowMilliseconds, expectedAccepted] of refreshDecisionCases) {
    const decision = decideShareActionAuthorityRefresh(
      shareAuthorityBaseline,
      candidate,
      nowMilliseconds,
    );
    if (decision.accepted !== expectedAccepted) {
      throw new Error(`post-handoff share action authority refresh self-test failed: ${name}`);
    }
  }
  const dispatchDecisionCases = [
    ["fresh bound frame", shareAuthoritySuccessor, shareAuthoritySuccessor.frameId, successorPublishedMilliseconds + 500, true],
    ["replayed dispatch authority", shareAuthorityBaseline, shareAuthorityBaseline.frameId, successorPublishedMilliseconds + 500, false],
    ["wrong dispatch frame", shareAuthoritySuccessor, shareAuthorityBaseline.frameId, successorPublishedMilliseconds + 500, false],
    ["empty dispatch frame", shareAuthoritySuccessor, "", successorPublishedMilliseconds + 500, false],
    ["cross-share dispatch authority", { ...shareAuthoritySuccessor, shareId: "share-2" }, shareAuthoritySuccessor.frameId, successorPublishedMilliseconds + 500, false],
    ["cross-target dispatch authority", { ...shareAuthoritySuccessor, windowId: "window-2" }, shareAuthoritySuccessor.frameId, successorPublishedMilliseconds + 500, false],
    ["dispatch geometry race", { ...shareAuthoritySuccessor, imageWidth: shareAuthoritySuccessor.imageWidth + 1 }, shareAuthoritySuccessor.frameId, successorPublishedMilliseconds + 500, false],
    ["1001ms-age dispatch authority", shareAuthoritySuccessor, shareAuthoritySuccessor.frameId, successorPublishedMilliseconds + 981, false],
    ["invalid dispatch age", { ...shareAuthoritySuccessor, captureAgeMs: -1 }, shareAuthoritySuccessor.frameId, successorPublishedMilliseconds + 500, false],
  ];
  for (const [name, authority, expectedFrameId, nowMilliseconds, expectedAccepted] of dispatchDecisionCases) {
    const decision = decideShareActionAuthorityDispatch(
      shareAuthorityBaseline,
      authority,
      expectedFrameId,
      nowMilliseconds,
    );
    if (decision.accepted !== expectedAccepted) {
      throw new Error(`share action authority dispatch self-test failed: ${name}`);
    }
  }
  const exactAgeBoundaryDecision = decideShareActionAuthorityRefresh(
    shareAuthorityBaseline,
    shareAuthoritySuccessor,
    successorPublishedMilliseconds + 980,
  );
  if (
    exactAgeBoundaryDecision.accepted !== true ||
    exactAgeBoundaryDecision.estimatedAgeMs !==
      APP_SHARE_ACTION_FRAME_MAX_ESTIMATED_AGE_MS ||
    freshShareActionAuthorityTimeout(APP_SHARE_HANDOFF_COMPLETION_GRACE_MS) !==
      APP_SHARE_ACTION_AUTHORITY_REFRESH_MAXIMUM_WAIT_MS ||
    freshShareActionAuthorityTimeout(16_000) !==
      APP_SHARE_ACTION_AUTHORITY_REFRESH_MAXIMUM_WAIT_MS ||
    freshShareActionAuthorityTimeout(11_001) !== 1 ||
    freshShareActionAuthorityTimeout(11_000) !== null ||
    freshShareActionAuthorityTimeout(Number.NaN) !== null
  ) {
    throw new Error("post-handoff share action authority refresh self-test failed: boundary");
  }
  const refreshReserveProof = freshShareActionAuthorityTimeout(
    APP_SHARE_HANDOFF_COMPLETION_GRACE_MS,
  );
  if (
    refreshReserveProof === null ||
    refreshReserveProof + APP_SHARE_HANDOFF_MINIMUM_ACTION_BUDGET_MS +
      APP_SHARE_HANDOFF_COMPLETION_RESERVE_MS > APP_SHARE_HANDOFF_COMPLETION_GRACE_MS
  ) {
    throw new Error("post-handoff share action authority refresh self-test failed: reserves");
  }

  const virtualRefreshStartedMilliseconds = Date.parse("2026-08-24T00:00:00.000Z");
  let virtualRefreshElapsedMilliseconds = 0;
  const delayedSuccessor = {
    ...shareAuthoritySuccessor,
    capturedAt: new Date(virtualRefreshStartedMilliseconds + 2_500).toISOString(),
  };
  const delayedRefresh = await waitForFreshShareActionAuthority({
    reference: shareAuthorityBaseline,
    timeoutMs: APP_SHARE_ACTION_AUTHORITY_REFRESH_MAXIMUM_WAIT_MS,
    loadCandidate: async () => ({
      snapshot: {},
      sample: { sequence: virtualRefreshElapsedMilliseconds >= 2_500 ? 12 : 11 },
      authority: virtualRefreshElapsedMilliseconds >= 2_500
        ? delayedSuccessor
        : shareAuthorityBaseline,
    }),
    monotonicMilliseconds: () => virtualRefreshElapsedMilliseconds,
    wallClockMilliseconds: () =>
      virtualRefreshStartedMilliseconds + virtualRefreshElapsedMilliseconds,
    pause: async (milliseconds) => {
      virtualRefreshElapsedMilliseconds += milliseconds;
    },
    sampleIntervalMs: 100,
  });
  if (
    delayedRefresh.authority !== delayedSuccessor ||
    delayedRefresh.decision.accepted !== true ||
    virtualRefreshElapsedMilliseconds !== 2_500
  ) {
    throw new Error(
      "post-handoff share action authority refresh self-test failed: valid 2.5s successor",
    );
  }

  let refusalElapsedMilliseconds = 0;
  let refusedDispatchCount = 0;
  let refusalPoll = 0;
  const refusalCandidates = [
    () => ({
      ...shareAuthoritySuccessor,
      capturedAt: new Date(
        virtualRefreshStartedMilliseconds + refusalElapsedMilliseconds - 981,
      ).toISOString(),
    }),
    () => ({ ...shareAuthoritySuccessor, shareId: "share-2" }),
    () => ({ ...shareAuthoritySuccessor, windowId: "window-2" }),
    () => ({ ...shareAuthoritySuccessor, imageWidth: shareAuthoritySuccessor.imageWidth + 1 }),
    () => shareAuthorityBaseline,
  ];
  let refusalTimeout = null;
  try {
    await waitForFreshShareActionAuthority({
      reference: shareAuthorityBaseline,
      timeoutMs: 600,
      loadCandidate: async () => ({
        snapshot: {},
        sample: {},
        authority: refusalCandidates[refusalPoll++ % refusalCandidates.length](),
      }),
      monotonicMilliseconds: () => refusalElapsedMilliseconds,
      wallClockMilliseconds: () =>
        virtualRefreshStartedMilliseconds + refusalElapsedMilliseconds,
      pause: async (milliseconds) => {
        refusalElapsedMilliseconds += milliseconds;
      },
      sampleIntervalMs: 100,
    });
    refusedDispatchCount += 1;
  } catch (error) {
    refusalTimeout = error;
  }
  const refusalDiagnostics = refusalTimeout?.shareActionAuthorityRefresh;
  if (
    refusedDispatchCount !== 0 ||
    !refusalTimeout?.message?.startsWith("fresh share action authority timed out:") ||
    refusalDiagnostics?.rejectionCounts?.staleAge < 1 ||
    refusalDiagnostics?.rejectionCounts?.shareMismatch < 1 ||
    refusalDiagnostics?.rejectionCounts?.targetMismatch < 1 ||
    refusalDiagnostics?.rejectionCounts?.geometryMismatch < 1 ||
    refusalDiagnostics?.rejectionCounts?.replayOrNonAdvancing < 1 ||
    !Number.isFinite(refusalDiagnostics?.minimumObservedAgeMs)
  ) {
    throw new Error(
      "post-handoff share action authority refresh self-test failed: bounded refusal diagnostics",
    );
  }

  let stalledReadElapsedMilliseconds = 0;
  let stalledReadOutstanding = 0;
  let stalledReadAbortObserved = false;
  let stalledReadTimeout = null;
  try {
    await waitForFreshShareActionAuthority({
      reference: shareAuthorityBaseline,
      timeoutMs: 500,
      loadCandidate: async ({ signal }) => await new Promise((resolve, reject) => {
        stalledReadOutstanding += 1;
        signal.addEventListener("abort", () => {
          stalledReadAbortObserved = true;
          stalledReadOutstanding -= 1;
          reject(new Error("expected deterministic read abort"));
        }, { once: true });
      }),
      monotonicMilliseconds: () => stalledReadElapsedMilliseconds,
      wallClockMilliseconds: () =>
        virtualRefreshStartedMilliseconds + stalledReadElapsedMilliseconds,
      pause: async (milliseconds) => {
        stalledReadElapsedMilliseconds += milliseconds;
      },
      scheduleReadAbort: (controller, milliseconds) => {
        let armed = true;
        queueMicrotask(() => {
          if (!armed) return;
          stalledReadElapsedMilliseconds += milliseconds;
          controller.abort();
        });
        return () => {
          armed = false;
        };
      },
    });
  } catch (error) {
    stalledReadTimeout = error;
  }
  if (
    !stalledReadAbortObserved ||
    stalledReadOutstanding !== 0 ||
    stalledReadTimeout?.shareActionAuthorityRefresh?.readAborts !== 1 ||
    !stalledReadTimeout?.message?.startsWith("fresh share action authority timed out:")
  ) {
    throw new Error(
      "post-handoff share action authority refresh self-test failed: stalled read abort",
    );
  }

  let lateResponseElapsedMilliseconds = 0;
  let lateResponseDispatchCount = 0;
  let lateResponseTimeout = null;
  const lateFreshSuccessor = {
    ...shareAuthoritySuccessor,
    capturedAt: new Date(virtualRefreshStartedMilliseconds + 501).toISOString(),
  };
  if (!decideShareActionAuthorityRefresh(
    shareAuthorityBaseline,
    lateFreshSuccessor,
    virtualRefreshStartedMilliseconds + 501,
  ).accepted) {
    throw new Error(
      "post-handoff share action authority refresh self-test failed: late fixture was not fresh",
    );
  }
  try {
    await waitForFreshShareActionAuthority({
      reference: shareAuthorityBaseline,
      timeoutMs: 500,
      loadCandidate: async () => {
        lateResponseElapsedMilliseconds = 501;
        return {
          snapshot: {},
          sample: {},
          authority: lateFreshSuccessor,
        };
      },
      monotonicMilliseconds: () => lateResponseElapsedMilliseconds,
      wallClockMilliseconds: () =>
        virtualRefreshStartedMilliseconds + lateResponseElapsedMilliseconds,
      pause: async (milliseconds) => {
        lateResponseElapsedMilliseconds += milliseconds;
      },
    });
    lateResponseDispatchCount += 1;
  } catch (error) {
    lateResponseTimeout = error;
  }
  if (
    lateResponseDispatchCount !== 0 ||
    lateResponseTimeout?.shareActionAuthorityRefresh?.distinctCandidates !== 0 ||
    !lateResponseTimeout?.message?.startsWith("fresh share action authority timed out:")
  ) {
    throw new Error(
      "post-handoff share action authority refresh self-test failed: late response refusal",
    );
  }
  console.log(
    "Post-handoff share action authority regressions passed: valid exact 2.5s successor accepted; 1001ms-age, wrong-share, wrong-target, changed-geometry, replay, and no-successor dispatch refused with reserved budgets; monotonic deadline aborts stalled reads and refuses late fresh responses.",
  );
  await runQuietSeatExecutionSelfTests();
  await runPointerArmExecutionSelfTests();
  await runAppShareFilesystemSelfTests();
  console.log("macOS packaged-evidence rig self-test passed.");
}

function classifySharedPointerBoundary(before, after) {
  const positionAvailable =
    Number.isFinite(before?.cursorX) && Number.isFinite(before?.cursorY) &&
    Number.isFinite(after?.cursorX) && Number.isFinite(after?.cursorY);
  const cursorPositionUnchanged = positionAvailable &&
    Math.abs(before.cursorX - after.cursorX) < 0.01 &&
    Math.abs(before.cursorY - after.cursorY) < 0.01;
  const counterProgress = hidPointerCounterProgress(before, after);
  const pointerActivityMonitorHealthy = counterProgress !== "unknown";
  const hidSystemPointerActivityObserved =
    before?.pointerBoundaryActivityObserved === true ||
    after?.pointerBoundaryActivityObserved === true ||
    counterProgress === "advanced";
  const sharedPointerBoundaryCorroborated =
    pointerActivityMonitorHealthy &&
    (cursorPositionUnchanged || hidSystemPointerActivityObserved);
  const sharedPointerActivityState =
    !pointerActivityMonitorHealthy ||
    (!cursorPositionUnchanged && !hidSystemPointerActivityObserved)
      ? "unknown"
      : hidSystemPointerActivityObserved
        ? "contaminated"
        : "quiet";
  return {
    cursorPositionUnchanged,
    sharedPointerActivityObserved: hidSystemPointerActivityObserved,
    hidSystemPointerActivityObserved,
    rawInputPointerActivityObserved: false,
    injectedPointerActivityObserved: false,
    pointerActivityMonitorHealthy,
    sharedPointerBoundaryCorroborated,
    sharedPointerBoundaryState:
      sharedPointerBoundaryCorroborated ? "corroborated" : "unknown",
    sharedPointerActivityState,
  };
}

function classifyActionBoundSharedPointerBoundary(before, after) {
  const standard = classifySharedPointerBoundary(before, after);
  const counterProgress = hidPointerCounterProgress(before, after);
  const pointerActivityMonitorHealthy = counterProgress !== "unknown";
  const hidSystemPointerActivityObserved =
    after?.pointerBoundaryActivityObserved === true || counterProgress === "advanced";
  const sharedPointerBoundaryCorroborated =
    pointerActivityMonitorHealthy &&
    (standard.cursorPositionUnchanged || hidSystemPointerActivityObserved);
  return {
    ...standard,
    sharedPointerActivityObserved: hidSystemPointerActivityObserved,
    hidSystemPointerActivityObserved,
    pointerActivityMonitorHealthy,
    sharedPointerBoundaryCorroborated,
    sharedPointerBoundaryState:
      sharedPointerBoundaryCorroborated ? "corroborated" : "unknown",
    sharedPointerActivityState:
      !pointerActivityMonitorHealthy ||
      (!standard.cursorPositionUnchanged && !hidSystemPointerActivityObserved)
        ? "unknown"
        : hidSystemPointerActivityObserved ? "contaminated" : "quiet",
  };
}

function classifySharedKeyboardBoundary(before, after) {
  const counterProgress = hidKeyboardCounterProgress(before, after);
  const keyboardActivityMonitorHealthy = counterProgress !== "unknown";
  const hidSystemKeyboardActivityObserved =
    before?.keyboardBoundaryActivityObserved === true ||
    after?.keyboardBoundaryActivityObserved === true ||
    counterProgress === "advanced";
  return {
    hidSystemKeyboardActivityObserved,
    keyboardActivityMonitorHealthy,
    sharedKeyboardBoundaryCorroborated: keyboardActivityMonitorHealthy,
    sharedKeyboardBoundaryState:
      keyboardActivityMonitorHealthy ? "corroborated" : "unknown",
    sharedKeyboardActivityState:
      !keyboardActivityMonitorHealthy
        ? "unknown"
        : hidSystemKeyboardActivityObserved ? "contaminated" : "quiet",
  };
}

function systemInvariants(before, after) {
  const foregroundIdentitySandwichHeld =
    before.foregroundIdentityStable === true && after.foregroundIdentityStable === true;
  const rawForegroundIdentitySandwichHeld =
    before.rawForegroundIdentityStable === true && after.rawForegroundIdentityStable === true &&
    Number.isSafeInteger(before.rawForegroundPID) && before.rawForegroundPID > 0 &&
    before.rawForegroundPID === before.foregroundPID &&
    before.rawForegroundPID === after.rawForegroundPID &&
    typeof before.rawForegroundPSN === "string" && before.rawForegroundPSN.length === 16 &&
    before.rawForegroundPSN === after.rawForegroundPSN;
  const foregroundAxFocusUnchanged =
    Number.isSafeInteger(before.foregroundAXFocusedWindowID) &&
    before.foregroundAXFocusedWindowID > 0 &&
    before.foregroundAXFocusedWindowID === before.foregroundAXMainWindowID &&
    before.foregroundAXFocusedWindowID === after.foregroundAXFocusedWindowID &&
    after.foregroundAXFocusedWindowID === after.foregroundAXMainWindowID;
  const foregroundAxFrontmostHeld =
    before.foregroundAXFrontmost === true && after.foregroundAXFrontmost === true;
  const pointerBoundary = classifySharedPointerBoundary(before, after);
  const keyboardBoundary = classifySharedKeyboardBoundary(before, after);
  return {
    foregroundUnchanged: before.foregroundPID > 0 && before.foregroundPID === after.foregroundPID,
    userFocusUnchanged:
      before.frontWindowID > 0 && before.frontWindowID === after.frontWindowID &&
      foregroundIdentitySandwichHeld && rawForegroundIdentitySandwichHeld &&
      foregroundAxFocusUnchanged && foregroundAxFrontmostHeld,
    foregroundIdentitySandwichHeld,
    rawForegroundIdentitySandwichHeld,
    foregroundAxFocusUnchanged,
    foregroundAxFrontmostHeld,
    ...pointerBoundary,
    ...keyboardBoundary,
    sharedInputSeatActivityObserved:
      pointerBoundary.sharedPointerActivityObserved === true ||
      keyboardBoundary.hidSystemKeyboardActivityObserved === true,
    spaceUnchanged: before.activeSpace > 0 && before.activeSpace === after.activeSpace,
  };
}

function actionBoundSystemInvariants(before, after) {
  const invariants = systemInvariants(before, after);
  const pointerBoundary = classifyActionBoundSharedPointerBoundary(before, after);
  return {
    ...invariants,
    ...pointerBoundary,
    sharedInputSeatActivityObserved:
      pointerBoundary.sharedPointerActivityObserved === true ||
      invariants.hidSystemKeyboardActivityObserved === true,
  };
}

function exactTargetReceiverMatches(snapshot, windowId, expectedFrontmost = null) {
  const exact = snapshot?.targetFocusedWindowID === windowId && snapshot?.targetMainWindowID === windowId;
  return expectedFrontmost === null ? exact : exact && snapshot?.targetAXFrontmost === expectedFrontmost;
}

function inputDeliveryProvenanceHeld(invariants) {
  const delivery = invariants?.inputDelivery;
  return (
    delivery != null &&
    ["macosAccessibility", "macosTargetedProcessEvent"].includes(delivery.route) &&
    delivery.exactTargetBound === true &&
    delivery.dispatchAttemptRecorded === true &&
    delivery.sharedInputSeatUsed === false &&
    delivery.globalHidInputUsed === false &&
    delivery.hardwareCursorMutationRequested === false
  );
}

function independentPointerLaneState(invariants) {
  const state = invariants?.sharedPointerActivityState;
  const diagnosticShapeValid = [
    "cursorPositionUnchanged",
    "sharedPointerActivityObserved",
    "hidSystemPointerActivityObserved",
    "rawInputPointerActivityObserved",
    "injectedPointerActivityObserved",
    "pointerActivityMonitorHealthy",
    "sharedPointerBoundaryCorroborated",
  ].every((field) => typeof invariants?.[field] === "boolean");
  if (
    !diagnosticShapeValid ||
    typeof invariants?.hidSystemKeyboardActivityObserved !== "boolean" ||
    typeof invariants?.keyboardActivityMonitorHealthy !== "boolean" ||
    typeof invariants?.sharedKeyboardBoundaryCorroborated !== "boolean" ||
    typeof invariants?.sharedInputSeatActivityObserved !== "boolean" ||
    invariants?.pointerActivityMonitorHealthy !== true ||
    invariants?.sharedPointerBoundaryCorroborated !== true ||
    invariants?.sharedPointerBoundaryState !== "corroborated" ||
    invariants?.keyboardActivityMonitorHealthy !== true ||
    invariants?.sharedKeyboardBoundaryCorroborated !== true ||
    invariants?.sharedKeyboardBoundaryState !== "corroborated"
  ) {
    return "unknown";
  }
  if (
    state === "quiet" &&
    invariants.cursorPositionUnchanged === true &&
    invariants.sharedPointerActivityObserved === false &&
    invariants.sharedKeyboardActivityState === "quiet" &&
    invariants.hidSystemKeyboardActivityObserved === false &&
    invariants.sharedInputSeatActivityObserved === false
  ) {
    return "quiet";
  }
  if (
    (state === "contaminated" && invariants.sharedPointerActivityObserved === true) ||
    (invariants.sharedKeyboardActivityState === "contaminated" &&
      invariants.hidSystemKeyboardActivityObserved === true)
  ) {
    return "concurrent-shared-seat-activity";
  }
  return "unknown";
}

function actionPointerLaneState(invariants) {
  const state = invariants?.sharedPointerActivityState;
  const diagnosticShapeValid = [
    "cursorPositionUnchanged",
    "sharedPointerActivityObserved",
    "hidSystemPointerActivityObserved",
    "rawInputPointerActivityObserved",
    "injectedPointerActivityObserved",
    "pointerActivityMonitorHealthy",
    "sharedPointerBoundaryCorroborated",
  ].every((field) => typeof invariants?.[field] === "boolean");
  if (
    !diagnosticShapeValid ||
    invariants?.pointerActivityMonitorHealthy !== true ||
    invariants?.sharedPointerBoundaryCorroborated !== true ||
    invariants?.sharedPointerBoundaryState !== "corroborated"
  ) {
    return "unknown";
  }
  if (
    state === "quiet" &&
    invariants.cursorPositionUnchanged === true &&
    invariants.sharedPointerActivityObserved === false &&
    invariants.hidSystemPointerActivityObserved === false &&
    invariants.rawInputPointerActivityObserved === false &&
    invariants.injectedPointerActivityObserved === false
  ) {
    return "quiet";
  }
  if (
    state === "contaminated" &&
    invariants.sharedPointerActivityObserved === true &&
    (
      invariants.cursorPositionUnchanged === false ||
      invariants.hidSystemPointerActivityObserved === true ||
      invariants.rawInputPointerActivityObserved === true ||
      invariants.injectedPointerActivityObserved === true
    )
  ) {
    return "concurrent-shared-seat-activity";
  }
  return "unknown";
}

function recordPointerLaneState(state) {
  if (state === "quiet") pointerEvidenceObserved.quiet = true;
  else if (state === "concurrent-shared-seat-activity") {
    pointerEvidenceObserved.concurrentSharedSeatActivity = true;
  } else pointerEvidenceObserved.unknown = true;
  return state;
}

function independentPointerLaneAccepted(invariants) {
  return recordPointerLaneState(independentPointerLaneState(invariants)) === "quiet";
}

function actionPointerLaneAccepted(invariants) {
  return recordPointerLaneState(actionPointerLaneState(invariants)) === "quiet";
}

function allIndependentInvariantsHeld(invariants) {
  return [
    invariants?.foregroundUnchanged,
    invariants?.userFocusUnchanged,
    invariants?.sharedPointerBoundaryCorroborated,
    invariants?.spaceUnchanged,
    independentPointerLaneAccepted(invariants),
  ].every((value) => value === true);
}

function allActionInvariantsHeld(invariants) {
  return [
    invariants?.foregroundUnchanged,
    invariants?.userFocusUnchanged,
    invariants?.hardwareCursorPreservedByHelper,
    invariants?.helperGlobalPointerPreservation === "confirmed",
    invariants?.sharedPointerBoundaryCorroborated,
    invariants?.sharedPointerBoundaryState === "corroborated",
    inputDeliveryProvenanceHeld(invariants),
    invariants?.spaceUnchanged,
    actionPointerLaneAccepted(invariants),
  ].every((value) => value === true);
}

function pointerEvidenceSummary() {
  return {
    requestedLane: pointerEvidenceLane,
    quietObserved: pointerEvidenceObserved.quiet,
    concurrentSharedSeatActivityObserved:
      pointerEvidenceObserved.concurrentSharedSeatActivity,
    unknownObserved: pointerEvidenceObserved.unknown,
    rawCursorPositionsRetained: false,
    rawPlatformActivityCountersRetained: false,
    rawHidSystemCountersRetained: false,
    hidSystemActivityClaimedAsPhysical: false,
  };
}

function remainingRequestPublicationTime(
  deadlineMilliseconds,
  stage,
  nowMilliseconds = Date.now(),
) {
  if (!Number.isSafeInteger(deadlineMilliseconds)) {
    throw new Error(`the pointer handoff request-publication ${stage} deadline is unavailable`);
  }
  const remaining = deadlineMilliseconds - nowMilliseconds;
  if (!Number.isSafeInteger(remaining) || remaining < 1) {
    throw new Error(`the pointer handoff request-publication ${stage} deadline elapsed`);
  }
  return remaining;
}

function remainingPointerHandoffTime(deadlineMilliseconds, stage) {
  if (!Number.isSafeInteger(deadlineMilliseconds)) {
    throw new Error(`the pointer handoff ${stage} deadline is unavailable`);
  }
  const remaining = deadlineMilliseconds - Date.now();
  if (!Number.isSafeInteger(remaining) || remaining < 1) {
    throw new Error(`the pointer handoff ${stage} deadline elapsed`);
  }
  return remaining;
}

async function waitForPointerPromptState(state, deadlineMilliseconds = null) {
  const waitTimeoutMs = deadlineMilliseconds === null
    ? 10_000
    : remainingPointerHandoffTime(deadlineMilliseconds, state.toLowerCase());
  return await waitFor(
    `independently observed ${state.toLowerCase()} pointer handoff prompt`,
    async () => {
      if (
        !pointerHandoffProcess ||
        pointerHandoffProcess.exitCode !== null ||
        pointerHandoffProcess.signalCode !== null
      ) {
        throw new Error("the pointer handoff prompt exited before delivery proof");
      }
      const probeTimeoutMs = deadlineMilliseconds === null
        ? SYSTEM_PROBE_TIMEOUT_MS
        : Math.min(
          SYSTEM_PROBE_TIMEOUT_MS,
          remainingPointerHandoffTime(deadlineMilliseconds, state.toLowerCase()),
        );
      const sample = processProbe(
        systemProbeBinary,
        null,
        { pid: pointerHandoffProcess.pid, state },
        probeTimeoutMs,
      );
      return pointerPromptDeliveryObserved(sample) && sample;
    },
    waitTimeoutMs,
  );
}

async function startPointerHandoff(presentationBaseline) {
  if (pointerEvidenceLane !== "deliberate-concurrency") return null;
  if (pointerHandoffProcess || pointerHandoffRequestPublicationAcknowledged) {
    throw new Error("the one-shot app-share handoff was already started");
  }
  remainingRequestPublicationTime(
    pointerHandoffRequestPublicationDeadlineMilliseconds,
    "start",
  );
  await mkdir(operatorDirectory, { recursive: false, mode: 0o700 });
  remainingRequestPublicationTime(
    pointerHandoffRequestPublicationDeadlineMilliseconds,
    "operator-directory",
  );
  pointerHandoffRequestId = randomBytes(16).toString("hex");
  pointerHandoffArmDeadlineMilliseconds = Date.now() + APP_SHARE_HANDOFF_WAIT_MS;
  pointerHandoffHardDeadlineMilliseconds =
    pointerHandoffArmDeadlineMilliseconds + APP_SHARE_HANDOFF_COMPLETION_GRACE_MS;
  pointerHandoffProcess = spawn(
    pointerHandoffBinary,
    [
      pointerHandoffControlPath,
      pointerHandoffStartPath,
      pointerHandoffCompletePath,
      pointerHandoffRequestPath,
      pointerHandoffRequestId,
      String(pointerHandoffArmDeadlineMilliseconds),
      String(pointerHandoffHardDeadlineMilliseconds),
    ],
    { env: childEnvironment(), stdio: "ignore" },
  );
  if (
    !Number.isSafeInteger(pointerHandoffProcess.pid) ||
    pointerHandoffProcess.pid <= 0 ||
    pointerHandoffProcess.pid === process.pid
  ) {
    throw new Error("the app-share handoff did not receive a distinct process identity");
  }

  const waitingPromptSample = await waitForPointerPromptState(
    POINTER_HANDOFF_WAITING_STATE,
    pointerHandoffRequestPublicationDeadlineMilliseconds,
  );
  const waitingPresentationInvariants = systemInvariants(
    presentationBaseline,
    waitingPromptSample,
  );
  requireIndependentInvariants("app-share WAITING presentation", waitingPresentationInvariants);
  requireCheck(
    "app-share WAITING presentation remained quiet",
    independentPointerLaneState(waitingPresentationInvariants) === "quiet",
    "the bound app appeared without shared-pointer or user-context activity",
  );

  const requestMarker = pointerHandoffRequestMarker(pointerHandoffProcess.pid);
  pointerHandoffRequestSha256 = createHash("sha256")
    .update(`${JSON.stringify(requestMarker)}\n`, "utf8")
    .digest("hex");
  const markerPublicationBudgetMilliseconds = Math.min(
    MARKER_PUBLISH_TIMEOUT_MS,
    remainingRequestPublicationTime(
      pointerHandoffRequestPublicationDeadlineMilliseconds,
      "marker-publication",
    ),
  );
  await publishAtomicMarkerOnce(
    pointerHandoffRequestPath,
    requestMarker,
    markerPublicationBudgetMilliseconds,
  );
  await writePointerHandoffState(APP_SHARE_HANDOFF_READY_STATE, {
    requestSha256: pointerHandoffRequestSha256,
  });
  const readyPromptSample = await waitForPointerPromptState(
    APP_SHARE_HANDOFF_READY_STATE,
    pointerHandoffRequestPublicationDeadlineMilliseconds,
  );
  const readyPresentationInvariants = systemInvariants(waitingPromptSample, readyPromptSample);
  requireIndependentInvariants("app-share READY presentation", readyPresentationInvariants);
  requireCheck(
    "app-share READY transition preserved the shared input seat",
    independentPointerLaneState(readyPresentationInvariants) === "quiet",
    "foreground, focus, cursor, HID counters, and active Space stayed unchanged",
  );
  pointerHandoffExactAppBundleObserved = true;
  pointerHandoffExactWindowObserved = true;
  pointerHandoffExactButtonObserved = true;
  remainingRequestPublicationTime(
    pointerHandoffRequestPublicationDeadlineMilliseconds,
    "completion",
  );
  pointerHandoffRequestPublicationAcknowledged = true;
  if (
    pointerHandoffProcess.exitCode !== null ||
    pointerHandoffProcess.signalCode !== null
  ) {
    throw new Error("the app-share handoff exited before the bound action request");
  }
  log(
    `ACTION REQUIRED: through the separately authorized exact-app share for ${APP_SHARE_BUNDLE_IDENTIFIER}, press ${APP_SHARE_READY_BUTTON_TEXT} exactly once. Do not use the shared desktop or retry.`,
  );
  return readyPromptSample;
}

async function waitForDeliberatePointerActivity(baseline) {
  if (pointerEvidenceLane !== "deliberate-concurrency") return null;
  if (!pointerHandoffRequestPublicationAcknowledged || !pointerHandoffProcess) {
    throw new Error("the app-share wait requires a published one-shot bound request");
  }
  if (
    !Number.isSafeInteger(pointerHandoffArmDeadlineMilliseconds) ||
    !Number.isSafeInteger(pointerHandoffHardDeadlineMilliseconds)
  ) {
    throw new Error("the app-share handoff has no shared absolute deadline");
  }

  const promptPid = pointerHandoffProcess.pid;
  const startReceipt = await waitFor(
    "fresh exact-app-share start receipt",
    async () => (await pathExists(pointerHandoffStartPath))
      ? await readAppShareStartReceipt(promptPid)
      : false,
    remainingPointerHandoffTime(pointerHandoffArmDeadlineMilliseconds, "start receipt"),
  );
  pointerHandoffStartReceiptSha256 = startReceipt.sha256;
  pointerHandoffStartReceiptCreatedAt = startReceipt.record.createdAt;
  pointerHandoffAcceptanceButtonActionObserved = true;
  const armedSample = await waitForPointerPromptState(
    APP_SHARE_HANDOFF_ARMED_STATE,
    pointerHandoffArmDeadlineMilliseconds,
  );
  const appShareActionInvariants = systemInvariants(baseline, armedSample);
  requireIndependentInvariants("exact-app-share action", appShareActionInvariants);
  requireCheck(
    "exact-app-share action did not use the shared input seat",
    independentPointerLaneState(appShareActionInvariants) === "quiet" &&
      appShareActionInvariants.foregroundUnchanged === true &&
      appShareActionInvariants.userFocusUnchanged === true &&
      appShareActionInvariants.spaceUnchanged === true,
    "foreground, focus, cursor, HID counters, and active Space stayed unchanged across the external app action",
  );
  pointerHandoffButtonDisabledAfterAction = true;
  pointerHandoffSharedHidInputObserved = false;
  pointerHandoffSampledSharedContextUnchanged = true;
  pointerHandoffActionDeadlineMilliseconds = Math.min(
    pointerHandoffHardDeadlineMilliseconds,
    Date.now() + APP_SHARE_HANDOFF_COMPLETION_GRACE_MS,
  );
  pointerHandoffProductActionStartedAt = new Date().toISOString();
  await writePointerHandoffState(POINTER_HANDOFF_ACTION_STATE, {
    requestSha256: pointerHandoffRequestSha256,
    startReceiptSha256: pointerHandoffStartReceiptSha256,
    productActionStartedAt: pointerHandoffProductActionStartedAt,
  });
  const actionSample = await waitForPointerPromptState(
    POINTER_HANDOFF_ACTION_STATE,
    pointerHandoffActionDeadlineMilliseconds,
  );
  const actionTransitionInvariants = systemInvariants(armedSample, actionSample);
  requireIndependentInvariants("app-share ACTION transition", actionTransitionInvariants);
  requireCheck(
    "app-share ACTION transition remained outside the shared input seat",
    independentPointerLaneState(actionTransitionInvariants) === "quiet",
    "the exact app remained bound while the product action window opened",
  );
  return actionSample;
}

async function completePointerHandoff(pointerHandoffCompletionBaseline) {
  if (pointerEvidenceLane !== "deliberate-concurrency") return;
  if (
    !pointerHandoffProcess ||
    !pointerHandoffRequestPublicationAcknowledged ||
    !pointerHandoffStartReceiptSha256 ||
    pointerHandoffCompletePublicationAcknowledged ||
    pointerHandoffActionDispatched !== true ||
    !pointerHandoffAcceptanceButtonActionObserved ||
    pointerHandoffSharedHidInputObserved !== false ||
    !pointerHandoffSampledSharedContextUnchanged ||
    !pointerHandoffProductBoundaryQuiet ||
    !pointerHandoffIndependentBoundaryQuiet
  ) {
    throw new Error("the app-share handoff completion preconditions were not satisfied");
  }
  let remainingCompletionMs = remainingPointerHandoffTime(
    pointerHandoffActionDeadlineMilliseconds,
    "completion",
  );

  const completionBoundary = processProbe(
    systemProbeBinary,
    null,
    { pid: pointerHandoffProcess.pid, state: POINTER_HANDOFF_ACTION_STATE },
    Math.min(SYSTEM_PROBE_TIMEOUT_MS, remainingCompletionMs),
  );
  remainingCompletionMs = remainingPointerHandoffTime(
    pointerHandoffActionDeadlineMilliseconds,
    "completion probe",
  );
  if (!pointerPromptDeliveryObserved(completionBoundary)) {
    throw new Error("the pointer handoff panel was not independently visible before completion");
  }
  const completionInvariants = systemInvariants(pointerHandoffCompletionBaseline, completionBoundary);
  requireIndependentInvariants("app-share pre-completion boundary", completionInvariants);
  requireCheck(
    "app-share pre-completion boundary remained quiet",
    independentPointerLaneState(completionInvariants) === "quiet",
    "the shared cursor, HID counters, foreground, focus, and Space stayed unchanged",
  );

  pointerHandoffProductActionCompletedAt = new Date().toISOString();
  await writePointerHandoffState(POINTER_HANDOFF_COMPLETE_STATE, {
    requestSha256: pointerHandoffRequestSha256,
    startReceiptSha256: pointerHandoffStartReceiptSha256,
    productActionStartedAt: pointerHandoffProductActionStartedAt,
    productActionCompletedAt: pointerHandoffProductActionCompletedAt,
  });
  await waitFor(
    "bound app-share completion receipt",
    async () => (await pathExists(pointerHandoffCompletePath))
      ? await readAppShareCompleteReceipt(pointerHandoffProcess.pid)
      : false,
    remainingPointerHandoffTime(pointerHandoffActionDeadlineMilliseconds, "completion receipt"),
  );
  const completePromptSample = await waitForPointerPromptState(
    POINTER_HANDOFF_COMPLETE_STATE,
    pointerHandoffActionDeadlineMilliseconds,
  );
  const completionPresentationInvariants = systemInvariants(
    completionBoundary,
    completePromptSample,
  );
  requireIndependentInvariants("app-share COMPLETE presentation", completionPresentationInvariants);
  requireCheck(
    "app-share COMPLETE presentation remained quiet",
    independentPointerLaneState(completionPresentationInvariants) === "quiet",
    "the app-produced completion receipt and green state preserved the shared context",
  );
  pointerHandoffSurfaceObservedAtProductBoundaries = true;
  remainingCompletionMs = remainingPointerHandoffTime(
    pointerHandoffActionDeadlineMilliseconds,
    "completion presentation",
  );
  pointerHandoffCompletePublicationAcknowledged = true;
  remainingPointerHandoffTime(pointerHandoffActionDeadlineMilliseconds, "completion publication");
  await delay(750);
  remainingPointerHandoffTime(pointerHandoffActionDeadlineMilliseconds, "prompt teardown");
  const promptTeardown = await terminate(pointerHandoffProcess, "app-share handoff");
  pointerHandoffProcess = null;
  pointerHandoffPromptClosed =
    promptTeardown.requested === true && promptTeardown.alreadyExited === false;
  if (!pointerHandoffPromptClosed) {
    throw new Error("the app-share handoff was not closed by the runner");
  }
}

const FIXTURE_ACTIONS = new Set([
  "ready",
  "set-value",
  "semantic",
  "click",
  "resize",
  "focus-field",
  "sibling-text",
  "sibling-click",
]);

function boundedCounter(value) {
  return Number.isSafeInteger(value) && value >= 0 ? value : null;
}

function fixtureCounterSnapshot(snapshot) {
  return {
    clicks: boundedCounter(snapshot?.clicks),
    semanticPresses: boundedCounter(snapshot?.semanticPresses),
    animationTick: boundedCounter(snapshot?.animationTick),
    resizeCount: boundedCounter(snapshot?.resizeCount),
    focusCount: boundedCounter(snapshot?.focusCount),
    moveEvents: boundedCounter(snapshot?.moveEvents),
    appliedControlSequence: boundedCounter(snapshot?.appliedControlSequence),
    siblingTextLength: boundedCounter(snapshot?.siblingTextLength),
    siblingClicks: boundedCounter(snapshot?.siblingClicks),
    siblingFocusCount: boundedCounter(snapshot?.siblingFocusCount),
    distinctSamePidWindows:
      Number.isSafeInteger(snapshot?.primaryWindowId) && snapshot.primaryWindowId > 0 &&
      Number.isSafeInteger(snapshot?.siblingWindowId) && snapshot.siblingWindowId > 0 &&
      snapshot.primaryWindowId !== snapshot.siblingWindowId,
    siblingFocusPrepared:
      Number.isSafeInteger(snapshot?.siblingFocusCount) && snapshot.siblingFocusCount > 0,
    semanticValueMatchesExpected: snapshot?.semanticValue === SEMANTIC_VALUE,
    lastAction: FIXTURE_ACTIONS.has(snapshot?.lastAction) ? snapshot.lastAction : "unrecognized",
  };
}

async function collectFailureDiagnostics() {
  const targetSiblingExpectedAfter =
    typeof failureProbeBaseline?.targetSiblingExpectedAfter === "boolean"
      ? failureProbeBaseline.targetSiblingExpectedAfter
      : Number.isSafeInteger(fixtureTargetPid) && fixtureTargetPid > 0;
  const diagnostics = {
    stage: failureProbeBaseline?.stage || "notReached",
    actionDispatched: pointerHandoffActionDispatched,
    systemProbe: {
      baselineCaptured: Boolean(failureProbeBaseline?.system),
      afterCaptured: false,
      equality: null,
    },
    fixtureCounters: {
      baselineCaptured: Boolean(failureProbeBaseline?.fixture),
      afterCaptured: false,
      before: failureProbeBaseline?.fixture
        ? fixtureCounterSnapshot(failureProbeBaseline.fixture)
        : null,
      after: null,
    },
    targetSiblingReceiver: {
      targetKnown:
        Number.isSafeInteger(fixtureTargetPid) && fixtureTargetPid > 0 &&
        Number.isSafeInteger(fixtureSiblingWindowId) && fixtureSiblingWindowId > 0,
      expectedFocusedAfter: targetSiblingExpectedAfter,
      afterCaptured: false,
      focusedAfter: null,
      mainAfter: null,
      expectationMet: null,
    },
  };

  if (failureProbeBaseline?.system && systemProbeBinary) {
    try {
      const after = processProbe(systemProbeBinary);
      diagnostics.systemProbe.afterCaptured = true;
      diagnostics.systemProbe.equality = systemInvariants(failureProbeBaseline.system, after);
    } catch {
      // Availability is recorded without persisting a path or platform diagnostic.
    }
  }
  if (failureProbeBaseline?.fixture && fixtureStatePath) {
    try {
      const after = await fixtureState(fixtureStatePath);
      diagnostics.fixtureCounters.afterCaptured = true;
      diagnostics.fixtureCounters.after = fixtureCounterSnapshot(after);
    } catch {
      // Availability is recorded without persisting a scratch path or raw state.
    }
  }
  if (diagnostics.targetSiblingReceiver.targetKnown && systemProbeBinary) {
    try {
      const targetAfter = processProbe(systemProbeBinary, fixtureTargetPid);
      const focusedAfter = targetAfter.targetFocusedWindowID === fixtureSiblingWindowId;
      const mainAfter = targetAfter.targetMainWindowID === fixtureSiblingWindowId;
      diagnostics.targetSiblingReceiver.afterCaptured = true;
      diagnostics.targetSiblingReceiver.focusedAfter = focusedAfter;
      diagnostics.targetSiblingReceiver.mainAfter = mainAfter;
      diagnostics.targetSiblingReceiver.expectationMet =
        focusedAfter === targetSiblingExpectedAfter && mainAfter === targetSiblingExpectedAfter;
    } catch {
      // Persist only bounded availability/equality booleans, never raw target identities.
    }
  }
  return diagnostics;
}

function requireActionInvariants(label, action) {
  requireCheck(
    `${label} route, helper-preservation, and shared-pointer invariants`,
    allActionInvariantsHeld(action.invariants),
    JSON.stringify(action.invariants),
  );
}

function requireIndependentInvariants(label, invariants) {
  requireCheck(
    `${label} independent foreground/focus/pointer/Space invariants`,
    allIndependentInvariantsHeld(invariants),
    JSON.stringify(invariants),
  );
}

async function terminate(child, label) {
  if (!child) return { requested: false, alreadyExited: true, exitCode: null, signal: null };
  if (child.exitCode !== null || child.signalCode !== null) {
    return { requested: false, alreadyExited: true, exitCode: child.exitCode, signal: child.signalCode };
  }

  const exit = new Promise((resolveExit) => {
    child.once("exit", (exitCode, signal) => resolveExit({ exitCode, signal }));
  });
  child.kill("SIGTERM");
  let outcome = await Promise.race([exit, delay(2_000).then(() => null)]);
  if (!outcome && child.exitCode === null && child.signalCode === null) {
    child.kill("SIGKILL");
    outcome = await exit;
  }
  log(`${label} stopped`);
  return { requested: true, alreadyExited: false, ...(outcome || { exitCode: child.exitCode, signal: child.signalCode }) };
}

async function apiState(signal = null) {
  const response = await fetch(`http://127.0.0.1:${port}/api/state`, {
    headers: { Authorization: `Bearer ${bearerToken}` },
    ...(signal === null ? {} : { signal }),
  });
  if (!response.ok) throw new Error(`state returned HTTP ${response.status}`);
  return (await response.json()).state;
}

async function healthReachable() {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/health`, {
      signal: AbortSignal.timeout(750),
    });
    return response.ok;
  } catch {
    return false;
  }
}

async function commandResponse(method, params = {}, callId = randomUUID(), timeoutMs = null) {
  if (timeoutMs !== null && (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1)) {
    throw new Error("command response timeout must be a positive integer");
  }
  const response = await fetch(`http://127.0.0.1:${port}/api/v1/command`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${bearerToken}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ callId, method, params }),
    ...(timeoutMs === null ? {} : { signal: AbortSignal.timeout(timeoutMs) }),
  });
  const body = await response.json();
  return { status: response.status, ok: response.ok && body.ok === true, body };
}

async function command(method, params = {}) {
  const response = await commandResponse(method, params);
  const { body } = response;
  if (!response.ok) {
    const errorCode = body.error?.code || "unknown";
    const errorMessage = body.error?.message || "";
    throw new Error(`${method} returned HTTP ${response.status}: ${errorCode} ${errorMessage}`);
  }
  return body;
}

async function cancelCommandResponse(callId) {
  const response = await fetch(`http://127.0.0.1:${port}/api/v1/command/cancel`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${bearerToken}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ callId }),
  });
  return { status: response.status, ok: response.ok, body: await response.json() };
}

async function fixtureState(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

async function saveCurrentScreenshot(publicState, filename, expectedWindowId) {
  if (nativeTextPayloadMayBeVisible) {
    throw new Error("refusing to create a retained screenshot after native text delivery began");
  }
  const observation = publicState.computerObservation;
  if (!observation?.screenshotUrl) throw new Error("current computer observation has no screenshot URL");
  if (observation.windowId !== expectedWindowId || observation.windowTitle !== FIXTURE_TITLE) {
    throw new Error("refusing to save a screenshot that is not bound to the exact fixture window");
  }
  const response = await fetch(`http://127.0.0.1:${port}${observation.screenshotUrl}`, {
    headers: { Authorization: `Bearer ${bearerToken}` },
  });
  if (!response.ok) throw new Error(`screenshot returned HTTP ${response.status}`);
  const data = Buffer.from(await response.arrayBuffer());
  if (data.includes(Buffer.from(bearerToken, "utf8"))) {
    throw new Error("refusing to persist a screenshot containing the bearer token");
  }
  const dimensions = pngDimensions(data);
  if (
    dimensions.width !== observation.imageWidth ||
    dimensions.height !== observation.imageHeight
  ) {
    throw new Error("captured PNG dimensions do not match the bound observation");
  }
  const path = join(outputDir, filename);
  await writeFile(path, data, { flag: "wx", mode: 0o600 });
  const record = {
    file: filename,
    sha256: createHash("sha256").update(data).digest("hex"),
    bytes: data.length,
    ...dimensions,
    frameId: observation.frameId,
    windowId: observation.windowId,
    sourceSequence: observation.sourceSequence ?? null,
    transportSequence: observation.share?.sequence ?? null,
  };
  screenshots.push(record);
  return record;
}

function actionSummary(body) {
  const result = body.result || {};
  return {
    ok: body.ok === true,
    actionId: result.actionId,
    effect: result.effect,
    deliveryMode: result.deliveryMode,
    frameId: result.frameId,
    shareId: result.shareId ?? null,
    sourceSequence: result.sourceSequence ?? null,
    characters: result.characters ?? null,
    utf16CodeUnits: result.utf16CodeUnits ?? null,
    invariants: result.invariants,
    evidence: result.evidence,
    backendEffect: result.backendEffect,
  };
}

function shareSample(observation, stage) {
  const share = observation?.share;
  if (!share?.active || observation.sourceSequence == null) return null;
  return {
    stage,
    shareId: share.id,
    sourceSequence: share.sourceSequence,
    sourceDroppedFrames: share.sourceDroppedFrames,
    sequence: share.sequence,
    transportDroppedFrames: share.transportDroppedFrames,
    lastAckedSequence: share.lastAckedSequence,
    ackPaced: share.ackPaced,
    backpressure: share.backpressure,
    captureBackend: share.captureBackend,
    nativeStream: share.nativeStream,
    systemIndicator: share.systemIndicator,
    selectionMode: share.selectionMode,
    windowWidth: observation.screenWidth,
    windowHeight: observation.screenHeight,
    imageWidth: observation.imageWidth,
    imageHeight: observation.imageHeight,
  };
}

function capturedFrameMatchesWindowGeometry(sample) {
  if (
    !Number.isSafeInteger(sample?.windowWidth) || sample.windowWidth <= 0 ||
    !Number.isSafeInteger(sample?.windowHeight) || sample.windowHeight <= 0 ||
    !Number.isSafeInteger(sample?.imageWidth) || sample.imageWidth <= 0 ||
    !Number.isSafeInteger(sample?.imageHeight) || sample.imageHeight <= 0
  ) {
    return false;
  }
  const windowAspect = sample.windowWidth / sample.windowHeight;
  const imageAspect = sample.imageWidth / sample.imageHeight;
  return Math.abs(windowAspect - imageAspect) <= 0.003;
}

function shareActionAuthority(observation, sample) {
  if (!observation || !sample) return null;
  return {
    frameId: observation.frameId,
    capturedAt: observation.capturedAt,
    captureAgeMs: observation.captureAgeMs,
    shareId: sample.shareId,
    topLevelShareId: observation.shareId,
    sourceSequence: sample.sourceSequence,
    topLevelSourceSequence: observation.sourceSequence,
    sequence: sample.sequence,
    windowId: observation.windowId,
    pid: observation.pid,
    windowTitle: observation.windowTitle,
    windowWidth: sample.windowWidth,
    windowHeight: sample.windowHeight,
    imageWidth: sample.imageWidth,
    imageHeight: sample.imageHeight,
  };
}

function exactShareActionAuthorityAdvanced(reference, candidate) {
  return (
    typeof reference?.frameId === "string" && reference.frameId.length > 0 &&
    typeof candidate?.frameId === "string" && candidate.frameId.length > 0 &&
    candidate.frameId !== reference.frameId &&
    typeof reference.capturedAt === "string" &&
    Number.isFinite(Date.parse(reference.capturedAt)) &&
    Number.isFinite(reference.captureAgeMs) && reference.captureAgeMs >= 0 &&
    typeof candidate.capturedAt === "string" &&
    Number.isFinite(Date.parse(candidate.capturedAt)) &&
    Number.isFinite(candidate.captureAgeMs) && candidate.captureAgeMs >= 0 &&
    typeof reference?.shareId === "string" && reference.shareId.length > 0 &&
    reference.topLevelShareId === reference.shareId &&
    candidate?.shareId === reference.shareId &&
    candidate.topLevelShareId === candidate.shareId &&
    Number.isSafeInteger(reference.sourceSequence) && reference.sourceSequence > 0 &&
    reference.topLevelSourceSequence === reference.sourceSequence &&
    Number.isSafeInteger(candidate.sourceSequence) &&
    candidate.sourceSequence > reference.sourceSequence &&
    candidate.topLevelSourceSequence === candidate.sourceSequence &&
    Number.isSafeInteger(reference.sequence) && reference.sequence > 0 &&
    Number.isSafeInteger(candidate.sequence) && candidate.sequence > reference.sequence &&
    candidate.windowId === reference.windowId &&
    candidate.pid === reference.pid &&
    candidate.windowTitle === reference.windowTitle &&
    candidate.windowWidth === reference.windowWidth &&
    candidate.windowHeight === reference.windowHeight &&
    candidate.imageWidth === reference.imageWidth &&
    candidate.imageHeight === reference.imageHeight &&
    capturedFrameMatchesWindowGeometry(reference) &&
    capturedFrameMatchesWindowGeometry(candidate)
  );
}

function estimatedShareActionAuthorityAgeMs(authority, nowMilliseconds = Date.now()) {
  const publishedMilliseconds = Date.parse(authority?.capturedAt);
  if (
    !Number.isFinite(publishedMilliseconds) ||
    !Number.isFinite(authority?.captureAgeMs) || authority.captureAgeMs < 0 ||
    !Number.isFinite(nowMilliseconds) || nowMilliseconds < publishedMilliseconds
  ) {
    return null;
  }
  const estimatedAge = authority.captureAgeMs + nowMilliseconds - publishedMilliseconds;
  return Number.isFinite(estimatedAge) && estimatedAge >= 0 ? estimatedAge : null;
}

function decideShareActionAuthorityRefresh(reference, candidate, nowMilliseconds) {
  const estimatedAgeMs = estimatedShareActionAuthorityAgeMs(candidate, nowMilliseconds);
  return {
    accepted:
      exactShareActionAuthorityAdvanced(reference, candidate) &&
      estimatedAgeMs !== null &&
      estimatedAgeMs <= APP_SHARE_ACTION_FRAME_MAX_ESTIMATED_AGE_MS,
    estimatedAgeMs,
  };
}

function decideShareActionAuthorityDispatch(
  reference,
  authority,
  expectedFrameId,
  nowMilliseconds,
) {
  const estimatedAgeMs = estimatedShareActionAuthorityAgeMs(authority, nowMilliseconds);
  return {
    accepted:
      exactShareActionAuthorityAdvanced(reference, authority) &&
      typeof expectedFrameId === "string" && expectedFrameId.length > 0 &&
      authority?.frameId === expectedFrameId &&
      estimatedAgeMs !== null &&
      estimatedAgeMs <= APP_SHARE_ACTION_FRAME_MAX_ESTIMATED_AGE_MS,
    estimatedAgeMs,
  };
}

function shareActionAuthorityRefreshRejectionReasons(reference, candidate, estimatedAgeMs) {
  const reasons = [];
  if (
    candidate?.frameId === reference?.frameId ||
    !Number.isSafeInteger(candidate?.sourceSequence) ||
    candidate.sourceSequence <= reference?.sourceSequence ||
    !Number.isSafeInteger(candidate?.sequence) ||
    candidate.sequence <= reference?.sequence
  ) {
    reasons.push("replayOrNonAdvancing");
  }
  if (
    candidate?.shareId !== reference?.shareId ||
    candidate?.topLevelShareId !== candidate?.shareId
  ) {
    reasons.push("shareMismatch");
  }
  if (
    candidate?.windowId !== reference?.windowId ||
    candidate?.pid !== reference?.pid ||
    candidate?.windowTitle !== reference?.windowTitle
  ) {
    reasons.push("targetMismatch");
  }
  if (
    candidate?.windowWidth !== reference?.windowWidth ||
    candidate?.windowHeight !== reference?.windowHeight ||
    candidate?.imageWidth !== reference?.imageWidth ||
    candidate?.imageHeight !== reference?.imageHeight ||
    !capturedFrameMatchesWindowGeometry(candidate)
  ) {
    reasons.push("geometryMismatch");
  }
  if (
    typeof candidate?.frameId !== "string" || candidate.frameId.length === 0 ||
    typeof candidate?.capturedAt !== "string" ||
    !Number.isFinite(Date.parse(candidate.capturedAt)) ||
    !Number.isFinite(candidate?.captureAgeMs) || candidate.captureAgeMs < 0 ||
    candidate?.topLevelSourceSequence !== candidate?.sourceSequence ||
    estimatedAgeMs === null
  ) {
    reasons.push("invalidMetadata");
  } else if (estimatedAgeMs > APP_SHARE_ACTION_FRAME_MAX_ESTIMATED_AGE_MS) {
    reasons.push("staleAge");
  }
  return reasons.length > 0 ? reasons : ["invalidMetadata"];
}

function freshShareActionAuthorityTimeoutError(diagnostics) {
  const error = new Error(
    `fresh share action authority timed out: polls=${diagnostics.polls}, distinctCandidates=${diagnostics.distinctCandidates}, minimumEstimatedAgeMs=${diagnostics.minimumObservedAgeMs ?? "none"}, rejectionCounts=${JSON.stringify(diagnostics.rejectionCounts)}`,
  );
  error.shareActionAuthorityRefresh = {
    ...diagnostics,
    rejectionCounts: { ...diagnostics.rejectionCounts },
  };
  return error;
}

function scheduleShareActionAuthorityReadAbort(controller, milliseconds) {
  const timeout = setTimeout(() => controller.abort(), milliseconds);
  return () => clearTimeout(timeout);
}

async function waitForFreshShareActionAuthority({
  reference,
  timeoutMs,
  loadCandidate,
  monotonicMilliseconds = () => performance.now(),
  wallClockMilliseconds = () => Date.now(),
  pause = delay,
  sampleIntervalMs = WAIT_STEP_MS,
  scheduleReadAbort = scheduleShareActionAuthorityReadAbort,
}) {
  if (
    !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 ||
    typeof loadCandidate !== "function" ||
    typeof monotonicMilliseconds !== "function" ||
    typeof wallClockMilliseconds !== "function" ||
    typeof pause !== "function" ||
    !Number.isSafeInteger(sampleIntervalMs) || sampleIntervalMs < 1 ||
    typeof scheduleReadAbort !== "function"
  ) {
    throw new Error("the fresh share action authority wait policy is incomplete");
  }
  const startedAtMilliseconds = monotonicMilliseconds();
  const deadlineMilliseconds = startedAtMilliseconds + timeoutMs;
  if (
    !Number.isFinite(startedAtMilliseconds) ||
    !Number.isFinite(deadlineMilliseconds)
  ) {
    throw new Error("the fresh share action authority wait deadline is unavailable");
  }
  const diagnostics = {
    polls: 0,
    readAborts: 0,
    readErrors: 0,
    distinctCandidates: 0,
    minimumObservedAgeMs: null,
    rejectionCounts: Object.fromEntries(
      SHARE_ACTION_AUTHORITY_REJECTION_KEYS.map((key) => [key, 0]),
    ),
  };
  let lastCandidateFingerprint = null;

  while (monotonicMilliseconds() < deadlineMilliseconds) {
    diagnostics.polls += 1;
    let loaded = null;
    const remainingReadMilliseconds = deadlineMilliseconds - monotonicMilliseconds();
    if (!Number.isFinite(remainingReadMilliseconds) || remainingReadMilliseconds <= 0) break;
    const readAbortController = new AbortController();
    const cancelScheduledReadAbort = scheduleReadAbort(
      readAbortController,
      Math.max(1, Math.ceil(remainingReadMilliseconds)),
    );
    if (typeof cancelScheduledReadAbort !== "function") {
      readAbortController.abort();
      throw new Error("the fresh share action authority read-abort policy is incomplete");
    }
    try {
      loaded = await loadCandidate({
        signal: readAbortController.signal,
        remainingMilliseconds: remainingReadMilliseconds,
      });
    } catch {
      if (readAbortController.signal.aborted) diagnostics.readAborts += 1;
      else diagnostics.readErrors += 1;
      // Transient public-state read failures are retried only inside the same
      // monotonic bound. The timeout diagnostic records counts only, never
      // error text or target/share identities.
    } finally {
      cancelScheduledReadAbort();
    }
    // A response that settles at or after the absolute monotonic deadline is
    // never accepted, even when its embedded wall-clock capture age is fresh.
    if (
      readAbortController.signal.aborted ||
      monotonicMilliseconds() >= deadlineMilliseconds
    ) break;
    const candidate = loaded?.authority ?? null;
    if (candidate) {
      const decisionNowMilliseconds = wallClockMilliseconds();
      const decision = decideShareActionAuthorityRefresh(
        reference,
        candidate,
        decisionNowMilliseconds,
      );
      if (decision.accepted) {
        return {
          ...loaded,
          decision,
          diagnostics: {
            ...diagnostics,
            rejectionCounts: { ...diagnostics.rejectionCounts },
          },
        };
      }
      const candidateFingerprint = [
        candidate.frameId,
        candidate.capturedAt,
        candidate.shareId,
        candidate.topLevelShareId,
        candidate.sourceSequence,
        candidate.topLevelSourceSequence,
        candidate.sequence,
        candidate.windowId,
        candidate.pid,
        candidate.windowTitle,
        candidate.windowWidth,
        candidate.windowHeight,
        candidate.imageWidth,
        candidate.imageHeight,
      ].join("\u0000");
      if (candidateFingerprint !== lastCandidateFingerprint) {
        diagnostics.distinctCandidates += 1;
        lastCandidateFingerprint = candidateFingerprint;
        if (decision.estimatedAgeMs !== null) {
          diagnostics.minimumObservedAgeMs = diagnostics.minimumObservedAgeMs === null
            ? decision.estimatedAgeMs
            : Math.min(diagnostics.minimumObservedAgeMs, decision.estimatedAgeMs);
        }
        for (const reason of shareActionAuthorityRefreshRejectionReasons(
          reference,
          candidate,
          decision.estimatedAgeMs,
        )) {
          diagnostics.rejectionCounts[reason] += 1;
        }
      }
    }
    const remainingMilliseconds = deadlineMilliseconds - monotonicMilliseconds();
    if (remainingMilliseconds < 1) break;
    await pause(Math.min(sampleIntervalMs, Math.max(1, Math.ceil(remainingMilliseconds))));
  }
  throw freshShareActionAuthorityTimeoutError(diagnostics);
}

function freshShareActionAuthorityTimeout(remainingMilliseconds) {
  if (!Number.isSafeInteger(remainingMilliseconds)) return null;
  const reservedMilliseconds = APP_SHARE_HANDOFF_COMPLETION_RESERVE_MS +
    APP_SHARE_HANDOFF_MINIMUM_ACTION_BUDGET_MS;
  const refreshBudgetMilliseconds = remainingMilliseconds - reservedMilliseconds;
  return refreshBudgetMilliseconds < 1
    ? null
    : Math.min(
      APP_SHARE_ACTION_AUTHORITY_REFRESH_MAXIMUM_WAIT_MS,
      refreshBudgetMilliseconds,
    );
}

async function waitForNextShareState(previousSequence, stage, predicate = () => true, timeoutMs = 20_000) {
  const snapshot = await waitFor(`${stage} share frame`, async () => {
    const candidate = await apiState();
    const observation = candidate.computerObservation;
    const sample = shareSample(observation, stage);
    return sample && sample.sequence > previousSequence && predicate(sample, observation) ? candidate : null;
  }, timeoutMs);
  const sample = shareSample(snapshot.computerObservation, stage);
  shareSamples.push(sample);
  return { snapshot, sample };
}

function validateShareSamples(samples) {
  requireCheck("persistent share sample count", samples.length >= 6, `${samples.length} distinct transport frames`);
  const shareIds = new Set(samples.map((sample) => sample.shareId));
  requireCheck("one persistent share authority", shareIds.size === 1, `${shareIds.size} share IDs`);

  for (const sample of samples) {
    requireCheck(`${sample.stage} native stream metadata`,
      sample.captureBackend === CAPTURE_BACKEND &&
        sample.nativeStream === true &&
        sample.systemIndicator === true &&
        sample.selectionMode === SELECTION_MODE,
      `${sample.captureBackend}, native=${sample.nativeStream}, indicator-policy=${sample.systemIndicator}, selection=${sample.selectionMode}`,
    );
    requireCheck(`${sample.stage} ack pacing metadata`,
      sample.ackPaced === true && sample.backpressure === "latest-frame-wins",
      `ack=${sample.ackPaced}, backpressure=${sample.backpressure}`,
    );
  }

  for (let index = 1; index < samples.length; index += 1) {
    const previous = samples[index - 1];
    const current = samples[index];
    requireCheck(`${current.stage} independent sequences advance`,
      current.sourceSequence > previous.sourceSequence && current.sequence > previous.sequence,
      `source ${previous.sourceSequence}->${current.sourceSequence}, transport ${previous.sequence}->${current.sequence}`,
    );
    requireCheck(`${current.stage} drop counters are monotonic`,
      current.sourceDroppedFrames >= previous.sourceDroppedFrames &&
        current.transportDroppedFrames >= previous.transportDroppedFrames,
      `source-drop ${previous.sourceDroppedFrames}->${current.sourceDroppedFrames}, transport-drop ${previous.transportDroppedFrames}->${current.transportDroppedFrames}`,
    );
    requireCheck(`${current.stage} previous frame was acknowledged`,
      current.lastAckedSequence >= previous.sequence && current.lastAckedSequence < current.sequence,
      `previous=${previous.sequence}, ack=${current.lastAckedSequence}, current=${current.sequence}`,
    );
  }
}

async function freshObserve(windowId) {
  const body = await command("computer.observe", { windowId });
  const observation = body.state.computerObservation;
  if (!observation || observation.windowId !== windowId || observation.windowTitle !== FIXTURE_TITLE) {
    throw new Error("computer.observe did not publish the exact fixture window");
  }
  return body;
}

function startHelperOnce(environment) {
  if (helperSpawnCount !== 0 || helperProcess) {
    throw new Error("the evidence rig refuses to spawn the packaged helper more than once");
  }
  helperSpawnCount += 1;
  helperProcess = spawn(helperPath, [], { env: environment, stdio: "ignore" });
  return helperProcess;
}

async function persistResult(result) {
  const serialized = `${JSON.stringify(result, null, 2)}\n`;
  assertNoToken(serialized, "machine-readable result");
  assertNoRetainedNativeTextPayload(serialized, "machine-readable result");
  assertNoRetainedPointerRawData(serialized, "machine-readable result");
  await writeFile(resultsPath, serialized, { encoding: "utf8", flag: "wx", mode: 0o600 });
}

async function preparePackage() {
  const outputParent = dirname(outputDir);
  const outputParentLinkState = await lstat(outputParent);
  const outputParentState = await stat(outputParent);
  requireCheck(
    "package output parent is an owner-private ordinary directory",
    outputParentLinkState.isDirectory() && !outputParentLinkState.isSymbolicLink() &&
      outputParentState.isDirectory() && (outputParentState.mode & 0o077) === 0 &&
      (typeof process.getuid !== "function" || outputParentState.uid === process.getuid()),
    "the direct package parent is ordinary, owner-private, and current-user owned",
  );
  try {
    await lstat(outputDir);
    throw new Error("fresh package output directory already exists");
  } catch (error) {
    if (error?.code !== "ENOENT") throw error;
  }

  const manifest = await bindCanonicalChecksumManifest(prepareExpectedManifestSha256);
  let packageOutputCreated = false;
  try {
    await mkdir(outputDir, { recursive: false, mode: 0o700 });
    packageOutputCreated = true;
    const outputLinkState = await lstat(outputDir);
    const outputState = await stat(outputDir);
    requireCheck(
      "package output directory is newly created and owner-private",
      outputLinkState.isDirectory() && !outputLinkState.isSymbolicLink() &&
        outputState.isDirectory() && (outputState.mode & 0o077) === 0 &&
        (typeof process.getuid !== "function" || outputState.uid === process.getuid()),
      "the preparer created the exact private package output directory",
    );
    const archiveExtraction = extractCandidateArchiveBounded(
      archivePath,
      outputDir,
      manifest.archiveSha256,
    );
    manifestBinding.archiveSha256 = archiveExtraction.archiveSha256;
    manifestBinding.archiveEntryMatched =
      archiveExtraction.archiveSha256 === manifest.archiveSha256;
    requireCheck(
      "archive checksum is bound by the canonical manifest",
      manifestBinding.archiveEntryMatched,
      archiveExtraction.archiveSha256,
    );
    console.log(JSON.stringify({
      schemaVersion: 1,
      productVersion: EXPECTED_VERSION,
      status: "prepared-package-without-candidate-execution",
      candidateBytesExecuted: false,
      outputCreatedFresh: true,
      outputOwnerPrivate: true,
      checksumManifestSha256: manifest.manifestSha256,
      archiveSha256: archiveExtraction.archiveSha256,
      entryCount: archiveExtraction.entryCount,
      compressedBytes: archiveExtraction.compressedBytes,
      uncompressedPayloadBytes: archiveExtraction.uncompressedPayloadBytes,
    }));
  } catch (error) {
    if (packageOutputCreated) {
      await rm(outputDir, { recursive: true, force: true });
    }
    throw error;
  }
}

async function main() {
  const outputParent = dirname(outputDir);
  const outputParentLinkState = await lstat(outputParent);
  const outputParentState = await stat(outputParent);
  requireCheck("evidence output parent is an owner-private ordinary directory",
    outputParentLinkState.isDirectory() && !outputParentLinkState.isSymbolicLink() &&
      outputParentState.isDirectory() && (outputParentState.mode & 0o077) === 0 &&
      (typeof process.getuid !== "function" || outputParentState.uid === process.getuid()),
    "the direct evidence parent is ordinary, owner-private, and current-user owned");
  const outputReservationStartedAtMilliseconds = Date.now();
  await mkdir(outputDir, { recursive: false, mode: 0o700 });
  outputReserved = true;
  if (pointerEvidenceLane === "deliberate-concurrency") {
    pointerHandoffRequestPublicationDeadlineMilliseconds =
      outputReservationStartedAtMilliseconds + REQUEST_PUBLICATION_MAXIMUM_WAIT_MS;
  }
  const outputState = await stat(outputDir);
  requireCheck("evidence output directory is newly created and owner-private",
    outputState.isDirectory() && (outputState.mode & 0o077) === 0 &&
      (typeof process.getuid !== "function" || outputState.uid === process.getuid()),
    "the runner created one private lane output directory");
  verifyHarnessSourceBinding("pre-run");
  await access(scratchParent, fsConstants.W_OK);
  scratchDir = await mkdtemp(join(scratchParent, "lbb-v0.12.35-scstream-"));
  const fixtureBinary = join(scratchDir, "helper-evidence-fixture");
  systemProbeBinary = join(scratchDir, "system-probe");
  const physicalPointerHandoffBinary = join(scratchDir, "physical-pointer-handoff");
  pointerHandoffAppPath = join(scratchDir, "LBB App Share Handoff.app");
  const pointerHandoffContentsPath = join(pointerHandoffAppPath, "Contents");
  const pointerHandoffMacOSPath = join(pointerHandoffContentsPath, "MacOS");
  pointerHandoffBinary = join(pointerHandoffMacOSPath, "lbb-app-share-handoff");
  pointerHandoffControlPath = join(scratchDir, "pointer-handoff-state");
  fixtureStatePath = join(scratchDir, "fixture-state.json");
  const fixtureControlPath = join(scratchDir, "fixture-control.json");
  const archiveExtractRoot = join(scratchDir, "archive");
  const harnessSha256 = {
    runner: await sha256(rigSourcePath),
    fixture: await sha256(fixtureSource),
    systemProbe: await sha256(systemProbeSource),
    appShareHandoff: await sha256(pointerHandoffSource),
    physicalPointerHandoff: await sha256(physicalPointerHandoffSource),
    acceptanceFinalizer: await sha256(acceptanceFinalizerSource),
  };

  requireCheck("macOS host", run("uname", ["-s"]) === "Darwin", "Darwin");
  const serverSha256 = await sha256(serverPath);
  const helperSha256 = await sha256(helperPath);

  const manifest = await bindCanonicalChecksumManifest(expectedManifestSha256);
  const manifestSha256 = manifest.manifestSha256;
  await mkdir(archiveExtractRoot, { mode: 0o700 });
  const archiveExtraction = extractCandidateArchiveBounded(
    archivePath,
    archiveExtractRoot,
    manifest.archiveSha256,
  );
  const archiveSha256 = archiveExtraction.archiveSha256;
  manifestBinding.archiveSha256 = archiveSha256;
  manifestBinding.archiveEntryMatched = manifest.archiveSha256 === archiveSha256;
  requireCheck(
    "archive checksum is bound by the canonical manifest",
    manifestBinding.archiveEntryMatched,
    archiveSha256,
  );
  const archivedServerPath = join(archiveExtractRoot, "local-browser-bridge");
  const archivedHelperPath = join(archiveExtractRoot, "Local Computer Helper.app", "Contents", "MacOS", "local-computer-helper");
  requireCheck("supplied server is archive-exact", (await sha256(archivedServerPath)) === serverSha256, serverSha256);
  requireCheck("supplied helper is archive-exact", (await sha256(archivedHelperPath)) === helperSha256, helperSha256);

  // Candidate parsing and the first invocation of either supplied executable
  // are delayed until the out-of-band manifest, canonical archive checksum,
  // traversal-safe extraction, and byte-for-byte binding have all passed.
  const serverArchitectures = architectures(serverPath);
  const helperArchitectures = architectures(helperPath);
  requireCheck("server universal architecture", serverArchitectures.join(",") === "arm64,x86_64", serverArchitectures.join(","));
  requireCheck("helper universal architecture", helperArchitectures.join(",") === "arm64,x86_64", helperArchitectures.join(","));

  const helperApp = dirname(dirname(dirname(helperPath)));
  requireCheck("helper uses packaged app layout", basename(helperApp) === "Local Computer Helper.app", basename(helperApp));
  run("codesign", ["--verify", "--strict", serverPath]);
  run("codesign", ["--verify", "--strict", helperPath]);
  run("codesign", ["--verify", "--deep", "--strict", helperApp]);
  check("strict code-signature verification", true, "server, helper executable, and helper app bundle passed strict verification");
  const bundleVersion = run("plutil", ["-extract", "CFBundleShortVersionString", "raw", "-o", "-", join(helperApp, "Contents", "Info.plist")]);
  const bundleBuildVersion = run("plutil", ["-extract", "CFBundleVersion", "raw", "-o", "-", join(helperApp, "Contents", "Info.plist")]);
  requireCheck("helper bundle version",
    bundleVersion === EXPECTED_VERSION && bundleBuildVersion === EXPECTED_VERSION,
    `${bundleVersion}/${bundleBuildVersion}`,
  );

  run("xcrun", ["swiftc", fixtureSource, "-o", fixtureBinary]);
  run("xcrun", ["swiftc", systemProbeSource, "-o", systemProbeBinary]);
  run("xcrun", [
    "swiftc", physicalPointerHandoffSource, "-o", physicalPointerHandoffBinary,
  ]);
  requireCheck(
    "optional physical-pointer adversarial prompt self-test",
    run(physicalPointerHandoffBinary, ["--self-test"]) ===
      "macOS pointer handoff prompt self-test passed",
    "source-only optional adversarial prompt cannot satisfy either release lane",
  );
  await mkdir(pointerHandoffMacOSPath, { recursive: true, mode: 0o700 });
  await writeFile(
    join(pointerHandoffContentsPath, "Info.plist"),
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
      `<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n` +
      `<plist version="1.0"><dict>` +
      `<key>CFBundleDisplayName</key><string>LBB App Share Handoff</string>` +
      `<key>CFBundleExecutable</key><string>lbb-app-share-handoff</string>` +
      `<key>CFBundleIdentifier</key><string>${APP_SHARE_BUNDLE_IDENTIFIER}</string>` +
      `<key>CFBundleName</key><string>LBB App Share Handoff</string>` +
      `<key>CFBundlePackageType</key><string>APPL</string>` +
      `<key>CFBundleShortVersionString</key><string>${EXPECTED_VERSION}</string>` +
      `<key>CFBundleVersion</key><string>${EXPECTED_VERSION}</string>` +
      `<key>LSUIElement</key><true/>` +
      `</dict></plist>\n`,
    { encoding: "utf8", flag: "wx", mode: 0o600 },
  );
  run("xcrun", ["swiftc", pointerHandoffSource, "-o", pointerHandoffBinary]);
  run("codesign", [
    "--force", "--sign", "-", "--identifier", APP_SHARE_BUNDLE_IDENTIFIER,
    pointerHandoffAppPath,
  ]);
  run("codesign", ["--verify", "--deep", "--strict", pointerHandoffAppPath]);
  requireCheck(
    "app-share handoff bundle identity",
    run("plutil", [
      "-extract", "CFBundleIdentifier", "raw", "-o", "-",
      join(pointerHandoffContentsPath, "Info.plist"),
    ]) === APP_SHARE_BUNDLE_IDENTIFIER,
    APP_SHARE_BUNDLE_IDENTIFIER,
  );
  requireCheck(
    "app-share handoff self-test",
    runExactLine(
      pointerHandoffBinary,
      ["--self-test"],
      "macOS app-share handoff self-test passed",
    ),
    "stable bundle/window/button vocabulary and create-once receipts",
  );
  const permissionProbe = processProbe(systemProbeBinary);
  requireCheck("screen-capture permission preflight", permissionProbe.screenCaptureReady === true, "preexisting permission; no request was made");
  requireCheck("accessibility permission preflight", permissionProbe.accessibilityReady === true, "preexisting permission; no request was made");
  requireCheck("active Space probe available", permissionProbe.activeSpace > 0, "nonzero active Space identity");
  requireCheck("sandwiched foreground AX focus probe available",
    permissionProbe.foregroundProbeHealthy === true &&
      permissionProbe.foregroundTransitionObserved === false &&
      permissionProbe.foregroundAXProbeHealthy === true &&
      permissionProbe.activeSpaceProbeHealthy === true &&
      permissionProbe.pointerActivityMonitorHealthy === true &&
      permissionProbe.foregroundPID > 0 && permissionProbe.frontWindowID > 0 &&
      permissionProbe.foregroundIdentityStable === true &&
      permissionProbe.rawForegroundIdentityStable === true &&
      permissionProbe.rawForegroundPID === permissionProbe.foregroundPID &&
      typeof permissionProbe.rawForegroundPSN === "string" &&
      permissionProbe.rawForegroundPSN.length === 16 &&
      permissionProbe.foregroundAXFocusedWindowID > 0 &&
      permissionProbe.foregroundAXMainWindowID === permissionProbe.foregroundAXFocusedWindowID &&
      permissionProbe.foregroundAXFrontmost === true,
    "stable NSWorkspace/raw PSN plus exact AX focused/main-window identities are available",
  );

  const quietSeatStartedAtMilliseconds = Date.now();
  try {
    const stabilized = await runNativeQuietSeatStabilization({
      baseline: permissionProbe,
      waitDeadlineMilliseconds:
        quietSeatStartedAtMilliseconds + QUIET_SEAT_MAXIMUM_WAIT_MS,
      maximumWaitMilliseconds: QUIET_SEAT_MAXIMUM_WAIT_MS,
      nowMilliseconds: () => Date.now(),
      probe: async (timeoutMilliseconds) =>
        processProbe(systemProbeBinary, null, null, timeoutMilliseconds),
      onSummary: (summary) => {
        quietSeatStabilization = summary;
      },
    });
    quietSeatStabilization = stabilized.summary;
  } catch (error) {
    if (error?.quietSeatStabilization) {
      quietSeatStabilization = { ...error.quietSeatStabilization };
    }
    throw error;
  }
  requireCheck(
    "native quiet-seat stabilization completed before candidate execution",
    quietSeatStabilization.completed === true &&
      quietSeatStabilization.completedBeforeCandidateExecution === true &&
      quietSeatStabilization.stableDurationMilliseconds >=
        QUIET_SEAT_REQUIRED_STABLE_MS &&
      quietSeatStabilization.stableTransitions >=
        QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS &&
      quietSeatStabilization.monitoringUnknown === false,
    `${quietSeatStabilization.stableDurationMilliseconds} ms across ` +
      `${quietSeatStabilization.stableTransitions} stable native transitions after ` +
      `${quietSeatStabilization.resetCount} reset(s)`,
  );

  // This timestamp and the first candidate invocation occur only after the
  // lane's native seat gate. Deliberate-concurrency retains its separate pointer
  // handoff and unchanged per-action/whole-run fail-closed proofs after this gate.
  laneStartedAt = new Date().toISOString();
  requireCheck("server version", exactVersion(serverPath, "local-browser-bridge") === EXPECTED_VERSION, EXPECTED_VERSION);
  requireCheck("helper version", exactVersion(helperPath, "local-computer-helper") === EXPECTED_VERSION, EXPECTED_VERSION);

  fixtureProcess = spawn(fixtureBinary, [], {
    env: childEnvironment({
      LBB_FIXTURE_STATE: fixtureStatePath,
      LBB_FIXTURE_CONTROL: fixtureControlPath,
      LBB_FIXTURE_EVIDENCE_LANE: pointerEvidenceLane,
    }),
    stdio: "ignore",
  });
  const fixtureReady = await waitFor("fixture state", async () => {
    try {
      const snapshot = await fixtureState(fixtureStatePath);
      return snapshot.evidenceLane === pointerEvidenceLane &&
        snapshot.lastAction === "ready" && snapshot.animationTick >= 2 &&
        Number.isSafeInteger(snapshot.primaryWindowId) && snapshot.primaryWindowId > 0 &&
        Number.isSafeInteger(snapshot.siblingWindowId) && snapshot.siblingWindowId > 0 &&
        snapshot.primaryWindowId !== snapshot.siblingWindowId &&
        snapshot.siblingTextLength === 0 && snapshot.siblingClicks === 0 && snapshot;
    } catch {
      return null;
    }
  });
  requireCheck("fixture process identity", fixtureReady.pid === fixtureProcess.pid, "state belongs to the spawned fixture");
  fixtureTargetPid = fixtureReady.pid;
  fixtureSiblingWindowId = fixtureReady.siblingWindowId;
  const systemBefore = processProbe(systemProbeBinary);
  const startupSiblingFocus = processProbe(systemProbeBinary, fixtureReady.pid);
  requireCheck("startup same-PID sibling is the target app's remembered receiver",
    exactTargetReceiverMatches(startupSiblingFocus, fixtureReady.siblingWindowId, false) &&
      systemBefore.foregroundPID !== fixtureReady.pid,
    `sibling-main-and-focused=${exactTargetReceiverMatches(startupSiblingFocus, fixtureReady.siblingWindowId, false)}, fixture-background=${systemBefore.foregroundPID !== fixtureReady.pid}`,
  );

  port = await freePort();
  const serverEnvironment = childEnvironment({
    LBB_DISABLE_UPDATE_CHECK: "true",
    LBB_PORT: String(port),
    LBB_TOKEN: bearerToken,
  });
  serverProcess = spawn(serverPath, ["--no-update-check"], { env: serverEnvironment, stdio: "ignore" });
  delete serverEnvironment.LBB_TOKEN;
  await waitFor("server health", healthReachable);

  const helperEnvironment = childEnvironment({
    LBB_DISABLE_UPDATE_CHECK: "true",
    LBB_PORT: String(port),
    LBB_TOKEN: bearerToken,
  });
  startHelperOnce(helperEnvironment);
  delete helperEnvironment.LBB_TOKEN;

  const connected = await waitFor("packaged helper handshake", async () => {
    const snapshot = await apiState();
    return snapshot.computerConnected === true && snapshot.computer?.version === EXPECTED_VERSION && snapshot;
  });
  const hello = connected.computer;
  capabilityBinding.inputDeliveryProvenanceV1 =
    hello.capabilities.includes("computer.input-delivery-provenance.v1");
  capabilityBinding.pointerActivityMonitorV1 =
    hello.capabilities.includes("computer.pointer-activity-monitor.v1");
  requireCheck("packaged helper compatible", hello.compatible === true, `protocol-compatible helper ${hello.version}`);
  requireCheck("single helper spawn", helperSpawnCount === 1, `${helperSpawnCount} packaged helper process`);
  requireCheck("acknowledged share advertised", hello.capabilities.includes("computer.share.ack"), "computer.share.ack present");
  requireCheck("native stream advertised", hello.capabilities.includes("computer.capture.native-stream.v1"), "computer.capture.native-stream.v1 present");
  requireCheck("input-delivery provenance advertised",
    capabilityBinding.inputDeliveryProvenanceV1,
    "computer.input-delivery-provenance.v1 present",
  );
  requireCheck("pointer-activity monitor advertised",
    capabilityBinding.pointerActivityMonitorV1,
    "computer.pointer-activity-monitor.v1 present",
  );

  const statusBody = await command("computer.status");
  const status = statusBody.result;
  requireCheck("helper status backend", status.backend === STATUS_BACKEND, status.backend);
  requireCheck("background shared-session mode",
    status.sessionMode === "background-window" && status.isolation === "shared-user-session/target-routed",
    `${status.sessionMode}/${status.isolation}`,
  );
  requireCheck("background input backend ready", status.inputReady === true, "target-routed input is ready");
  requireCheck("semantic backend ready", status.semanticReady === true, "macos-accessibility is ready without prompting");
  const targetWindows = status.windows.filter((window) => window.title === FIXTURE_TITLE && window.pid === fixtureReady.pid);
  requireCheck("fixture exact window discovered", targetWindows.length === 1, `${targetWindows.length} matching windows`);
  const targetWindow = targetWindows[0];
  const siblingWindows = status.windows.filter(
    (window) => window.title === SIBLING_FIXTURE_TITLE && window.pid === fixtureReady.pid,
  );
  requireCheck("genuine same-PID sibling window discovered",
    siblingWindows.length === 1 && siblingWindows[0].id !== targetWindow.id &&
      siblingWindows[0].pid === targetWindow.pid &&
      String(fixtureReady.primaryWindowId) === targetWindow.id &&
      String(fixtureReady.siblingWindowId) === siblingWindows[0].id,
    `${siblingWindows.length} sibling matches, distinct=${siblingWindows[0]?.id !== targetWindow.id}`,
  );
  const siblingWindow = siblingWindows[0];
  requireCheck("fixture is a background target",
    targetWindow.focused === false && siblingWindow.focused === false &&
      systemBefore.foregroundPID !== fixtureReady.pid,
    "neither same-PID fixture window owns the user's foreground process or front window",
  );

  let observed = await freshObserve(targetWindow.id);
  let observation = observed.state.computerObservation;
  requireCheck("exact fixture window observed",
    observation.windowId === targetWindow.id && observation.windowTitle === FIXTURE_TITLE && observation.pid === fixtureReady.pid,
    `${observation.imageWidth}x${observation.imageHeight}`,
  );
  requireCheck("semantic snapshot available", observation.semanticAvailable === true && observation.elements.length > 0, `${observation.elements.length} elements`);
  const observeScreenshot = await saveCurrentScreenshot(observed.state, "computer-01-exact-window-observe.png", targetWindow.id);

  const semanticField = observation.elements.find(
    (element) => element.name === "Semantic value" && element.actions.includes("setValue"),
  );
  requireCheck("semantic value element discovered", Boolean(semanticField), semanticField?.role || "missing");
  const setValueBody = await command("computer.setValue", {
    frameId: observation.frameId,
    elementRef: semanticField.ref,
    value: SEMANTIC_VALUE,
  });
  const setValue = actionSummary(setValueBody);
  requireCheck("semantic setValue confirmed",
    setValue.effect === "Confirmed" && setValue.backendEffect?.postcondition === "value-confirmed",
    `${setValue.effect}/${setValue.backendEffect?.postcondition}`,
  );
  requireActionInvariants("semantic setValue", setValue);
  await waitFor("fixture semantic value", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.semanticValue === SEMANTIC_VALUE && snapshot;
  });
  observed = await freshObserve(targetWindow.id);
  observation = observed.state.computerObservation;
  const setValueScreenshot = await saveCurrentScreenshot(observed.state, "computer-02-semantic-set-value.png", targetWindow.id);
  requireCheck("semantic setValue screenshot changed",
    setValueScreenshot.sha256 !== observeScreenshot.sha256,
    "fresh exact-window pixels differ from the pre-action observation",
  );

  const semanticButton = observation.elements.find(
    (element) => element.name === "Semantic action" && element.actions.includes("press"),
  );
  requireCheck("semantic action element discovered", Boolean(semanticButton), semanticButton?.role || "missing");
  const invokeBody = await command("computer.invoke", {
    frameId: observation.frameId,
    elementRef: semanticButton.ref,
    action: "press",
  });
  const invoke = actionSummary(invokeBody);
  const semanticFixtureState = await waitFor("fixture semantic action", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.semanticPresses === 1 && snapshot;
  });
  requireCheck("semantic invoke target-side proof", semanticFixtureState.lastAction === "semantic", "fixture semantic counter advanced to 1");
  requireCheck("semantic invoke conservatively classified",
    invoke.effect === "Partial" && invoke.backendEffect?.effectObserved === true &&
      !invoke.evidence.some((item) => item.supportsConfirmation === true),
    `${invoke.effect}/${invoke.backendEffect?.postcondition}`,
  );
  requireActionInvariants("semantic invoke", invoke);
  observed = await freshObserve(targetWindow.id);
  observation = observed.state.computerObservation;
  const invokeScreenshot = await saveCurrentScreenshot(observed.state, "computer-03-semantic-invoke.png", targetWindow.id);
  requireCheck("semantic invoke screenshot changed",
    invokeScreenshot.sha256 !== setValueScreenshot.sha256,
    "fresh exact-window pixels differ from the setValue stage",
  );

  const shareStartBody = await command("computer.share.start", {
    windowId: targetWindow.id,
    fps: SHARE_FPS,
  });
  const shareStart = shareStartBody.result;
  requireCheck("persistent live share started",
    shareStart.active === true && shareStart.captureScope === "exact-window" &&
      shareStart.captureMode === "persistent-native-stream",
    `${shareStart.captureMode}/${shareStart.captureScope}`,
  );
  requireCheck("SCStream start metadata",
    shareStart.captureBackend === CAPTURE_BACKEND && shareStart.nativeStream === true &&
      shareStart.systemIndicator === true && shareStart.selectionMode === SELECTION_MODE,
    `${shareStart.captureBackend}, native=${shareStart.nativeStream}, indicator-policy=${shareStart.systemIndicator}, selection=${shareStart.selectionMode}`,
  );
  requireCheck("live share ack pacing",
    shareStart.ackPaced === true && shareStart.backpressure === "latest-frame-wins",
    `${shareStart.backpressure}, ackPaced=${shareStart.ackPaced}`,
  );
  requireCheck("live share cursor policy", shareStart.cursorComposited === true, "system cursor excluded; synthetic session pointer composited");

  let current = await waitForNextShareState(0, "share-initial");
  const firstShareId = current.sample.shareId;
  const shareStartScreenshot = await saveCurrentScreenshot(current.snapshot, "computer-04-persistent-scstream-start.png", targetWindow.id);
  requireCheck("persistent-share screenshot is fresh",
    shareStartScreenshot.sha256 !== invokeScreenshot.sha256 && shareStartScreenshot.sourceSequence > 0,
    `sourceSequence=${shareStartScreenshot.sourceSequence}`,
  );
  for (const stage of ["share-cadence-1", "share-cadence-2"]) {
    current = await waitForNextShareState(current.sample.sequence, stage);
  }

  const beforePixelAction = current.sample;
  observation = current.snapshot.computerObservation;
  const clickX = Math.round(observation.imageWidth / 2);
  const clickY = Math.max(0, observation.imageHeight - 80);
  const pixelSystemBefore = processProbe(systemProbeBinary);
  const pixelFixtureBefore = await fixtureState(fixtureStatePath);
  const pixelSiblingFocusBefore = processProbe(systemProbeBinary, fixtureReady.pid);
  requireCheck("same-PID sibling is remembered before primary pixel dispatch",
    exactTargetReceiverMatches(pixelSiblingFocusBefore, Number(siblingWindow.id), false) &&
      pixelFixtureBefore.siblingTextLength === 0 && pixelFixtureBefore.siblingClicks === 0,
    `sibling-main-and-focused=${exactTargetReceiverMatches(pixelSiblingFocusBefore, Number(siblingWindow.id), false)}, sibling-state-clean=${pixelFixtureBefore.siblingTextLength === 0 && pixelFixtureBefore.siblingClicks === 0}`,
  );
  failureProbeBaseline = {
    stage: "liveSharePixelAction",
    system: pixelSystemBefore,
    fixture: pixelFixtureBefore,
    targetSiblingExpectedAfter: true,
  };
  const activeProbePromise = processProbeWaitingForActive(
    systemProbeBinary,
    fixtureReady.pid,
    Number(targetWindow.id),
    8_000,
  );
  await delay(20);
  const clickPromise = command("computer.click", {
    frameId: observation.frameId,
    x: clickX,
    y: clickY,
    button: "left",
    clickCount: 1,
    durationMs: LONG_PIXEL_ACTION_MS,
  }).then((body) => ({ body }), (error) => ({ error }));
  const [activeRequestedReceiver, clickOutcome] = await Promise.all([
    activeProbePromise,
    clickPromise,
  ]);
  const activeUserOwnerHeld =
    activeRequestedReceiver.foregroundIdentityStable === true &&
    activeRequestedReceiver.rawForegroundIdentityStable === true &&
    activeRequestedReceiver.foregroundPID === pixelSystemBefore.foregroundPID &&
    activeRequestedReceiver.rawForegroundPID === pixelSystemBefore.rawForegroundPID &&
    activeRequestedReceiver.rawForegroundPSN === pixelSystemBefore.rawForegroundPSN &&
    activeRequestedReceiver.foregroundAXFocusedWindowID === pixelSystemBefore.foregroundAXFocusedWindowID &&
    activeRequestedReceiver.foregroundAXMainWindowID === pixelSystemBefore.foregroundAXMainWindowID;
  requireCheck("exact active target receiver observed during pixel lease",
    activeRequestedReceiver.activeTargetObserved === true &&
      exactTargetReceiverMatches(activeRequestedReceiver, Number(targetWindow.id), true) &&
      activeUserOwnerHeld,
    `active-target=${activeRequestedReceiver.activeTargetObserved === true}, exact-main-and-focused=${exactTargetReceiverMatches(activeRequestedReceiver, Number(targetWindow.id), true)}, saved-user-owner=${activeUserOwnerHeld}`,
  );
  if (clickOutcome.error) throw clickOutcome.error;
  const clickBody = clickOutcome.body;
  const click = actionSummary(clickBody);
  const clickedFixtureState = await waitFor("fixture pixel click", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.clicks === 1 &&
      snapshot.siblingTextLength === pixelFixtureBefore.siblingTextLength &&
      snapshot.siblingClicks === pixelFixtureBefore.siblingClicks && snapshot;
  });
  const pixelSiblingFocusAfter = await waitFor("same-PID sibling restoration after pixel action", () => {
    const snapshot = processProbe(systemProbeBinary, fixtureReady.pid);
    return exactTargetReceiverMatches(snapshot, Number(siblingWindow.id), false) ? snapshot : null;
  });
  requireCheck("background pixel click targets only primary and restores sibling receiver",
    clickedFixtureState.lastAction === "click" &&
      clickedFixtureState.clicks === pixelFixtureBefore.clicks + 1 &&
      clickedFixtureState.siblingClicks === pixelFixtureBefore.siblingClicks &&
      clickedFixtureState.siblingTextLength === pixelFixtureBefore.siblingTextLength &&
      exactTargetReceiverMatches(pixelSiblingFocusAfter, Number(siblingWindow.id), false),
    "only the primary click counter advanced and the target app's prior sibling receiver was restored",
  );
  requireCheck("pixel click conservatively classified", click.effect === "Unverifiable", click.effect);
  requireCheck("pixel action bound to persistent share",
    click.shareId === firstShareId && click.sourceSequence === beforePixelAction.sourceSequence,
    `share=${click.shareId === firstShareId}, source=${click.sourceSequence}`,
  );
  requireActionInvariants("live-share pixel click", click);
  const pixelSystemInvariants = systemInvariants(
    pixelSystemBefore,
    processProbe(systemProbeBinary),
  );
  requireIndependentInvariants("same-PID pixel routing", pixelSystemInvariants);

  current = await waitForNextShareState(beforePixelAction.sequence, "share-after-long-action", (sample) =>
    sample.sourceDroppedFrames > beforePixelAction.sourceDroppedFrames,
  );
  const afterPixelAction = current.sample;
  requireCheck("native source remained live during input",
    afterPixelAction.sourceSequence - beforePixelAction.sourceSequence >
      afterPixelAction.sequence - beforePixelAction.sequence &&
      afterPixelAction.sourceDroppedFrames > beforePixelAction.sourceDroppedFrames,
    `source +${afterPixelAction.sourceSequence - beforePixelAction.sourceSequence}/drop +${afterPixelAction.sourceDroppedFrames - beforePixelAction.sourceDroppedFrames}; transport +${afterPixelAction.sequence - beforePixelAction.sequence}`,
  );
  requireCheck("settled synthetic pointer published",
    current.snapshot.computerObservation.pointer?.visible === true &&
      current.snapshot.computerObservation.pointer?.action === "click" &&
      current.snapshot.computerObservation.pointer?.windowId === targetWindow.id,
    `action=${current.snapshot.computerObservation.pointer?.action}`,
  );
  const pixelActionScreenshot = await saveCurrentScreenshot(current.snapshot, "computer-05-live-share-pixel-action.png", targetWindow.id);
  requireCheck("live-share action screenshot changed",
    pixelActionScreenshot.sha256 !== shareStartScreenshot.sha256,
    "fresh exact-window pixels include the action stage",
  );

  failureProbeBaseline = {
    stage: "persistentShareResize",
    system: processProbe(systemProbeBinary),
    fixture: await fixtureState(fixtureStatePath),
    targetSiblingExpectedAfter: true,
  };
  const preResizeWindowWidth = current.snapshot.computerObservation.screenWidth;
  const preResizeWindowHeight = current.snapshot.computerObservation.screenHeight;
  await writeFile(fixtureControlPath, `${JSON.stringify({
    sequence: 1,
    action: "resize",
    contentWidth: 820,
    contentHeight: 520,
  })}\n`);
  const resizedFixtureState = await waitFor("fixture resize", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.appliedControlSequence === 1 && snapshot.resizeCount === 1 &&
      snapshot.contentWidth === 820 && snapshot.contentHeight === 520 && snapshot;
  });
  const resizeTransition = await waitForNextShareState(afterPixelAction.sequence, "share-after-resize", (sample) =>
    sample.shareId === firstShareId &&
      (sample.windowWidth !== preResizeWindowWidth || sample.windowHeight !== preResizeWindowHeight),
  );
  current = await waitForNextShareState(
    resizeTransition.sample.sequence,
    "share-resize-settled",
    (sample) =>
      sample.shareId === firstShareId &&
      sample.sourceSequence > resizeTransition.sample.sourceSequence &&
      sample.windowWidth === resizeTransition.sample.windowWidth &&
      sample.windowHeight === resizeTransition.sample.windowHeight &&
      capturedFrameMatchesWindowGeometry(sample),
  );
  requireCheck("persistent share survived exact-target resize",
    current.sample.shareId === firstShareId && current.sample.sourceSequence > afterPixelAction.sourceSequence,
    `${preResizeWindowWidth}x${preResizeWindowHeight} -> ${current.sample.windowWidth}x${current.sample.windowHeight}`,
  );
  requireCheck("settled resize frame matches exact-window geometry",
    capturedFrameMatchesWindowGeometry(current.sample),
    `window=${current.sample.windowWidth}x${current.sample.windowHeight}, image=${current.sample.imageWidth}x${current.sample.imageHeight}`,
  );
  const resizeScreenshot = await saveCurrentScreenshot(current.snapshot, "computer-06-persistent-share-resize.png", targetWindow.id);
  requireCheck("resize screenshot dimensions match settled observation",
    resizeScreenshot.width === current.sample.imageWidth &&
      resizeScreenshot.height === current.sample.imageHeight,
    `${resizeScreenshot.width}x${resizeScreenshot.height}`,
  );
  requireCheck("resize screenshot geometry changed",
    resizeScreenshot.width !== pixelActionScreenshot.width ||
      resizeScreenshot.height !== pixelActionScreenshot.height,
    `${pixelActionScreenshot.width}x${pixelActionScreenshot.height} -> ${resizeScreenshot.width}x${resizeScreenshot.height}`,
  );
  current = await waitForNextShareState(current.sample.sequence, "share-post-resize-cadence");

  const preHandoffFixture = await fixtureState(fixtureStatePath);
  let postResizeFixtureBefore = preHandoffFixture;
  let actionPromptBaseline = null;
  let receiptBoundShareActionAuthority = null;
  let postHandoffShareActionAuthority = null;
  if (pointerEvidenceLane === "deliberate-concurrency") {
    const handoffPresentationBefore = processProbe(systemProbeBinary);
    failureProbeBaseline = {
      stage: "waitExactAppShareAction",
      actionDispatched: false,
      system: handoffPresentationBefore,
      fixture: preHandoffFixture,
      targetSiblingExpectedAfter: true,
    };
    const armedBaseline = await startPointerHandoff(handoffPresentationBefore);
    failureProbeBaseline.system = armedBaseline;
    actionPromptBaseline = await waitForDeliberatePointerActivity(armedBaseline);
    failureProbeBaseline = {
      stage: "refreshExactShareActionAuthority",
      actionDispatched: false,
      system: actionPromptBaseline,
      fixture: preHandoffFixture,
      targetSiblingExpectedAfter: true,
    };
    const armedShareSnapshot = await apiState();
    const armedShareObservation = armedShareSnapshot.computerObservation;
    const armedShareSample = shareSample(armedShareObservation, "share-at-app-share-arm");
    const armedShareAuthority = shareActionAuthority(armedShareObservation, armedShareSample);
    receiptBoundShareActionAuthority = armedShareAuthority;
    requireCheck(
      "app-share receipt retained the exact persistent share",
      armedShareAuthority !== null &&
        armedShareAuthority.windowId === targetWindow.id &&
        armedShareAuthority.pid === fixtureReady.pid &&
        armedShareAuthority.windowTitle === FIXTURE_TITLE &&
        armedShareAuthority.shareId === firstShareId &&
        armedShareAuthority.topLevelShareId === firstShareId &&
        armedShareAuthority.topLevelSourceSequence === armedShareAuthority.sourceSequence &&
        armedShareAuthority.windowWidth === current.sample.windowWidth &&
        armedShareAuthority.windowHeight === current.sample.windowHeight &&
        armedShareAuthority.imageWidth === current.sample.imageWidth &&
        armedShareAuthority.imageHeight === current.sample.imageHeight &&
        capturedFrameMatchesWindowGeometry(armedShareAuthority),
      `target=${armedShareAuthority?.windowId === targetWindow.id && armedShareAuthority?.pid === fixtureReady.pid && armedShareAuthority?.windowTitle === FIXTURE_TITLE}, share=${armedShareAuthority?.shareId === firstShareId && armedShareAuthority?.topLevelShareId === firstShareId}, source-bound=${armedShareAuthority?.topLevelSourceSequence === armedShareAuthority?.sourceSequence}, geometry=${armedShareAuthority?.windowWidth === current.sample.windowWidth && armedShareAuthority?.windowHeight === current.sample.windowHeight && armedShareAuthority?.imageWidth === current.sample.imageWidth && armedShareAuthority?.imageHeight === current.sample.imageHeight}`,
    );
    const freshnessTimeoutMs = freshShareActionAuthorityTimeout(remainingPointerHandoffTime(
      pointerHandoffActionDeadlineMilliseconds,
      "fresh share action authority",
    ));
    if (freshnessTimeoutMs === null) {
      throw new Error("insufficient bounded time remained to refresh share action authority");
    }
    current = await waitForFreshShareActionAuthority({
      reference: armedShareAuthority,
      timeoutMs: freshnessTimeoutMs,
      loadCandidate: async ({ signal }) => {
        const snapshot = await apiState(signal);
        const candidateObservation = snapshot.computerObservation;
        const sample = shareSample(
          candidateObservation,
          "share-after-app-share-action-authority",
        );
        return sample
          ? {
            snapshot,
            sample,
            authority: shareActionAuthority(candidateObservation, sample),
          }
          : null;
      },
    });
    shareSamples.push(current.sample);
    const freshShareAuthority = current.authority;
    const refreshedAuthorityDecision = current.decision;
    postHandoffShareActionAuthority = freshShareAuthority;
    requireCheck(
      "post-handoff share action authority is fresh and exact",
      refreshedAuthorityDecision.accepted,
      `target=${freshShareAuthority?.windowId === targetWindow.id && freshShareAuthority?.pid === fixtureReady.pid && freshShareAuthority?.windowTitle === FIXTURE_TITLE}, share=${freshShareAuthority?.shareId === firstShareId && freshShareAuthority?.topLevelShareId === firstShareId}, source-advanced=${freshShareAuthority?.sourceSequence > armedShareAuthority.sourceSequence}, transport-advanced=${freshShareAuthority?.sequence > armedShareAuthority.sequence}, frame-rebased=${freshShareAuthority?.frameId !== armedShareAuthority.frameId}, estimatedAgeMs=${refreshedAuthorityDecision.estimatedAgeMs}`,
    );
    pointerHandoffAuthorityRefreshedAfterReceipt = true;
    postResizeFixtureBefore = await fixtureState(fixtureStatePath);
    requireCheck(
      "app-share handoff and frame refresh caused no target mutation",
      postResizeFixtureBefore.clicks === preHandoffFixture.clicks &&
        postResizeFixtureBefore.semanticPresses === preHandoffFixture.semanticPresses &&
        postResizeFixtureBefore.semanticValue === preHandoffFixture.semanticValue &&
        postResizeFixtureBefore.resizeCount === preHandoffFixture.resizeCount &&
        postResizeFixtureBefore.focusCount === preHandoffFixture.focusCount &&
        postResizeFixtureBefore.siblingTextLength === preHandoffFixture.siblingTextLength &&
        postResizeFixtureBefore.siblingClicks === preHandoffFixture.siblingClicks &&
        postResizeFixtureBefore.appliedControlSequence === preHandoffFixture.appliedControlSequence &&
        postResizeFixtureBefore.contentWidth === preHandoffFixture.contentWidth &&
        postResizeFixtureBefore.contentHeight === preHandoffFixture.contentHeight,
      "the exact target functional baseline stayed unchanged until fresh action authority was bound",
    );
  }
  // The deliberate lane returns only after the bound external button receipt,
  // an independently quiet shared-seat boundary, the ACTION presentation, and
  // a strictly newer same-share frame server-published after the post-receipt
  // read barrier. The fresh stream frame restores short-lived product action
  // authority without a one-shot observe, pausing capture, or weakening the
  // helper's age limit.
  const beforePostResizeAction = current.sample;
  observation = current.snapshot.computerObservation;
  const postResizeClickX = Math.round(observation.imageWidth / 2);
  const postResizeClickY = Math.max(0, observation.imageHeight - 80);
  const postResizeSystemBefore = pointerEvidenceLane === "deliberate-concurrency"
    ? actionPromptBaseline
    : processProbe(systemProbeBinary);
  failureProbeBaseline = {
    stage: "postResizePixelAction",
    actionDispatched: false,
    system: postResizeSystemBefore,
    fixture: postResizeFixtureBefore,
    targetSiblingExpectedAfter: true,
  };
  const postResizeActionDurationMs = pointerEvidenceLane === "deliberate-concurrency"
    ? APP_SHARE_CONCURRENCY_ACTION_MS
    : POST_RESIZE_PIXEL_ACTION_MS;
  const actionResponseTimeoutMs = pointerEvidenceLane === "deliberate-concurrency"
    ? remainingPointerHandoffTime(pointerHandoffActionDeadlineMilliseconds, "action") -
      APP_SHARE_HANDOFF_COMPLETION_RESERVE_MS
    : null;
  if (pointerEvidenceLane === "deliberate-concurrency") {
    if (
      !Number.isSafeInteger(actionResponseTimeoutMs) ||
      actionResponseTimeoutMs < APP_SHARE_HANDOFF_MINIMUM_ACTION_BUDGET_MS
    ) {
      throw new Error("insufficient bounded action time remained after the exact-app-share arm");
    }
    pointerHandoffActionDispatched = null;
    failureProbeBaseline.actionDispatched = null;
  }
  if (pointerEvidenceLane === "deliberate-concurrency") {
    const dispatchAuthorityDecision = decideShareActionAuthorityDispatch(
      receiptBoundShareActionAuthority,
      postHandoffShareActionAuthority,
      observation.frameId,
      Date.now(),
    );
    requireCheck(
      "post-handoff share action authority remained fresh at dispatch",
      dispatchAuthorityDecision.accepted,
      `frame-bound=${postHandoffShareActionAuthority?.frameId === observation.frameId}, estimatedAgeMs=${dispatchAuthorityDecision.estimatedAgeMs}`,
    );
    pointerHandoffAuthorityFreshAtDispatch = true;
  }
  const postResizeClickResponse = await commandResponse(
    "computer.click",
    {
      frameId: observation.frameId,
      x: postResizeClickX,
      y: postResizeClickY,
      button: "left",
      clickCount: 1,
      durationMs: postResizeActionDurationMs,
    },
    randomUUID(),
    actionResponseTimeoutMs,
  );
  if (!postResizeClickResponse.ok) {
    const responseCode = postResizeClickResponse.body.error?.code;
    if (pointerEvidenceLane === "deliberate-concurrency") {
      pointerHandoffActionDispatched =
        postResizeClickResponse.status === 400 && responseCode === "BAD_REQUEST"
          ? false
          : null;
      failureProbeBaseline.actionDispatched = pointerHandoffActionDispatched;
    }
    throw new Error(
      `computer.click returned HTTP ${postResizeClickResponse.status}: ${responseCode || "unknown"}`,
    );
  }
  const postResizeClickBody = postResizeClickResponse.body;
  const postResizeClick = actionSummary(postResizeClickBody);
  if (pointerEvidenceLane === "deliberate-concurrency") {
    pointerHandoffActionDispatched =
      postResizeClick.invariants?.inputDelivery?.dispatchAttemptRecorded === true;
    failureProbeBaseline.actionDispatched = pointerHandoffActionDispatched;
    requireCheck(
      "post-resize command returned a recorded dispatch attempt",
      pointerHandoffActionDispatched === true,
      "a returned response cannot complete the handoff without product dispatch evidence",
    );
  }
  const postActionProbeTimeoutMs = pointerEvidenceLane === "deliberate-concurrency"
    ? Math.min(
      SYSTEM_PROBE_TIMEOUT_MS,
      remainingPointerHandoffTime(pointerHandoffActionDeadlineMilliseconds, "post-action probe"),
    )
    : SYSTEM_PROBE_TIMEOUT_MS;
  const postResizeSystemAfter = pointerEvidenceLane === "deliberate-concurrency"
    ? processProbe(
      systemProbeBinary,
      null,
      { pid: pointerHandoffProcess.pid, state: POINTER_HANDOFF_ACTION_STATE },
      postActionProbeTimeoutMs,
    )
    : processProbe(systemProbeBinary);
  if (
    pointerEvidenceLane === "deliberate-concurrency" &&
    !pointerPromptDeliveryObserved(postResizeSystemAfter)
  ) {
    throw new Error("the app-share handoff was not independently bound immediately after dispatch");
  }
  const postResizeFixtureAfter = await waitFor("post-resize fixture pixel click", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.clicks === postResizeFixtureBefore.clicks + 1 && snapshot;
  });
  requireCheck("post-resize pixel click target-side proof",
    postResizeFixtureAfter.lastAction === "click" &&
      postResizeFixtureAfter.clicks === 2 &&
      postResizeFixtureAfter.semanticPresses === postResizeFixtureBefore.semanticPresses &&
      postResizeFixtureAfter.semanticValue === postResizeFixtureBefore.semanticValue &&
      postResizeFixtureAfter.resizeCount === postResizeFixtureBefore.resizeCount &&
      postResizeFixtureAfter.focusCount === postResizeFixtureBefore.focusCount &&
      postResizeFixtureAfter.appliedControlSequence === postResizeFixtureBefore.appliedControlSequence &&
      postResizeFixtureAfter.contentWidth === 820 && postResizeFixtureAfter.contentHeight === 520,
    "only the exact fixture click counter changed functional state after resize",
  );
  if (pointerEvidenceLane === "deliberate-concurrency") {
    pointerHandoffTargetPostconditionObserved = true;
  }
  requireCheck("post-resize action bound to settled persistent share",
    postResizeClick.frameId === observation.frameId &&
      postResizeClick.shareId === firstShareId &&
      postResizeClick.sourceSequence === beforePostResizeAction.sourceSequence,
    `frame=${postResizeClick.frameId === observation.frameId}, share=${postResizeClick.shareId === firstShareId}, source=${postResizeClick.sourceSequence}`,
  );
  requireCheck("post-resize pixel click conservatively classified",
    postResizeClick.effect === "Unverifiable",
    postResizeClick.effect,
  );
  requireActionInvariants("post-resize pixel click", postResizeClick);
  const postResizeActionInvariants = pointerEvidenceLane === "deliberate-concurrency"
    ? actionBoundSystemInvariants(postResizeSystemBefore, postResizeSystemAfter)
    : systemInvariants(postResizeSystemBefore, postResizeSystemAfter);
  requireIndependentInvariants("post-resize pixel action", postResizeActionInvariants);
  if (pointerEvidenceLane === "deliberate-concurrency") {
    pointerHandoffProductBoundaryQuiet = actionPointerLaneState(postResizeClick.invariants) === "quiet";
    pointerHandoffIndependentBoundaryQuiet =
      independentPointerLaneState(postResizeActionInvariants) === "quiet";
    requireCheck(
      "exact-app-share concurrency preserved both product and independent shared-seat boundaries",
      pointerHandoffProductBoundaryQuiet && pointerHandoffIndependentBoundaryQuiet &&
        pointerHandoffTargetPostconditionObserved,
      "both post-resize boundaries stayed quiet and the exact target postcondition advanced",
    );
    await completePointerHandoff(postResizeSystemAfter);
  }
  current = await waitForNextShareState(
    beforePostResizeAction.sequence,
    "share-after-post-resize-action",
    (sample) =>
      sample.shareId === firstShareId &&
      sample.sourceSequence > beforePostResizeAction.sourceSequence &&
      sample.windowWidth === beforePostResizeAction.windowWidth &&
      sample.windowHeight === beforePostResizeAction.windowHeight &&
      capturedFrameMatchesWindowGeometry(sample),
  );
  const afterPostResizeAction = current.sample;
  requireCheck("post-resize action retained resized exact-window stream",
    afterPostResizeAction.windowWidth === resizeTransition.sample.windowWidth &&
      afterPostResizeAction.windowHeight === resizeTransition.sample.windowHeight &&
      capturedFrameMatchesWindowGeometry(afterPostResizeAction),
    `window=${afterPostResizeAction.windowWidth}x${afterPostResizeAction.windowHeight}, image=${afterPostResizeAction.imageWidth}x${afterPostResizeAction.imageHeight}`,
  );

  const nativeTextSetupSystemBefore = processProbe(systemProbeBinary);
  const nativeTextSetupFixtureBefore = await fixtureState(fixtureStatePath);
  const nativeTextPriorSiblingFocus = processProbe(systemProbeBinary, fixtureReady.pid);
  requireCheck("same-PID sibling is the prior receiver before text preparation",
    exactTargetReceiverMatches(nativeTextPriorSiblingFocus, Number(siblingWindow.id), false) &&
      nativeTextSetupFixtureBefore.siblingTextLength === 0 &&
      nativeTextSetupFixtureBefore.siblingClicks === 0,
    `sibling-main-and-focused=${exactTargetReceiverMatches(nativeTextPriorSiblingFocus, Number(siblingWindow.id), false)}, sibling-state-clean=${nativeTextSetupFixtureBefore.siblingTextLength === 0 && nativeTextSetupFixtureBefore.siblingClicks === 0}`,
  );
  failureProbeBaseline = {
    stage: "nativeTextFieldFocus",
    system: nativeTextSetupSystemBefore,
    fixture: nativeTextSetupFixtureBefore,
    targetSiblingExpectedAfter: true,
  };
  await writeFile(fixtureControlPath, `${JSON.stringify({
    sequence: 2,
    action: "focus-semantic-field",
  })}\n`);
  const nativeTextFocusedState = await waitFor("fixture semantic field focus", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.appliedControlSequence === 2 && snapshot.focusCount === 1 &&
      snapshot.siblingFocusCount === 1 && snapshot;
  });
  const nativeTextPreparedSiblingFocus = await waitFor("prepared same-PID sibling receiver", () => {
    const snapshot = processProbe(systemProbeBinary, fixtureReady.pid);
    return exactTargetReceiverMatches(snapshot, Number(siblingWindow.id), false) ? snapshot : null;
  });
  requireCheck("background fixture field prepared without mutation while sibling remains prior receiver",
    nativeTextFocusedState.lastAction === "focus-field" &&
      nativeTextSetupFixtureBefore.focusCount === 0 &&
      nativeTextSetupFixtureBefore.siblingFocusCount === 0 &&
      nativeTextSetupFixtureBefore.appliedControlSequence === 1 &&
      nativeTextFocusedState.focusCount === nativeTextSetupFixtureBefore.focusCount + 1 &&
      nativeTextFocusedState.siblingFocusCount === nativeTextSetupFixtureBefore.siblingFocusCount + 1 &&
      nativeTextFocusedState.clicks === nativeTextSetupFixtureBefore.clicks &&
      nativeTextFocusedState.semanticPresses === nativeTextSetupFixtureBefore.semanticPresses &&
      nativeTextFocusedState.semanticValue === SEMANTIC_VALUE &&
      nativeTextFocusedState.semanticValue === nativeTextSetupFixtureBefore.semanticValue &&
      nativeTextFocusedState.siblingTextLength === nativeTextSetupFixtureBefore.siblingTextLength &&
      nativeTextFocusedState.siblingClicks === nativeTextSetupFixtureBefore.siblingClicks &&
      nativeTextFocusedState.resizeCount === nativeTextSetupFixtureBefore.resizeCount &&
      nativeTextFocusedState.moveEvents === nativeTextSetupFixtureBefore.moveEvents &&
      nativeTextFocusedState.contentWidth === nativeTextSetupFixtureBefore.contentWidth &&
      nativeTextFocusedState.contentHeight === nativeTextSetupFixtureBefore.contentHeight &&
      exactTargetReceiverMatches(nativeTextPreparedSiblingFocus, Number(siblingWindow.id), false),
    "the primary responder was prepared, the sibling was restored internally, and no target field mutated",
  );
  const nativeTextSetupInvariants = systemInvariants(
    nativeTextSetupSystemBefore,
    processProbe(systemProbeBinary),
  );
  requireIndependentInvariants("field preparation", nativeTextSetupInvariants);

  current = await waitForNextShareState(
    afterPostResizeAction.sequence,
    "share-after-native-text-focus",
    (sample) => sample.shareId === firstShareId && capturedFrameMatchesWindowGeometry(sample),
  );
  observation = current.snapshot.computerObservation;
  const nativeTextSystemBefore = processProbe(systemProbeBinary);
  const nativeTextReceiverBefore = processProbe(systemProbeBinary, fixtureReady.pid);
  requireCheck("same-PID sibling remains the exact prior receiver immediately before text request",
    exactTargetReceiverMatches(nativeTextReceiverBefore, Number(siblingWindow.id), false),
    `sibling-main-and-focused=${exactTargetReceiverMatches(nativeTextReceiverBefore, Number(siblingWindow.id), false)}`,
  );
  failureProbeBaseline = {
    stage: "nativeTextDelivery",
    system: nativeTextSystemBefore,
    fixture: nativeTextFocusedState,
    targetSiblingExpectedAfter: true,
  };
  nativeTextPayloadMayBeVisible = true;
  const nativeTextBody = await command("computer.typeText", {
    frameId: observation.frameId,
    text: NATIVE_TEXT_SUFFIX,
  });
  const nativeText = actionSummary(nativeTextBody);
  const nativeTextFixtureState = await waitFor("fixture native text read-back", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.semanticValue === `${SEMANTIC_VALUE}${NATIVE_TEXT_SUFFIX}` &&
      snapshot.siblingTextLength === nativeTextFocusedState.siblingTextLength &&
      snapshot.siblingClicks === nativeTextFocusedState.siblingClicks && snapshot;
  });
  const nativeTextReceiverAfter = await waitFor("same-PID sibling restoration after native text", () => {
    const snapshot = processProbe(systemProbeBinary, fixtureReady.pid);
    return exactTargetReceiverMatches(snapshot, Number(siblingWindow.id), false) ? snapshot : null;
  });
  requireCheck("native typeText exact fixture read-back with zero sibling mutation",
    nativeTextFixtureState.lastAction === "set-value" &&
      nativeTextFixtureState.clicks === nativeTextFocusedState.clicks &&
      nativeTextFixtureState.semanticPresses === nativeTextFocusedState.semanticPresses &&
      nativeTextFixtureState.siblingTextLength === nativeTextFocusedState.siblingTextLength &&
      nativeTextFixtureState.siblingClicks === nativeTextFocusedState.siblingClicks &&
      nativeTextFixtureState.siblingFocusCount === nativeTextFocusedState.siblingFocusCount &&
      nativeTextFixtureState.resizeCount === nativeTextFocusedState.resizeCount &&
      nativeTextFixtureState.focusCount === nativeTextFocusedState.focusCount &&
      nativeTextFixtureState.moveEvents === nativeTextFocusedState.moveEvents &&
      exactTargetReceiverMatches(nativeTextReceiverAfter, Number(siblingWindow.id), false),
    `appended ${NATIVE_TEXT_SUFFIX.length} ASCII characters only to the primary field and restored the prior sibling receiver`,
  );
  requireCheck("native typeText bounded product result",
    nativeText.effect === "Unverifiable" &&
      nativeText.deliveryMode === "exact-window-background" &&
      nativeText.frameId === observation.frameId &&
      nativeText.characters === NATIVE_TEXT_SUFFIX.length &&
      nativeText.utf16CodeUnits === NATIVE_TEXT_SUFFIX.length,
    `${nativeText.effect}/${nativeText.deliveryMode}, characters=${nativeText.characters}, utf16=${nativeText.utf16CodeUnits}`,
  );
  requireActionInvariants("native typeText", nativeText);
  const nativeTextInvariants = systemInvariants(
    nativeTextSystemBefore,
    processProbe(systemProbeBinary),
  );
  requireIndependentInvariants("native typeText", nativeTextInvariants);

  current = await waitForNextShareState(
    current.sample.sequence,
    "share-after-native-text-delivery",
    (sample, candidateObservation) =>
      sample.shareId === firstShareId &&
      capturedFrameMatchesWindowGeometry(sample) &&
      candidateObservation.shareId === firstShareId &&
      candidateObservation.sourceSequence === sample.sourceSequence &&
      candidateObservation.elements.some(
        (element) => element.name === "Semantic value" && element.actions.includes("setValue"),
      ),
  );
  const restoreObservation = current.snapshot.computerObservation;
  const restoreField = restoreObservation.elements.find(
    (element) => element.name === "Semantic value" && element.actions.includes("setValue"),
  );
  requireCheck("native text restore element discovered", Boolean(restoreField), restoreField?.role || "missing");
  requireCheck("native text restore frame is bound to the persistent share authority",
    restoreObservation.shareId === firstShareId &&
      restoreObservation.sourceSequence === current.sample.sourceSequence &&
      restoreObservation.share?.active === true &&
      restoreObservation.share?.id === firstShareId,
    `share=${restoreObservation.shareId === firstShareId}, source=${restoreObservation.sourceSequence}`,
  );
  const nativeTextRestoreBody = await command("computer.setValue", {
    frameId: restoreObservation.frameId,
    elementRef: restoreField.ref,
    value: SEMANTIC_VALUE,
  });
  const nativeTextRestore = actionSummary(nativeTextRestoreBody);
  requireCheck("native text fixture value restored",
    nativeTextRestore.effect === "Confirmed" &&
      nativeTextRestore.backendEffect?.postcondition === "value-confirmed",
    `${nativeTextRestore.effect}/${nativeTextRestore.backendEffect?.postcondition}`,
  );
  requireActionInvariants("native text restore", nativeTextRestore);
  const nativeTextRestoredFixtureState = await waitFor("fixture semantic value restore", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.semanticValue === SEMANTIC_VALUE && snapshot;
  });
  requireCheck("native text restore exact fixture state",
    nativeTextRestoredFixtureState.lastAction === "set-value" &&
      nativeTextRestoredFixtureState.clicks === nativeTextFixtureState.clicks &&
      nativeTextRestoredFixtureState.semanticPresses === nativeTextFixtureState.semanticPresses &&
      nativeTextRestoredFixtureState.siblingTextLength === nativeTextFixtureState.siblingTextLength &&
      nativeTextRestoredFixtureState.siblingClicks === nativeTextFixtureState.siblingClicks &&
      nativeTextRestoredFixtureState.siblingFocusCount === nativeTextFixtureState.siblingFocusCount &&
      nativeTextRestoredFixtureState.resizeCount === nativeTextFixtureState.resizeCount &&
      nativeTextRestoredFixtureState.focusCount === nativeTextFixtureState.focusCount &&
      nativeTextRestoredFixtureState.moveEvents === nativeTextFixtureState.moveEvents,
    "only the deterministic semantic value was restored",
  );
  current = await waitForNextShareState(
    current.sample.sequence,
    "share-after-native-text-restore",
    (sample) => sample.shareId === firstShareId && capturedFrameMatchesWindowGeometry(sample),
  );

  validateShareSamples(shareSamples);
  const shareStatusBody = await command("computer.share.status");
  const shareStatus = shareStatusBody.result;
  requireCheck("share status remains native and ack paced",
    shareStatus.active === true && shareStatus.id === firstShareId &&
      shareStatus.captureBackend === CAPTURE_BACKEND && shareStatus.nativeStream === true &&
      shareStatus.ackPaced === true && shareStatus.backpressure === "latest-frame-wins",
    `${shareStatus.captureBackend}, source=${shareStatus.sourceSequence}, transport=${shareStatus.sequence}`,
  );
  requireCheck("share status drop domains retained",
    shareStatus.sourceDroppedFrames >= afterPixelAction.sourceDroppedFrames &&
      shareStatus.transportDroppedFrames >= beforePixelAction.transportDroppedFrames,
    `source-drop=${shareStatus.sourceDroppedFrames}, transport-drop=${shareStatus.transportDroppedFrames}`,
  );

  observation = current.snapshot.computerObservation;
  const canceledFrameId = observation.frameId;
  const canceledScreenshotUrl = observation.screenshotUrl;
  requireCheck("cancellation starts from a current resized share frame",
    observation.share?.active === true && observation.share?.id === firstShareId &&
      observation.sourceSequence === current.sample.sourceSequence &&
      typeof canceledScreenshotUrl === "string" && canceledScreenshotUrl.startsWith("/api/computer/screenshot?"),
    `share=${observation.share?.id === firstShareId}, source=${observation.sourceSequence}, screenshot-bound=${typeof canceledScreenshotUrl === "string"}`,
  );
  const cancellationCallId = randomUUID();
  const cancellationMoveX = Math.max(0, Math.round(observation.imageWidth / 4));
  const cancellationMoveY = Math.max(0, Math.round(observation.imageHeight / 3));
  const cancellationParams = {
    frameId: canceledFrameId,
    x: cancellationMoveX,
    y: cancellationMoveY,
    durationMs: CANCELED_MOVE_DURATION_MS,
  };
  const cancellationFixtureBefore = await fixtureState(fixtureStatePath);
  const cancellationSystemBefore = processProbe(systemProbeBinary);
  requireCheck("bounded fixture mouse-move instrumentation ready",
    Number.isSafeInteger(cancellationFixtureBefore.moveEvents) &&
      cancellationFixtureBefore.moveEvents >= 0 &&
      cancellationFixtureBefore.moveEvents < 1_000_000,
    `baseline=${boundedCounter(cancellationFixtureBefore.moveEvents)}`,
  );
  failureProbeBaseline = {
    stage: "explicitCommandCancellation",
    system: cancellationSystemBefore,
    fixture: cancellationFixtureBefore,
    targetSiblingExpectedAfter: true,
  };

  const cancellationStartedAt = Date.now();
  const originalCanceledRequest = commandResponse(
    "computer.move",
    cancellationParams,
    cancellationCallId,
  );
  const cancellationDispatchProofStartedAt = Date.now();
  const cancellationDispatchedFixtureState = await waitFor(
    "fixture target-routed cancellation move dispatch",
    async () => {
      const snapshot = await fixtureState(fixtureStatePath);
      return snapshot.moveEvents > cancellationFixtureBefore.moveEvents && snapshot;
    },
    CANCELLATION_DISPATCH_PROOF_TIMEOUT_MS,
  );
  const cancellationDispatchProofElapsedMs = Date.now() - cancellationDispatchProofStartedAt;
  requireCheck("cancellation waits for target-routed native move delivery",
    cancellationDispatchedFixtureState.moveEvents > cancellationFixtureBefore.moveEvents &&
      cancellationDispatchedFixtureState.moveEvents <= 1_000_000 &&
      cancellationDispatchedFixtureState.clicks === cancellationFixtureBefore.clicks &&
      cancellationDispatchedFixtureState.semanticPresses === cancellationFixtureBefore.semanticPresses &&
      cancellationDispatchedFixtureState.semanticValue === cancellationFixtureBefore.semanticValue &&
      cancellationDispatchedFixtureState.resizeCount === cancellationFixtureBefore.resizeCount &&
      cancellationDispatchedFixtureState.focusCount === cancellationFixtureBefore.focusCount &&
      cancellationDispatchedFixtureState.appliedControlSequence === cancellationFixtureBefore.appliedControlSequence,
    `moveEvents=${cancellationFixtureBefore.moveEvents}->${cancellationDispatchedFixtureState.moveEvents}, wait=${cancellationDispatchProofElapsedMs}ms`,
  );

  const inProgressDuplicate = await commandResponse(
    "computer.move",
    cancellationParams,
    cancellationCallId,
  );
  requireCheck("exact duplicate observes one causally dispatched product action",
    inProgressDuplicate.status === 409 && inProgressDuplicate.ok === false &&
      inProgressDuplicate.body.error?.code === "CALL_IN_PROGRESS" &&
      inProgressDuplicate.body.callId === cancellationCallId &&
      inProgressDuplicate.body.replayed === undefined,
    `HTTP ${inProgressDuplicate.status}, ${inProgressDuplicate.body.error?.code}`,
  );

  const cancelAccepted = await cancelCommandResponse(cancellationCallId);
  requireCheck("authenticated exact-call cancellation accepted",
    cancelAccepted.status === 202 && cancelAccepted.ok === true &&
      cancelAccepted.body.ok === true &&
      cancelAccepted.body.callId === cancellationCallId &&
      cancelAccepted.body.cancellationRequested === true,
    `HTTP ${cancelAccepted.status}, requested=${cancelAccepted.body.cancellationRequested}`,
  );
  const canceledOriginal = await originalCanceledRequest;
  const cancellationElapsedMs = Date.now() - cancellationStartedAt;
  requireCheck("canceled original reports conservative outcome unknown",
    canceledOriginal.status === 504 && canceledOriginal.ok === false &&
      canceledOriginal.body.error?.code === "COMMAND_OUTCOME_UNKNOWN" &&
      canceledOriginal.body.taxonomy?.code === "outcome_unknown" &&
      canceledOriginal.body.taxonomy?.retriable === false &&
      canceledOriginal.body.taxonomy?.recoveryHint === "reobserve" &&
      canceledOriginal.body.callId === cancellationCallId &&
      canceledOriginal.body.replayed === undefined,
    `HTTP ${canceledOriginal.status}, ${canceledOriginal.body.error?.code}/${canceledOriginal.body.taxonomy?.code}, elapsed=${cancellationElapsedMs}ms`,
  );

  const duplicateCancel = await cancelCommandResponse(cancellationCallId);
  requireCheck("completed call cannot be canceled twice",
    duplicateCancel.status === 409 && duplicateCancel.ok === false &&
      duplicateCancel.body.error?.code === "CALL_NOT_IN_PROGRESS" &&
      duplicateCancel.body.callId === cancellationCallId,
    `HTTP ${duplicateCancel.status}, ${duplicateCancel.body.error?.code}`,
  );
  const replayedCanceled = await commandResponse(
    "computer.move",
    cancellationParams,
    cancellationCallId,
  );
  const replayedWithoutMarker = { ...replayedCanceled.body };
  delete replayedWithoutMarker.replayed;
  requireCheck("exact canceled call replays without redispatch",
    replayedCanceled.status === 504 && replayedCanceled.ok === false &&
      replayedCanceled.body.error?.code === "COMMAND_OUTCOME_UNKNOWN" &&
      replayedCanceled.body.callId === cancellationCallId &&
      replayedCanceled.body.replayed === true &&
      JSON.stringify(replayedWithoutMarker) === JSON.stringify(canceledOriginal.body),
    `HTTP ${replayedCanceled.status}, replayed=${replayedCanceled.body.replayed}`,
  );
  const reusedCallId = await commandResponse(
    "computer.move",
    { ...cancellationParams, x: cancellationMoveX + 1 },
    cancellationCallId,
  );
  requireCheck("changed command cannot reuse canceled call identity",
    reusedCallId.status === 409 && reusedCallId.ok === false &&
      reusedCallId.body.error?.code === "CALL_ID_REUSED" &&
      reusedCallId.body.taxonomy?.code === "invalid_request" &&
      reusedCallId.body.callId === cancellationCallId &&
      reusedCallId.body.replayed === undefined,
    `HTTP ${reusedCallId.status}, ${reusedCallId.body.error?.code}/${reusedCallId.body.taxonomy?.code}`,
  );

  const canceledState = await waitFor("explicit cancellation public authority teardown", async () => {
    const snapshot = await apiState();
    return snapshot.computer?.sessionId === hello.sessionId &&
      snapshot.computerObservation === null &&
      snapshot.computer?.share?.active === false &&
      snapshot.computer?.share?.stopped === true &&
      snapshot.computer?.share?.reason === "outcome-unknown" && snapshot;
  });
  requireCheck("cancellation fail-closed state names the exact-session revocation",
    canceledState.computer.share.code === "COMMAND_OUTCOME_UNKNOWN",
    `${canceledState.computer.share.reason}/${canceledState.computer.share.code}`,
  );
  const clearedScreenshotResponse = await fetch(`http://127.0.0.1:${port}${canceledScreenshotUrl}`, {
    headers: { Authorization: `Bearer ${bearerToken}` },
  });
  const clearedScreenshotBody = await clearedScreenshotResponse.json();
  requireCheck("cancellation tears down the computer screenshot surface",
    clearedScreenshotResponse.status === 404 &&
      clearedScreenshotBody.error?.code === "NO_COMPUTER_SCREENSHOT",
    `HTTP ${clearedScreenshotResponse.status}, ${clearedScreenshotBody.error?.code}`,
  );
  await delay(Math.ceil(3_000 / SHARE_FPS));
  const canceledStateSettled = await apiState();
  requireCheck("canceled share surface stays torn down",
    canceledStateSettled.computer?.sessionId === hello.sessionId &&
      canceledStateSettled.computerObservation === null &&
      canceledStateSettled.computer?.share?.active === false &&
      canceledStateSettled.computer?.share?.reason === "outcome-unknown",
    "no queued SCStream frame republished authority after cancellation",
  );

  const explicitStopBody = await command("computer.share.stop");
  const explicitStop = explicitStopBody.result;
  requireCheck("post-cancellation share stop is idempotently fail-closed",
    explicitStop.active === false && explicitStop.stopped === false &&
      explicitStop.reason === "not-active",
    `${explicitStop.active}/${explicitStop.stopped}/${explicitStop.reason}`,
  );
  const stoppedState = await apiState();
  requireCheck("explicit stop preserves surface teardown",
    stoppedState.computerObservation === null &&
      stoppedState.computer?.share?.active === false &&
      stoppedState.computer?.share?.stopped === false &&
      stoppedState.computer?.share?.reason === "not-active",
    "computer observation, screenshot, pointer, and share authority remain unavailable",
  );

  const gatedOldFrameAction = await commandResponse("computer.click", {
    frameId: canceledFrameId,
    x: postResizeClickX,
    y: postResizeClickY,
    button: "left",
    clickCount: 1,
    durationMs: 50,
  });
  requireCheck("revoked authority gates pre-recovery mutations before helper relay",
    gatedOldFrameAction.status === 409 && gatedOldFrameAction.ok === false &&
      gatedOldFrameAction.body.error?.code === "NO_COMPUTER_FRAME" &&
      gatedOldFrameAction.body.taxonomy?.code === "stale_snapshot" &&
      gatedOldFrameAction.body.taxonomy?.retriable === true &&
      gatedOldFrameAction.body.taxonomy?.recoveryHint === "reobserve",
    `HTTP ${gatedOldFrameAction.status}, ${gatedOldFrameAction.body.error?.code}/${gatedOldFrameAction.body.taxonomy?.code}/${gatedOldFrameAction.body.taxonomy?.recoveryHint}`,
  );
  const gatedState = await apiState();
  requireCheck("gated old-frame action cannot recreate a computer surface",
    gatedState.computerObservation === null && gatedState.computer?.share?.active === false,
    "no frame, screenshot, pointer, or share authority was republished",
  );

  const recoveryObserve = await freshObserve(targetWindow.id);
  const recoveryObservation = recoveryObserve.state.computerObservation;
  requireCheck("explicit one-shot observation recovers exact-session authority",
    recoveryObserve.state.computer?.sessionId === hello.sessionId &&
      recoveryObservation.frameId !== canceledFrameId &&
      recoveryObservation.windowId === targetWindow.id &&
      recoveryObservation.windowTitle === FIXTURE_TITLE &&
      recoveryObservation.pid === fixtureReady.pid &&
      recoveryObservation.share?.active === false &&
      typeof recoveryObservation.screenshotUrl === "string" &&
      recoveryObservation.screenshotUrl.startsWith("/api/computer/screenshot?"),
    `session-preserved=${recoveryObserve.state.computer?.sessionId === hello.sessionId}, fresh-frame=${recoveryObservation.frameId !== canceledFrameId}`,
  );

  const staleAction = await commandResponse("computer.click", {
    frameId: canceledFrameId,
    x: postResizeClickX,
    y: postResizeClickY,
    button: "left",
    clickCount: 1,
    durationMs: 50,
  });
  requireCheck("pre-cancellation frame stays stale after explicit recovery",
    staleAction.status === 409 && staleAction.ok === false &&
      staleAction.body.error?.code === "COMPUTER_STALE_FRAME" &&
      staleAction.body.taxonomy?.code === "stale_snapshot" &&
      staleAction.body.taxonomy?.retriable === true &&
      staleAction.body.taxonomy?.recoveryHint === "reobserve",
    `HTTP ${staleAction.status}, ${staleAction.body.error?.code}/${staleAction.body.taxonomy?.code}/${staleAction.body.taxonomy?.recoveryHint}`,
  );
  const recoveredState = await apiState();
  requireCheck("rejected stale action preserves the recovered exact frame",
    recoveredState.computerObservation?.frameId === recoveryObservation.frameId &&
      recoveredState.computerObservation?.windowId === targetWindow.id &&
      recoveredState.computer?.share?.active === false,
    "the old frame was refused without replacing or revoking the recovered one-shot frame",
  );

  const finalFixtureState = await fixtureState(fixtureStatePath);
  requireCheck("fixture final state",
    finalFixtureState.clicks === 2 && finalFixtureState.semanticPresses === 1 &&
      finalFixtureState.semanticValue === SEMANTIC_VALUE &&
      finalFixtureState.resizeCount === 1 && finalFixtureState.focusCount === 1 &&
      finalFixtureState.siblingFocusCount === 1 &&
      finalFixtureState.siblingTextLength === 0 && finalFixtureState.siblingClicks === 0 &&
      finalFixtureState.primaryWindowId === Number(targetWindow.id) &&
      finalFixtureState.siblingWindowId === Number(siblingWindow.id) &&
      finalFixtureState.moveEvents >= cancellationDispatchedFixtureState.moveEvents &&
      finalFixtureState.moveEvents <= 1_000_000 &&
      finalFixtureState.appliedControlSequence === 2 &&
      finalFixtureState.contentWidth === 820 && finalFixtureState.contentHeight === 520,
    JSON.stringify({
      clicks: finalFixtureState.clicks,
      semanticPresses: finalFixtureState.semanticPresses,
      semanticValueMatchesExpected: finalFixtureState.semanticValue === SEMANTIC_VALUE,
      resizeCount: finalFixtureState.resizeCount,
      focusCount: finalFixtureState.focusCount,
      siblingFocusCount: finalFixtureState.siblingFocusCount,
      siblingTextLength: finalFixtureState.siblingTextLength,
      siblingClicks: finalFixtureState.siblingClicks,
      distinctSamePidWindows:
        finalFixtureState.primaryWindowId !== finalFixtureState.siblingWindowId,
      moveEvents: finalFixtureState.moveEvents,
      appliedControlSequence: finalFixtureState.appliedControlSequence,
      contentWidth: finalFixtureState.contentWidth,
      contentHeight: finalFixtureState.contentHeight,
    }),
  );
  requireCheck("canceled move, gated refusal, recovery, and stale refusal caused no functional fixture mutation",
    finalFixtureState.clicks === cancellationFixtureBefore.clicks &&
      finalFixtureState.semanticPresses === cancellationFixtureBefore.semanticPresses &&
      finalFixtureState.semanticValue === cancellationFixtureBefore.semanticValue &&
      finalFixtureState.resizeCount === cancellationFixtureBefore.resizeCount &&
      finalFixtureState.focusCount === cancellationFixtureBefore.focusCount &&
      finalFixtureState.siblingFocusCount === cancellationFixtureBefore.siblingFocusCount &&
      finalFixtureState.siblingTextLength === cancellationFixtureBefore.siblingTextLength &&
      finalFixtureState.siblingClicks === cancellationFixtureBefore.siblingClicks &&
      finalFixtureState.appliedControlSequence === cancellationFixtureBefore.appliedControlSequence &&
      finalFixtureState.contentWidth === cancellationFixtureBefore.contentWidth &&
      finalFixtureState.contentHeight === cancellationFixtureBefore.contentHeight &&
      finalFixtureState.moveEvents >= cancellationDispatchedFixtureState.moveEvents,
    "functional fixture state stayed exact; only bounded move-delivery instrumentation advanced",
  );
  const finalTargetReceiver = processProbe(systemProbeBinary, fixtureReady.pid);
  requireCheck("final same-PID receiver main and focus are restored before target close",
    exactTargetReceiverMatches(finalTargetReceiver, Number(siblingWindow.id), false),
    `sibling-main-and-focused=${exactTargetReceiverMatches(finalTargetReceiver, Number(siblingWindow.id), false)}`,
  );
  const cancellationInvariants = systemInvariants(
    cancellationSystemBefore,
    processProbe(systemProbeBinary),
  );
  requireIndependentInvariants("cancellation/stop", cancellationInvariants);
  requireCheck("every retained screenshot is bound only to the primary exact window",
    screenshots.length === 6 && screenshots.every(
      (screenshot) => screenshot.windowId === targetWindow.id &&
        screenshot.windowId !== siblingWindow.id,
    ),
    `${screenshots.length} primary-only screenshot records; sibling excluded=${screenshots.every((screenshot) => screenshot.windowId !== siblingWindow.id)}`,
  );

  const targetCloseSystemBefore = processProbe(systemProbeBinary);
  failureProbeBaseline = {
    stage: "freshShareAfterCancellationRecovery",
    system: targetCloseSystemBefore,
    fixture: finalFixtureState,
    targetSiblingExpectedAfter: true,
  };
  const targetCloseShareStartBody = await command("computer.share.start", {
    windowId: targetWindow.id,
    fps: SHARE_FPS,
  });
  const targetCloseShare = targetCloseShareStartBody.result;
  requireCheck("target-close persistent share started with fresh authority",
    targetCloseShare.active === true && targetCloseShare.id !== firstShareId &&
      targetCloseShare.windowId === targetWindow.id && targetCloseShare.pid === fixtureReady.pid &&
      targetCloseShare.captureMode === "persistent-native-stream" &&
      targetCloseShare.captureBackend === CAPTURE_BACKEND && targetCloseShare.nativeStream === true,
    `fresh-share=${targetCloseShare.id !== firstShareId}, target=${targetCloseShare.windowId === targetWindow.id}, backend=${targetCloseShare.captureBackend}`,
  );
  const targetCloseShareStartState = targetCloseShareStartBody.state;
  const targetCloseShareStartObservation = targetCloseShareStartState.computerObservation;
  requireCheck("fresh share start retires the recovered one-shot frame under exact new authority",
    targetCloseShareStartState.computer?.sessionId === hello.sessionId &&
      targetCloseShareStartState.computer?.share?.active === true &&
      targetCloseShareStartState.computer?.share?.id === targetCloseShare.id &&
      targetCloseShareStartObservation?.frameId !== recoveryObservation.frameId &&
      targetCloseShareStartObservation?.windowId === targetWindow.id &&
      targetCloseShareStartObservation?.windowTitle === FIXTURE_TITLE &&
      targetCloseShareStartObservation?.pid === fixtureReady.pid &&
      targetCloseShareStartObservation?.shareId == null &&
      targetCloseShareStartObservation?.sourceSequence == null &&
      targetCloseShareStartObservation?.share?.active === true &&
      targetCloseShareStartObservation?.share?.id === targetCloseShare.id &&
      typeof targetCloseShareStartObservation?.screenshotUrl === "string" &&
      targetCloseShareStartObservation.screenshotUrl.startsWith("/api/computer/screenshot?"),
    `same-session=${targetCloseShareStartState.computer?.sessionId === hello.sessionId}, new-frame=${targetCloseShareStartObservation?.frameId !== recoveryObservation.frameId}, exact-share=${targetCloseShareStartObservation?.share?.id === targetCloseShare.id}`,
  );
  const targetCloseActive = await waitFor("target-close persistent share authority", async () => {
    const snapshot = await apiState();
    const activeObservation = snapshot.computerObservation;
    const sample = shareSample(activeObservation, "target-close-share-active");
    return snapshot.computerConnected === true &&
      snapshot.computer?.sessionId === hello.sessionId &&
      activeObservation?.windowId === targetWindow.id &&
      activeObservation?.pid === fixtureReady.pid &&
      sample?.shareId === targetCloseShare.id &&
      typeof activeObservation.screenshotUrl === "string" &&
      activeObservation.screenshotUrl.startsWith("/api/computer/screenshot?")
      ? { snapshot, sample }
      : null;
  });
  const targetCloseObservation = targetCloseActive.snapshot.computerObservation;
  const targetCloseFrameId = targetCloseObservation.frameId;
  const targetCloseScreenshotUrl = targetCloseObservation.screenshotUrl;
  requireCheck("exact target is alive while its closing share frame is current",
    fixtureProcess?.exitCode === null && fixtureProcess?.signalCode === null &&
      targetCloseObservation.share?.active === true &&
      targetCloseObservation.share?.id === targetCloseShare.id &&
      targetCloseObservation.frameId !== recoveryObservation.frameId &&
      targetCloseObservation.frameId !== targetCloseShareStartObservation.frameId &&
      targetCloseObservation.sourceSequence > 0 &&
      targetCloseObservation.sourceSequence === targetCloseActive.sample.sourceSequence,
    `share=${targetCloseObservation.share?.id === targetCloseShare.id}, recovered-retired=${targetCloseObservation.frameId !== recoveryObservation.frameId}, start-frame-retired=${targetCloseObservation.frameId !== targetCloseShareStartObservation.frameId}, source=${targetCloseObservation.sourceSequence}`,
  );

  failureProbeBaseline = {
    stage: "activeShareExactTargetClose",
    system: targetCloseSystemBefore,
    fixture: finalFixtureState,
    targetSiblingExpectedAfter: false,
  };
  const fixtureTargetClosure = await terminate(fixtureProcess, "exact fixture target");
  fixtureProcess = null;
  requireCheck("rig closed the exact target while its persistent share was active",
    fixtureTargetClosure.requested === true && fixtureTargetClosure.alreadyExited === false,
    `signal-requested=${fixtureTargetClosure.requested}, already-exited=${fixtureTargetClosure.alreadyExited}`,
  );

  const targetClosedState = await waitFor("exact-target close share failure", async () => {
    const snapshot = await apiState();
    const share = snapshot.computer?.share;
    return snapshot.computerConnected === true &&
      snapshot.computer?.sessionId === hello.sessionId &&
      snapshot.computerObservation === null &&
      share?.active === false && share?.stopped === true &&
      share?.reason === "capture-error" && share?.id === targetCloseShare.id &&
      TARGET_CLOSE_CAPTURE_CODES.has(share?.code)
      ? snapshot
      : null;
  });
  requireCheck("helper-originated exact-share failure stops server authority",
    targetClosedState.computer.share.id === targetCloseShare.id &&
      TARGET_CLOSE_CAPTURE_CODES.has(targetClosedState.computer.share.code) &&
      helperProcess?.exitCode === null && helperProcess?.signalCode === null &&
      helperSpawnCount === 1,
    `${targetClosedState.computer.share.reason}/${targetClosedState.computer.share.code}, same-helper=${helperSpawnCount === 1}`,
  );

  const targetClosedScreenshotResponse = await fetch(
    `http://127.0.0.1:${port}${targetCloseScreenshotUrl}`,
    { headers: { Authorization: `Bearer ${bearerToken}` } },
  );
  const targetClosedScreenshotBody = await targetClosedScreenshotResponse.json();
  requireCheck("target close clears the exact share screenshot surface",
    targetClosedScreenshotResponse.status === 404 &&
      targetClosedScreenshotBody.error?.code === "NO_COMPUTER_SCREENSHOT",
    `HTTP ${targetClosedScreenshotResponse.status}, ${targetClosedScreenshotBody.error?.code}`,
  );
  await delay(Math.ceil((TARGET_CLOSE_SETTLE_FRAME_PERIODS * 1_000) / SHARE_FPS));
  const targetClosedSettledState = await apiState();
  requireCheck("closed target cannot republish a queued native frame",
    targetClosedSettledState.computer?.sessionId === hello.sessionId &&
      targetClosedSettledState.computerObservation === null &&
      targetClosedSettledState.computer?.share?.active === false &&
      targetClosedSettledState.computer?.share?.stopped === true &&
      targetClosedSettledState.computer?.share?.reason === "capture-error" &&
      targetClosedSettledState.computer?.share?.id === targetCloseShare.id &&
      targetClosedSettledState.computer?.share?.code === targetClosedState.computer.share.code,
    `${TARGET_CLOSE_SETTLE_FRAME_PERIODS} frame periods settled without authority republish`,
  );

  const targetClosedStaleAction = await commandResponse("computer.click", {
    frameId: targetCloseFrameId,
    x: postResizeClickX,
    y: postResizeClickY,
    button: "left",
    clickCount: 1,
    durationMs: 50,
  });
  requireCheck("closed-target frame is refused before helper action relay",
    targetClosedStaleAction.status === 409 && targetClosedStaleAction.ok === false &&
      targetClosedStaleAction.body.error?.code === "NO_COMPUTER_FRAME" &&
      targetClosedStaleAction.body.taxonomy?.code === "stale_snapshot" &&
      targetClosedStaleAction.body.taxonomy?.retriable === true &&
      targetClosedStaleAction.body.taxonomy?.recoveryHint === "reobserve",
    `HTTP ${targetClosedStaleAction.status}, ${targetClosedStaleAction.body.error?.code}/${targetClosedStaleAction.body.taxonomy?.code}`,
  );
  const targetClosedAfterStaleAction = await apiState();
  requireCheck("stale closed-target action cannot recreate a computer surface",
    targetClosedAfterStaleAction.computerObservation === null &&
      targetClosedAfterStaleAction.computer?.share?.active === false &&
      targetClosedAfterStaleAction.computer?.share?.id === targetCloseShare.id &&
      targetClosedAfterStaleAction.computer?.share?.reason === "capture-error",
    "the retired frame remained unavailable after the rejected mutation",
  );

  const targetClosedObserve = await commandResponse("computer.observe", {
    windowId: targetWindow.id,
  });
  requireCheck("explicit observe cannot recover a closed exact target",
    targetClosedObserve.status === 409 && targetClosedObserve.ok === false &&
      targetClosedObserve.body.error?.code === "COMPUTER_NO_WINDOW" &&
      targetClosedObserve.body.taxonomy?.code === "target_changed" &&
      targetClosedObserve.body.taxonomy?.retriable === true &&
      targetClosedObserve.body.taxonomy?.recoveryHint === "reobserve",
    `HTTP ${targetClosedObserve.status}, ${targetClosedObserve.body.error?.code}/${targetClosedObserve.body.taxonomy?.code}`,
  );
  const targetClosedAfterObserve = await apiState();
  requireCheck("closed-target observe refusal preserves terminal teardown",
    targetClosedAfterObserve.computerObservation === null &&
      targetClosedAfterObserve.computer?.share?.active === false &&
      targetClosedAfterObserve.computer?.share?.id === targetCloseShare.id &&
      targetClosedAfterObserve.computer?.share?.reason === "capture-error" &&
      targetClosedAfterObserve.computer?.share?.code === targetClosedState.computer.share.code,
    "the failed recovery did not restore observation, screenshot, pointer, share, or frame authority",
  );
  const targetClosedStatusBody = await command("computer.status");
  const targetClosedStatus = targetClosedStatusBody.result;
  requireCheck("helper independently observes the exact target pair is gone",
    !targetClosedStatus.windows.some(
      (window) => window.id === targetWindow.id && window.pid === fixtureReady.pid,
    ) && !targetClosedStatus.windows.some(
      (window) => window.id === siblingWindow.id && window.pid === fixtureReady.pid,
    ),
    "the helper no longer enumerates either real window from the closed fixture process",
  );
  const targetCloseStopBody = await command("computer.share.stop");
  const targetCloseStop = targetCloseStopBody.result;
  requireCheck("target-close share stop is idempotently clean",
    targetCloseStop.active === false && targetCloseStop.stopped === false &&
      targetCloseStop.reason === "not-active",
    `${targetCloseStop.active}/${targetCloseStop.stopped}/${targetCloseStop.reason}`,
  );
  const targetCloseStoppedState = await apiState();
  requireCheck("target-close cleanup leaves no share or frame authority",
    targetCloseStoppedState.computerConnected === true &&
      targetCloseStoppedState.computer?.sessionId === hello.sessionId &&
      targetCloseStoppedState.computerObservation === null &&
      targetCloseStoppedState.computer?.share?.active === false &&
      targetCloseStoppedState.computer?.share?.stopped === false &&
      targetCloseStoppedState.computer?.share?.reason === "not-active",
    "the same helper remains ready with no target, share, observation, screenshot, or frame authority",
  );
  const targetCloseInvariants = systemInvariants(
    targetCloseSystemBefore,
    processProbe(systemProbeBinary),
  );
  requireIndependentInvariants("target-close", targetCloseInvariants);

  const systemAfter = processProbe(systemProbeBinary);
  const independentInvariants = systemInvariants(systemBefore, systemAfter);
  requireIndependentInvariants("whole-run", independentInvariants);
  const pointerEvidence = pointerEvidenceSummary();
  requireCheck(
    "requested pointer evidence lane completed without unknown attribution",
    pointerEvidence.unknownObserved === false &&
      pointerEvidence.quietObserved === true &&
      pointerEvidence.concurrentSharedSeatActivityObserved === false,
    JSON.stringify(pointerEvidence),
  );
  verifyHarnessSourceBinding("post-run");

  const helperTeardown = await terminate(helperProcess, "helper");
  helperProcess = null;
  requireCheck("helper remained alive until controlled teardown", helperTeardown.requested === true, "SIGTERM was sent by the rig");
  const disconnected = await waitFor("helper transport teardown", async () => {
    const snapshot = await apiState();
    return snapshot.computerConnected === false && snapshot;
  });
  requireCheck("helper transport disconnected cleanly",
    disconnected.computer === null && disconnected.computerObservation === null,
    "helper session and observation were revoked",
  );
  requireCheck("helper was never respawned", helperSpawnCount === 1, `${helperSpawnCount} packaged helper process`);

  const serverTeardown = await terminate(serverProcess, "server");
  serverProcess = null;
  requireCheck("server remained alive until controlled teardown", serverTeardown.requested === true, "SIGTERM was sent by the rig");
  await waitFor("loopback listener teardown", async () => !(await healthReachable()), 5_000);
  requireCheck("server listener closed", true, "the evidence port no longer accepts health requests");

  if (pointerEvidenceLane === "deliberate-concurrency") {
    requireCheck(
      "app-share handoff records were published and the app was closed before success",
      pointerHandoffRequestPublicationAcknowledged &&
        pointerHandoffStartReceiptSha256 !== null &&
        pointerHandoffCompletePublicationAcknowledged &&
        pointerHandoffPromptClosed &&
        pointerHandoffProcess === null &&
        await pathExists(pointerHandoffRequestPath) &&
        await pathExists(pointerHandoffStartPath) &&
        await pathExists(pointerHandoffCompletePath),
      "the runner and bound app completed the request/start/complete create-once chain",
    );
  }

  const finalSample = shareSamples.at(-1);
  const result = {
    schemaVersion: 8,
    productVersion: EXPECTED_VERSION,
    status: "passed-release-candidate",
    evidenceClass: "exact-release-candidate-package-live-observation",
    startedAt: laneStartedAt,
    capturedAt: new Date().toISOString(),
    candidateNotice: "This becomes release evidence only after the supplied checksum manifest and archive are published immutably for v0.12.35.",
    releaseCandidateBinding: { ...releaseCandidateBinding },
    harnessSourceBinding: { ...harnessSourceBinding },
    capabilityBinding: { ...capabilityBinding },
    quietSeatStabilization: { ...quietSeatStabilization },
    pointerEvidence,
    appShareHandoff: pointerHandoffSummary(),
    environment: {
      operatingSystem: run("sw_vers", ["-productVersion"]),
      architecture: run("uname", ["-m"]),
      backend: status.backend,
      semanticMode: observation.semanticMode,
      sessionMode: status.sessionMode,
      screenCapturePermissionPreexisting: permissionProbe.screenCaptureReady,
      accessibilityPermissionPreexisting: permissionProbe.accessibilityReady,
      permissionRequestPerformed: false,
      foregroundFocusOracle: "sandwiched-pid+read-only-AXFocusedWindow+AXFrontmost",
    },
    package: {
      checksumManifest: manifestBinding,
      archive: {
        file: basename(archivePath),
        sha256: archiveSha256,
        checksumManifestMatched: true,
        checksumManifestSha256: manifestSha256,
        canonicalEntryMatched: true,
        extractedInputsMatched: true,
      },
      serverVersion: EXPECTED_VERSION,
      helperVersion: EXPECTED_VERSION,
      helperBundleVersion: bundleVersion,
      helperBundleBuildVersion: bundleBuildVersion,
      serverSha256,
      helperSha256,
      serverArchitectures,
      helperArchitectures,
      strictCodeSignatureVerification: "passed",
    },
    harness: {
      runnerSha256: harnessSha256.runner,
      fixtureSha256: harnessSha256.fixture,
      systemProbeSha256: harnessSha256.systemProbe,
      appShareHandoffSha256: harnessSha256.appShareHandoff,
      physicalPointerHandoffSha256: harnessSha256.physicalPointerHandoff,
      acceptanceFinalizerSha256: harnessSha256.acceptanceFinalizer,
      packagedHelperSpawnCount: helperSpawnCount,
    },
    fixture: {
      evidenceLane: finalFixtureState.evidenceLane,
      application: targetWindow.appName,
      windowTitle: targetWindow.title,
      windowId: targetWindow.id,
      pid: targetWindow.pid,
      samePidSibling: {
        windowTitle: siblingWindow.title,
        windowId: siblingWindow.id,
        pid: siblingWindow.pid,
        distinctFromPrimary: siblingWindow.id !== targetWindow.id,
      },
      finalState: {
        clicks: finalFixtureState.clicks,
        semanticPresses: finalFixtureState.semanticPresses,
        semanticValueMatchesExpected: finalFixtureState.semanticValue === SEMANTIC_VALUE,
        animationTick: finalFixtureState.animationTick,
        resizeCount: finalFixtureState.resizeCount,
        focusCount: finalFixtureState.focusCount,
        siblingFocusCount: finalFixtureState.siblingFocusCount,
        siblingTextLength: finalFixtureState.siblingTextLength,
        siblingClicks: finalFixtureState.siblingClicks,
        moveEvents: finalFixtureState.moveEvents,
        appliedControlSequence: finalFixtureState.appliedControlSequence,
        contentWidth: finalFixtureState.contentWidth,
        contentHeight: finalFixtureState.contentHeight,
        lastAction: finalFixtureState.lastAction,
      },
    },
    checks: {
      helperHandshake: {
        compatible: hello.compatible,
        version: hello.version,
        protocolVersion: hello.protocolVersion,
        helperSpawnCount,
        shareAckAdvertised: hello.capabilities.includes("computer.share.ack"),
        nativeStreamAdvertised: hello.capabilities.includes("computer.capture.native-stream.v1"),
      },
      exactWindowObserve: {
        passed: true,
        windowId: targetWindow.id,
        semanticAvailable: true,
        semanticElementCount: observed.state.computerObservation.elements.length,
      },
      semanticSetValue: setValue,
      semanticInvoke: {
        ...invoke,
        independentFixturePostcondition: "semantic counter advanced exactly once",
      },
      backgroundPixelAction: {
        ...click,
        durationMs: LONG_PIXEL_ACTION_MS,
        independentFixturePostcondition: "only the primary click counter advanced exactly once",
        priorSiblingMainAndFocused:
          exactTargetReceiverMatches(pixelSiblingFocusBefore, Number(siblingWindow.id), false),
        activeRequestedMainAndFocused:
          exactTargetReceiverMatches(activeRequestedReceiver, Number(targetWindow.id), true),
        priorSiblingRestored:
          exactTargetReceiverMatches(pixelSiblingFocusAfter, Number(siblingWindow.id), false),
        siblingMutationObserved:
          clickedFixtureState.siblingClicks !== pixelFixtureBefore.siblingClicks ||
          clickedFixtureState.siblingTextLength !== pixelFixtureBefore.siblingTextLength,
        independentNonInterruptionSample: pixelSystemInvariants,
      },
      postResizePixelAction: {
        ...postResizeClick,
        durationMs: postResizeActionDurationMs,
        resizedWindowWidth: afterPostResizeAction.windowWidth,
        resizedWindowHeight: afterPostResizeAction.windowHeight,
        capturedImageWidth: afterPostResizeAction.imageWidth,
        capturedImageHeight: afterPostResizeAction.imageHeight,
        independentFixturePostcondition: "only the exact fixture click counter changed functional state after resize",
        independentNonInterruptionSample: postResizeActionInvariants,
      },
      nativeTextDelivery: {
        ...nativeText,
        requestCharacters: NATIVE_TEXT_SUFFIX.length,
        requestUtf16CodeUnits: NATIVE_TEXT_SUFFIX.length,
        contentRetainedInEvidenceOutputs: false,
        temporaryScratchFixtureStateUsed: true,
        fixtureFieldFocusCount: nativeTextFocusedState.focusCount,
        exactFixtureReadBack: nativeTextFixtureState.semanticValue === `${SEMANTIC_VALUE}${NATIVE_TEXT_SUFFIX}`,
        priorSiblingMainAndFocused:
          exactTargetReceiverMatches(nativeTextReceiverBefore, Number(siblingWindow.id), false),
        priorSiblingRestored:
          exactTargetReceiverMatches(nativeTextReceiverAfter, Number(siblingWindow.id), false),
        siblingTextLengthBefore: nativeTextFocusedState.siblingTextLength,
        siblingTextLengthAfter: nativeTextFixtureState.siblingTextLength,
        siblingClicksBefore: nativeTextFocusedState.siblingClicks,
        siblingClicksAfter: nativeTextFixtureState.siblingClicks,
        siblingMutationObserved:
          nativeTextFixtureState.siblingTextLength !== nativeTextFocusedState.siblingTextLength ||
          nativeTextFixtureState.siblingClicks !== nativeTextFocusedState.siblingClicks,
        restoredToExpectedValue: nativeTextRestoredFixtureState.semanticValue === SEMANTIC_VALUE,
        fieldPreparationNonInterruptionSample: nativeTextSetupInvariants,
        independentNonInterruptionSample: nativeTextInvariants,
        restore: nativeTextRestore,
      },
      explicitCancellation: {
        callId: cancellationCallId,
        method: "computer.move",
        durationMs: CANCELED_MOVE_DURATION_MS,
        elapsedMs: cancellationElapsedMs,
        dispatchProof: {
          type: "target-routed-fixture-mouse-move",
          timeoutMs: CANCELLATION_DISPATCH_PROOF_TIMEOUT_MS,
          waitElapsedMs: cancellationDispatchProofElapsedMs,
          moveEventsBefore: cancellationFixtureBefore.moveEvents,
          moveEventsObserved: cancellationDispatchedFixtureState.moveEvents,
        },
        inProgressDuplicate: {
          httpStatus: inProgressDuplicate.status,
          code: inProgressDuplicate.body.error.code,
        },
        cancellationAccepted: {
          httpStatus: cancelAccepted.status,
          requested: cancelAccepted.body.cancellationRequested,
        },
        original: {
          httpStatus: canceledOriginal.status,
          code: canceledOriginal.body.error.code,
          taxonomy: canceledOriginal.body.taxonomy,
          replayed: false,
        },
        duplicateCancel: {
          httpStatus: duplicateCancel.status,
          code: duplicateCancel.body.error.code,
        },
        replay: {
          httpStatus: replayedCanceled.status,
          code: replayedCanceled.body.error.code,
          replayed: replayedCanceled.body.replayed,
          exactCachedBody: JSON.stringify(replayedWithoutMarker) === JSON.stringify(canceledOriginal.body),
        },
        changedRequestReuse: {
          httpStatus: reusedCallId.status,
          code: reusedCallId.body.error.code,
          taxonomy: reusedCallId.body.taxonomy,
        },
        publicRevocation: {
          exactHelperSessionPreserved: canceledState.computer.sessionId === hello.sessionId,
          observationCleared: canceledState.computerObservation === null,
          screenshotHttpStatus: clearedScreenshotResponse.status,
          screenshotErrorCode: clearedScreenshotBody.error.code,
          shareActive: canceledState.computer.share.active,
          shareStopped: canceledState.computer.share.stopped,
          reason: canceledState.computer.share.reason,
          code: canceledState.computer.share.code,
          stayedTornDownAfterThreeFramePeriods: canceledStateSettled.computerObservation === null &&
            canceledStateSettled.computer.share.active === false,
        },
        explicitStop: {
          active: explicitStop.active,
          stopped: explicitStop.stopped,
          reason: explicitStop.reason,
          observationCleared: stoppedState.computerObservation === null,
        },
        preRecoveryGate: {
          httpStatus: gatedOldFrameAction.status,
          code: gatedOldFrameAction.body.error.code,
          taxonomy: gatedOldFrameAction.body.taxonomy,
          helperFunctionalMutationObserved: false,
          surfaceRecreated: gatedState.computerObservation !== null ||
            gatedState.computer.share.active === true,
        },
        explicitRecovery: {
          method: "computer.observe",
          exactHelperSessionPreserved: recoveryObserve.state.computer.sessionId === hello.sessionId,
          oldFrameIdReused: recoveryObservation.frameId === canceledFrameId,
          exactWindowRecovered: recoveryObservation.windowId === targetWindow.id,
          shareActive: recoveryObservation.share.active,
          freshPersistentShare: {
            shareId: targetCloseShare.id,
            priorShareIdReused: targetCloseShare.id === firstShareId,
            exactHelperSessionPreserved:
              targetCloseShareStartState.computer.sessionId === hello.sessionId,
            recoveredOneShotFrameRetired:
              targetCloseShareStartObservation.frameId !== recoveryObservation.frameId,
            startObservationBoundToFreshShare:
              targetCloseShareStartObservation.share?.id === targetCloseShare.id,
            firstStreamFrameRetiredOneShotAuthority:
              targetCloseObservation.frameId !== recoveryObservation.frameId &&
              targetCloseObservation.frameId !== targetCloseShareStartObservation.frameId,
          },
        },
        staleFrameRefusal: {
          httpStatus: staleAction.status,
          code: staleAction.body.error.code,
          taxonomy: staleAction.body.taxonomy,
          recoveredFramePreserved: recoveredState.computerObservation.frameId === recoveryObservation.frameId,
        },
        fixtureMutationCounters: {
          clicksBefore: cancellationFixtureBefore.clicks,
          clicksAfter: finalFixtureState.clicks,
          semanticPressesBefore: cancellationFixtureBefore.semanticPresses,
          semanticPressesAfter: finalFixtureState.semanticPresses,
          resizeCountBefore: cancellationFixtureBefore.resizeCount,
          resizeCountAfter: finalFixtureState.resizeCount,
          focusCountBefore: cancellationFixtureBefore.focusCount,
          focusCountAfter: finalFixtureState.focusCount,
          moveEventsBefore: cancellationFixtureBefore.moveEvents,
          moveEventsAfter: finalFixtureState.moveEvents,
          expectedMoveInstrumentationAdvanced:
            finalFixtureState.moveEvents > cancellationFixtureBefore.moveEvents,
          semanticValueMatchedBefore: cancellationFixtureBefore.semanticValue === SEMANTIC_VALUE,
          semanticValueMatchedAfter: finalFixtureState.semanticValue === SEMANTIC_VALUE,
        },
        independentNonInterruptionSample: cancellationInvariants,
      },
      persistentShare: {
        shareId: firstShareId,
        fps: SHARE_FPS,
        captureMode: shareStart.captureMode,
        captureScope: shareStart.captureScope,
        captureBackend: CAPTURE_BACKEND,
        nativeStream: true,
        systemIndicatorPolicy: true,
        selectionMode: SELECTION_MODE,
        systemCursorExcluded: true,
        syntheticPointerComposited: shareStart.cursorComposited,
        ackPaced: shareStatus.ackPaced,
        backpressure: shareStatus.backpressure,
        initialSourceSequence: shareSamples[0].sourceSequence,
        finalSourceSequence: finalSample.sourceSequence,
        initialTransportSequence: shareSamples[0].sequence,
        finalTransportSequence: finalSample.sequence,
        sourceDroppedFrames: shareStatus.sourceDroppedFrames,
        transportDroppedFrames: shareStatus.transportDroppedFrames,
        lastAckedSequence: shareStatus.lastAckedSequence,
        sourceProgressDuringPixelAction: afterPixelAction.sourceSequence - beforePixelAction.sourceSequence,
        transportProgressDuringPixelAction: afterPixelAction.sequence - beforePixelAction.sequence,
        sourceDropsDuringPixelAction: afterPixelAction.sourceDroppedFrames - beforePixelAction.sourceDroppedFrames,
        resizeSurvived: true,
        postResizeActionSourceSequence: postResizeClick.sourceSequence,
        stopped: canceledState.computer.share.stopped,
        stopReason: canceledState.computer.share.reason,
        explicitStopWasIdempotent: explicitStop.stopped === false && explicitStop.reason === "not-active",
        samples: shareSamples,
      },
      samePidMultiWindowRouting: {
        primaryWindowId: targetWindow.id,
        siblingWindowId: siblingWindow.id,
        samePid: siblingWindow.pid === targetWindow.pid,
        startupSiblingMainAndFocused:
          exactTargetReceiverMatches(startupSiblingFocus, Number(siblingWindow.id), false),
        finalSiblingMainAndFocused:
          exactTargetReceiverMatches(finalTargetReceiver, Number(siblingWindow.id), false),
        primarySelectionRemainedExact: observed.state.computerObservation.windowId === targetWindow.id,
        positivePointer: {
          primaryMutationObserved: clickedFixtureState.clicks === pixelFixtureBefore.clicks + 1,
          siblingMutationObserved:
            clickedFixtureState.siblingClicks !== pixelFixtureBefore.siblingClicks ||
            clickedFixtureState.siblingTextLength !== pixelFixtureBefore.siblingTextLength,
          priorSiblingRestored:
            exactTargetReceiverMatches(pixelSiblingFocusAfter, Number(siblingWindow.id), false),
          activeRequestedReceiverObserved:
            exactTargetReceiverMatches(activeRequestedReceiver, Number(targetWindow.id), true),
          foregroundOwnerDuringActiveLease: {
            nsWorkspacePidUnchanged:
              activeRequestedReceiver.foregroundPID === pixelSystemBefore.foregroundPID,
            rawPsnAndPidUnchanged:
              activeRequestedReceiver.rawForegroundIdentityStable === true &&
              activeRequestedReceiver.rawForegroundPID === pixelSystemBefore.rawForegroundPID &&
              activeRequestedReceiver.rawForegroundPSN === pixelSystemBefore.rawForegroundPSN,
            exactAxWindowUnchanged:
              activeRequestedReceiver.foregroundAXFocusedWindowID ===
                pixelSystemBefore.foregroundAXFocusedWindowID &&
              activeRequestedReceiver.foregroundAXMainWindowID ===
                pixelSystemBefore.foregroundAXMainWindowID,
          },
          userOracle: pixelSystemInvariants,
        },
        positiveNativeText: {
          primaryExactReadBack: nativeTextFixtureState.semanticValue === `${SEMANTIC_VALUE}${NATIVE_TEXT_SUFFIX}`,
          siblingMutationObserved:
            nativeTextFixtureState.siblingTextLength !== nativeTextFocusedState.siblingTextLength ||
            nativeTextFixtureState.siblingClicks !== nativeTextFocusedState.siblingClicks,
          priorSiblingRestored:
            exactTargetReceiverMatches(nativeTextReceiverAfter, Number(siblingWindow.id), false),
          userOracle: nativeTextInvariants,
        },
        retainedScreenshots: {
          count: screenshots.length,
          primaryOnly: screenshots.every(
            (screenshot) => screenshot.windowId === targetWindow.id &&
              screenshot.windowId !== siblingWindow.id,
          ),
        },
        wrongSiblingNegative: {
          status: "unproven-live",
          attempted: false,
          productionHookUsed: false,
          focusRaceUsed: false,
          reason: "Forcing the sibling after helper focus preparation would require a timing race or a production-only hook; foregrounding the fixture would interrupt the user. The rig does none of those, so the pre-dispatch refusal remains a unit/source contract until a deterministic non-activating mechanism exists.",
        },
      },
      exactTargetClose: {
        shareId: targetCloseShare.id,
        frameId: targetCloseFrameId,
        windowId: targetWindow.id,
        pid: fixtureReady.pid,
        shareActiveAtClose: targetCloseObservation.share.active,
        beganAfterExplicitOneShotRecovery: true,
        recoveredOneShotFrameRetired:
          targetCloseShareStartObservation.frameId !== recoveryObservation.frameId,
        exactTargetTermination: fixtureTargetClosure,
        helperObservedTargetAbsent: !targetClosedStatus.windows.some(
          (window) => window.id === targetWindow.id && window.pid === fixtureReady.pid,
        ),
        helperSessionPreserved: targetClosedState.computer.sessionId === hello.sessionId,
        helperSpawnCount,
        terminalShare: {
          active: targetClosedState.computer.share.active,
          stopped: targetClosedState.computer.share.stopped,
          reason: targetClosedState.computer.share.reason,
          code: targetClosedState.computer.share.code,
          exactShareIdMatched: targetClosedState.computer.share.id === targetCloseShare.id,
        },
        authorityClear: {
          observationCleared: targetClosedState.computerObservation === null,
          screenshotHttpStatus: targetClosedScreenshotResponse.status,
          screenshotErrorCode: targetClosedScreenshotBody.error.code,
          stayedClearedAfterFramePeriods: TARGET_CLOSE_SETTLE_FRAME_PERIODS,
          queuedFrameRepublished: targetClosedSettledState.computerObservation !== null,
        },
        staleFrameRefusal: {
          httpStatus: targetClosedStaleAction.status,
          code: targetClosedStaleAction.body.error.code,
          taxonomy: targetClosedStaleAction.body.taxonomy,
          helperActionRelayed: false,
          surfaceRecreated: targetClosedAfterStaleAction.computerObservation !== null,
        },
        closedTargetObserveRefusal: {
          httpStatus: targetClosedObserve.status,
          code: targetClosedObserve.body.error.code,
          taxonomy: targetClosedObserve.body.taxonomy,
          surfaceRecreated: targetClosedAfterObserve.computerObservation !== null,
        },
        explicitStop: {
          active: targetCloseStop.active,
          stopped: targetCloseStop.stopped,
          reason: targetCloseStop.reason,
        },
        independentNonInterruptionSample: targetCloseInvariants,
      },
      independentNonInterruptionSample: independentInvariants,
      teardown: {
        helper: helperTeardown,
        helperConnected: disconnected.computerConnected,
        helperStateCleared: disconnected.computer === null,
        observationCleared: disconnected.computerObservation === null,
        server: serverTeardown,
        serverListenerClosed: true,
        fixture: fixtureTargetClosure,
        fixtureClosedDuringActiveShare: true,
      },
    },
    screenshots,
    assertions: {
      passed: checks.filter((item) => item.passed).length,
      failed: checks.filter((item) => !item.passed).length,
      total: checks.length,
      details: checks,
    },
    limitations: [
      "This run proves the supplied v0.12.35 macOS release-candidate archive, not an immutable GitHub release until those exact checksums are published.",
      "Each lane starts candidate execution only after a 30-second, 60-transition native SystemProbe epoch with cumulative pointer and keyboard HID counters plus sampled foreground, focus, cursor, and active-Space equality. The gate reduces pre-existing shared-seat contamination but cannot reserve the active login seat; all unchanged per-action and whole-run proofs still fail closed on later observed activity.",
      "The app-share chain proves an exact acceptance app/window/button action and sampled foreground, focus, Space, pointer, and keyboard-HID boundaries. It does not identify the controller/provider, continuously prove zero transient programmatic changes, grant product authority, or establish a separate input seat.",
      "Create-once marker filesystem steps are deadline-checked before and after each operation and use nonblocking/no-follow opens, but user-space JavaScript cannot preempt an operating-system filesystem call already in progress. A stalled local filesystem can delay fail-closed termination; it cannot produce a passing result after the deadline.",
      "systemIndicatorPolicy=true proves that the helper did not suppress ScreenCaptureKit's system indication; exact-window screenshots cannot prove a particular system banner was visible.",
      "Programmatic exact-window selection is authenticated bridge policy, not a native macOS content picker.",
      "The helper uses target-routed background input on the active user Space, not a separate login session, VM, virtual display, sandbox, or independent input seat.",
      "The same-PID positive gate uses two genuine fixture NSWindows and an independent read-only AXFocusedWindow probe. It proves primary pointer/text routing and restoration of the prior sibling while a sandwiched foreground PID plus AXFocusedWindow/AXFrontmost oracle remains unchanged.",
      "A live wrong-sibling-at-dispatch negative is intentionally unproven: changing focus after helper preparation would be a flaky race or require a production-only hook, while foregrounding or raising the fixture would violate the non-interruption contract. Production unit/source contracts retain the fail-closed receiver mismatch requirement.",
      "The fixture-only post-resize proof combines exact PID/window/frame/share binding, fixture counters, and non-interruption identities; the rig deliberately does not capture or fingerprint unrelated window contents.",
      "Generic AX invocation remains Partial in the product result; this deterministic fixture supplies separate target-side evidence and does not upgrade that product classification.",
      "Native typeText remains Unverifiable in the product result. This deterministic fixture adds an independent exact read-back, then restores the prior value through confirmed Accessibility setValue. The ephemeral test content is not retained in evidence outputs, but it exists temporarily in process memory and the scratch fixture-state file until cleanup.",
      "The long live-share click intentionally creates native source-frame replacement. A zero transport-drop count is valid when server acknowledgements keep pace.",
      "Explicit cancellation deliberately reports the in-flight move outcome as unknown. The rig proves target-routed dispatch, authority teardown, a pre-recovery server gate, explicit one-shot observation recovery, continued stale-frame refusal, and no functional fixture mutation beyond its bounded move-delivery counter; it does not relabel the canceled native trajectory as a confirmed no-op.",
      "After cancellation recovery, the rig starts a fresh persistent exact-window share and deliberately terminates only the spawned fixture process. Depending on whether target enumeration or ScreenCaptureKit reports closure first, the terminal share code is COMPUTER_NO_WINDOW or COMPUTER_CAPTURE_FAILED; both paths must identify the exact share, stop capture, clear server authority, and refuse the retired frame.",
      "Screen Recording and Accessibility permissions were already present. The rig did not request, approve, or modify macOS permissions.",
    ],
  };

  await persistResult(result);
  successfulResultWritten = true;
  log(`${checks.filter((item) => item.passed).length}/${checks.length} checks passed`);
}

if (preparePackageMode) {
  try {
    await preparePackage();
  } catch (error) {
    console.error(`Package preparation failed: ${sanitizePathDetail(error?.message || String(error))}`);
    process.exitCode = 1;
  }
} else {
  try {
    await main();
  } catch (error) {
    const failureDiagnostics = await collectFailureDiagnostics();
    const failure = {
    schemaVersion: 8,
    productVersion: EXPECTED_VERSION,
    status: "failed-release-candidate",
    evidenceClass: "release-candidate-negative-result",
    startedAt: laneStartedAt,
    capturedAt: new Date().toISOString(),
    fatal: sanitizePathDetail(error?.message || String(error)),
    helperSpawnCount,
    packageBinding: manifestBinding,
    releaseCandidateBinding: { ...releaseCandidateBinding },
    harnessSourceBinding: { ...harnessSourceBinding },
    capabilityBinding: { ...capabilityBinding },
    quietSeatStabilization: { ...quietSeatStabilization },
    screenshots,
    pointerEvidence: pointerEvidenceSummary(),
    appShareHandoff: pointerHandoffSummary(),
    failureDiagnostics,
    assertions: {
      passed: checks.filter((item) => item.passed).length,
      failed: checks.filter((item) => !item.passed).length,
      total: checks.length,
      details: checks,
    },
    };
    if (outputReserved) {
      await mkdir(outputDir, { recursive: true });
      await persistResult(failure);
    }
    log(`FATAL ${failure.fatal}`);
    process.exitCode = 1;
  } finally {
    await terminate(pointerHandoffProcess, "pointer handoff prompt");
    pointerHandoffProcess = null;
    await terminate(helperProcess, "helper");
    await terminate(serverProcess, "server");
    await terminate(fixtureProcess, "fixture");
    if (scratchDir?.startsWith(`${scratchParent}/lbb-v0.12.35-scstream-`)) {
      await rm(scratchDir, { recursive: true, force: true });
      log("scratch directory removed");
    }
    if (outputReserved) {
      const persistedLog = `${logLines.join("\n")}\n`;
      assertNoToken(persistedLog, "evidence log");
      assertNoRetainedNativeTextPayload(persistedLog, "evidence log");
      assertNoRetainedPointerRawData(persistedLog, "evidence log");
      await mkdir(outputDir, { recursive: true });
      await writeFile(logPath, persistedLog, { encoding: "utf8", flag: "wx", mode: 0o600 });
    }
    if (!successfulResultWritten && process.exitCode !== 1) process.exitCode = 1;
  }
}
