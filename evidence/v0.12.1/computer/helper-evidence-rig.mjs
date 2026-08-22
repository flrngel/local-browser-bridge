#!/usr/bin/env node

import { spawn, spawnSync } from "node:child_process";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { constants as fsConstants } from "node:fs";
import { createServer } from "node:net";
import { basename, dirname, join, resolve } from "node:path";

const EXPECTED_VERSION = "0.12.1";
const EXPECTED_ARCHIVE = `local-browser-bridge-v${EXPECTED_VERSION}-macos-universal.tar.gz`;
const FIXTURE_TITLE = "LBB v0.12.1 Persistent SCStream Evidence";
const SEMANTIC_VALUE = "v0.12.1-semantic-value";
const STATUS_BACKEND = "background-window/ax+skylight+screencapturekit-stream";
const CAPTURE_BACKEND = "macos-screencapturekit-scstream";
const SELECTION_MODE = "programmatic-exact-window";
const WAIT_STEP_MS = 100;
const SHARE_FPS = 10;
const LONG_PIXEL_ACTION_MS = 900;
const GENERATED_OUTPUT_NAMES = [
  "helper-results.json",
  "helper-rig.log",
  "computer-01-exact-window-observe.png",
  "computer-02-semantic-set-value.png",
  "computer-03-semantic-invoke.png",
  "computer-04-persistent-scstream-start.png",
  "computer-05-live-share-pixel-action.png",
  "computer-06-persistent-share-resize.png",
];

const [serverInput, helperInput, outputInput, scratchParentInput, archiveInput, sumsInput] = process.argv.slice(2);
if (!serverInput || !helperInput || !outputInput || !scratchParentInput || !archiveInput || !sumsInput) {
  console.error(
    "Usage: node helper-evidence-rig.mjs <server> <helper> <output-dir> <scratch-parent> <macos-archive> <SHA256SUMS.txt>",
  );
  process.exit(2);
}

const serverPath = resolve(serverInput);
const helperPath = resolve(helperInput);
const outputDir = resolve(outputInput);
const scratchParent = resolve(scratchParentInput);
const archivePath = resolve(archiveInput);
const sumsPath = resolve(sumsInput);
const fixtureSource = resolve(outputDir, "HelperEvidenceFixture.swift");
const systemProbeSource = resolve(outputDir, "SystemProbe.swift");
const resultsPath = join(outputDir, "helper-results.json");
const logPath = join(outputDir, "helper-rig.log");

const bearerToken = randomBytes(32).toString("base64url");
const logLines = [];
const checks = [];
const screenshots = [];
const shareSamples = [];
let scratchDir;
let serverProcess;
let helperProcess;
let fixtureProcess;
let port;
let helperSpawnCount = 0;
let successfulResultWritten = false;
let outputReserved = false;
let systemProbeBinary;
let fixtureStatePath;
let failureProbeBaseline;

function sanitizePathDetail(value) {
  let text = String(value);
  if (text.includes(bearerToken)) return "Sensitive failure detail withheld";
  for (const [path, replacement] of [
    [scratchDir, "[SCRATCH]"],
    [serverPath, basename(serverPath)],
    [helperPath, basename(helperPath)],
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

function run(commandName, args) {
  const result = spawnSync(commandName, args, { encoding: "utf8", maxBuffer: 4 * 1024 * 1024 });
  if (result.status !== 0) {
    const detail = sanitizePathDetail((result.stderr || result.stdout || "no diagnostic output").trim());
    throw new Error(`${basename(commandName)} failed: ${detail}`);
  }
  return result.stdout.trim();
}

async function sha256(path) {
  return createHash("sha256").update(await readFile(path)).digest("hex");
}

async function pathExists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
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

function processProbe(path) {
  return JSON.parse(run(path, []));
}

function systemInvariants(before, after) {
  return {
    foregroundUnchanged: before.foregroundPID > 0 && before.foregroundPID === after.foregroundPID,
    userFocusUnchanged: before.frontWindowID > 0 && before.frontWindowID === after.frontWindowID,
    cursorUnchanged:
      Math.abs(before.cursorX - after.cursorX) < 0.01 && Math.abs(before.cursorY - after.cursorY) < 0.01,
    spaceUnchanged: before.activeSpace > 0 && before.activeSpace === after.activeSpace,
  };
}

function allInvariantsHeld(invariants) {
  return [
    invariants?.foregroundUnchanged,
    invariants?.userFocusUnchanged,
    invariants?.cursorUnchanged,
    invariants?.spaceUnchanged,
  ].every((value) => value === true);
}

const FIXTURE_ACTIONS = new Set(["ready", "set-value", "semantic", "click", "resize"]);

function boundedCounter(value) {
  return Number.isSafeInteger(value) && value >= 0 ? value : null;
}

function fixtureCounterSnapshot(snapshot) {
  return {
    clicks: boundedCounter(snapshot?.clicks),
    semanticPresses: boundedCounter(snapshot?.semanticPresses),
    animationTick: boundedCounter(snapshot?.animationTick),
    resizeCount: boundedCounter(snapshot?.resizeCount),
    appliedControlSequence: boundedCounter(snapshot?.appliedControlSequence),
    semanticValueMatchesExpected: snapshot?.semanticValue === SEMANTIC_VALUE,
    lastAction: FIXTURE_ACTIONS.has(snapshot?.lastAction) ? snapshot.lastAction : "unrecognized",
  };
}

async function collectFailureDiagnostics() {
  const diagnostics = {
    stage: failureProbeBaseline?.stage || "notReached",
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
  return diagnostics;
}

function requireActionInvariants(label, action) {
  requireCheck(`${label} non-interruption invariants`, allInvariantsHeld(action.invariants), JSON.stringify(action.invariants));
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

async function apiState() {
  const response = await fetch(`http://127.0.0.1:${port}/api/state`, {
    headers: { Authorization: `Bearer ${bearerToken}` },
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

async function command(method, params = {}) {
  const response = await fetch(`http://127.0.0.1:${port}/api/v1/command`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${bearerToken}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ callId: randomUUID(), method, params }),
  });
  const body = await response.json();
  if (!response.ok || !body.ok) {
    const errorCode = body.error?.code || "unknown";
    const errorMessage = body.error?.message || "";
    throw new Error(`${method} returned HTTP ${response.status}: ${errorCode} ${errorMessage}`);
  }
  return body;
}

async function fixtureState(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

async function saveCurrentScreenshot(publicState, filename, expectedWindowId) {
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
  await writeFile(path, data);
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
  await writeFile(resultsPath, serialized);
}

async function main() {
  await mkdir(outputDir, { recursive: true });
  const existingOutputs = [];
  for (const name of GENERATED_OUTPUT_NAMES) {
    if (await pathExists(join(outputDir, name))) existingOutputs.push(name);
  }
  if (existingOutputs.length > 0) {
    throw new Error(`refusing to overwrite existing evidence outputs: ${existingOutputs.join(", ")}`);
  }
  outputReserved = true;
  await access(scratchParent, fsConstants.W_OK);
  scratchDir = await mkdtemp(join(scratchParent, "lbb-v0.12.1-scstream-"));
  const fixtureBinary = join(scratchDir, "helper-evidence-fixture");
  systemProbeBinary = join(scratchDir, "system-probe");
  fixtureStatePath = join(scratchDir, "fixture-state.json");
  const fixtureControlPath = join(scratchDir, "fixture-control.json");
  const archiveExtractRoot = join(scratchDir, "archive");
  const harnessSha256 = {
    runner: await sha256(resolve(outputDir, "helper-evidence-rig.mjs")),
    fixture: await sha256(fixtureSource),
    systemProbe: await sha256(systemProbeSource),
  };

  requireCheck("macOS host", run("uname", ["-s"]) === "Darwin", "Darwin");
  requireCheck("exact archive name", basename(archivePath) === EXPECTED_ARCHIVE, basename(archivePath));
  requireCheck("server version", exactVersion(serverPath, "local-browser-bridge") === EXPECTED_VERSION, EXPECTED_VERSION);
  requireCheck("helper version", exactVersion(helperPath, "local-computer-helper") === EXPECTED_VERSION, EXPECTED_VERSION);

  const serverSha256 = await sha256(serverPath);
  const helperSha256 = await sha256(helperPath);
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

  const archiveSha256 = await sha256(archivePath);
  const sums = await readFile(sumsPath, "utf8");
  const manifestEntry = sums
    .split(/\r?\n/)
    .map((line) => line.trim().split(/\s+/))
    .find((parts) => parts.at(-1)?.replace(/^\*/, "") === basename(archivePath));
  requireCheck("archive checksum listed", Boolean(manifestEntry), basename(archivePath));
  requireCheck("archive checksum matches", manifestEntry?.[0]?.toLowerCase() === archiveSha256, archiveSha256);

  const archiveEntries = run("tar", ["-tzf", archivePath]).split(/\r?\n/).filter(Boolean);
  requireCheck("archive paths are traversal-safe",
    archiveEntries.every((entry) => !entry.startsWith("/") && !entry.split("/").includes("..")),
    `${archiveEntries.length} safe entries`,
  );
  const verboseArchiveEntries = run("tar", ["-tvzf", archivePath]).split(/\r?\n/).filter(Boolean);
  requireCheck("archive contains only regular files and directories",
    verboseArchiveEntries.every((entry) => entry.startsWith("-") || entry.startsWith("d")),
    "no links or special filesystem entries",
  );
  requireCheck("archive contains exact server", archiveEntries.includes("local-browser-bridge"), "local-browser-bridge");
  requireCheck("archive contains exact helper",
    archiveEntries.includes("Local Computer Helper.app/Contents/MacOS/local-computer-helper"),
    "Local Computer Helper.app/Contents/MacOS/local-computer-helper",
  );
  await mkdir(archiveExtractRoot);
  run("tar", ["-xzf", archivePath, "-C", archiveExtractRoot]);
  const archivedServerPath = join(archiveExtractRoot, "local-browser-bridge");
  const archivedHelperPath = join(archiveExtractRoot, "Local Computer Helper.app", "Contents", "MacOS", "local-computer-helper");
  requireCheck("supplied server is archive-exact", (await sha256(archivedServerPath)) === serverSha256, serverSha256);
  requireCheck("supplied helper is archive-exact", (await sha256(archivedHelperPath)) === helperSha256, helperSha256);

  run("xcrun", ["swiftc", fixtureSource, "-o", fixtureBinary]);
  run("xcrun", ["swiftc", systemProbeSource, "-o", systemProbeBinary]);
  const permissionProbe = processProbe(systemProbeBinary);
  requireCheck("screen-capture permission preflight", permissionProbe.screenCaptureReady === true, "preexisting permission; no request was made");
  requireCheck("accessibility permission preflight", permissionProbe.accessibilityReady === true, "preexisting permission; no request was made");
  requireCheck("active Space probe available", permissionProbe.activeSpace > 0, "nonzero active Space identity");
  requireCheck("front-window probe available", permissionProbe.foregroundPID > 0 && permissionProbe.frontWindowID > 0, "exact front process/window identity available");

  fixtureProcess = spawn(fixtureBinary, [], {
    env: {
      ...process.env,
      LBB_FIXTURE_STATE: fixtureStatePath,
      LBB_FIXTURE_CONTROL: fixtureControlPath,
    },
    stdio: "ignore",
  });
  const fixtureReady = await waitFor("fixture state", async () => {
    try {
      const snapshot = await fixtureState(fixtureStatePath);
      return snapshot.lastAction === "ready" && snapshot.animationTick >= 2 && snapshot;
    } catch {
      return null;
    }
  });
  requireCheck("fixture process identity", fixtureReady.pid === fixtureProcess.pid, "state belongs to the spawned fixture");
  const systemBefore = processProbe(systemProbeBinary);

  port = await freePort();
  const serverEnvironment = {
    ...process.env,
    LBB_DISABLE_UPDATE_CHECK: "true",
    LBB_PORT: String(port),
    LBB_TOKEN: bearerToken,
  };
  serverProcess = spawn(serverPath, ["--no-update-check"], { env: serverEnvironment, stdio: "ignore" });
  delete serverEnvironment.LBB_TOKEN;
  await waitFor("server health", healthReachable);

  const helperEnvironment = {
    ...process.env,
    LBB_DISABLE_UPDATE_CHECK: "true",
    LBB_PORT: String(port),
    LBB_TOKEN: bearerToken,
  };
  startHelperOnce(helperEnvironment);
  delete helperEnvironment.LBB_TOKEN;

  const connected = await waitFor("packaged helper handshake", async () => {
    const snapshot = await apiState();
    return snapshot.computerConnected === true && snapshot.computer?.version === EXPECTED_VERSION && snapshot;
  });
  const hello = connected.computer;
  requireCheck("packaged helper compatible", hello.compatible === true, `protocol-compatible helper ${hello.version}`);
  requireCheck("single helper spawn", helperSpawnCount === 1, `${helperSpawnCount} packaged helper process`);
  requireCheck("acknowledged share advertised", hello.capabilities.includes("computer.share.ack"), "computer.share.ack present");
  requireCheck("native stream advertised", hello.capabilities.includes("computer.capture.native-stream.v1"), "computer.capture.native-stream.v1 present");

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
  requireCheck("fixture is a background target",
    targetWindow.focused === false && systemBefore.foregroundPID !== fixtureReady.pid,
    "the fixture neither owns the foreground process nor the front window",
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
  failureProbeBaseline = {
    stage: "liveSharePixelAction",
    system: processProbe(systemProbeBinary),
    fixture: await fixtureState(fixtureStatePath),
  };
  const clickBody = await command("computer.click", {
    frameId: observation.frameId,
    x: clickX,
    y: clickY,
    button: "left",
    clickCount: 1,
    durationMs: LONG_PIXEL_ACTION_MS,
  });
  const click = actionSummary(clickBody);
  const clickedFixtureState = await waitFor("fixture pixel click", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.clicks === 1 && snapshot;
  });
  requireCheck("background pixel click target-side proof", clickedFixtureState.lastAction === "click", "fixture click counter advanced to 1");
  requireCheck("pixel click conservatively classified", click.effect === "Unverifiable", click.effect);
  requireCheck("pixel action bound to persistent share",
    click.shareId === firstShareId && click.sourceSequence === beforePixelAction.sourceSequence,
    `share=${click.shareId === firstShareId}, source=${click.sourceSequence}`,
  );
  requireActionInvariants("live-share pixel click", click);

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

  const shareStopBody = await command("computer.share.stop");
  const shareStop = shareStopBody.result;
  requireCheck("live share stopped",
    shareStop.active === false && shareStop.stopped === true &&
      shareStop.id === firstShareId && shareStop.reason === "requested",
    shareStop.reason,
  );
  const stoppedState = await apiState();
  requireCheck("share stop revoked observation", stoppedState.computerObservation === null, "computer observation and frame authority cleared");

  const finalFixtureState = await fixtureState(fixtureStatePath);
  requireCheck("fixture final state",
    finalFixtureState.clicks === 1 && finalFixtureState.semanticPresses === 1 &&
      finalFixtureState.semanticValue === SEMANTIC_VALUE && finalFixtureState.resizeCount === 1,
    JSON.stringify({
      clicks: finalFixtureState.clicks,
      semanticPresses: finalFixtureState.semanticPresses,
      semanticValue: finalFixtureState.semanticValue,
      resizeCount: finalFixtureState.resizeCount,
    }),
  );

  const systemAfter = processProbe(systemProbeBinary);
  const independentInvariants = systemInvariants(systemBefore, systemAfter);
  requireCheck("independent foreground/focus/cursor/Space invariants",
    allInvariantsHeld(independentInvariants),
    JSON.stringify(independentInvariants),
  );

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
  const fixtureTeardown = await terminate(fixtureProcess, "fixture");
  fixtureProcess = null;
  requireCheck("fixture remained alive until controlled teardown", fixtureTeardown.requested === true, "SIGTERM was sent by the rig");

  const finalSample = shareSamples.at(-1);
  const result = {
    schemaVersion: 2,
    productVersion: EXPECTED_VERSION,
    status: "passed-release-candidate",
    evidenceClass: "exact-release-candidate-package-live-observation",
    capturedAt: new Date().toISOString(),
    candidateNotice: "This becomes release evidence only after the supplied checksum manifest and archive are published immutably for v0.12.1.",
    environment: {
      operatingSystem: run("sw_vers", ["-productVersion"]),
      architecture: run("uname", ["-m"]),
      backend: status.backend,
      semanticMode: observation.semanticMode,
      sessionMode: status.sessionMode,
      screenCapturePermissionPreexisting: permissionProbe.screenCaptureReady,
      accessibilityPermissionPreexisting: permissionProbe.accessibilityReady,
      permissionRequestPerformed: false,
    },
    package: {
      archive: {
        file: basename(archivePath),
        sha256: archiveSha256,
        checksumManifestMatched: true,
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
      packagedHelperSpawnCount: helperSpawnCount,
    },
    fixture: {
      application: targetWindow.appName,
      windowTitle: targetWindow.title,
      windowId: targetWindow.id,
      pid: targetWindow.pid,
      finalState: {
        clicks: finalFixtureState.clicks,
        semanticPresses: finalFixtureState.semanticPresses,
        semanticValue: finalFixtureState.semanticValue,
        animationTick: finalFixtureState.animationTick,
        resizeCount: finalFixtureState.resizeCount,
        contentWidth: resizedFixtureState.contentWidth,
        contentHeight: resizedFixtureState.contentHeight,
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
        independentFixturePostcondition: "click counter advanced exactly once",
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
        stopped: shareStop.stopped,
        stopReason: shareStop.reason,
        samples: shareSamples,
      },
      independentNonInterruptionSample: independentInvariants,
      teardown: {
        helper: helperTeardown,
        helperConnected: disconnected.computerConnected,
        helperStateCleared: disconnected.computer === null,
        observationCleared: disconnected.computerObservation === null,
        server: serverTeardown,
        serverListenerClosed: true,
        fixture: fixtureTeardown,
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
      "This run proves the supplied v0.12.1 macOS release-candidate archive, not an immutable GitHub release until those exact checksums are published.",
      "systemIndicatorPolicy=true proves that the helper did not suppress ScreenCaptureKit's system indication; exact-window screenshots cannot prove a particular system banner was visible.",
      "Programmatic exact-window selection is authenticated bridge policy, not a native macOS content picker.",
      "The helper uses target-routed background input on the active user Space, not a separate login session, VM, virtual display, sandbox, or independent input seat.",
      "Generic AX invocation remains Partial in the product result; this deterministic fixture supplies separate target-side evidence and does not upgrade that product classification.",
      "The long live-share click intentionally creates native source-frame replacement. A zero transport-drop count is valid when server acknowledgements keep pace.",
      "Screen Recording and Accessibility permissions were already present. The rig did not request, approve, or modify macOS permissions.",
    ],
  };

  await persistResult(result);
  successfulResultWritten = true;
  log(`${checks.filter((item) => item.passed).length}/${checks.length} checks passed`);
}

try {
  await main();
} catch (error) {
  const failureDiagnostics = await collectFailureDiagnostics();
  const failure = {
    schemaVersion: 2,
    productVersion: EXPECTED_VERSION,
    status: "failed-release-candidate",
    evidenceClass: "release-candidate-negative-result",
    capturedAt: new Date().toISOString(),
    fatal: sanitizePathDetail(error?.message || String(error)),
    helperSpawnCount,
    screenshots,
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
  await terminate(helperProcess, "helper");
  await terminate(serverProcess, "server");
  await terminate(fixtureProcess, "fixture");
  if (scratchDir?.startsWith(`${scratchParent}/lbb-v0.12.1-scstream-`)) {
    await rm(scratchDir, { recursive: true, force: true });
    log("scratch directory removed");
  }
  if (outputReserved) {
    const persistedLog = `${logLines.join("\n")}\n`;
    assertNoToken(persistedLog, "evidence log");
    await mkdir(outputDir, { recursive: true });
    await writeFile(logPath, persistedLog);
  }
  if (!successfulResultWritten && process.exitCode !== 1) process.exitCode = 1;
}
