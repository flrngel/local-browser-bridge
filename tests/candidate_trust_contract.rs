use std::fs;

fn normalized_source(path: &str) -> String {
    fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

#[test]
fn macos_release_archive_inventory_is_canonical_and_shared_by_all_producers_and_consumers() {
    let expected = [
        "LICENSE",
        "Local Browser Bridge.app",
        "Local Browser Bridge.app/Contents",
        "Local Browser Bridge.app/Contents/Info.plist",
        "Local Browser Bridge.app/Contents/MacOS",
        "Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop",
        "Local Browser Bridge.app/Contents/_CodeSignature",
        "Local Browser Bridge.app/Contents/_CodeSignature/CodeResources",
        "Local Computer Helper.app",
        "Local Computer Helper.app/Contents",
        "Local Computer Helper.app/Contents/Info.plist",
        "Local Computer Helper.app/Contents/MacOS",
        "Local Computer Helper.app/Contents/MacOS/local-computer-helper",
        "Local Computer Helper.app/Contents/_CodeSignature",
        "Local Computer Helper.app/Contents/_CodeSignature/CodeResources",
        "THIRD_PARTY_LICENSES.txt",
        "local-browser-bridge",
    ];
    let inventory = normalized_source("packaging/macos/release-archive-inventory.txt");
    assert_eq!(inventory, expected.join("\n") + "\n");

    for producer in [
        normalized_source(".github/workflows/deploy.yml"),
        normalized_source("scripts/deploy.sh"),
    ] {
        assert!(producer.contains("packaging/macos/release-archive-inventory.txt"));
        assert!(producer.contains("sed 's:/$::' | LC_ALL=C sort"));
    }
}

#[test]
fn candidate_binder_is_source_attempt_artifact_and_attestation_bound() {
    let script = fs::read_to_string("scripts/fetch-verify-release-candidate.sh").unwrap();

    for required in [
        "${BASH_SOURCE[0]}",
        "candidate trust script must be executed through its canonical path without symlink traversal",
        "candidate trust script must be one ordinary, singly linked file",
        "candidate trust script must be current-user owned and not group/other writable",
        "git -C \"$SCRIPT_DIRECTORY\" rev-parse --show-toplevel",
        "Candidate trust script is not executing from its canonical source-tree location.",
        "status --porcelain=v2 --untracked-files=all",
        "rev-parse --abbrev-ref HEAD",
        "diff --quiet HEAD --",
        "diff --cached --quiet",
        "ls-files --deleted",
        "ls-files --others --exclude-standard",
        "fsck --full",
        "HEAD:$SCRIPT_RELATIVE",
        "hash-object -- \"$SCRIPT_PATH\"",
        "candidate destination and parent must use canonical paths without symlink traversal",
        "candidate destination parent must be owned by the current user with mode 0700",
        "candidate destination ancestry contains an unprotected writable directory",
        "assert_destination_identity",
        "candidate destination identity changed during verification",
        "actions/runs/$RUN_ID/attempts/$RUN_ATTEMPT",
        "actions/runs/$RUN_ID/attempts/$RUN_ATTEMPT/jobs?per_page=100",
        "actions/runs/$RUN_ID/artifacts?per_page=100",
        "actions/artifacts/$ARTIFACT_ID\" > \"$ARTIFACT_API_JSON",
        "Assemble frozen release candidate",
        "direct artifact metadata is not bound to the successful current-attempt assemble job",
        "release-candidate-artifact-$ARTIFACT_ID.zip",
        "EXPECTED_ARTIFACT_BYTES",
        "EXPECTED_ARTIFACT_SHA256",
        "python3 - \"$ARTIFACT_ZIP\" \"$PAYLOAD_DIRECTORY\"",
        "outer artifact ZIP inventory changed before bounded extraction",
        "RELEASE_FILES=(\"${ASSETS[@]}\" \"SHA256SUMS.txt\")",
        "shasum -a 256 -c SHA256SUMS.txt",
        "verify-release-assets.sh",
        "gh attestation verify",
        "--source-ref \"$WORKFLOW_REF\"",
        "--source-digest \"$SOURCE_SHA\"",
        "--signer-workflow \"$REPOSITORY/.github/workflows/deploy.yml\"",
        "--deny-self-hosted-runners",
        "--format json",
        ".verificationResult.statement.predicate.runDetails.metadata.invocationId",
        ".verificationResult.signature.certificate.runInvocationURI",
        "runnerEnvironment == \"github-hosted\"",
        "artifactZipSha256:$artifact_sha256",
        "checksumManifestSha256:$manifest_sha256",
        "passed:true",
    ] {
        assert!(
            script.contains(required),
            "candidate binder is missing {required}"
        );
    }

    let raw_download = script.find("actions/artifacts/$ARTIFACT_ID/zip").unwrap();
    let raw_digest = script.find("raw artifact ZIP digest mismatch").unwrap();
    let extraction = script
        .find("python3 - \"$ARTIFACT_ZIP\" \"$PAYLOAD_DIRECTORY\"")
        .unwrap();
    let inventory = script
        .find("outer artifact ZIP inventory changed before bounded extraction")
        .unwrap();
    let attestation = script.find("gh attestation verify").unwrap();
    let binding = script.rfind("candidate-binding.json").unwrap();
    assert!(raw_download < raw_digest);
    assert!(raw_digest < extraction);
    assert!(extraction < inventory);
    assert!(extraction < attestation);
    assert!(attestation < binding);

    for required in [
        "maximum_entry_bytes = 256 * 1024 * 1024",
        "maximum_total_bytes = 512 * 1024 * 1024",
        "outer artifact ZIP contains duplicate entries",
        "outer artifact ZIP exceeds the bounded uncompressed candidate size",
        "os.O_EXCL | getattr(os, \"O_NOFOLLOW\", 0)",
        "outer artifact ZIP entry expanded beyond its declared size",
    ] {
        assert!(
            script.contains(required),
            "candidate trust binder bounded extraction is missing `{required}`"
        );
    }

    for forbidden in [
        "chmod +x \"$PAYLOAD_DIRECTORY",
        "--no-update-check",
        "--version",
        "local-browser-bridge.exe\" &",
        "local-computer-helper.exe\" &",
    ] {
        assert!(
            !script.contains(forbidden),
            "candidate trust binder must not execute candidate bytes: {forbidden}"
        );
    }
}

#[test]
fn bash_candidate_binder_requires_static_only_release_asset_verification() {
    let script = normalized_source("scripts/fetch-verify-release-candidate.sh");
    let expected_call = concat!(
        "bash \"$SOURCE_ROOT/scripts/verify-release-assets.sh\" \\\n",
        "  \"$VERSION\" \"$PAYLOAD_DIRECTORY\" --static-only >/dev/null"
    );

    assert!(
        script.contains(expected_call),
        "the candidate binder must explicitly select the non-executing release-asset verifier path"
    );
    assert_eq!(
        script.matches("--static-only").count(),
        1,
        "the binder must have one unambiguous static-only policy call"
    );
}

#[test]
fn withdrawn_v01211_candidate_metadata_is_explicitly_non_runtime_evidence() {
    let root = "evidence/v0.12.11/computer/attempts/withdrawn-414dd7f-macos-dual-lane-receipt-gap";
    let readme = fs::read_to_string(format!("{root}/README.md")).unwrap();
    let metadata: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(format!("{root}/candidate-metadata.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(metadata["status"], "withdrawn-before-execution");
    assert_eq!(metadata["reason"], "macos-dual-lane-receipt-gap");
    assert_eq!(metadata["candidateExecuted"], false);
    assert_eq!(metadata["macosStarted"], false);
    assert_eq!(metadata["windowsStarted"], false);
    assert_eq!(metadata["stockChromeStarted"], false);
    assert_eq!(metadata["releaseCreated"], false);
    assert_eq!(metadata["publishJobStepsRun"], 0);
    assert!(readme.contains("must not be retried or reused"));
    assert!(readme.replace('\n', " ").contains("not runtime evidence"));
}
