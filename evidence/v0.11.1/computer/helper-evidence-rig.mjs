#!/usr/bin/env node

import { spawn, spawnSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { basename, join, resolve } from "node:path";
import { tmpdir } from "node:os";

const EXPECTED_VERSION = "0.11.1";
const FIXTURE_TITLE = "LBB v0.11.1 Helper Evidence";
const SEMANTIC_VALUE = "packaged-helper-408";
const WAIT_STEP_MS = 100;

const [serverInput, helperInput, outputInput, scratchParentInput, archiveInput, sumsInput] = process.argv.slice(2);
if (!serverInput || !helperInput || !outputInput) {
  console.error(
    "Usage: node helper-evidence-rig.mjs <server> <helper> <output-dir> [scratch-parent] [macos-archive] [SHA256SUMS.txt]",
  );
  process.exit(2);
}

const serverPath = resolve(serverInput);
const helperPath = resolve(helperInput);
const outputDir = resolve(outputInput);
const scratchParent = resolve(scratchParentInput || tmpdir());
const archivePath = archiveInput ? resolve(archiveInput) : null;
const sumsPath = sumsInput ? resolve(sumsInput) : null;
const fixtureSource = resolve(outputDir, "HelperEvidenceFixture.swift");
const systemProbeSource = resolve(outputDir, "SystemProbe.swift");
const resultsPath = join(outputDir, "helper-results.json");
const logPath = join(outputDir, "helper-rig.log");

const logLines = [];
const checks = [];
const token = randomBytes(32).toString("base64url");
let scratchDir;
let serverProcess;
let helperProcess;
let fixtureProcess;
let port;

function log(message) {
  const line = `${new Date().toISOString()} ${message}`;
  logLines.push(line);
  console.log(line);
}

function check(name, passed, detail) {
  checks.push({ name, passed: Boolean(passed), detail });
  log(`${passed ? "PASS" : "FAIL"} ${name}: ${detail}`);
  return Boolean(passed);
}

function requireCheck(name, passed, detail) {
  if (!check(name, passed, detail)) {
    throw new Error(`${name}: ${detail}`);
  }
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
  throw new Error(`${description} timed out${lastError ? `: ${lastError.message}` : ""}`);
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

function run(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(`${basename(command)} failed: ${(result.stderr || result.stdout).trim()}`);
  }
  return result.stdout.trim();
}

async function sha256(path) {
  return createHash("sha256").update(await readFile(path)).digest("hex");
}

function pngDimensions(data) {
  const signature = "89504e470d0a1a0a";
  if (data.length < 24 || data.subarray(0, 8).toString("hex") !== signature) {
    throw new Error("captured screenshot is not a PNG");
  }
  return { width: data.readUInt32BE(16), height: data.readUInt32BE(20) };
}

function exactVersion(path, executableName) {
  return run(path, ["--version"]).replace(`${executableName} `, "");
}

function processProbe(path) {
  return JSON.parse(run(path, []));
}

async function terminate(child, label) {
  if (!child || child.exitCode !== null) return;
  child.kill("SIGTERM");
  const exited = await Promise.race([
    new Promise((resolveExit) => child.once("exit", () => resolveExit(true))),
    delay(2_000).then(() => false),
  ]);
  if (!exited && child.exitCode === null) {
    child.kill("SIGKILL");
    await new Promise((resolveExit) => child.once("exit", resolveExit));
  }
  log(`${label} stopped`);
}

async function state() {
  const response = await fetch(`http://127.0.0.1:${port}/api/state`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) throw new Error(`state returned HTTP ${response.status}`);
  return (await response.json()).state;
}

async function command(method, params = {}) {
  const response = await fetch(`http://127.0.0.1:${port}/api/v1/command`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ method, params }),
  });
  const body = await response.json();
  if (!response.ok || !body.ok) {
    throw new Error(`${method} returned HTTP ${response.status}: ${body.error?.code || "unknown"} ${body.error?.message || ""}`);
  }
  return body;
}

async function fixtureState(path) {
  return JSON.parse(await readFile(path, "utf8"));
}

async function saveCurrentScreenshot(publicState, filename) {
  const observation = publicState.computerObservation;
  if (!observation?.screenshotUrl) throw new Error("current computer observation has no screenshot URL");
  const response = await fetch(`http://127.0.0.1:${port}${observation.screenshotUrl}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) throw new Error(`screenshot returned HTTP ${response.status}`);
  const data = Buffer.from(await response.arrayBuffer());
  const path = join(outputDir, filename);
  await writeFile(path, data);
  return {
    file: filename,
    sha256: createHash("sha256").update(data).digest("hex"),
    bytes: data.length,
    ...pngDimensions(data),
  };
}

function actionSummary(body) {
  const result = body.result || {};
  return {
    ok: body.ok === true,
    actionId: result.actionId,
    effect: result.effect,
    deliveryMode: result.deliveryMode,
    frameId: result.frameId,
    invariants: result.invariants,
    evidence: result.evidence,
    backendEffect: result.backendEffect,
  };
}

async function main() {
  await mkdir(outputDir, { recursive: true });
  scratchDir = await mkdtemp(join(scratchParent, "lbb-408-helper-"));
  const fixtureBinary = join(scratchDir, "helper-evidence-fixture");
  const systemProbeBinary = join(scratchDir, "system-probe");
  const fixtureStatePath = join(scratchDir, "fixture-state.json");

  requireCheck("server version", exactVersion(serverPath, "local-browser-bridge") === EXPECTED_VERSION, EXPECTED_VERSION);
  requireCheck("helper version", exactVersion(helperPath, "local-computer-helper") === EXPECTED_VERSION, EXPECTED_VERSION);
  const serverSha256 = await sha256(serverPath);
  const helperSha256 = await sha256(helperPath);
  const architectures = run("lipo", ["-archs", serverPath]).split(/\s+/).sort();
  const helperArchitectures = run("lipo", ["-archs", helperPath]).split(/\s+/).sort();
  requireCheck("server universal architecture", architectures.join(",") === "arm64,x86_64", architectures.join(","));
  requireCheck("helper universal architecture", helperArchitectures.join(",") === "arm64,x86_64", helperArchitectures.join(","));
  run("codesign", ["--verify", "--strict", serverPath]);
  run("codesign", ["--verify", "--strict", helperPath]);
  check("strict code-signature verification", true, "server and helper passed codesign --verify --strict");

  let archive = null;
  if (archivePath) {
    const archiveSha256 = await sha256(archivePath);
    let checksumManifestMatched = null;
    if (sumsPath) {
      const sums = await readFile(sumsPath, "utf8");
      const expected = sums
        .split(/\r?\n/)
        .map((line) => line.trim().split(/\s+/))
        .find((parts) => parts.at(-1) === basename(archivePath))?.[0];
      checksumManifestMatched = expected === archiveSha256;
      requireCheck("published archive checksum", checksumManifestMatched, archiveSha256);
    }
    archive = { file: basename(archivePath), sha256: archiveSha256, checksumManifestMatched };
  }

  run("swiftc", [fixtureSource, "-o", fixtureBinary]);
  run("swiftc", [systemProbeSource, "-o", systemProbeBinary]);
  const systemBefore = processProbe(systemProbeBinary);
  requireCheck("screen-capture permission preflight", systemBefore.screenCaptureReady === true, "permission was already granted; no request was made");
  requireCheck("accessibility permission preflight", systemBefore.accessibilityReady === true, "permission was already granted; no request was made");

  fixtureProcess = spawn(fixtureBinary, [], {
    env: { ...process.env, LBB_FIXTURE_STATE: fixtureStatePath },
    stdio: "ignore",
  });
  await waitFor("fixture state", async () => {
    try {
      const snapshot = await fixtureState(fixtureStatePath);
      return snapshot.lastAction === "ready" && snapshot;
    } catch {
      return null;
    }
  });

  port = await freePort();
  const childEnvironment = {
    ...process.env,
    LBB_DISABLE_UPDATE_CHECK: "true",
    LBB_PORT: String(port),
    LBB_TOKEN: token,
  };
  serverProcess = spawn(serverPath, ["--no-update-check"], { env: childEnvironment, stdio: "ignore" });
  await waitFor("server health", async () => {
    const response = await fetch(`http://127.0.0.1:${port}/health`).catch(() => null);
    return response?.ok;
  });
  helperProcess = spawn(helperPath, [], { env: childEnvironment, stdio: "ignore" });

  const connected = await waitFor("packaged helper handshake", async () => {
    const snapshot = await state();
    return snapshot.computer?.version === EXPECTED_VERSION && snapshot;
  });
  const hello = connected.computer;
  requireCheck("packaged helper compatible", hello.compatible === true, `protocol-compatible helper ${hello.version}`);
  requireCheck("acknowledged share advertised", hello.capabilities.includes("computer.share.ack"), "computer.share.ack present");

  const statusBody = await command("computer.status");
  const status = statusBody.result;
  requireCheck("helper status backend", status.backend === "background-window/skylight+cgwindow", status.backend);
  requireCheck("semantic backend ready", status.semanticReady === true, "macos-accessibility is ready without prompting");
  const targetWindow = status.windows.find((window) => window.title === FIXTURE_TITLE);
  requireCheck("fixture exact window discovered", Boolean(targetWindow), targetWindow ? `${targetWindow.appName} — ${targetWindow.title}` : "missing");

  const observed = await command("computer.observe", { windowId: targetWindow.id });
  let observation = observed.state.computerObservation;
  requireCheck("exact fixture window observed", observation.windowId === targetWindow.id && observation.windowTitle === FIXTURE_TITLE, `${observation.imageWidth}x${observation.imageHeight}`);
  requireCheck("semantic snapshot available", observation.semanticAvailable === true && observation.elements.length > 0, `${observation.elements.length} elements`);
  const initialScreenshot = await saveCurrentScreenshot(observed.state, "computer-01-packaged-observe.png");

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
  requireCheck("semantic setValue confirmed", setValue.effect === "Confirmed" && setValue.backendEffect?.postcondition === "value-confirmed", `${setValue.effect}/${setValue.backendEffect?.postcondition}`);
  await waitFor("fixture semantic value", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.semanticValue === SEMANTIC_VALUE && snapshot;
  });

  observation = setValueBody.state.computerObservation;
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
  requireCheck("semantic invoke fixture effect", semanticFixtureState.lastAction === "semantic", "fixture semantic counter advanced to 1");
  requireCheck("semantic invoke confirmed", invoke.effect === "Confirmed" && invoke.backendEffect?.postcondition === "element-state-changed", `${invoke.effect}/${invoke.backendEffect?.postcondition}`);

  observation = invokeBody.state.computerObservation;
  const shareStartBody = await command("computer.share.start", {
    windowId: targetWindow.id,
    fps: 10,
  });
  const shareStart = shareStartBody.result;
  requireCheck("live share started", shareStart.active === true && shareStart.captureScope === "exact-window", `${shareStart.fps} fps exact-window`);
  requireCheck("live share ack pacing", shareStart.ackPaced === true && shareStart.backpressure === "latest-frame-wins", `${shareStart.backpressure}, ackPaced=${shareStart.ackPaced}`);

  const sharingState = await waitFor("acknowledged share frames", async () => {
    const snapshot = await state();
    const share = snapshot.computerObservation?.share;
    return share?.active && share.sequence >= 2 && share.lastAckedSequence >= 1 && snapshot;
  }, 20_000);
  observation = sharingState.computerObservation;
  const clickX = Math.round(observation.imageWidth / 2);
  const clickY = Math.max(0, observation.imageHeight - 80);
  const clickBody = await command("computer.click", {
    frameId: observation.frameId,
    x: clickX,
    y: clickY,
    button: "left",
    clickCount: 1,
    durationMs: 100,
  });
  const click = actionSummary(clickBody);
  const clickedFixtureState = await waitFor("fixture pixel click", async () => {
    const snapshot = await fixtureState(fixtureStatePath);
    return snapshot.clicks === 1 && snapshot;
  });
  requireCheck("pixel click delivered to fixture", clickedFixtureState.lastAction === "click", "fixture click counter advanced to 1");
  requireCheck("pixel click conservatively classified", click.effect === "Unverifiable", click.effect);
  const held = click.invariants && [
    click.invariants.foregroundUnchanged,
    click.invariants.userFocusUnchanged,
    click.invariants.cursorUnchanged,
    click.invariants.spaceUnchanged,
  ].every(Boolean);
  requireCheck("pixel delivery invariants held", held, "foreground, focus, hardware cursor, and Space remained unchanged");

  const activeShare = await waitFor("post-action share frame", async () => {
    const snapshot = await state();
    const share = snapshot.computerObservation?.share;
    return share?.active && share.sequence > observation.share.sequence && snapshot;
  }, 20_000);
  const finalScreenshot = await saveCurrentScreenshot(activeShare, "computer-02-share-action.png");
  const shareStatusBody = await command("computer.share.status");
  const shareStatus = shareStatusBody.result;
  requireCheck("live share sequence advanced", shareStatus.sequence >= 3 && shareStatus.lastAckedSequence >= 1, `sequence=${shareStatus.sequence}, ack=${shareStatus.lastAckedSequence}`);
  requireCheck("live share remained ack paced", shareStatus.ackPaced === true && shareStatus.backpressure === "latest-frame-wins", `${shareStatus.backpressure}, dropped=${shareStatus.droppedFrames}`);
  const shareStopBody = await command("computer.share.stop");
  const shareStop = shareStopBody.result;
  requireCheck("live share stopped", shareStop.active === false && shareStop.stopped === true && shareStop.reason === "requested", shareStop.reason);

  const finalFixtureState = await fixtureState(fixtureStatePath);
  requireCheck("fixture final state", finalFixtureState.clicks === 1 && finalFixtureState.semanticPresses === 1 && finalFixtureState.semanticValue === SEMANTIC_VALUE, JSON.stringify({ clicks: finalFixtureState.clicks, semanticPresses: finalFixtureState.semanticPresses, semanticValue: finalFixtureState.semanticValue }));
  const systemAfter = processProbe(systemProbeBinary);
  requireCheck("independent foreground unchanged", systemAfter.foregroundApplication === systemBefore.foregroundApplication, `${systemBefore.foregroundApplication} -> ${systemAfter.foregroundApplication}`);
  requireCheck("independent hardware cursor unchanged", systemAfter.cursorX === systemBefore.cursorX && systemAfter.cursorY === systemBefore.cursorY, `${systemBefore.cursorX},${systemBefore.cursorY} -> ${systemAfter.cursorX},${systemAfter.cursorY}`);

  await terminate(helperProcess, "helper");
  helperProcess = null;
  const disconnected = await waitFor("helper transport teardown", async () => {
    const snapshot = await state();
    return snapshot.computerConnected === false && snapshot;
  });
  requireCheck("helper transport disconnected cleanly", disconnected.computer === null && disconnected.computerObservation === null, "published helper session and observation were revoked");

  const result = {
    schemaVersion: 1,
    productVersion: EXPECTED_VERSION,
    evidenceClass: "exact-published-package-live-observation",
    capturedAt: new Date().toISOString(),
    environment: {
      operatingSystem: run("sw_vers", ["-productVersion"]),
      architecture: run("uname", ["-m"]),
      backend: status.backend,
      semanticMode: observation.semanticMode,
      sessionMode: status.sessionMode,
      screenCapturePermissionPreexisting: systemBefore.screenCaptureReady,
      accessibilityPermissionPreexisting: systemBefore.accessibilityReady,
      permissionRequestPerformed: false,
    },
    package: {
      archive,
      serverVersion: EXPECTED_VERSION,
      helperVersion: EXPECTED_VERSION,
      serverSha256,
      helperSha256,
      serverArchitectures: architectures,
      helperArchitectures,
      strictCodeSignatureVerification: "passed",
    },
    fixture: {
      application: targetWindow.appName,
      windowTitle: targetWindow.title,
      windowId: targetWindow.id,
      imageWidth: observation.imageWidth,
      imageHeight: observation.imageHeight,
      semanticElementsObserved: observation.elements.length,
      finalState: {
        clicks: finalFixtureState.clicks,
        semanticPresses: finalFixtureState.semanticPresses,
        semanticValue: finalFixtureState.semanticValue,
        lastAction: finalFixtureState.lastAction,
      },
    },
    checks: {
      helperHandshake: {
        compatible: hello.compatible,
        version: hello.version,
        protocolVersion: hello.protocolVersion,
        shareAckAdvertised: hello.capabilities.includes("computer.share.ack"),
      },
      exactWindowObserve: {
        passed: true,
        semanticAvailable: true,
        semanticElementCount: initialScreenshot ? observed.state.computerObservation.elements.length : 0,
      },
      semanticSetValue: setValue,
      semanticInvoke: invoke,
      pixelClick: click,
      liveShare: {
        started: shareStart.active,
        fps: shareStart.fps,
        captureScope: shareStart.captureScope,
        cursorComposited: shareStart.cursorComposited,
        ackPaced: shareStatus.ackPaced,
        backpressure: shareStatus.backpressure,
        observedSequence: shareStatus.sequence,
        lastAckedSequence: shareStatus.lastAckedSequence,
        droppedFrames: shareStatus.droppedFrames,
        stopped: shareStop.stopped,
        stopReason: shareStop.reason,
      },
      independentNonInterruptionSample: {
        foregroundBefore: systemBefore.foregroundApplication,
        foregroundAfter: systemAfter.foregroundApplication,
        hardwareCursorBefore: [systemBefore.cursorX, systemBefore.cursorY],
        hardwareCursorAfter: [systemAfter.cursorX, systemAfter.cursorY],
      },
      transportTeardown: {
        computerConnected: disconnected.computerConnected,
        computerStateCleared: disconnected.computer === null,
        observationCleared: disconnected.computerObservation === null,
      },
    },
    screenshots: [initialScreenshot, finalScreenshot],
    assertions: {
      passed: checks.filter((item) => item.passed).length,
      failed: checks.filter((item) => !item.passed).length,
      total: checks.length,
      details: checks,
    },
    limitations: [
      "This run covers the packaged macOS helper on the active macOS Space; it is not Windows UI Automation runtime evidence.",
      "The helper uses background-window routing on the active desktop, not a separate VM, RDP seat, or isolated operating-system desktop.",
      "The pixel action remains classified Unverifiable by the product; the fixture-side click counter is independent evidence for this deterministic target only.",
      "Screen Recording and Accessibility permissions were already present. The rig did not request, approve, or modify macOS permissions.",
      "The rig verifies the supplied archive checksum when SHA256SUMS.txt is provided, but GitHub attestation verification is an external release-verification step.",
    ],
  };

  const serialized = `${JSON.stringify(result, null, 2)}\n`;
  if (serialized.includes(token)) throw new Error("refusing to persist the bearer token");
  await writeFile(resultsPath, serialized);
  log(`${checks.filter((item) => item.passed).length}/${checks.length} checks passed`);
}

try {
  await main();
} catch (error) {
  log(`FATAL ${error.stack || error.message}`);
  process.exitCode = 1;
} finally {
  await terminate(helperProcess, "helper");
  await terminate(serverProcess, "server");
  await terminate(fixtureProcess, "fixture");
  if (scratchDir?.startsWith(`${scratchParent}/lbb-408-helper-`)) {
    await rm(scratchDir, { recursive: true, force: true });
    log("scratch directory removed");
  }
  const persistedLog = `${logLines.join("\n")}\n`;
  if (persistedLog.includes(token)) {
    console.error("Refusing to persist a log containing the bearer token");
  } else {
    await mkdir(outputDir, { recursive: true });
    await writeFile(logPath, persistedLog);
  }
}
