#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  chmod,
  constants,
  link,
  lstat,
  mkdtemp,
  mkdir,
  open,
  readFile,
  readdir,
  realpath,
  rm,
  unlink,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";
import { deflateSync, inflateSync } from "node:zlib";

const PRODUCT_VERSION = "0.12.20";
const RESULT_SCHEMA_VERSION = 6;
const AGGREGATE_SCHEMA_VERSION = 1;
const OUTPUT_FILE = "macos-acceptance.json";
const MAX_FRESH_AGE_MS = 12 * 60 * 60 * 1_000;
const FUTURE_TOLERANCE_MS = 5_000;
const FILE_TIMESTAMP_TOLERANCE_MS = 10_000;
const MAX_RESULT_BYTES = 8 * 1024 * 1024;
const MAX_LOG_BYTES = 8 * 1024 * 1024;
const MAX_MARKER_BYTES = 16 * 1024;
const MAX_SCREENSHOT_BYTES = 64 * 1024 * 1024;
const MAX_MOTION_SPAN_MS = 300_000;
const MAX_REQUEST_TO_COMPLETE_MS = 310_000;
const QUIET_SEAT_REQUIRED_STABLE_MS = 30_000;
const QUIET_SEAT_MAXIMUM_WAIT_MS = 30 * 60_000;
const QUIET_SEAT_SAMPLE_INTERVAL_MS = 500;
const QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS = 60;
const MAX_LANE_DURATION_MS = 2 * 60 * 60 * 1_000;
const MAX_DELIBERATE_REVIEW_DELAY_MS = 30 * 60 * 1_000;
const MAX_PID = 2_147_483_647;
const IS_WINDOWS = process.platform === "win32";
const POSIX_PERMISSION_METADATA_AVAILABLE = !IS_WINDOWS && typeof process.getuid === "function";
const FINALIZER_SOURCE_PATH = fileURLToPath(import.meta.url);
const FINALIZER_SOURCE_SHA256 = createHash("sha256")
  .update(await readFile(FINALIZER_SOURCE_PATH))
  .digest("hex");

const SCREENSHOT_FILES = [
  "computer-01-exact-window-observe.png",
  "computer-02-semantic-set-value.png",
  "computer-03-semantic-invoke.png",
  "computer-04-persistent-scstream-start.png",
  "computer-05-live-share-pixel-action.png",
  "computer-06-persistent-share-resize.png",
];
const BASE_LANE_FILES = [
  ...SCREENSHOT_FILES,
  "helper-results.json",
  "helper-rig.log",
].sort();
const REQUEST_MARKER = "operator/macos-pointer-concurrency-handoff-request.json";
const COMPLETE_MARKER = "operator/macos-pointer-concurrency-handoff-complete.json";
const DELIBERATE_LANE_FILES = [
  ...BASE_LANE_FILES,
  REQUEST_MARKER,
  COMPLETE_MARKER,
].sort();
const ARCHIVE_FILE = `local-browser-bridge-v${PRODUCT_VERSION}-macos-universal.tar.gz`;
const CANDIDATE_NOTICE =
  `This becomes release evidence only after the supplied checksum manifest and archive are published immutably for v${PRODUCT_VERSION}.`;

const RESULT_FIELDS = [
  "schemaVersion",
  "productVersion",
  "status",
  "evidenceClass",
  "startedAt",
  "capturedAt",
  "candidateNotice",
  "releaseCandidateBinding",
  "harnessSourceBinding",
  "capabilityBinding",
  "quietSeatStabilization",
  "pointerEvidence",
  "operatorHandoff",
  "environment",
  "package",
  "harness",
  "fixture",
  "checks",
  "screenshots",
  "assertions",
  "limitations",
];
const SCREENSHOT_FIELDS = [
  "file",
  "sha256",
  "bytes",
  "width",
  "height",
  "frameId",
  "windowId",
  "sourceSequence",
  "transportSequence",
];
const CANDIDATE_FIELDS = [
  "sourceSha",
  "tagObjectSha",
  "workflowRunId",
  "workflowRunAttempt",
  "artifactId",
  "artifactZipSha256",
  "checksumManifestSha256",
];
const SOURCE_FIELDS = [
  "sourceSha",
  "annotatedTagObjectSha",
  "detachedHead",
  "cleanTrackedAndUntracked",
  "fsckPassed",
  "exactTrackedHarnessBlobs",
];
const PACKAGE_FIELDS = [
  "checksumManifest",
  "archive",
  "serverVersion",
  "helperVersion",
  "helperBundleVersion",
  "helperBundleBuildVersion",
  "serverSha256",
  "helperSha256",
  "serverArchitectures",
  "helperArchitectures",
  "strictCodeSignatureVerification",
];
const MANIFEST_FIELDS = [
  "file",
  "expectedSha256",
  "actualSha256",
  "expectedSha256Matched",
  "exactCanonicalAssetSet",
  "canonicalEntryCount",
  "archiveFile",
  "archiveSha256",
  "archiveEntryMatched",
];
const ARCHIVE_FIELDS = [
  "file",
  "sha256",
  "checksumManifestMatched",
  "checksumManifestSha256",
  "canonicalEntryMatched",
  "extractedInputsMatched",
];
const HARNESS_FIELDS = [
  "runnerSha256",
  "fixtureSha256",
  "systemProbeSha256",
  "pointerHandoffSha256",
  "acceptanceFinalizerSha256",
  "packagedHelperSpawnCount",
];
const POINTER_FIELDS = [
  "requestedLane",
  "quietObserved",
  "concurrentSharedSeatActivityObserved",
  "unknownObserved",
  "rawCursorPositionsRetained",
  "rawPlatformActivityCountersRetained",
  "rawHidSystemCountersRetained",
  "hidSystemActivityClaimedAsPhysical",
];
const HANDOFF_FIELDS = [
  "requested",
  "requestPublicationAcknowledged",
  "completePublicationAcknowledged",
  "promptClosed",
  "sustainedMotionSamples",
  "sustainedMotionSpanMilliseconds",
  "clickFreeMotionObserved",
  "actionDispatched",
  "productBoundaryContaminated",
  "independentBoundaryContaminated",
  "markerNotificationOnly",
  "markerAcceptedAsAuthority",
  "externalAcknowledgementConsumed",
  "rawPromptIdentityRetainedInResult",
  "rawPointerDataRetained",
];
const QUIET_SEAT_FIELDS = [
  "required",
  "completed",
  "requiredStableMilliseconds",
  "maximumWaitMilliseconds",
  "sampleIntervalMilliseconds",
  "requiredStableTransitions",
  "stableDurationMilliseconds",
  "observedSamples",
  "stableTransitions",
  "resetCount",
  "monitoringUnknown",
  "completedBeforeCandidateExecution",
  "rawPointerDataRetained",
];
const REQUEST_MARKER_FIELDS = [
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
const COMPLETE_MARKER_FIELDS = [
  ...REQUEST_MARKER_FIELDS,
  "sustainedMotionSamples",
  "sustainedMotionSpanMilliseconds",
  "productBoundaryContaminated",
  "independentBoundaryContaminated",
  "clickFreeMotionObserved",
];

class FinalizerError extends Error {}

function fail(message) {
  throw new FinalizerError(message);
}

function objectValue(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${label} must be one JSON object.`);
  }
  return value;
}

function exactKeys(value, expected, label) {
  objectValue(value, label);
  const actual = Object.keys(value);
  if (
    actual.length !== expected.length ||
    actual.some((field, index) => field !== expected[index])
  ) {
    fail(`${label} fields are not in exact canonical order.`);
  }
}

function exactString(value, expected, label) {
  if (typeof value !== "string" || value !== expected) {
    fail(`${label} must be ${JSON.stringify(expected)}.`);
  }
}

function canonicalString(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    fail(`${label} has an invalid canonical form.`);
  }
  return value;
}

function exactBoolean(value, expected, label) {
  if (typeof value !== "boolean" || value !== expected) {
    fail(`${label} must be ${expected}.`);
  }
}

function exactInteger(value, minimum, maximum, label) {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) {
    fail(`${label} must be an integer from ${minimum} through ${maximum}.`);
  }
  return value;
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

function validateFreshTimestamp(milliseconds, now, label) {
  if (milliseconds > now + FUTURE_TOLERANCE_MS) fail(`${label} is future-dated.`);
  if (now - milliseconds > MAX_FRESH_AGE_MS) fail(`${label} is stale.`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function assertSupportedFilesystemIdentity() {
  if (!IS_WINDOWS && !POSIX_PERMISSION_METADATA_AVAILABLE) {
    fail("POSIX filesystem identity metadata is unavailable.");
  }
}

function sameStats(before, after) {
  return (
    before.dev === after.dev &&
    before.ino === after.ino &&
    before.size === after.size &&
    before.mtimeNs === after.mtimeNs &&
    before.ctimeNs === after.ctimeNs &&
    before.nlink === after.nlink
  );
}

function validateOwnedOrdinaryFile(stats, label, maximumBytes) {
  assertSupportedFilesystemIdentity();
  if (!stats.isFile() || stats.isSymbolicLink() || stats.nlink !== 1n) {
    fail(`${label} must be one ordinary, singly linked file.`);
  }
  if (POSIX_PERMISSION_METADATA_AVAILABLE && stats.uid !== BigInt(process.getuid())) {
    fail(`${label} must be owned by the finalizer user.`);
  }
  if (stats.size < 1n || stats.size > BigInt(maximumBytes)) {
    fail(`${label} has an invalid byte length.`);
  }
}

async function readStableFile(path, label, maximumBytes) {
  let handle;
  try {
    handle = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  } catch {
    fail(`${label} could not be opened read-only without following links.`);
  }
  try {
    const before = await handle.stat({ bigint: true });
    validateOwnedOrdinaryFile(before, label, maximumBytes);
    const bytes = await handle.readFile();
    const after = await handle.stat({ bigint: true });
    const pathAfter = await lstat(path, { bigint: true });
    validateOwnedOrdinaryFile(after, label, maximumBytes);
    validateOwnedOrdinaryFile(pathAfter, label, maximumBytes);
    if (!sameStats(before, after) || !sameStats(after, pathAfter)) {
      fail(`${label} changed while it was read.`);
    }
    if (BigInt(bytes.length) !== after.size) fail(`${label} size changed while it was read.`);
    return {
      bytes,
      sha256: sha256(bytes),
      stats: after,
      identity: `${after.dev}:${after.ino}`,
    };
  } finally {
    try {
      await handle.close();
    } catch {
      fail(`${label} read handle did not close cleanly.`);
    }
  }
}

function strictUtf8(bytes, label) {
  if (bytes.length === 0 || bytes.includes(0) || (bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf)) {
    fail(`${label} has invalid text bytes.`);
  }
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    fail(`${label} is not strict UTF-8.`);
  }
}

function parseJsonWithoutDuplicateKeys(text, label) {
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch {
    fail(`${label} is not one complete JSON value.`);
  }

  let offset = 0;
  const whitespace = () => {
    while (offset < text.length && /[\t\n\r ]/.test(text[offset])) offset += 1;
  };
  const stringToken = () => {
    const start = offset;
    if (text[offset] !== '"') fail(`${label} contains invalid JSON syntax.`);
    offset += 1;
    while (offset < text.length) {
      const character = text[offset];
      if (character === '"') {
        offset += 1;
        try {
          return JSON.parse(text.slice(start, offset));
        } catch {
          fail(`${label} contains an invalid JSON string.`);
        }
      }
      if (character === "\\") {
        offset += 2;
      } else {
        offset += 1;
      }
    }
    fail(`${label} contains an unterminated JSON string.`);
  };
  const valueToken = () => {
    whitespace();
    if (text[offset] === "{") {
      offset += 1;
      whitespace();
      const keys = new Set();
      if (text[offset] === "}") {
        offset += 1;
        return;
      }
      while (offset < text.length) {
        whitespace();
        const key = stringToken();
        if (keys.has(key)) fail(`${label} contains a duplicate object key.`);
        keys.add(key);
        whitespace();
        if (text[offset] !== ":") fail(`${label} contains invalid JSON object syntax.`);
        offset += 1;
        valueToken();
        whitespace();
        if (text[offset] === "}") {
          offset += 1;
          return;
        }
        if (text[offset] !== ",") fail(`${label} contains invalid JSON object syntax.`);
        offset += 1;
      }
      fail(`${label} contains an unterminated JSON object.`);
    }
    if (text[offset] === "[") {
      offset += 1;
      whitespace();
      if (text[offset] === "]") {
        offset += 1;
        return;
      }
      while (offset < text.length) {
        valueToken();
        whitespace();
        if (text[offset] === "]") {
          offset += 1;
          return;
        }
        if (text[offset] !== ",") fail(`${label} contains invalid JSON array syntax.`);
        offset += 1;
      }
      fail(`${label} contains an unterminated JSON array.`);
    }
    if (text[offset] === '"') {
      stringToken();
      return;
    }
    const start = offset;
    while (offset < text.length && !/[\t\n\r ,\]}]/.test(text[offset])) offset += 1;
    if (start === offset) fail(`${label} contains an invalid JSON scalar.`);
    try {
      JSON.parse(text.slice(start, offset));
    } catch {
      fail(`${label} contains an invalid JSON scalar.`);
    }
  };
  valueToken();
  whitespace();
  if (offset !== text.length) fail(`${label} contains trailing JSON data.`);
  return parsed;
}

async function readStableJson(path, label, maximumBytes) {
  const record = await readStableFile(path, label, maximumBytes);
  return {
    ...record,
    value: parseJsonWithoutDuplicateKeys(strictUtf8(record.bytes, label), label),
  };
}

function pathContains(parent, child) {
  const difference = relative(parent, child);
  return difference === "" || (!difference.startsWith(`..${sep}`) && difference !== "..");
}

async function validateCanonicalPrivateDirectory(input, label, now) {
  assertSupportedFilesystemIdentity();
  if (typeof input !== "string" || input.length === 0 || input.includes("\0")) {
    fail(`${label} path is invalid.`);
  }
  const absolute = resolve(input);
  const canonical = await realpath(absolute).catch(() => fail(`${label} does not exist.`));
  if (!isAbsolute(canonical) || canonical !== absolute) {
    fail(`${label} must be supplied through its canonical absolute path without symlink traversal.`);
  }
  const stats = await lstat(canonical, { bigint: true });
  if (!stats.isDirectory() || stats.isSymbolicLink()) fail(`${label} must be one ordinary directory.`);
  if (POSIX_PERMISSION_METADATA_AVAILABLE && (stats.mode & 0o077n) !== 0n) {
    fail(`${label} must not grant group or other filesystem access.`);
  }
  if (POSIX_PERMISSION_METADATA_AVAILABLE && stats.uid !== BigInt(process.getuid())) {
    fail(`${label} must be owned by the finalizer user.`);
  }
  const modifiedAt = Number(stats.ctimeNs / 1_000_000n);
  validateFreshTimestamp(modifiedAt, now, `${label} metadata`);
  return { path: canonical, stats };
}

async function walkLane(root, label, now) {
  const files = [];
  const directories = [];
  const identities = new Set();
  const visit = async (directory, relativeDirectory = "") => {
    const entries = (await readdir(directory)).sort();
    for (const name of entries) {
      if (name.length === 0 || name === "." || name === ".." || name.includes("/") || name.includes("\0")) {
        fail(`${label} contains an invalid directory entry.`);
      }
      const path = join(directory, name);
      const relativePath = relativeDirectory ? `${relativeDirectory}/${name}` : name;
      const entry = await lstat(path, { bigint: true });
      const modifiedAt = Number(entry.ctimeNs / 1_000_000n);
      validateFreshTimestamp(modifiedAt, now, `${label} entry ${relativePath}`);
      if (entry.isSymbolicLink()) fail(`${label} entry ${relativePath} must not be a symbolic link.`);
      if (entry.isDirectory()) {
        if (POSIX_PERMISSION_METADATA_AVAILABLE && (entry.mode & 0o077n) !== 0n) {
          fail(`${label} directory ${relativePath} must be private.`);
        }
        if (POSIX_PERMISSION_METADATA_AVAILABLE && entry.uid !== BigInt(process.getuid())) {
          fail(`${label} directory ${relativePath} must be owned by the finalizer user.`);
        }
        directories.push(relativePath);
        await visit(path, relativePath);
      } else if (entry.isFile()) {
        validateOwnedOrdinaryFile(entry, `${label} entry ${relativePath}`, MAX_SCREENSHOT_BYTES);
        const identity = `${entry.dev}:${entry.ino}`;
        if (identities.has(identity)) fail(`${label} contains hard-linked duplicate file identities.`);
        identities.add(identity);
        files.push(relativePath);
      } else {
        fail(`${label} entry ${relativePath} must be an ordinary file or directory.`);
      }
    }
  };
  await visit(root);
  return { files: files.sort(), directories: directories.sort(), identities };
}

function exactArray(actual, expected, label) {
  if (!Array.isArray(actual) || actual.length !== expected.length || actual.some((item, index) => item !== expected[index])) {
    fail(`${label} does not match the exact canonical inventory.`);
  }
}

function validatePng(bytes, expectedWidth, expectedHeight, label) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  if (bytes.length < 57 || !bytes.subarray(0, 8).equals(signature)) fail(`${label} is not a PNG file.`);
  let offset = 8;
  let width = 0;
  let height = 0;
  let sawHeader = false;
  let sawImageData = false;
  let imageDataEnded = false;
  let sawEnd = false;
  const imageData = [];
  while (offset < bytes.length) {
    if (offset > bytes.length - 12) fail(`${label} has a truncated PNG chunk.`);
    const length = bytes.readUInt32BE(offset);
    const dataStart = offset + 8;
    const dataEnd = dataStart + length;
    const chunkEnd = dataEnd + 4;
    if (length > MAX_SCREENSHOT_BYTES || dataEnd < dataStart || chunkEnd > bytes.length) {
      fail(`${label} has an invalid PNG chunk boundary.`);
    }
    const typeBytes = bytes.subarray(offset + 4, offset + 8);
    const type = typeBytes.toString("ascii");
    if (!/^[A-Za-z]{4}$/.test(type) || (typeBytes[2] & 0x20) !== 0) {
      fail(`${label} has an invalid PNG chunk type.`);
    }
    const expectedCrc = bytes.readUInt32BE(dataEnd);
    const actualCrc = crc32(bytes.subarray(offset + 4, dataEnd));
    if (actualCrc !== expectedCrc) fail(`${label} has a PNG chunk CRC mismatch.`);
    const data = bytes.subarray(dataStart, dataEnd);
    if (!sawHeader) {
      if (type !== "IHDR" || length !== 13 || offset !== 8) {
        fail(`${label} has no canonical leading IHDR chunk.`);
      }
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      if (
        width < 1 || height < 1 || width !== expectedWidth || height !== expectedHeight ||
        width * height > 1_000_000 || data[8] !== 8 || data[9] !== 6 ||
        data[10] !== 0 || data[11] !== 0 || data[12] !== 0
      ) {
        fail(`${label} IHDR does not describe the bounded noninterlaced RGBA8 capture.`);
      }
      sawHeader = true;
    } else if (type === "IHDR") {
      fail(`${label} contains more than one IHDR chunk.`);
    } else if (type === "IDAT") {
      if (imageDataEnded || length < 1) fail(`${label} has a noncanonical IDAT sequence.`);
      sawImageData = true;
      imageData.push(data);
    } else if (type === "IEND") {
      if (!sawImageData || length !== 0 || chunkEnd !== bytes.length) {
        fail(`${label} has an invalid IEND boundary.`);
      }
      sawEnd = true;
    } else {
      if (sawImageData) imageDataEnded = true;
      fail(`${label} contains an unexpected ancillary or critical PNG chunk.`);
    }
    offset = chunkEnd;
    if (sawEnd) break;
  }
  if (!sawHeader || !sawImageData || !sawEnd || offset !== bytes.length) {
    fail(`${label} does not contain one complete PNG image.`);
  }
  const rowBytes = width * 4;
  const expectedInflatedBytes = height * (rowBytes + 1);
  let inflated;
  try {
    inflated = inflateSync(Buffer.concat(imageData), { maxOutputLength: expectedInflatedBytes });
  } catch {
    fail(`${label} pixel stream could not be decoded within its bound.`);
  }
  if (inflated.length !== expectedInflatedBytes) {
    fail(`${label} decoded pixel stream has an invalid length.`);
  }
  const pixels = Buffer.alloc(width * height * 4);
  const paeth = (left, above, upperLeft) => {
    const estimate = left + above - upperLeft;
    const leftDistance = Math.abs(estimate - left);
    const aboveDistance = Math.abs(estimate - above);
    const upperLeftDistance = Math.abs(estimate - upperLeft);
    if (leftDistance <= aboveDistance && leftDistance <= upperLeftDistance) return left;
    return aboveDistance <= upperLeftDistance ? above : upperLeft;
  };
  for (let row = 0; row < height; row += 1) {
    const encodedRowOffset = row * (rowBytes + 1);
    const filter = inflated[encodedRowOffset];
    if (filter > 4) fail(`${label} decoded pixel row has an invalid filter.`);
    const pixelRowOffset = row * rowBytes;
    for (let column = 0; column < rowBytes; column += 1) {
      const encoded = inflated[encodedRowOffset + 1 + column];
      const left = column >= 4 ? pixels[pixelRowOffset + column - 4] : 0;
      const above = row > 0 ? pixels[pixelRowOffset - rowBytes + column] : 0;
      const upperLeft = row > 0 && column >= 4
        ? pixels[pixelRowOffset - rowBytes + column - 4]
        : 0;
      let predictor = 0;
      if (filter === 1) predictor = left;
      else if (filter === 2) predictor = above;
      else if (filter === 3) predictor = Math.floor((left + above) / 2);
      else if (filter === 4) predictor = paeth(left, above, upperLeft);
      pixels[pixelRowOffset + column] = (encoded + predictor) & 0xff;
    }
  }
  return createHash("sha256")
    .update(Buffer.from(`${width}x${height}\0`, "ascii"))
    .update(pixels)
    .digest("hex");
}

function validateReleaseCandidateBinding(value, label) {
  exactKeys(value, CANDIDATE_FIELDS, label);
  return {
    sourceSha: canonicalString(value.sourceSha, /^[0-9a-f]{40}$/, `${label} sourceSha`),
    tagObjectSha: canonicalString(value.tagObjectSha, /^[0-9a-f]{40}$/, `${label} tagObjectSha`),
    workflowRunId: canonicalString(value.workflowRunId, /^[1-9][0-9]*$/, `${label} workflowRunId`),
    workflowRunAttempt: canonicalString(value.workflowRunAttempt, /^[1-9][0-9]*$/, `${label} workflowRunAttempt`),
    artifactId: canonicalString(value.artifactId, /^[1-9][0-9]*$/, `${label} artifactId`),
    artifactZipSha256: canonicalString(value.artifactZipSha256, /^[0-9a-f]{64}$/, `${label} artifactZipSha256`),
    checksumManifestSha256: canonicalString(value.checksumManifestSha256, /^[0-9a-f]{64}$/, `${label} checksumManifestSha256`),
  };
}

function validateSourceBinding(value, label) {
  exactKeys(value, SOURCE_FIELDS, label);
  const source = {
    sourceSha: canonicalString(value.sourceSha, /^[0-9a-f]{40}$/, `${label} sourceSha`),
    annotatedTagObjectSha: canonicalString(value.annotatedTagObjectSha, /^[0-9a-f]{40}$/, `${label} annotatedTagObjectSha`),
    detachedHead: value.detachedHead,
    cleanTrackedAndUntracked: value.cleanTrackedAndUntracked,
    fsckPassed: value.fsckPassed,
    exactTrackedHarnessBlobs: value.exactTrackedHarnessBlobs,
  };
  for (const field of SOURCE_FIELDS.slice(2)) exactBoolean(source[field], true, `${label} ${field}`);
  return source;
}

function validatePackageBinding(value, label) {
  exactKeys(value, PACKAGE_FIELDS, label);
  exactKeys(value.checksumManifest, MANIFEST_FIELDS, `${label} checksumManifest`);
  exactKeys(value.archive, ARCHIVE_FIELDS, `${label} archive`);
  const manifest = {
    file: value.checksumManifest.file,
    expectedSha256: value.checksumManifest.expectedSha256,
    actualSha256: value.checksumManifest.actualSha256,
    expectedSha256Matched: value.checksumManifest.expectedSha256Matched,
    exactCanonicalAssetSet: value.checksumManifest.exactCanonicalAssetSet,
    canonicalEntryCount: value.checksumManifest.canonicalEntryCount,
    archiveFile: value.checksumManifest.archiveFile,
    archiveSha256: value.checksumManifest.archiveSha256,
    archiveEntryMatched: value.checksumManifest.archiveEntryMatched,
  };
  exactString(manifest.file, "SHA256SUMS.txt", `${label} checksumManifest file`);
  for (const field of ["expectedSha256", "actualSha256", "archiveSha256"]) {
    canonicalString(manifest[field], /^[0-9a-f]{64}$/, `${label} checksumManifest ${field}`);
  }
  exactBoolean(manifest.expectedSha256Matched, true, `${label} expectedSha256Matched`);
  exactBoolean(manifest.exactCanonicalAssetSet, true, `${label} exactCanonicalAssetSet`);
  exactInteger(manifest.canonicalEntryCount, 4, 4, `${label} canonicalEntryCount`);
  exactString(manifest.archiveFile, ARCHIVE_FILE, `${label} archiveFile`);
  exactBoolean(manifest.archiveEntryMatched, true, `${label} archiveEntryMatched`);
  if (manifest.expectedSha256 !== manifest.actualSha256) fail(`${label} manifest hashes do not match.`);

  const archive = {
    file: value.archive.file,
    sha256: value.archive.sha256,
    checksumManifestMatched: value.archive.checksumManifestMatched,
    checksumManifestSha256: value.archive.checksumManifestSha256,
    canonicalEntryMatched: value.archive.canonicalEntryMatched,
    extractedInputsMatched: value.archive.extractedInputsMatched,
  };
  exactString(archive.file, ARCHIVE_FILE, `${label} archive file`);
  for (const field of ["sha256", "checksumManifestSha256"]) {
    canonicalString(archive[field], /^[0-9a-f]{64}$/, `${label} archive ${field}`);
  }
  for (const field of ["checksumManifestMatched", "canonicalEntryMatched", "extractedInputsMatched"]) {
    exactBoolean(archive[field], true, `${label} archive ${field}`);
  }
  if (
    archive.sha256 !== manifest.archiveSha256 ||
    archive.checksumManifestSha256 !== manifest.actualSha256
  ) {
    fail(`${label} archive is not internally bound to the checksum manifest.`);
  }

  for (const field of ["serverVersion", "helperVersion", "helperBundleVersion", "helperBundleBuildVersion"]) {
    exactString(value[field], PRODUCT_VERSION, `${label} ${field}`);
  }
  for (const field of ["serverSha256", "helperSha256"]) {
    canonicalString(value[field], /^[0-9a-f]{64}$/, `${label} ${field}`);
  }
  for (const field of ["serverArchitectures", "helperArchitectures"]) {
    exactArray(value[field], ["arm64", "x86_64"], `${label} ${field}`);
  }
  exactString(value.strictCodeSignatureVerification, "passed", `${label} strictCodeSignatureVerification`);
  return {
    checksumManifest: manifest,
    archive,
    serverVersion: value.serverVersion,
    helperVersion: value.helperVersion,
    helperBundleVersion: value.helperBundleVersion,
    helperBundleBuildVersion: value.helperBundleBuildVersion,
    serverSha256: value.serverSha256,
    helperSha256: value.helperSha256,
    serverArchitectures: [...value.serverArchitectures],
    helperArchitectures: [...value.helperArchitectures],
    strictCodeSignatureVerification: value.strictCodeSignatureVerification,
  };
}

function validateHarnessBinding(value, label) {
  exactKeys(value, HARNESS_FIELDS, label);
  const harness = {};
  for (const field of HARNESS_FIELDS.slice(0, 5)) {
    harness[field] = canonicalString(value[field], /^[0-9a-f]{64}$/, `${label} ${field}`);
  }
  harness.packagedHelperSpawnCount = exactInteger(
    value.packagedHelperSpawnCount,
    1,
    1,
    `${label} packagedHelperSpawnCount`,
  );
  return harness;
}

function validateCapabilityBinding(value, label) {
  exactKeys(value, ["inputDeliveryProvenanceV1", "pointerActivityMonitorV1"], label);
  exactBoolean(value.inputDeliveryProvenanceV1, true, `${label} inputDeliveryProvenanceV1`);
  exactBoolean(value.pointerActivityMonitorV1, true, `${label} pointerActivityMonitorV1`);
  return {
    inputDeliveryProvenanceV1: true,
    pointerActivityMonitorV1: true,
  };
}

function validatePointerEvidence(value, lane, label) {
  exactKeys(value, POINTER_FIELDS, label);
  exactString(value.requestedLane, lane, `${label} requestedLane`);
  exactBoolean(value.quietObserved, true, `${label} quietObserved`);
  exactBoolean(
    value.concurrentSharedSeatActivityObserved,
    lane === "deliberate-concurrency",
    `${label} concurrentSharedSeatActivityObserved`,
  );
  for (const field of POINTER_FIELDS.slice(3)) exactBoolean(value[field], false, `${label} ${field}`);
}

function validateOperatorHandoff(value, lane, label) {
  exactKeys(value, HANDOFF_FIELDS, label);
  const deliberate = lane === "deliberate-concurrency";
  exactBoolean(value.requested, deliberate, `${label} requested`);
  for (const field of [
    "requestPublicationAcknowledged",
    "completePublicationAcknowledged",
    "promptClosed",
    "clickFreeMotionObserved",
    "actionDispatched",
    "productBoundaryContaminated",
    "independentBoundaryContaminated",
  ]) {
    exactBoolean(value[field], deliberate, `${label} ${field}`);
  }
  exactInteger(
    value.sustainedMotionSamples,
    deliberate ? 3 : 0,
    deliberate ? 1_000_000 : 0,
    `${label} sustainedMotionSamples`,
  );
  exactInteger(
    value.sustainedMotionSpanMilliseconds,
    deliberate ? 500 : 0,
    deliberate ? MAX_MOTION_SPAN_MS : 0,
    `${label} sustainedMotionSpanMilliseconds`,
  );
  exactBoolean(value.markerNotificationOnly, true, `${label} markerNotificationOnly`);
  for (const field of [
    "markerAcceptedAsAuthority",
    "externalAcknowledgementConsumed",
    "rawPromptIdentityRetainedInResult",
    "rawPointerDataRetained",
  ]) {
    exactBoolean(value[field], false, `${label} ${field}`);
  }
}

function validateQuietSeatStabilization(value, lane, label) {
  exactKeys(value, QUIET_SEAT_FIELDS, label);
  const required = lane === "quiet" || lane === "deliberate-concurrency";
  exactBoolean(value.required, required, `${label} required`);
  exactInteger(
    value.requiredStableMilliseconds,
    QUIET_SEAT_REQUIRED_STABLE_MS,
    QUIET_SEAT_REQUIRED_STABLE_MS,
    `${label} requiredStableMilliseconds`,
  );
  exactInteger(
    value.maximumWaitMilliseconds,
    QUIET_SEAT_MAXIMUM_WAIT_MS,
    QUIET_SEAT_MAXIMUM_WAIT_MS,
    `${label} maximumWaitMilliseconds`,
  );
  exactInteger(
    value.sampleIntervalMilliseconds,
    QUIET_SEAT_SAMPLE_INTERVAL_MS,
    QUIET_SEAT_SAMPLE_INTERVAL_MS,
    `${label} sampleIntervalMilliseconds`,
  );
  exactInteger(
    value.requiredStableTransitions,
    QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS,
    QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS,
    `${label} requiredStableTransitions`,
  );
  exactBoolean(value.completed, required, `${label} completed`);
  exactInteger(
    value.stableDurationMilliseconds,
    required ? QUIET_SEAT_REQUIRED_STABLE_MS : 0,
    required ? QUIET_SEAT_MAXIMUM_WAIT_MS : 0,
    `${label} stableDurationMilliseconds`,
  );
  exactInteger(
    value.observedSamples,
    required ? QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS + 1 : 0,
    required ? 1_000_000 : 0,
    `${label} observedSamples`,
  );
  exactInteger(
    value.stableTransitions,
    required ? QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS : 0,
    required ? 1_000_000 : 0,
    `${label} stableTransitions`,
  );
  exactInteger(value.resetCount, 0, required ? 1_000_000 : 0, `${label} resetCount`);
  exactBoolean(value.monitoringUnknown, false, `${label} monitoringUnknown`);
  exactBoolean(
    value.completedBeforeCandidateExecution,
    required,
    `${label} completedBeforeCandidateExecution`,
  );
  exactBoolean(value.rawPointerDataRetained, false, `${label} rawPointerDataRetained`);
}

function validateAssertions(value, label) {
  exactKeys(value, ["passed", "failed", "total", "details"], label);
  const passed = exactInteger(value.passed, 1, 1_000_000, `${label} passed`);
  const failed = exactInteger(value.failed, 0, 0, `${label} failed`);
  const total = exactInteger(value.total, 1, 1_000_000, `${label} total`);
  if (passed !== total || failed !== 0 || !Array.isArray(value.details) || value.details.length !== total) {
    fail(`${label} counts do not describe one completely passing detail set.`);
  }
  for (const [index, detail] of value.details.entries()) {
    exactKeys(detail, ["name", "passed", "detail"], `${label} detail ${index + 1}`);
    if (typeof detail.name !== "string" || detail.name.length < 1 || detail.name.length > 512) {
      fail(`${label} detail ${index + 1} name is invalid.`);
    }
    exactBoolean(detail.passed, true, `${label} detail ${index + 1} passed`);
    if (typeof detail.detail !== "string" || detail.detail.length > 4_096 || detail.detail.includes("\0")) {
      fail(`${label} detail ${index + 1} text is invalid.`);
    }
  }
  return { passed, failed, total };
}

function validateResultEnvelope(result, lane, resultMtimeMs, now, label) {
  exactKeys(result, RESULT_FIELDS, label);
  exactInteger(result.schemaVersion, RESULT_SCHEMA_VERSION, RESULT_SCHEMA_VERSION, `${label} schemaVersion`);
  exactString(result.productVersion, PRODUCT_VERSION, `${label} productVersion`);
  exactString(result.status, "passed-release-candidate", `${label} status`);
  exactString(
    result.evidenceClass,
    "exact-release-candidate-package-live-observation",
    `${label} evidenceClass`,
  );
  exactString(result.candidateNotice, CANDIDATE_NOTICE, `${label} candidateNotice`);
  const startedAtMs = canonicalTimestamp(result.startedAt, `${label} startedAt`);
  const capturedAtMs = canonicalTimestamp(result.capturedAt, `${label} capturedAt`);
  validateFreshTimestamp(startedAtMs, now, `${label} startedAt`);
  validateFreshTimestamp(capturedAtMs, now, `${label} capturedAt`);
  if (capturedAtMs < startedAtMs || capturedAtMs - startedAtMs > MAX_LANE_DURATION_MS) {
    fail(`${label} timestamps do not describe one bounded forward-moving lane.`);
  }
  if (Math.abs(resultMtimeMs - capturedAtMs) > FILE_TIMESTAMP_TOLERANCE_MS) {
    fail(`${label} file timestamp is not bound to capturedAt.`);
  }
  for (const field of ["environment", "fixture", "checks"]) objectValue(result[field], `${label} ${field}`);
  exactString(result.fixture.evidenceLane, lane, `${label} fixture evidenceLane`);
  if (!Array.isArray(result.limitations) || result.limitations.length < 1 || result.limitations.some((item) => typeof item !== "string" || item.length < 1)) {
    fail(`${label} limitations are invalid.`);
  }
  validatePointerEvidence(result.pointerEvidence, lane, `${label} pointerEvidence`);
  validateOperatorHandoff(result.operatorHandoff, lane, `${label} operatorHandoff`);
  validateQuietSeatStabilization(
    result.quietSeatStabilization,
    lane,
    `${label} quietSeatStabilization`,
  );
  return {
    startedAt: result.startedAt,
    capturedAt: result.capturedAt,
    releaseCandidate: validateReleaseCandidateBinding(result.releaseCandidateBinding, `${label} releaseCandidateBinding`),
    source: validateSourceBinding(result.harnessSourceBinding, `${label} harnessSourceBinding`),
    capability: validateCapabilityBinding(result.capabilityBinding, `${label} capabilityBinding`),
    package: validatePackageBinding(result.package, `${label} package`),
    harness: validateHarnessBinding(result.harness, `${label} harness`),
    assertions: validateAssertions(result.assertions, `${label} assertions`),
  };
}

async function validateScreenshots(root, result, label) {
  if (!Array.isArray(result.screenshots) || result.screenshots.length !== SCREENSHOT_FILES.length) {
    fail(`${label} must bind exactly six screenshots.`);
  }
  const summaries = [];
  const screenshotHashes = new Set();
  const screenshotPixelHashes = new Set();
  const frameIds = new Set();
  let windowId = null;
  for (const [index, expectedFile] of SCREENSHOT_FILES.entries()) {
    const screenshot = result.screenshots[index];
    exactKeys(screenshot, SCREENSHOT_FIELDS, `${label} screenshot ${index + 1}`);
    exactString(screenshot.file, expectedFile, `${label} screenshot ${index + 1} file`);
    canonicalString(screenshot.sha256, /^[0-9a-f]{64}$/, `${label} screenshot ${index + 1} sha256`);
    const bytes = exactInteger(screenshot.bytes, 1, MAX_SCREENSHOT_BYTES, `${label} screenshot ${index + 1} bytes`);
    const width = exactInteger(screenshot.width, 1, 100_000, `${label} screenshot ${index + 1} width`);
    const height = exactInteger(screenshot.height, 1, 100_000, `${label} screenshot ${index + 1} height`);
    canonicalString(
      screenshot.frameId,
      /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
      `${label} screenshot ${index + 1} frameId`,
    );
    canonicalString(screenshot.windowId, /^[1-9][0-9]*$/, `${label} screenshot ${index + 1} windowId`);
    if (windowId === null) windowId = screenshot.windowId;
    if (screenshot.windowId !== windowId) fail(`${label} screenshots are not bound to one exact window.`);
    if (frameIds.has(screenshot.frameId)) fail(`${label} screenshots reuse a frame identity.`);
    frameIds.add(screenshot.frameId);
    if (index < 3) {
      if (screenshot.sourceSequence !== null || screenshot.transportSequence !== null) {
        fail(`${label} one-shot screenshot ${index + 1} has a stream sequence.`);
      }
    } else {
      exactInteger(screenshot.sourceSequence, 1, Number.MAX_SAFE_INTEGER, `${label} screenshot ${index + 1} sourceSequence`);
      exactInteger(screenshot.transportSequence, 1, Number.MAX_SAFE_INTEGER, `${label} screenshot ${index + 1} transportSequence`);
    }
    const record = await readStableFile(join(root, expectedFile), `${label} ${expectedFile}`, MAX_SCREENSHOT_BYTES);
    if (record.bytes.length !== bytes || record.sha256 !== screenshot.sha256) {
      fail(`${label} screenshot ${index + 1} bytes are not hash-bound by helper-results.json.`);
    }
    const pixelSha256 = validatePng(
      record.bytes,
      width,
      height,
      `${label} screenshot ${index + 1}`,
    );
    if (screenshotHashes.has(record.sha256)) fail(`${label} screenshots are not six byte-distinct captures.`);
    if (screenshotPixelHashes.has(pixelSha256)) {
      fail(`${label} screenshots are not six decoded-pixel-distinct captures.`);
    }
    screenshotHashes.add(record.sha256);
    screenshotPixelHashes.add(pixelSha256);
    summaries.push({
      file: expectedFile,
      sha256: record.sha256,
      pixelSha256,
      bytes,
      width,
      height,
    });
  }
  return summaries;
}

function validateMarkerBase(marker, fields, expectedKind, label, now) {
  exactKeys(marker, fields, label);
  exactInteger(marker.schemaVersion, 1, 1, `${label} schemaVersion`);
  exactString(marker.kind, expectedKind, `${label} kind`);
  exactString(marker.productVersion, PRODUCT_VERSION, `${label} productVersion`);
  canonicalString(marker.requestId, /^[0-9a-f]{32}$/, `${label} requestId`);
  const createdAtMs = canonicalTimestamp(marker.createdAt, `${label} createdAt`);
  validateFreshTimestamp(createdAtMs, now, `${label} createdAt`);
  exactInteger(marker.runnerPid, 1, MAX_PID, `${label} runnerPid`);
  exactInteger(marker.promptPid, 1, MAX_PID, `${label} promptPid`);
  if (marker.runnerPid === marker.promptPid) fail(`${label} runnerPid and promptPid must be distinct.`);
  for (const field of ["requestDelivered", "panelOnScreen", "panelNonactivating", "notificationOnly"]) {
    exactBoolean(marker[field], true, `${label} ${field}`);
  }
  exactBoolean(marker.acceptedAsAuthority, false, `${label} acceptedAsAuthority`);
  return createdAtMs;
}

async function validateDeliberateMarkers(root, result, now, label) {
  const requestRecord = await readStableJson(join(root, REQUEST_MARKER), `${label} request marker`, MAX_MARKER_BYTES);
  const completeRecord = await readStableJson(join(root, COMPLETE_MARKER), `${label} complete marker`, MAX_MARKER_BYTES);
  const requestAt = validateMarkerBase(
    requestRecord.value,
    REQUEST_MARKER_FIELDS,
    "macos-pointer-concurrency-handoff-request",
    `${label} request marker`,
    now,
  );
  const completeAt = validateMarkerBase(
    completeRecord.value,
    COMPLETE_MARKER_FIELDS,
    "macos-pointer-concurrency-handoff-complete",
    `${label} complete marker`,
    now,
  );
  for (const field of ["requestId", "runnerPid", "promptPid"]) {
    if (requestRecord.value[field] !== completeRecord.value[field]) {
      fail(`${label} marker ${field} binding does not match.`);
    }
  }
  if (completeAt < requestAt || completeAt - requestAt > MAX_REQUEST_TO_COMPLETE_MS) {
    fail(`${label} marker timestamps are outside the bounded handoff interval.`);
  }
  const laneStartedAt = Date.parse(result.startedAt);
  const laneCapturedAt = Date.parse(result.capturedAt);
  if (requestAt < laneStartedAt || completeAt > laneCapturedAt) {
    fail(`${label} marker timestamps are outside the deliberate lane interval.`);
  }
  if (
    Math.abs(Number(requestRecord.stats.mtimeNs / 1_000_000n) - requestAt) > FILE_TIMESTAMP_TOLERANCE_MS ||
    Math.abs(Number(completeRecord.stats.mtimeNs / 1_000_000n) - completeAt) > FILE_TIMESTAMP_TOLERANCE_MS
  ) {
    fail(`${label} marker file timestamps are not bound to createdAt.`);
  }
  const complete = completeRecord.value;
  exactInteger(complete.sustainedMotionSamples, 3, 1_000_000, `${label} complete marker sustainedMotionSamples`);
  exactInteger(complete.sustainedMotionSpanMilliseconds, 500, MAX_MOTION_SPAN_MS, `${label} complete marker sustainedMotionSpanMilliseconds`);
  for (const field of ["productBoundaryContaminated", "independentBoundaryContaminated", "clickFreeMotionObserved"]) {
    exactBoolean(complete[field], true, `${label} complete marker ${field}`);
  }
  if (
    complete.sustainedMotionSamples !== result.operatorHandoff.sustainedMotionSamples ||
    complete.sustainedMotionSpanMilliseconds !== result.operatorHandoff.sustainedMotionSpanMilliseconds
  ) {
    fail(`${label} complete marker is not bound to the result handoff summary.`);
  }
  return [
    { file: REQUEST_MARKER, sha256: requestRecord.sha256 },
    { file: COMPLETE_MARKER, sha256: completeRecord.sha256 },
  ];
}

async function validateLane(input, lane, now) {
  const label = `${lane} lane`;
  const directory = await validateCanonicalPrivateDirectory(input, `${label} directory`, now);
  const inventory = await walkLane(directory.path, label, now);
  const expectedFiles = lane === "quiet" ? BASE_LANE_FILES : DELIBERATE_LANE_FILES;
  const expectedDirectories = lane === "quiet" ? [] : ["operator"];
  exactArray(inventory.files, expectedFiles, `${label} file inventory`);
  exactArray(inventory.directories, expectedDirectories, `${label} directory inventory`);

  const resultRecord = await readStableJson(
    join(directory.path, "helper-results.json"),
    `${label} helper-results.json`,
    MAX_RESULT_BYTES,
  );
  const result = resultRecord.value;
  const envelope = validateResultEnvelope(
    result,
    lane,
    Number(resultRecord.stats.mtimeNs / 1_000_000n),
    now,
    `${label} result`,
  );
  const screenshots = await validateScreenshots(directory.path, result, label);
  const logRecord = await readStableFile(join(directory.path, "helper-rig.log"), `${label} helper-rig.log`, MAX_LOG_BYTES);
  const logText = strictUtf8(logRecord.bytes, `${label} helper-rig.log`);
  if (!logText.endsWith("\n") || logText.includes("\0")) fail(`${label} helper-rig.log is not canonical text.`);
  const operatorMarkers = lane === "quiet"
    ? []
    : await validateDeliberateMarkers(directory.path, result, now, label);
  return {
    path: directory.path,
    inventory,
    resultSha256: resultRecord.sha256,
    logSha256: logRecord.sha256,
    envelope,
    screenshots,
    operatorMarkers,
  };
}

function requireIdentical(left, right, label) {
  if (!isDeepStrictEqual(left, right)) fail(`macOS lane ${label} bindings are not identical.`);
}

function requireDistinctResults(leftHash, rightHash) {
  if (leftHash === rightHash) fail("quiet and deliberate-concurrency result bytes must be distinct.");
}

function requireGloballyDistinctScreenshots(quiet, deliberate) {
  const hashes = new Set();
  const pixelHashes = new Set();
  for (const screenshot of [...quiet.screenshots, ...deliberate.screenshots]) {
    if (hashes.has(screenshot.sha256)) {
      fail("macOS dual-lane evidence must contain twelve byte-distinct screenshot captures.");
    }
    hashes.add(screenshot.sha256);
    if (pixelHashes.has(screenshot.pixelSha256)) {
      fail("macOS dual-lane evidence must contain twelve decoded-pixel-distinct screenshot captures.");
    }
    pixelHashes.add(screenshot.pixelSha256);
  }
  if (
    hashes.size !== SCREENSHOT_FILES.length * 2 ||
    pixelHashes.size !== SCREENSHOT_FILES.length * 2
  ) {
    fail("macOS dual-lane evidence must contain exactly twelve file and decoded-pixel hashes.");
  }
}

function validateCrossBindings(quiet, deliberate, executingFinalizerSha256, now) {
  for (const field of ["releaseCandidate", "source", "capability", "package", "harness"]) {
    requireIdentical(quiet.envelope[field], deliberate.envelope[field], field);
  }
  const { releaseCandidate, source, package: packageBinding } = quiet.envelope;
  if (
    releaseCandidate.sourceSha !== source.sourceSha ||
    releaseCandidate.tagObjectSha !== source.annotatedTagObjectSha
  ) {
    fail("candidate and checked-out harness source bindings do not match.");
  }
  if (
    releaseCandidate.checksumManifestSha256 !== packageBinding.checksumManifest.actualSha256 ||
    releaseCandidate.checksumManifestSha256 !== packageBinding.archive.checksumManifestSha256
  ) {
    fail("candidate and package checksum-manifest bindings do not match.");
  }
  if (quiet.envelope.harness.acceptanceFinalizerSha256 !== executingFinalizerSha256) {
    fail("executing macOS acceptance finalizer does not match the exact tagged harness binding.");
  }
  if (Date.parse(deliberate.envelope.startedAt) <= Date.parse(quiet.envelope.capturedAt)) {
    fail("deliberate-concurrency lane must start only after the quiet lane passed.");
  }
  if (now - Date.parse(deliberate.envelope.capturedAt) > MAX_DELIBERATE_REVIEW_DELAY_MS) {
    fail("macOS dual-lane evidence was not finalized within the bounded 30-minute review interval.");
  }
  requireDistinctResults(quiet.resultSha256, deliberate.resultSha256);
  requireGloballyDistinctScreenshots(quiet, deliberate);
}

function canonicalAggregate(quiet, deliberate, finalizedAt) {
  return {
    schemaVersion: AGGREGATE_SCHEMA_VERSION,
    productVersion: PRODUCT_VERSION,
    status: "passed-release-candidate",
    evidenceClass: "exact-release-candidate-macos-dual-lane-aggregate",
    finalizedAt,
    bindings: {
      releaseCandidate: quiet.envelope.releaseCandidate,
      source: quiet.envelope.source,
      package: quiet.envelope.package,
      harness: quiet.envelope.harness,
    },
    lanes: {
      quiet: {
        resultFile: "helper-results.json",
        resultSha256: quiet.resultSha256,
        logFile: "helper-rig.log",
        logSha256: quiet.logSha256,
        startedAt: quiet.envelope.startedAt,
        capturedAt: quiet.envelope.capturedAt,
        assertions: quiet.envelope.assertions,
        screenshots: quiet.screenshots,
        operatorMarkers: quiet.operatorMarkers,
      },
      deliberateConcurrency: {
        resultFile: "helper-results.json",
        resultSha256: deliberate.resultSha256,
        logFile: "helper-rig.log",
        logSha256: deliberate.logSha256,
        startedAt: deliberate.envelope.startedAt,
        capturedAt: deliberate.envelope.capturedAt,
        assertions: deliberate.envelope.assertions,
        screenshots: deliberate.screenshots,
        operatorMarkers: deliberate.operatorMarkers,
      },
    },
    aggregateChecks: {
      laneDirectoriesDisjoint: true,
      exactInventories: true,
      resultsByteDistinct: true,
      passingResultSchemaVersion: RESULT_SCHEMA_VERSION,
      inventoryFileCount: BASE_LANE_FILES.length + DELIBERATE_LANE_FILES.length,
      screenshotCount: SCREENSHOT_FILES.length * 2,
      screenshotHashesMatched: true,
      screenshotPixelHashesMatched: true,
      operatorMarkerHashesMatched: true,
    },
  };
}

async function writeCreateOnce(outputDirectory, aggregate) {
  const outputPath = join(outputDirectory, OUTPUT_FILE);
  const bytes = Buffer.from(`${JSON.stringify(aggregate)}\n`, "utf8");
  const temporaryPath = join(
    outputDirectory,
    `.${OUTPUT_FILE}.${process.pid}.${createHash("sha256").update(bytes).digest("hex").slice(0, 16)}.tmp`,
  );
  let handle;
  let temporaryExists = false;
  try {
    handle = await open(temporaryPath, "wx", 0o600);
    temporaryExists = true;
    await handle.writeFile(bytes);
    await handle.sync();
    await handle.close();
    handle = null;
    try {
      await link(temporaryPath, outputPath);
    } catch (error) {
      if (error?.code === "EEXIST") fail(`${OUTPUT_FILE} already exists; refusing to overwrite it.`);
      throw error;
    }
    await unlink(temporaryPath);
    temporaryExists = false;
    if (!IS_WINDOWS) {
      const directoryHandle = await open(
        outputDirectory,
        constants.O_RDONLY | (constants.O_DIRECTORY ?? 0) | (constants.O_NOFOLLOW ?? 0),
      );
      try {
        await directoryHandle.sync();
      } finally {
        await directoryHandle.close();
      }
    }
    exactArray((await readdir(outputDirectory)).sort(), [OUTPUT_FILE], "aggregate output inventory");
    const persisted = await readStableFile(outputPath, OUTPUT_FILE, MAX_RESULT_BYTES);
    if (!persisted.bytes.equals(bytes)) fail(`${OUTPUT_FILE} changed after create-once publication.`);
    return { path: outputPath, sha256: persisted.sha256, bytes: persisted.bytes.length };
  } finally {
    if (handle) await handle.close().catch(() => {});
    if (temporaryExists) await unlink(temporaryPath).catch(() => {});
  }
}

async function finalize(quietInput, deliberateInput, aggregateInput) {
  const now = Date.now();
  const finalizerRecord = await readStableFile(
    FINALIZER_SOURCE_PATH,
    "executing macOS acceptance finalizer",
    MAX_RESULT_BYTES,
  );
  const aggregateDirectory = await validateCanonicalPrivateDirectory(
    aggregateInput,
    "aggregate output directory",
    now,
  );
  const aggregateEntries = await readdir(aggregateDirectory.path);
  if (aggregateEntries.length !== 0) fail("aggregate output directory must be fresh and empty.");

  const [quiet, deliberate] = await Promise.all([
    validateLane(quietInput, "quiet", now),
    validateLane(deliberateInput, "deliberate-concurrency", now),
  ]);
  if (
    pathContains(quiet.path, deliberate.path) ||
    pathContains(deliberate.path, quiet.path)
  ) {
    fail("quiet and deliberate-concurrency lane directories must be disjoint.");
  }
  if (pathContains(quiet.path, aggregateDirectory.path) || pathContains(deliberate.path, aggregateDirectory.path)) {
    fail("aggregate output directory must not be inside a lane directory.");
  }
  for (const identity of quiet.inventory.identities) {
    if (deliberate.inventory.identities.has(identity)) {
      fail("quiet and deliberate-concurrency lane files must not share file identities.");
    }
  }
  validateCrossBindings(quiet, deliberate, finalizerRecord.sha256, now);
  const finalizedAt = new Date().toISOString();
  return await writeCreateOnce(
    aggregateDirectory.path,
    canonicalAggregate(quiet, deliberate, finalizedAt),
  );
}

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (0xedb88320 & -(crc & 1));
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBytes = Buffer.from(type, "ascii");
  const chunk = Buffer.alloc(12 + data.length);
  chunk.writeUInt32BE(data.length, 0);
  typeBytes.copy(chunk, 4);
  data.copy(chunk, 8);
  chunk.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])), 8 + data.length);
  return chunk;
}

function selfTestPng(width, height, color) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const row = Buffer.alloc(1 + width * 4);
  for (let pixel = 0; pixel < width; pixel += 1) {
    row[1 + pixel * 4] = color;
    row[2 + pixel * 4] = 255 - color;
    row[3 + pixel * 4] = (color * 17) % 256;
    row[4 + pixel * 4] = 255;
  }
  const raw = Buffer.concat(Array.from({ length: height }, () => row));
  return Buffer.concat([
    signature,
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(raw)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

function selfTestUndecodablePng(width, height) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    signature,
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", Buffer.from([0x00])),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

function selfTestPngWithDistinctEncoding(bytes) {
  const idatLength = bytes.readUInt32BE(33);
  const idatStart = 41;
  const idatEnd = idatStart + idatLength;
  const raw = inflateSync(bytes.subarray(idatStart, idatEnd));
  return Buffer.concat([
    bytes.subarray(0, 33),
    pngChunk("IDAT", deflateSync(raw, { level: 0 })),
    bytes.subarray(idatEnd + 4),
  ]);
}

function selfTestBindings() {
  const hash = (character) => character.repeat(64);
  return {
    releaseCandidateBinding: {
      sourceSha: "a".repeat(40),
      tagObjectSha: "b".repeat(40),
      workflowRunId: "32650000000",
      workflowRunAttempt: "1",
      artifactId: "9500000000",
      artifactZipSha256: hash("c"),
      checksumManifestSha256: hash("d"),
    },
    harnessSourceBinding: {
      sourceSha: "a".repeat(40),
      annotatedTagObjectSha: "b".repeat(40),
      detachedHead: true,
      cleanTrackedAndUntracked: true,
      fsckPassed: true,
      exactTrackedHarnessBlobs: true,
    },
    package: {
      checksumManifest: {
        file: "SHA256SUMS.txt",
        expectedSha256: hash("d"),
        actualSha256: hash("d"),
        expectedSha256Matched: true,
        exactCanonicalAssetSet: true,
        canonicalEntryCount: 4,
        archiveFile: ARCHIVE_FILE,
        archiveSha256: hash("e"),
        archiveEntryMatched: true,
      },
      archive: {
        file: ARCHIVE_FILE,
        sha256: hash("e"),
        checksumManifestMatched: true,
        checksumManifestSha256: hash("d"),
        canonicalEntryMatched: true,
        extractedInputsMatched: true,
      },
      serverVersion: PRODUCT_VERSION,
      helperVersion: PRODUCT_VERSION,
      helperBundleVersion: PRODUCT_VERSION,
      helperBundleBuildVersion: PRODUCT_VERSION,
      serverSha256: hash("f"),
      helperSha256: hash("0"),
      serverArchitectures: ["arm64", "x86_64"],
      helperArchitectures: ["arm64", "x86_64"],
      strictCodeSignatureVerification: "passed",
    },
    harness: {
      runnerSha256: hash("1"),
      fixtureSha256: hash("2"),
      systemProbeSha256: hash("3"),
      pointerHandoffSha256: hash("4"),
      acceptanceFinalizerSha256: FINALIZER_SOURCE_SHA256,
      packagedHelperSpawnCount: 1,
    },
  };
}

function selfTestPointer(lane) {
  const deliberate = lane === "deliberate-concurrency";
  return {
    pointerEvidence: {
      requestedLane: lane,
      quietObserved: true,
      concurrentSharedSeatActivityObserved: deliberate,
      unknownObserved: false,
      rawCursorPositionsRetained: false,
      rawPlatformActivityCountersRetained: false,
      rawHidSystemCountersRetained: false,
      hidSystemActivityClaimedAsPhysical: false,
    },
    operatorHandoff: {
      requested: deliberate,
      requestPublicationAcknowledged: deliberate,
      completePublicationAcknowledged: deliberate,
      promptClosed: deliberate,
      sustainedMotionSamples: deliberate ? 3 : 0,
      sustainedMotionSpanMilliseconds: deliberate ? 750 : 0,
      clickFreeMotionObserved: deliberate,
      actionDispatched: deliberate,
      productBoundaryContaminated: deliberate,
      independentBoundaryContaminated: deliberate,
      markerNotificationOnly: true,
      markerAcceptedAsAuthority: false,
      externalAcknowledgementConsumed: false,
      rawPromptIdentityRetainedInResult: false,
      rawPointerDataRetained: false,
    },
  };
}

function selfTestQuietSeatStabilization(lane) {
  const required = lane === "quiet" || lane === "deliberate-concurrency";
  return {
    required,
    completed: required,
    requiredStableMilliseconds: QUIET_SEAT_REQUIRED_STABLE_MS,
    maximumWaitMilliseconds: QUIET_SEAT_MAXIMUM_WAIT_MS,
    sampleIntervalMilliseconds: QUIET_SEAT_SAMPLE_INTERVAL_MS,
    requiredStableTransitions: QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS,
    stableDurationMilliseconds: required ? QUIET_SEAT_REQUIRED_STABLE_MS : 0,
    observedSamples: required ? QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS + 1 : 0,
    stableTransitions: required ? QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS : 0,
    resetCount: 0,
    monitoringUnknown: false,
    completedBeforeCandidateExecution: required,
    rawPointerDataRetained: false,
  };
}

async function createSelfTestLane(parent, name, lane, mutate = null) {
  const lanePath = join(parent, name);
  await mkdir(lanePath, { mode: 0o700 });
  const now = Date.now();
  const startedAt = new Date(now - (lane === "quiet" ? 4_000 : 2_500)).toISOString();
  const capturedAt = new Date(now - (lane === "quiet" ? 3_000 : 500)).toISOString();
  const screenshots = [];
  for (const [index, file] of SCREENSHOT_FILES.entries()) {
    const width = 2 + index;
    const height = 3 + index;
    const png = selfTestPng(width, height, index + (lane === "quiet" ? 1 : 31));
    await writeFile(join(lanePath, file), png, { mode: 0o600 });
    screenshots.push({
      file,
      sha256: sha256(png),
      bytes: png.length,
      width,
      height,
      frameId: `0000000${index + 1}-0000-4000-8000-00000000000${index + 1}`,
      windowId: lane === "quiet" ? "101" : "202",
      sourceSequence: index < 3 ? null : index + 1,
      transportSequence: index < 3 ? null : index - 2,
    });
  }
  const bindings = selfTestBindings();
  const pointer = selfTestPointer(lane);
  const result = {
    schemaVersion: RESULT_SCHEMA_VERSION,
    productVersion: PRODUCT_VERSION,
    status: "passed-release-candidate",
    evidenceClass: "exact-release-candidate-package-live-observation",
    startedAt,
    capturedAt,
    candidateNotice: CANDIDATE_NOTICE,
    releaseCandidateBinding: bindings.releaseCandidateBinding,
    harnessSourceBinding: bindings.harnessSourceBinding,
    capabilityBinding: {
      inputDeliveryProvenanceV1: true,
      pointerActivityMonitorV1: true,
    },
    quietSeatStabilization: selfTestQuietSeatStabilization(lane),
    pointerEvidence: pointer.pointerEvidence,
    operatorHandoff: pointer.operatorHandoff,
    environment: { selfTest: true },
    package: bindings.package,
    harness: bindings.harness,
    fixture: { evidenceLane: lane, selfTest: true },
    checks: { selfTest: true },
    screenshots,
    assertions: {
      passed: 1,
      failed: 0,
      total: 1,
      details: [{ name: "self-test", passed: true, detail: "passed" }],
    },
    limitations: ["Synthetic finalizer self-test fixture."],
  };
  if (mutate) mutate(result);
  await writeFile(join(lanePath, "helper-results.json"), `${JSON.stringify(result, null, 2)}\n`, { mode: 0o600 });
  await writeFile(join(lanePath, "helper-rig.log"), `${capturedAt} PASS synthetic finalizer fixture\n`, { mode: 0o600 });
  if (lane === "deliberate-concurrency") {
    const operatorPath = join(lanePath, "operator");
    await mkdir(operatorPath, { mode: 0o700 });
    const base = {
      schemaVersion: 1,
      kind: "macos-pointer-concurrency-handoff-request",
      productVersion: PRODUCT_VERSION,
      requestId: "5".repeat(32),
      createdAt: new Date(now - 2_000).toISOString(),
      runnerPid: 101,
      promptPid: 202,
      requestDelivered: true,
      panelOnScreen: true,
      panelNonactivating: true,
      notificationOnly: true,
      acceptedAsAuthority: false,
    };
    const complete = {
      ...base,
      kind: "macos-pointer-concurrency-handoff-complete",
      createdAt: new Date(now - 1_000).toISOString(),
      sustainedMotionSamples: result.operatorHandoff.sustainedMotionSamples,
      sustainedMotionSpanMilliseconds: result.operatorHandoff.sustainedMotionSpanMilliseconds,
      productBoundaryContaminated: true,
      independentBoundaryContaminated: true,
      clickFreeMotionObserved: true,
    };
    await writeFile(join(operatorPath, basename(REQUEST_MARKER)), `${JSON.stringify(base, null, 2)}\n`, { mode: 0o600 });
    await writeFile(join(operatorPath, basename(COMPLETE_MARKER)), `${JSON.stringify(complete, null, 2)}\n`, { mode: 0o600 });
  }
  return lanePath;
}

async function freshSelfTestOutput(parent, name) {
  const path = join(parent, name);
  await mkdir(path, { mode: 0o700 });
  return path;
}

async function expectSelfTestFailure(action, fragment) {
  try {
    await action();
  } catch (error) {
    if (error instanceof FinalizerError && error.message.includes(fragment)) return;
    throw error;
  }
  throw new Error(`self-test expected rejection containing ${JSON.stringify(fragment)}`);
}

async function runSelfTest() {
  const canonicalTemporaryRoot = await realpath(tmpdir());
  const root = await mkdtemp(join(canonicalTemporaryRoot, "lbb-macos-finalizer-self-test-"));
  await rm(root, { recursive: true, force: true });
  await mkdir(root, { mode: 0o700 });
  try {
    const quiet = await createSelfTestLane(root, "positive-quiet", "quiet");
    const deliberate = await createSelfTestLane(root, "positive-deliberate", "deliberate-concurrency");
    const output = await freshSelfTestOutput(root, "positive-output");
    const published = await finalize(quiet, deliberate, output);
    const aggregateBytes = await readFile(published.path);
    const aggregateText = strictUtf8(aggregateBytes, "self-test aggregate");
    if (!aggregateText.endsWith("\n") || aggregateText.slice(0, -1).includes("\n")) {
      throw new Error("self-test aggregate was not compact canonical JSON.");
    }
    const aggregate = parseJsonWithoutDuplicateKeys(aggregateText, "self-test aggregate");
    if (
      aggregate.schemaVersion !== AGGREGATE_SCHEMA_VERSION ||
      aggregate.aggregateChecks.passingResultSchemaVersion !== RESULT_SCHEMA_VERSION ||
      aggregate.lanes.quiet.resultSha256 === aggregate.lanes.deliberateConcurrency.resultSha256 ||
      aggregate.lanes.quiet.operatorMarkers.length !== 0 ||
      aggregate.lanes.deliberateConcurrency.operatorMarkers.length !== 2 ||
      aggregate.aggregateChecks.inventoryFileCount !== 18 ||
      aggregate.aggregateChecks.screenshotCount !== 12 ||
      aggregate.aggregateChecks.screenshotPixelHashesMatched !== true
    ) {
      throw new Error("self-test aggregate did not preserve the dual-lane bindings.");
    }
    const before = Buffer.from(aggregateBytes);
    await expectSelfTestFailure(() => finalize(quiet, deliberate, output), "fresh and empty");
    if (!(await readFile(published.path)).equals(before)) {
      throw new Error("self-test create-once aggregate changed after overwrite refusal.");
    }

    if (POSIX_PERMISSION_METADATA_AVAILABLE) {
      const permissiveQuiet = await createSelfTestLane(root, "permissive-quiet", "quiet");
      const permissiveDeliberate = await createSelfTestLane(
        root,
        "permissive-deliberate",
        "deliberate-concurrency",
      );
      const permissiveOutput = await freshSelfTestOutput(root, "permissive-output");
      await chmod(permissiveOutput, 0o755);
      await expectSelfTestFailure(
        () => finalize(permissiveQuiet, permissiveDeliberate, permissiveOutput),
        "must not grant group or other filesystem access",
      );

      const nestedPermissiveQuiet = await createSelfTestLane(
        root,
        "nested-permissive-quiet",
        "quiet",
      );
      const nestedPermissiveDeliberate = await createSelfTestLane(
        root,
        "nested-permissive-deliberate",
        "deliberate-concurrency",
      );
      const permissiveOperatorPath = join(nestedPermissiveDeliberate, "operator");
      await chmod(permissiveOperatorPath, 0o755);
      const nestedPermissiveOutput = await freshSelfTestOutput(
        root,
        "nested-permissive-output",
      );
      await expectSelfTestFailure(
        () => finalize(
          nestedPermissiveQuiet,
          nestedPermissiveDeliberate,
          nestedPermissiveOutput,
        ),
        "directory operator must be private",
      );
    }

    const swapOutput = await freshSelfTestOutput(root, "swap-output");
    await expectSelfTestFailure(
      () => finalize(deliberate, quiet, swapOutput),
      "canonical inventory",
    );

    const extraQuiet = await createSelfTestLane(root, "extra-quiet", "quiet");
    const extraDeliberate = await createSelfTestLane(root, "extra-deliberate", "deliberate-concurrency");
    await writeFile(join(extraQuiet, "unexpected.txt"), "unexpected\n", { mode: 0o600 });
    const extraOutput = await freshSelfTestOutput(root, "extra-output");
    await expectSelfTestFailure(
      () => finalize(extraQuiet, extraDeliberate, extraOutput),
      "canonical inventory",
    );

    const mismatchQuiet = await createSelfTestLane(root, "mismatch-quiet", "quiet");
    const mismatchDeliberate = await createSelfTestLane(
      root,
      "mismatch-deliberate",
      "deliberate-concurrency",
      (result) => { result.releaseCandidateBinding.artifactId = "9500000001"; },
    );
    const mismatchOutput = await freshSelfTestOutput(root, "mismatch-output");
    await expectSelfTestFailure(
      () => finalize(mismatchQuiet, mismatchDeliberate, mismatchOutput),
      "releaseCandidate bindings are not identical",
    );

    const laneMismatchQuiet = await createSelfTestLane(root, "lane-mismatch-quiet", "quiet");
    const laneMismatchDeliberate = await createSelfTestLane(
      root,
      "lane-mismatch-deliberate",
      "deliberate-concurrency",
      (result) => { result.fixture.evidenceLane = "quiet"; },
    );
    const laneMismatchOutput = await freshSelfTestOutput(root, "lane-mismatch-output");
    await expectSelfTestFailure(
      () => finalize(laneMismatchQuiet, laneMismatchDeliberate, laneMismatchOutput),
      "fixture evidenceLane",
    );

    const shortGateQuiet = await createSelfTestLane(
      root,
      "short-gate-quiet",
      "quiet",
      (result) => { result.quietSeatStabilization.stableDurationMilliseconds = 29_999; },
    );
    const shortGateDeliberate = await createSelfTestLane(
      root,
      "short-gate-deliberate",
      "deliberate-concurrency",
    );
    const shortGateOutput = await freshSelfTestOutput(root, "short-gate-output");
    await expectSelfTestFailure(
      () => finalize(shortGateQuiet, shortGateDeliberate, shortGateOutput),
      "quietSeatStabilization stableDurationMilliseconds",
    );

    const unknownGateQuiet = await createSelfTestLane(root, "unknown-gate-quiet", "quiet");
    const unknownGateDeliberate = await createSelfTestLane(
      root,
      "unknown-gate-deliberate",
      "deliberate-concurrency",
      (result) => { result.quietSeatStabilization.monitoringUnknown = true; },
    );
    const unknownGateOutput = await freshSelfTestOutput(root, "unknown-gate-output");
    await expectSelfTestFailure(
      () => finalize(unknownGateQuiet, unknownGateDeliberate, unknownGateOutput),
      "quietSeatStabilization monitoringUnknown",
    );

    const missingGateQuiet = await createSelfTestLane(
      root,
      "missing-gate-quiet",
      "quiet",
      (result) => { delete result.quietSeatStabilization; },
    );
    const missingGateDeliberate = await createSelfTestLane(
      root,
      "missing-gate-deliberate",
      "deliberate-concurrency",
    );
    const missingGateOutput = await freshSelfTestOutput(root, "missing-gate-output");
    await expectSelfTestFailure(
      () => finalize(missingGateQuiet, missingGateDeliberate, missingGateOutput),
      "fields are not in exact canonical order",
    );

    const overlapQuiet = await createSelfTestLane(root, "overlap-quiet", "quiet");
    const overlapDeliberate = await createSelfTestLane(
      root,
      "overlap-deliberate",
      "deliberate-concurrency",
      (result) => {
        result.startedAt = new Date(Date.now() - 4_000).toISOString();
      },
    );
    const overlapOutput = await freshSelfTestOutput(root, "overlap-output");
    await expectSelfTestFailure(
      () => finalize(overlapQuiet, overlapDeliberate, overlapOutput),
      "must start only after the quiet lane passed",
    );

    const reusedQuiet = await createSelfTestLane(root, "reused-screenshot-quiet", "quiet");
    const reusedDeliberate = await createSelfTestLane(
      root,
      "reused-screenshot-deliberate",
      "deliberate-concurrency",
    );
    const reusedBytes = await readFile(join(reusedQuiet, SCREENSHOT_FILES[0]));
    await writeFile(join(reusedDeliberate, SCREENSHOT_FILES[0]), reusedBytes, { mode: 0o600 });
    const reusedResultPath = join(reusedDeliberate, "helper-results.json");
    const reusedResult = JSON.parse(await readFile(reusedResultPath, "utf8"));
    reusedResult.screenshots[0].sha256 = sha256(reusedBytes);
    reusedResult.screenshots[0].bytes = reusedBytes.length;
    await writeFile(reusedResultPath, `${JSON.stringify(reusedResult, null, 2)}\n`, { mode: 0o600 });
    const reusedOutput = await freshSelfTestOutput(root, "reused-screenshot-output");
    await expectSelfTestFailure(
      () => finalize(reusedQuiet, reusedDeliberate, reusedOutput),
      "twelve byte-distinct screenshot captures",
    );

    const pixelReplayQuiet = await createSelfTestLane(root, "pixel-replay-quiet", "quiet");
    const pixelReplayDeliberate = await createSelfTestLane(
      root,
      "pixel-replay-deliberate",
      "deliberate-concurrency",
    );
    const replayPixels = await readFile(join(pixelReplayQuiet, SCREENSHOT_FILES[0]));
    const replayWithDistinctBytes = selfTestPngWithDistinctEncoding(replayPixels);
    await writeFile(
      join(pixelReplayDeliberate, SCREENSHOT_FILES[0]),
      replayWithDistinctBytes,
      { mode: 0o600 },
    );
    const pixelReplayResultPath = join(pixelReplayDeliberate, "helper-results.json");
    const pixelReplayResult = JSON.parse(await readFile(pixelReplayResultPath, "utf8"));
    pixelReplayResult.screenshots[0].sha256 = sha256(replayWithDistinctBytes);
    pixelReplayResult.screenshots[0].bytes = replayWithDistinctBytes.length;
    await writeFile(
      pixelReplayResultPath,
      `${JSON.stringify(pixelReplayResult, null, 2)}\n`,
      { mode: 0o600 },
    );
    const pixelReplayOutput = await freshSelfTestOutput(root, "pixel-replay-output");
    await expectSelfTestFailure(
      () => finalize(pixelReplayQuiet, pixelReplayDeliberate, pixelReplayOutput),
      "twelve decoded-pixel-distinct screenshot captures",
    );

    const tamperQuiet = await createSelfTestLane(root, "tamper-quiet", "quiet");
    const tamperDeliberate = await createSelfTestLane(root, "tamper-deliberate", "deliberate-concurrency");
    await writeFile(join(tamperQuiet, SCREENSHOT_FILES[0]), selfTestPng(2, 3, 99), { mode: 0o600 });
    const tamperOutput = await freshSelfTestOutput(root, "tamper-output");
    await expectSelfTestFailure(
      () => finalize(tamperQuiet, tamperDeliberate, tamperOutput),
      "not hash-bound",
    );

    const decodeQuiet = await createSelfTestLane(root, "decode-quiet", "quiet");
    const decodeDeliberate = await createSelfTestLane(root, "decode-deliberate", "deliberate-concurrency");
    const undecodable = selfTestUndecodablePng(2, 3);
    await writeFile(join(decodeQuiet, SCREENSHOT_FILES[0]), undecodable, { mode: 0o600 });
    const decodeResultPath = join(decodeQuiet, "helper-results.json");
    const decodeResult = JSON.parse(await readFile(decodeResultPath, "utf8"));
    decodeResult.screenshots[0].sha256 = sha256(undecodable);
    decodeResult.screenshots[0].bytes = undecodable.length;
    await writeFile(decodeResultPath, `${JSON.stringify(decodeResult, null, 2)}\n`, { mode: 0o600 });
    const decodeOutput = await freshSelfTestOutput(root, "decode-output");
    await expectSelfTestFailure(
      () => finalize(decodeQuiet, decodeDeliberate, decodeOutput),
      "pixel stream could not be decoded",
    );

    const duplicateQuiet = await createSelfTestLane(root, "duplicate-quiet", "quiet");
    const duplicateDeliberate = await createSelfTestLane(root, "duplicate-deliberate", "deliberate-concurrency");
    const duplicateResultPath = join(duplicateQuiet, "helper-results.json");
    const duplicateText = await readFile(duplicateResultPath, "utf8");
    await writeFile(
      duplicateResultPath,
      duplicateText.replace(
        `  "schemaVersion": ${RESULT_SCHEMA_VERSION},`,
        `  "schemaVersion": ${RESULT_SCHEMA_VERSION},\n  "schemaVersion": ${RESULT_SCHEMA_VERSION},`,
      ),
      { mode: 0o600 },
    );
    const duplicateOutput = await freshSelfTestOutput(root, "duplicate-output");
    await expectSelfTestFailure(
      () => finalize(duplicateQuiet, duplicateDeliberate, duplicateOutput),
      "duplicate object key",
    );

    const staleQuiet = await createSelfTestLane(
      root,
      "stale-quiet",
      "quiet",
      (result) => { result.capturedAt = new Date(Date.now() - MAX_FRESH_AGE_MS - 60_000).toISOString(); },
    );
    const staleDeliberate = await createSelfTestLane(root, "stale-deliberate", "deliberate-concurrency");
    const staleOutput = await freshSelfTestOutput(root, "stale-output");
    await expectSelfTestFailure(
      () => finalize(staleQuiet, staleDeliberate, staleOutput),
      "is stale",
    );

    const markerQuiet = await createSelfTestLane(root, "marker-quiet", "quiet");
    const markerDeliberate = await createSelfTestLane(root, "marker-deliberate", "deliberate-concurrency");
    const completeMarkerPath = join(markerDeliberate, COMPLETE_MARKER);
    const mismatchedMarker = JSON.parse(await readFile(completeMarkerPath, "utf8"));
    mismatchedMarker.requestId = "7".repeat(32);
    await writeFile(completeMarkerPath, `${JSON.stringify(mismatchedMarker, null, 2)}\n`, { mode: 0o600 });
    const markerOutput = await freshSelfTestOutput(root, "marker-output");
    await expectSelfTestFailure(
      () => finalize(markerQuiet, markerDeliberate, markerOutput),
      "marker requestId binding does not match",
    );

    await expectSelfTestFailure(
      async () => requireDistinctResults("6".repeat(64), "6".repeat(64)),
      "must be distinct",
    );
    console.log("macOS dual-lane acceptance finalizer self-test passed.");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

const argumentsList = process.argv.slice(2);
if (argumentsList.length === 1 && argumentsList[0] === "--self-test") {
  await runSelfTest();
} else if (argumentsList.length === 3) {
  try {
    const result = await finalize(argumentsList[0], argumentsList[1], argumentsList[2]);
    console.log(`${OUTPUT_FILE} created (${result.bytes} bytes, SHA-256 ${result.sha256}).`);
  } catch (error) {
    console.error(error instanceof FinalizerError ? error.message : "macOS acceptance finalization failed unexpectedly.");
    process.exitCode = 1;
  }
} else {
  console.error(
    "Usage: node scripts/finalize-macos-acceptance.mjs <quiet-lane-dir> <deliberate-concurrency-lane-dir> <fresh-aggregate-dir>",
  );
  console.error("       node scripts/finalize-macos-acceptance.mjs --self-test");
  process.exitCode = 2;
}
