use std::fs;
use std::process::Command;

fn finalizer_source() -> String {
    fs::read_to_string("scripts/finalize-macos-acceptance.mjs").unwrap()
}

#[test]
fn macos_v0_12_19_result_schema_is_aligned_end_to_end() {
    let finalizer = finalizer_source().replace("\r\n", "\n");
    let producer = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let documentation = fs::read_to_string("evidence/v0.12.19/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");
    let verifier = fs::read_to_string("scripts/verify-release-acceptance-evidence.sh")
        .unwrap()
        .replace("\r\n", "\n");

    assert!(finalizer.contains("const RESULT_SCHEMA_VERSION = 6;"));
    assert!(producer.matches("schemaVersion: 6,").count() >= 2);
    assert!(documentation.contains("retained schema-v6 result"));
    assert!(documentation.contains("Schema v6 uses a tri-state dispatch field"));
    for producer_contract in [
        "passingResultSchemaVersion: RESULT_SCHEMA_VERSION",
        "aggregate.aggregateChecks.passingResultSchemaVersion !== RESULT_SCHEMA_VERSION",
    ] {
        assert!(
            finalizer.contains(producer_contract),
            "macOS finalizer is missing result-schema producer contract: {producer_contract}"
        );
    }
    for required in [
        ".schemaVersion == 6",
        "validate_mac_result_schema_binding()",
        "--argjson quiet_result_schema_version",
        "--argjson deliberate_result_schema_version",
        ".aggregateChecks.passingResultSchemaVersion == $quiet_result_schema_version",
        "$quiet_result_schema_version == $deliberate_result_schema_version",
        "self-test accepted a stale macOS aggregate result schema",
        "self-test accepted mismatched macOS lane result schemas",
    ] {
        assert!(
            verifier.contains(required),
            "release evidence verifier is missing the result-schema binding: {required}"
        );
    }
    assert!(!verifier.contains(".aggregateChecks.passingResultSchemaVersion == 5"));
}

#[test]
fn macos_v0_12_19_release_verifier_recomputes_every_tagged_harness_hash() {
    let verifier = fs::read_to_string("scripts/verify-release-acceptance-evidence.sh")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "write_mac_harness_source_binding()",
        "validate_mac_harness_source_binding()",
        "local harness_root=\"evidence/v${EVIDENCE_PRODUCT_VERSION}/computer\"",
        "helper-evidence-rig.mjs",
        "HelperEvidenceFixture.swift",
        "SystemProbe.swift",
        "PointerHandoff.swift",
        "scripts/finalize-macos-acceptance.mjs",
        "test -f \"$source_path\" && test ! -L \"$source_path\"",
        "runner_sha256=\"$(sha256_file \"$runner\")\"",
        "fixture_sha256=\"$(sha256_file \"$fixture\")\"",
        "system_probe_sha256=\"$(sha256_file \"$system_probe\")\"",
        "pointer_handoff_sha256=\"$(sha256_file \"$pointer_handoff\")\"",
        "finalizer_sha256=\"$(sha256_file \"$finalizer\")\"",
        ".bindings.harness == $expected[0]",
        "length == 2 and all(.[]; .harness == $expected[0])",
        "macOS retained lanes or aggregate do not match the exact tagged harness source hashes",
        "self-test accepted a macOS aggregate tagged-harness hash mismatch",
        "self-test accepted a quiet-lane tagged-harness hash mismatch",
        "self-test accepted a deliberate-lane tagged-harness hash mismatch",
    ] {
        assert!(
            verifier.contains(required),
            "release evidence verifier is missing tagged-harness binding: {required}"
        );
    }
    for field in [
        "runnerSha256",
        "fixtureSha256",
        "systemProbeSha256",
        "pointerHandoffSha256",
        "acceptanceFinalizerSha256",
    ] {
        assert!(
            verifier.contains(field),
            "release evidence verifier does not bind tagged harness field {field}"
        );
    }
    assert!(
        verifier.contains(".harness.acceptanceFinalizerSha256 == $acceptance_finalizer_sha256")
    );
    assert!(
        verifier.contains(
            ".bindings.harness.acceptanceFinalizerSha256 == $acceptance_finalizer_sha256"
        )
    );
}

#[test]
fn macos_v0_12_14_dual_lane_finalizer_is_dependency_free_and_fail_closed() {
    let source = finalizer_source().replace("\r\n", "\n");

    for import in source.lines().filter(|line| line.contains(" from \"")) {
        let module = import
            .split(" from \"")
            .nth(1)
            .and_then(|tail| tail.split('"').next())
            .unwrap();
        assert!(
            module.starts_with("node:"),
            "finalizer imports a non-standard dependency: {module}"
        );
    }

    for required in [
        "const PRODUCT_VERSION = \"0.12.19\";",
        "const RESULT_SCHEMA_VERSION = 6;",
        "const AGGREGATE_SCHEMA_VERSION = 1;",
        "const OUTPUT_FILE = \"macos-acceptance.json\";",
        "const MAX_FRESH_AGE_MS = 12 * 60 * 60 * 1_000;",
        "const MAX_LANE_DURATION_MS = 2 * 60 * 60 * 1_000;",
        "const MAX_DELIBERATE_REVIEW_DELAY_MS = 30 * 60 * 1_000;",
        "const QUIET_SEAT_REQUIRED_STABLE_MS = 30_000;",
        "const QUIET_SEAT_MAXIMUM_WAIT_MS = 30 * 60_000;",
        "const QUIET_SEAT_SAMPLE_INTERVAL_MS = 500;",
        "const QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS = 60;",
        "const IS_WINDOWS = process.platform === \"win32\";",
        "const POSIX_PERMISSION_METADATA_AVAILABLE = !IS_WINDOWS && typeof process.getuid === \"function\";",
        "const SCREENSHOT_FILES = [",
        "computer-01-exact-window-observe.png",
        "computer-06-persistent-share-resize.png",
        "helper-results.json",
        "helper-rig.log",
        "operator/macos-pointer-concurrency-handoff-request.json",
        "operator/macos-pointer-concurrency-handoff-complete.json",
        "const FINALIZER_SOURCE_PATH = fileURLToPath(import.meta.url);",
        "acceptanceFinalizerSha256",
        "executing macOS acceptance finalizer does not match the exact tagged harness binding",
        "bounded 30-minute review interval",
        "validateCanonicalPrivateDirectory",
        "must be supplied through its canonical absolute path without symlink traversal",
        "must not grant group or other filesystem access",
        "assertSupportedFilesystemIdentity",
        "POSIX filesystem identity metadata is unavailable",
        "POSIX_PERMISSION_METADATA_AVAILABLE && (stats.mode & 0o077n) !== 0n",
        "POSIX_PERMISSION_METADATA_AVAILABLE && (entry.mode & 0o077n) !== 0n",
        "validateFreshTimestamp",
        "validateQuietSeatStabilization",
        "completedBeforeCandidateExecution",
        "monitoringUnknown",
        "walkLane",
        "exactArray(inventory.files, expectedFiles",
        "exactArray(inventory.directories, expectedDirectories",
        "O_RDONLY | (constants.O_NOFOLLOW ?? 0)",
        "must be one ordinary, singly linked file",
        "contains hard-linked duplicate file identities",
        "lane files must not share file identities",
        "parseJsonWithoutDuplicateKeys",
        "contains a duplicate object key",
        "result bytes must be distinct",
        "deliberate-concurrency lane must start only after the quiet lane passed",
        "macOS lane ${label} bindings are not identical",
        "candidate and checked-out harness source bindings do not match",
        "candidate and package checksum-manifest bindings do not match",
        "schemaVersion: AGGREGATE_SCHEMA_VERSION",
        "evidenceClass: \"exact-release-candidate-macos-dual-lane-aggregate\"",
        "releaseCandidate: quiet.envelope.releaseCandidate",
        "source: quiet.envelope.source",
        "package: quiet.envelope.package",
        "harness: quiet.envelope.harness",
        "quiet: {",
        "deliberateConcurrency: {",
        "startedAt: quiet.envelope.startedAt",
        "startedAt: deliberate.envelope.startedAt",
        "resultSha256: quiet.resultSha256",
        "resultSha256: deliberate.resultSha256",
        "operatorMarkers: quiet.operatorMarkers",
        "operatorMarkers: deliberate.operatorMarkers",
        "inventoryFileCount: BASE_LANE_FILES.length + DELIBERATE_LANE_FILES.length",
        "screenshotHashesMatched: true",
        "operatorMarkerHashesMatched: true",
        "JSON.stringify(aggregate)",
        "await open(temporaryPath, \"wx\", 0o600)",
        "await handle.sync()",
        "await link(temporaryPath, outputPath)",
        "if (!IS_WINDOWS) {",
        "await directoryHandle.sync()",
        "aggregate output inventory",
        "already exists; refusing to overwrite it",
        "aggregate output directory must be fresh and empty",
        "await chmod(permissiveOutput, 0o755)",
        "await chmod(permissiveOperatorPath, 0o755)",
        "directory operator must be private",
        "macOS dual-lane acceptance finalizer self-test passed.",
    ] {
        assert!(
            source.contains(required),
            "macOS dual-lane finalizer is missing {required}"
        );
    }

    for forbidden in [
        "node_modules",
        "npm ",
        "npx ",
        "child_process",
        "fetch(",
        "exec(",
        "spawn(",
        "rename(temporaryPath, outputPath)",
        "writeFile(outputPath",
    ] {
        assert!(
            !source.contains(forbidden),
            "macOS dual-lane finalizer contains forbidden behavior: {forbidden}"
        );
    }
}

#[test]
fn macos_v0_12_14_dual_lane_finalizer_enforces_lane_and_screenshot_invariants() {
    let source = finalizer_source().replace("\r\n", "\n");
    for required in [
        "exactString(value.requestedLane, lane",
        "exactString(result.fixture.evidenceLane, lane",
        "exactBoolean(value.quietObserved, true",
        "lane === \"deliberate-concurrency\"",
        "value.concurrentSharedSeatActivityObserved",
        "for (const field of POINTER_FIELDS.slice(3))",
        "exactBoolean(value.requested, deliberate",
        "requestPublicationAcknowledged",
        "completePublicationAcknowledged",
        "clickFreeMotionObserved",
        "actionDispatched",
        "productBoundaryContaminated",
        "independentBoundaryContaminated",
        "deliberate ? 3 : 0",
        "deliberate ? 500 : 0",
        "markerAcceptedAsAuthority",
        "externalAcknowledgementConsumed",
        "validateDeliberateMarkers",
        "marker timestamps are outside the deliberate lane interval",
        "complete marker is not bound to the result handoff summary",
        "result.screenshots.length !== SCREENSHOT_FILES.length",
        "exactKeys(screenshot, SCREENSHOT_FIELDS",
        "record.bytes.length !== bytes || record.sha256 !== screenshot.sha256",
        "const pixelSha256 = validatePng(",
        "width * height > 1_000_000",
        "data[8] !== 8 || data[9] !== 6",
        "crc32(bytes.subarray(offset + 4, dataEnd))",
        "contains an unexpected ancillary or critical PNG chunk",
        "inflateSync(Buffer.concat(imageData), { maxOutputLength: expectedInflatedBytes })",
        "pixel stream could not be decoded within its bound",
        "decoded pixel row has an invalid filter",
        "screenshots are not six byte-distinct captures",
        "screenshots are not six decoded-pixel-distinct captures",
        "macOS dual-lane evidence must contain twelve byte-distinct screenshot captures",
        "macOS dual-lane evidence must contain twelve decoded-pixel-distinct screenshot captures",
        "requireGloballyDistinctScreenshots(quiet, deliberate)",
        "screenshot.sourceSequence !== null || screenshot.transportSequence !== null",
        "screenshots are not bound to one exact window",
        "screenshots reuse a frame identity",
        "value.details.length !== total",
        "exactBoolean(detail.passed, true",
        "result.fixture.evidenceLane = \"quiet\"",
    ] {
        assert!(
            source.contains(required),
            "macOS dual-lane finalizer omits invariant {required}"
        );
    }

    let aggregate = source
        .split("function canonicalAggregate(quiet, deliberate, finalizedAt)")
        .nth(1)
        .unwrap()
        .split("async function writeCreateOnce")
        .next()
        .unwrap();
    for sensitive in [
        "environment:",
        "fixture:",
        "checks:",
        "limitations:",
        "frameId:",
        "windowId:",
        "path:",
    ] {
        assert!(
            !aggregate.contains(sensitive),
            "sanitized aggregate retains sensitive or unnecessary field {sensitive}"
        );
    }
}

#[test]
fn macos_v0_12_14_dual_lane_finalizer_self_test_passes() {
    let syntax = Command::new("node")
        .args(["--check", "scripts/finalize-macos-acceptance.mjs"])
        .output()
        .expect("failed to syntax-check macOS acceptance finalizer");
    assert!(
        syntax.status.success(),
        "macOS finalizer syntax check failed:\n{}\n{}",
        String::from_utf8_lossy(&syntax.stdout),
        String::from_utf8_lossy(&syntax.stderr)
    );

    let output = Command::new("node")
        .args(["scripts/finalize-macos-acceptance.mjs", "--self-test"])
        .output()
        .expect("failed to run macOS acceptance finalizer self-test");
    assert!(
        output.status.success(),
        "macOS finalizer self-test failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "macOS dual-lane acceptance finalizer self-test passed."
    );
}
