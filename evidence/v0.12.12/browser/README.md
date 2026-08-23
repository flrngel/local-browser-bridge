# Windows v0.12.12 stock-Chrome acceptance

This directory defines the release gate for the v0.12.12 browser extension and
packaged Windows helper. It is protocol infrastructure, not passing evidence.
Passing evidence exists only when the checked-in coordinator creates a final,
candidate-bound `browser-acceptance.json` from one fresh run.

Version v0.12.11 was withdrawn before any candidate byte ran. Its release
receipt could bind only one macOS result even though policy required two fresh,
non-mergeable macOS lanes. Windows and Chrome acceptance never started, and no
v0.12.11 Release exists. The retained withdrawal record is
[`withdrawn-414dd7f-macos-dual-lane-receipt-gap`](../../v0.12.11/computer/attempts/withdrawn-414dd7f-macos-dual-lane-receipt-gap/README.md).

No prior result, screenshot, notification, operator marker, candidate download,
or generated evidence byte may be reused for v0.12.12. A failed or interrupted
attempt is preserved as negative evidence and is never resumed or rerun against
the same frozen candidate.

## Test boundary

Use the ordinary user's installed Google Chrome, their existing profile and
signed-in state, one new dedicated Chrome window, and a fresh test-owned
extension directory. Load the extension through `chrome://extensions` and the
native **Load unpacked** picker. Do not relaunch Chrome with flags, create a
test profile, enable remote debugging, call CDP directly, use
`--load-extension`, or modify an existing Local Browser Bridge identity.

Local Browser Bridge owns `chrome.debugger` while its browser lease is active.
Chrome permits only one debugger client per tab, so Chrome MCP must not attach
during that lease. The passing record therefore requires:

- `debuggerOwnerDuringBridgeLease: local-browser-bridge-extension`;
- `competingDebuggerAttachmentAllowed: false`;
- `chromeMcpUsedDuringBridgeLease: false`; and
- `chromeMcpReleaseEvidenceClaimed: false`.

The Local Browser Bridge API executes the browser method matrix. The exact
candidate-bound helper uses the authenticated loopback API for stock-Chrome UI,
the native picker, the extension popup, exact-window sharing, native input, and
the retained screenshots. External computer-use software may interpret a frame
and obtain action-time human consent, but it is not credited as product control
or screenshot capture.

Consent is action-specific. Pause immediately before installing the new
identity, changing Developer Mode when required, enabling Full Access, saving
the ephemeral token, initiating and confirming token clearing, and removing the
exact test-owned identity. Never treat a prior click, chat message, or generic
authorization as consent for a later checkpoint.

## Authoritative coordinator

Do not copy PowerShell fragments from documentation. Run the tagged source blob
[`scripts/test-windows-stock-chrome.ps1`](../../../scripts/test-windows-stock-chrome.ps1)
exactly once from a clean detached checkout. It self-spawns the exact 64-bit
Windows PowerShell 5.1 system binary with `-NoProfile`, clears its parameter
handoff variables, checks its own tagged blob, and fails closed.

First use
[`scripts/verify-windows-release-candidate.ps1`](../../../scripts/verify-windows-release-candidate.ps1)
to download and verify the exact current-run `release-candidate` artifact. The
wrapper must bind the repository, annotated tag object and peel, workflow run
and attempt, artifact ID, raw artifact-ZIP SHA-256, canonical five-file
inventory, LF checksum manifest, every payload digest, PE identity, all five
GitHub attestations, and both attestation attempt URIs before candidate
execution. Pass its create-once `candidate-binding.json` to the coordinator.

Choose a fresh private destination whose full path is at most 90 characters,
and pass absolute paths to ordinary `git.exe` and `gh.exe` installations. If
`GH_TOKEN` is absent, the wrapper uses a secure prompt; if it is present, the
wrapper moves it into a `SecureString`, clears the parent-process copy, and
exposes it only to bounded `gh` children. Run the trust wrapper once:

```powershell
$trustDestination = Join-Path ([IO.Path]::GetTempPath()) ("lbb-1212-" + [Guid]::NewGuid().ToString("N"))
& .\scripts\verify-windows-release-candidate.ps1 `
  -Version 0.12.12 `
  -WorkflowRunId EXACT_RUN_ID `
  -WorkflowRunAttempt EXACT_RUN_ATTEMPT `
  -ArtifactId EXACT_ARTIFACT_ID `
  -SourceSha EXACT_SOURCE_SHA `
  -TagObjectSha EXACT_ANNOTATED_TAG_OBJECT_SHA `
  -Destination $trustDestination `
  -TrustedGit (Get-Command git.exe).Source `
  -TrustedGh (Get-Command gh.exe).Source
```

Continue only after it prints `Windows release-candidate trust gate passed.`
Use its printed `Payload`, `Binding`, and `Source` paths verbatim. The `Source`
path is the clean detached checkout from which to launch the coordinator; the
`Payload` and `Binding` paths become `-Candidate` and `-CandidateBinding`.

From the exact clean v0.12.12 checkout, invoke:

```powershell
& .\scripts\test-windows-stock-chrome.ps1 `
  -FinalSha EXACT_SOURCE_SHA `
  -TagObjectSha EXACT_ANNOTATED_TAG_OBJECT_SHA `
  -WorkflowRunId EXACT_RUN_ID `
  -WorkflowRunAttempt EXACT_RUN_ATTEMPT `
  -ReleaseCandidateArtifactId EXACT_ARTIFACT_ID `
  -ReleaseCandidateArtifactZipSha256 EXACT_RAW_ARTIFACT_ZIP_SHA256 `
  -ManifestSha EXACT_SHA256SUMS_SHA256 `
  -Candidate C:\absolute\private\candidate\payload `
  -CandidateBinding C:\absolute\private\candidate\candidate-binding.json `
  -PrivateParent C:\absolute\private\acceptance-parent `
  -TrustedGit C:\absolute\trusted\git.exe `
  -TrustedGh C:\absolute\trusted\gh.exe
```

When the coordinator asks, enter an independent least-privilege GitHub token in
its secure prompt. The coordinator passes it only to bounded `gh` children and
does not use or persist the caller's `gh` configuration. Never place a token on
a command line, in a script, in the evidence directory, or in retained output.

The coordinator's source-only smoke check is:

```powershell
& "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
  -NoLogo -NoProfile -File .\scripts\test-windows-stock-chrome.ps1 -SelfTest
```

It must print exactly:

```text
Windows stock-Chrome coordinator self-test passed.
```

## Visible acceptance sequence

After all trust and source gates pass, the coordinator pauses at the visible
checkpoints. Complete only the requested action, then stop using the Windows
session until the next checkpoint.

1. Open a new dedicated stock-Chrome window and navigate visibly to
   `chrome://extensions`.
2. If required, approve changing Developer Mode, then click **Load unpacked**
   and select only the freshly extracted candidate directory in the native
   picker.
3. Confirm exactly one test-owned Local Browser Bridge card exists and that its
   popup visibly says **Not configured** before any token is saved.
4. Approve Full Access and save only the fresh ephemeral token when prompted.
5. Run the browser API matrix through the bridge: tab/window lifecycle,
   navigation, DOM observe/action, screenshot, debugger lease, and failure/
   cancellation behavior.
6. Start exact-window computer sharing. Confirm Chrome's native debugger notice
   or pill says that Local Browser Bridge is debugging/using the browser. Test
   the extension's visible **Stop** and **Cancel** handback paths, confirm each
   lease releases, and prove a new lease can resume afterward.
7. Review all six digest-bound sanitized screenshots. They must show the
   installed extension and API result, the exact-window share action, the
   fail-closed paused state after visible **Stop**, the fail-closed paused state
   after Chrome's native **Cancel**, and the recovered active state after the
   trusted-popup resume. No crop may expose tokens, browsing history, account
   identity, unrelated windows, paths, or coordinates. This protocol performs
   no OCR and makes no automated text-inspection or pixel-redaction claim; the
   required visible state is confirmed by a human against each final PNG's
   recorded digest.
8. In the trusted popup, click **Clear saved token**, wait for the popup's
   native `confirm()` dialog, and let the human accept it at that checkpoint.
   Record `confirmationAcceptedByHuman:true`; do not inspect `chrome.storage`
   or retain the token or raw extension storage. Verify the popup returns to
   **Not configured**, restore the initial Full Access and Developer Mode
   states, then remove only the exact test-owned extension identity.

Switch back from the dedicated window before removal when needed so the helper
does not confuse the target with the extensions page. Never remove, disable, or
edit another extension.

## Evidence and release use

Immediately before finalization, the private non-reparse evidence directory
contains exactly seventeen ordinary files:

- `candidate-preflight.json`, `candidate-postflight.json`,
  `browser-api-matrix.json`, `browser-computer-helper-chain.json`, and
  `operator-results.json`;
- six sanitized PNGs; and
- six digest-bound post-review JSON sidecars.

The finalizer creates the eighteenth file, `browser-acceptance.json`, once. Raw
screenshots, pending-review files, API bodies, credentials, tokens, filesystem
paths, browser history, raw window/frame/extension identifiers, and external
tool logs are excluded. The result must report stock user Chrome, exact manual
`chrome://extensions` installation, all required API and helper assertions,
visible native debugger indication, Stop/Cancel/recovery, restored settings,
cleared credentials, removed test identity, and complete owned-process/listener
cleanup.

After macOS quiet, macOS deliberate-concurrency, Windows helper, and this stock-
Chrome lane all pass, copy the allowlisted sanitized evidence into one new
single-parent release-evidence commit whose only parent is the exact source
commit. The release gate
verifies that remote commit, its tree, every digest and candidate binding, and
the V2 receipt. A branch name or environment variable is a selector, never
authority. No release is published from a failed, resumed, mixed-parent,
partially reviewed, or locally-only evidence tree.
