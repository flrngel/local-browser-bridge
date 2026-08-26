# Windows v0.12.31 stock-Chrome acceptance

This directory defines the release gate for the v0.12.31 browser extension and
packaged Windows helper. It is protocol infrastructure, not passing evidence.
Passing evidence exists only when the checked-in coordinator creates one fresh,
candidate-bound `browser-acceptance.json` from a completed run.

The immutable v0.12.26 candidate was withdrawn twice before stock-Chrome
acceptance. Attempt 1 stopped in the macOS quiet lane after an independently
corroborated shared-seat pointer event during native `typeText`. Attempt 2
stopped before any candidate execution when the trust verifier rejected two
otherwise-valid attestations for the byte-identical extension, one from each
workflow attempt. Version 0.12.27 corrected that defect by validating every
returned statement's exact source, tag, workflow, GitHub-hosted runner,
same-run, and subject identity, then requiring exactly one current-attempt
statement. Zero, duplicate-current, malformed, wrong-subject, and other-run
results fail closed.

The exact v0.12.27 candidate passed both packaged macOS lanes and their
independent audit. Its Windows trust, source, parser, and self-tests also
passed, but an ad-hoc external launcher lost the repository runner's stdout,
stderr, exit code, and process-start telemetry before an evidence directory
appeared. Because the runner probes both packaged executables before creating
that directory, candidate-byte execution could neither be proved nor excluded.
The attempt was therefore terminal outcome-unknown and the version was
withdrawn without a Computer Use action, stock-Chrome run, evidence commit,
approval, or public Release. Version 0.12.31 retains the exact-attempt trust
rules and completes the checked-in coordinator's source-only named-Job
ownership boundary. Recovery is serialized by the stable admission mutex and
one monotonic deadline, drains only the exact inspected previous Job, waits for
namespace disappearance, and performs one fail-closed fresh create. A raced
same-name object is never adopted, configured, terminated, retained, or
retried. The worker is bound to the configured kill-on-close Job before Worker
or Intent state is published.

The Windows-native non-product self-test covers all clean, recovery, delayed
handle, timeout, create-race, guard-transfer, descendant-cleanup, control-process,
and stream-cleanup scenarios with unique test-only names. It launches no
candidate and performs no Chrome action. The read-only watcher still starts
only after the atomic foreground-arm request marker exists; every `Follow`
projection remains notification-only with `uiActionAllowed: false` and grants
no consent or action authority. No v0.12.26 or v0.12.27 candidate byte, result,
screenshot, approval, or receipt is reused. Fresh packaged Windows and
stock-Chrome acceptance remain mandatory.

v0.12.21 was withdrawn before any candidate execution or publication
after review found that the macOS fetch binder could execute the candidate's
`--version` and `--licenses` paths before the quiet-seat gate.

The exact v0.12.22 candidate later failed closed in its first quiet macOS action
because a keyboard-aware independent classifier was incorrectly applied to the
pointer-only sealed action record. The action itself was Confirmed and its
route, focus, pointer, and Space fields were safe. Deliberate macOS, Windows,
and stock-Chrome never started; publication was canceled and no v0.12.22
Release exists. No v0.12.22 binary, screenshot, result, or approval may be
reused for this v0.12.31 run.

The exact v0.12.23 candidate then passed its packaged quiet macOS lane and
reached the deliberate lane's exact app-share start receipt, but its runner
reused a stream frame older than the three-second product-authority lease. The
helper correctly refused `computer.click` with `COMPUTER_STALE_FRAME` before
dispatch. Windows and stock-Chrome never started, publication was canceled,
and no v0.12.23 Release exists. Its retained negative record cannot satisfy or
be reused by v0.12.31.

Versions v0.12.11 through v0.12.13 were withdrawn before Windows stock-Chrome
acceptance completed. Their candidate bytes, screenshots, approvals, operator
messages, and evidence cannot be reused for v0.12.31. An interrupted or failed
v0.12.31 attempt is preserved only as sanitized negative metadata and is never
resumed or rerun against the same frozen candidate.

## Test boundary

Use the ordinary user's installed Google Chrome, existing profile and signed-in
state, one new dedicated Chrome window, and one fresh test-owned extension
directory. The executor opens `chrome://extensions`, invokes Chrome's native
**Load unpacked** picker, and selects only the byte-verified candidate directory.
Do not relaunch Chrome with flags, create a test profile, enable remote
debugging, call CDP directly, use `--load-extension`, or mutate an existing
Local Browser Bridge identity.
The accepted installation path is Chrome's native **Load unpacked** picker.

Local Browser Bridge owns `chrome.debugger` while its browser lease is active.
Chrome allows only one debugger client per tab, so Chrome MCP must not attach
during that lease. A passing record requires:

- `debuggerOwnerDuringBridgeLease: local-browser-bridge-extension`;
- `competingDebuggerAttachmentAllowed: false`;
- `chromeMcpUsedDuringBridgeLease: false`; and
- `chromeMcpReleaseEvidenceClaimed: false`.

The Local Browser Bridge API executes the browser-method matrix. The exact
candidate-bound helper uses only the authenticated loopback API for stock-Chrome
UI, the native picker, the extension popup, exact-window sharing, native input,
and retained screenshots. External orchestration is not credited as product
control or screenshot capture. Its exact retained orchestration identity is
`user-orchestrator-secured-ssh-exported-file-review`: the user-facing
orchestrator coordinates the interactive-session executor over pinned secured
SSH, while the independent reviewer reads only exported files.

Two create-once records conforming to
[`external-surface-attestation.schema.json`](external-surface-attestation.schema.json)
bound that exclusion honestly. The preflight record is a point-in-time statement
that Chrome MCP was not used and every Computer Use app-share was released before
candidate execution. The postflight record, written only after all six independent
reviews finish, states that Chrome MCP was never used through review, no Computer
Use share was resumed, and the reviewer used exported digest-bound files only.
The reviewer reads those exported files through the filesystem and never needs a
Computer Use share. Both
records contain the exact release-candidate binding and its canonical SHA-256 and
must use the same orchestrator session reference later used to deliver the scoped
user approval. They are orchestration attestations, not cryptographic proof of a
different Windows identity.

## One scoped action-time approval

The only browser-control safety confirmation is one batched user approval
immediately before the first covered Chrome mutation. The separate secure
GitHub-token prompt, when needed, supplies a credential to the trust gate and is
not browser-control consent. The approval is neither a standing authorization
nor a generic chat instruction. The create-once
[`scoped-action-approval.schema.json`](scoped-action-approval.schema.json)
record binds the request and response to the exact release candidate, run nonce,
scope digest, executor session, distinct orchestrator session, explicit
`user-via-orchestrator` delivery role, request digest, confirmation time, and expiry.
It covers exactly these ordered actions for this single candidate run:

1. conditional Developer Mode change;
2. load and run the exact unpacked candidate;
3. conditional Full Access change;
4. save the ephemeral loopback credential;
5. clear the ephemeral loopback credential;
6. remove the exact test-owned extension;
7. restore the captured browser settings; and
8. failure rollback.

The scope is limited to loopback, the dedicated window, restoration of captured
state, and no unrelated-extension mutation. The approval is consumed before the
first covered action, cannot be replayed, and remains valid for owned cleanup if
the run fails. Any candidate, scope, session, or request-digest change invalidates
it. Expiry before consumption also fails closed.

Creating the dedicated window and navigating it to `chrome://extensions` are
intentional non-covered, low-risk setup needed to capture the exact live state
shown in the approval challenge. No covered or security-sensitive action occurs
before approval; approval and a distinct fresh state revalidation must both
precede the first such dispatch.

Every covered helper action carries the same `approvalRef`; actions outside the
scope carry `approvalRef: none`. Each action also carries a granular `riskRef`,
a create-once operator-decision digest, and its dispatch timestamp. The finalizer
checks the reference equality and time ordering. Developer Mode and Full Access
are conditional checkpoints: if their captured initial state already satisfies
the run, no toggle is dispatched, but the approval still covers the contingency.

No record claims repeated user clicks, user screenshot review, or user-authored
operator responses. After the single approval, the executor performs the entire
flow, including Chrome's token-clear confirmation and rollback when needed.

## Independent executor and reviewer

Executor UI decisions travel through create-once request/response files. The
helper chain records distinct opaque executor and reviewer session references,
an independent-session boundary, status-decision and fresh-frame-decision
counts, a response-chain digest, and deletion of the private exchange scratch.
Every frame-dependent decision must bind a fresh helper frame; exact-window
selection binds the fresh reduced status inventory instead. The finalizer checks
that request count equals status decisions plus fresh-frame decisions plus the
single scoped-approval request.

Both exchanges live in fresh owner-only, inheritance-disabled, non-reparse
directories. A request is serialized once with `FileMode.CreateNew`, atomically
published, and hashed from the exact bytes written. A response is atomically
claimed, its old pathname is reserved against replay, and its JSON and SHA-256
come from one exclusive stable read. Every response must echo the exact request
SHA-256, candidate-binding SHA-256, and `inputDigestSha256`. For helper UI
decisions, that input digest is the exact fresh PNG digest or reduced
`computer.status` response digest. For screenshot cropping it is the source PNG
digest; for the ordered six-crop review it is the canonical review-input digest.
After every response is claimed and validated, the executor exclusively re-reads
and re-hashes the exact request and every referenced frame or ordered crop before
accepting the decision or deleting scratch. Missing, reordered, extra, stale,
replayed, same-session, changed-session,
uncertain, or sensitive responses fail closed.

The coordinator prints `OPERATOR_REQUEST <absolute scratch path>` or
`REVIEWER_REQUEST <absolute scratch path>` only as a live orchestration signal;
no absolute exchange path is retained. One independent reviewer agent keeps one
opaque reviewer session reference for all UI interpretation, cropping, and six
final reviews. It writes only the exact response shape declared by each request.
The procedure assigns reviewer responses only to that reviewer session and the
user approval only to a third orchestrator session after it obtains the user's
answer to the single exact candidate-and-scope challenge. The finalizer rejects
session-reference collisions or responder-role mismatches; this is procedural
and record-level separation subject to the same-SID limitation below.

The live responder reads the request through a stable ordinary-file read. It
creates `response-<requestId>.json.new` with `FileMode.CreateNew`, writes the
complete response, calls `Flush(true)`, closes it, and atomically renames it to
`response-<requestId>.json` in the same directory. It must never write the final
pathname directly. The consumer waits while `.new` exists, atomically claims
the final file, and holds exclusive zero-byte reservations on both publication
names until exact-inventory cleanup. Partial publication, replacement, replay,
and every unregistered request, response, or frame fail closed. The response has
exactly these ordered envelope fields:

```text
schemaVersion, evidenceType, requestId, requestSha256,
candidateBindingSha256, inputDigestSha256, responderKind,
responderSessionRef, respondedAtUtc, decision
```

All binding values come from the request except `requestSha256`, which is the
SHA-256 of the exact request-file bytes, and `respondedAtUtc`, which must be a
canonical fresh timestamp inside the request's interval. Operator `decision`
has exactly one shape selected by `kind`: `{index}`, `{x,y}`, `{value}`,
`{passed}`, or—for the one user challenge only—
`{approved,approvedBy,confirmationMode}`. Reviewer `decision` is either the
exact seven-field crop verdict or the exact ordered-six-entry review verdict
described by `allowedResponse`. No compatibility aliases or human/manual
receipt fields are accepted.

Response bytes are canonical: UTF-8 without BOM, the exact property order emitted
by Windows PowerShell 5.1 `ConvertTo-Json -Depth 30 -Compress`, and exactly one LF.
Pretty JSON, CRLF, duplicate keys, case-colliding keys, reordered keys, and trailing
data are rejected. Use the shipped
[`scripts/write-stock-chrome-operator-response.ps1`](../../../scripts/write-stock-chrome-operator-response.ps1)
publisher instead of implementing this byte contract ad hoc. Any operator or
reviewer request may publish the exact negative decision `{"unable":true}`; the
consumer re-hashes its request and inputs and then enters rollback immediately.
`{"unable":false}` and mixed-field unable decisions are invalid. User denial keeps
the explicit approval shape with `approved:false`; unsafe screenshot findings keep
their detailed crop/review fields.

Distinct opaque session references and create-once digest bindings prove the
recorded protocol/session separation only. They are not cryptographic proof of
different people, Windows SIDs, or processes: software already controlling the
same owner account could forge exchange JSON. The trust boundary therefore also
depends on disciplined orchestration that assigns the executor, independent
reviewer, and user-facing approval delivery to separate live agent sessions.

The executor sanitizes six exact-window captures and creates a digest-bound
review request. A separate reviewer session opens each final PNG and produces
one create-once record conforming to
[`independent-visual-review.schema.json`](independent-visual-review.schema.json).
Its six ordered entries bind:

- sequence, purpose, and exact filename;
- final image SHA-256 and dimensions;
- the SHA-256 of the required-visible-state statement;
- digest match and required-state verdict; and
- `sensitivePixelsObserved: false` and `uncertain: false`.

Those last two values report only what the independent reviewer observed. They
are not proof that arbitrary pixels are safe. The required aggregate therefore
states `visualJudgmentNotPixelSafetyProof: true`. The protocol makes no OCR,
automated semantic-inspection, or automatic pixel-redaction claim. A reviewer
that sees sensitive content or is uncertain must fail closed; it cannot set a
passing verdict.

The six required views are:

1. the installed v0.12.31 extension and Chrome's debugger-use indicator;
2. the browser API result;
3. the exact-window computer-share action and synthetic session pointer;
4. the fail-closed paused state after visible **Stop**;
5. the fail-closed paused state after Chrome's native **Cancel**; and
6. the recovered active state after both trusted-popup resume cycles.

## Authoritative coordinator

Do not copy PowerShell fragments into a new wrapper. Run the exact source blob
[`scripts/test-windows-stock-chrome.ps1`](../../../scripts/test-windows-stock-chrome.ps1)
exactly once from a clean detached checkout. It self-spawns the exact 64-bit
Windows PowerShell 5.1 system binary with `-NoProfile`, clears its parameter
handoff variables, checks its source blob, and fails closed.

First run
[`scripts/verify-windows-release-candidate.ps1`](../../../scripts/verify-windows-release-candidate.ps1)
to bind the repository, exact `main` source commit, workflow run and attempt,
artifact ID, raw artifact-ZIP SHA-256, canonical five-file inventory, LF checksum
manifest, every payload digest, PE identity, all five GitHub attestations, and
both attestation attempt URIs. Pass its create-once `candidate-binding.json` to
the coordinator.

Choose a fresh private destination whose full path is at most 90 characters,
and pass absolute paths to ordinary `git.exe` and `gh.exe` installations. Run
the trust wrapper once:

```powershell
$trustDestination = Join-Path ([IO.Path]::GetTempPath()) ("lbb-1230-" + [Guid]::NewGuid().ToString("N"))
& .\scripts\verify-windows-release-candidate.ps1 `
  -Version 0.12.31 `
  -WorkflowRunId EXACT_RUN_ID `
  -WorkflowRunAttempt EXACT_RUN_ATTEMPT `
  -ArtifactId EXACT_ARTIFACT_ID `
  -SourceSha EXACT_SOURCE_SHA `
  -Destination $trustDestination `
  -TrustedGit (Get-Command git.exe).Source `
  -TrustedGh (Get-Command gh.exe).Source
```

Continue only after it prints `Windows release-candidate trust gate passed.`
Use its printed `Payload`, `Binding`, and `Source` paths verbatim. From that exact
clean checkout, create one persistent orchestrator reference and the preflight
external-surface attestation. `AttestExternalSurfaces` initializes the fresh output
directory with an owner-only protected ACL when it writes preflight:

```powershell
$orchestratorRef = & .\scripts\write-stock-chrome-operator-response.ps1 -Mode NewSessionRef
$privateParent = Join-Path ([IO.Path]::GetTempPath()) ("lbb-1230-acceptance-" + [Guid]::NewGuid().ToString("N"))
$preflightExternal = Join-Path $privateParent "external-surface-preflight.json"
$postflightExternal = Join-Path $privateParent "external-surface-postflight.json"
& .\scripts\write-stock-chrome-operator-response.ps1 `
  -Mode AttestExternalSurfaces `
  -CandidateBindingPath C:\absolute\private\candidate\candidate-binding.json `
  -ExternalSurfacePhase preflight `
  -ResponderSessionRef $orchestratorRef `
  -OutputPath $preflightExternal
```

At this point every Computer Use app-share must already be released. Invoke:

```powershell
$pipeName = "lbb-gh-" + [Guid]::NewGuid().ToString("N")
& .\scripts\test-windows-stock-chrome.ps1 `
  -FinalSha EXACT_SOURCE_SHA `
  -WorkflowRunId EXACT_RUN_ID `
  -WorkflowRunAttempt EXACT_RUN_ATTEMPT `
  -ReleaseCandidateArtifactId EXACT_ARTIFACT_ID `
  -ReleaseCandidateArtifactZipSha256 EXACT_RAW_ARTIFACT_ZIP_SHA256 `
  -ManifestSha EXACT_SHA256SUMS_SHA256 `
  -Candidate C:\absolute\private\candidate\payload `
  -CandidateBinding C:\absolute\private\candidate\candidate-binding.json `
  -PrivateParent $privateParent `
  -TrustedGit C:\absolute\trusted\git.exe `
  -TrustedGh C:\absolute\trusted\gh.exe `
  -ExternalSurfacePreflightAttestation $preflightExternal `
  -ExternalSurfacePostflightAttestation $postflightExternal `
  -GitHubTokenPipeName $pipeName
```

Remote SSH coordination is supported only when the SSH PowerShell process is in
the signed-in interactive desktop session: its `SessionId` must be greater than
zero and exactly match Explorer's session. A service or Session-0 `sshd` cannot
drive the helper UI and is unsupported. The verified portable recovery `sshd`
path satisfies this condition; every other SSH setup must prove it again before
candidate execution.

Run the coordinator asynchronously while one independent reviewer agent watches
its `OPERATOR_REQUEST` and `REVIEWER_REQUEST` lines. That reviewer keeps one fresh
64-hex reviewer reference for the whole run, obtains each exact request and named
PNG over the secured operator channel, verifies the published digests, interprets
only those exported files, and publishes its decision with:

```powershell
$reviewerRef = & .\scripts\write-stock-chrome-operator-response.ps1 -Mode NewSessionRef
& .\scripts\write-stock-chrome-operator-response.ps1 `
  -Mode Respond `
  -RequestPath C:\absolute\private\exchange\request-EXACT_ID.json `
  -ResponderKind independent-agent `
  -ResponderSessionRef $reviewerRef `
  -DecisionJson $canonicalCompactDecision
```

When the request kind is `scoped-user-approval`, the reviewer stops. The
user-facing orchestrator presents the exact candidate/scope challenge, obtains the
single real answer, and calls the same publisher with
`-ResponderKind user-via-orchestrator`, `$orchestratorRef`, and the exact approval
decision. No other request may use that role. After the six-crop review completes,
the coordinator prints `EXTERNAL_SURFACE_POSTFLIGHT_REQUIRED <path>` and waits up
to 15 minutes. The same orchestrator session then writes:

```powershell
& .\scripts\write-stock-chrome-operator-response.ps1 `
  -Mode AttestExternalSurfaces `
  -CandidateBindingPath C:\absolute\private\candidate\candidate-binding.json `
  -ExternalSurfacePhase postflight `
  -ResponderSessionRef $orchestratorRef `
  -OutputPath $postflightExternal
```

The executor rejects a different attestor/approval reference, a postflight earlier
than helper completion or independent review, a changed candidate binding, or a
postflight that does not close the exact no-Chrome-MCP/no-resumed-Computer-Use-
share interval. The reviewer uses ordinary filesystem image reads only and cannot
inspect live Chrome.

For unattended execution, the shipped responder's `RelayGitHubToken` mode creates
that fresh owner-only named-pipe server and streams exactly one independent
least-privilege GitHub token as a bounded printable-ASCII line from stdin. The
coordinator reads it byte by byte directly
into a read-only `SecureString`; the pipe name is non-secret and the token never
appears in an argument, environment variable, file, or retained output. It
exposes plaintext only inside each bounded `gh` child environment and then
clears it. If `-GitHubTokenPipeName` is omitted, the coordinator uses one secure
token prompt instead. This credential feed is independent trust-gate input, not
browser-control consent or a second user approval.

Generate the non-secret pipe name, start the coordinator with it, and within its
30-second connection deadline stream the credential from a secured controller.
For the verified recovery SSH alias, the controller command is:

```bash
gh auth token | ssh flrngel19-recovery 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoLogo -NoProfile -NonInteractive -File C:\absolute\clean\scripts\write-stock-chrome-operator-response.ps1 -Mode RelayGitHubToken -GitHubTokenPipeName lbb-gh-EXACT_FRESH_32_HEX'
```

The relay has one client, a 4096-byte maximum, and one absolute 30-second
accept/read/write/flush deadline. It never converts production credential bytes
to an immutable string and clears its one-byte transfer buffer on every exit.

For maximum unattended execution, provision that least-privilege token through
the named-pipe channel before starting the coordinator and have the
orchestrator/reviewer service ready before the first request marker appears.
The only inherently non-automatable browser-control checkpoint is the one
candidate-bound user approval above. A CAPTCHA, browser security interstitial,
unexpected account or legal prompt, permission expansion, candidate change, or
scope change is not covered by that approval and must stop for a new user
handoff; the harness must never guess through it.

The source-only smoke check is:

```powershell
& "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" `
  -NoLogo -NoProfile -File .\scripts\test-windows-stock-chrome.ps1 -SelfTest
```

It must print exactly:

```text
Windows stock-Chrome coordinator self-test passed.
```

## Autonomous acceptance sequence

After trust and source gates pass, the coordinator and executor:

1. capture the exact Chrome-window inventory, open one dedicated stock-Chrome
   window, and navigate it to `chrome://extensions`;
2. capture candidate absence and initial Developer Mode from fresh frames, then
   create a separate fresh approval-challenge frame, request the single
   candidate-bound approval, and use one more distinct fresh frame to verify
   candidate absence, Developer Mode, and the dedicated-window binding remain
   unchanged immediately before the first covered mutation;
3. perform only required conditional toggles and load the verified directory
   with the native picker, then capture Full Access and token state before either
   is changed;
4. verify exactly one new test-owned card, version 0.12.31, no load errors, and
   popup state **Not configured** before saving the ephemeral credential;
5. run the browser API matrix, start exact-window sharing, and verify Chrome's
   native debugger-use notice;
6. exercise visible **Stop**, native **Cancel**, both fail-closed 423 refusals,
   and a fresh successful lease after each trusted-popup resume;
7. capture and sanitize the six views, then hand the immutable digest-bound
   request to the independent reviewer session;
8. clear the saved credential, accept the popup confirmation under the scoped
   approval, verify **Not configured**, restore captured settings, and remove
   only the exact test-owned extension; and
9. terminate only owned helper/server processes, release listeners and control,
   close only the dedicated window/tabs, verify executable/payload integrity,
   and delete raw images, exchange scratch, and the extracted test directory.

Switch back from the dedicated window before removal when necessary so the
helper does not confuse the target with the extensions page. Never remove,
disable, or edit another extension. Cleanup claims are passing only after fresh
live UI proves the candidate card is absent and Developer Mode, Full Access, and
saved-token state match their captured initial values. Failure cleanup uses the
same narrow ownership and scope constraints; it never broadens authority. Any
failed run deletes the entire exact test-owned partial evidence directory before
writing sanitized attempt metadata, so an uncertain or sensitive review cannot
leave a PNG or sidecar behind. The cleanup record reports the measured
`partialEvidenceDirectoryDeleted` result and uses
`sensitiveScratchDisposition: unknown` if deletion or any sensitive-scratch
cleanup cannot be proven.

## Evidence inventory and release use

Immediately before finalization, the private non-reparse evidence directory
contains exactly twenty-one ordinary files:

- nine aggregate records: `candidate-preflight.json`,
  `candidate-postflight.json`, `browser-api-matrix.json`,
  `browser-computer-helper-chain.json`, `operator-results.json`,
  `scoped-action-approval.json`, `independent-visual-review.json`,
  `external-surface-preflight.json`, and `external-surface-postflight.json`;
- six sanitized PNGs; and
- six digest-bound finalized screenshot sidecars.

The finalizer creates the twenty-second file, `browser-acceptance.json`, once. Raw
screenshots, pending-review files, request/response exchange scratch, API bodies,
credentials, tokens, filesystem paths, browser history, raw identifiers, and
external tool logs are excluded. The retained record must report exact native
`chrome://extensions` installation by the executor, all API/helper assertions,
the native debugger indication, Stop/Cancel/recovery, the one scoped approval,
the independent six-image review, restored settings, cleared credentials,
removed test identity, and complete owned-process/listener cleanup.

The aggregate operator record conforms to
[`operator-results.schema.json`](operator-results.schema.json), and the packaged
helper chain conforms to
[`computer-helper-chain.schema.json`](computer-helper-chain.schema.json).

Only after macOS quiet, macOS deliberate-concurrency, Windows helper, and this
stock-Chrome lane all pass may the allowlisted sanitized evidence enter one new
single-parent release-evidence commit whose only parent is the exact source
commit. A branch name or environment variable is a selector, never authority.
No release is published from a failed, resumed, mixed-parent, partially
reviewed, or locally-only evidence tree.
