# macOS v0.12.33 persistent-share evidence candidate

This directory defines a fresh deterministic evidence harness for the packaged
macOS v0.12.33 server and helper candidate. It is protocol infrastructure, not
passing evidence; no v0.12.33 packaged run exists yet.

The immutable v0.12.26 candidate was withdrawn in two separate workflow
attempts. Attempt 1 passed its source, artifact, provenance, and pre-execution
quiet-seat gates, then the macOS quiet lane correctly stopped when a healthy
HID-system boundary observed unrelated shared-seat pointer activity during
native `typeText`; Windows and stock-Chrome never started. Attempt 2 rebuilt a
fresh artifact but stopped before any candidate byte executed because the
trust verifier rejected the extension's two otherwise-valid attestations, one
from each workflow attempt. Version 0.12.27 corrected that defect: every
returned statement had to retain the exact source, tag, workflow,
GitHub-hosted runner, same-run, and subject binding, with exactly one statement
for the current attempt. Zero, duplicate-current, malformed, wrong-subject, or
other-run results fail closed.

The exact v0.12.27 candidate passed both packaged macOS lanes and their
independent audit. Its Windows trust, source, parser, and self-tests also
passed, but an ad-hoc external launcher lost the repository runner's stdout,
stderr, exit code, and process-start telemetry before an evidence directory
appeared. The runner probes both packaged executables before creating that
directory, so candidate-byte execution could neither be proved nor excluded.
The attempt was terminal outcome-unknown and v0.12.27 was withdrawn without a
Computer Use action, stock-Chrome run, evidence commit, approval, or public
Release. Version 0.12.33 retains the exact-attempt trust rules and completes the
checked-in Windows coordinator's source-only named-Job ownership boundary. The
coordinator acquires the stable admission mutex before recovery, uses one
monotonic deadline, opens and drains only the exact previous named Job, waits
for namespace disappearance, and calls `CreateJobObject` exactly once. Any
raced existing object or other nonzero create status fails closed. The worker
is bound to a configured kill-on-close Job before Worker or Intent state is
published.

The Windows-native non-product self-test covers clean acquisition, live-owner
recovery, delayed and deadline-exceeding extra handles, a same-name create race,
pre-transfer guard closure, transferred lifetime ownership and descendant
cleanup, unrelated-process isolation, and stream cleanup. It uses only unique
self-test names and fixtures; it does not launch candidate binaries. The
read-only watcher still starts only after the atomic foreground-arm request
marker exists, and every `Follow` projection remains notification-only with
`uiActionAllowed: false` and grants no consent or action authority. Neither
v0.12.26 nor v0.12.27 supplies candidate bytes or evidence for this fresh
harness. The coordinator assumes the independently verified GitHub-attested
candidate is trusted; it is not a hostile-code sandbox or sudden-power-loss
journal. Fresh packaged-platform and browser acceptance remain mandatory.

v0.12.21 was withdrawn before any candidate execution or publication
after review found that the macOS fetch binder could execute the candidate's
`--version` and `--licenses` paths before the quiet-seat gate.

The exact v0.12.22 candidate passed its native quiet-seat preflight and its
first semantic `setValue` was Confirmed with safe sealed route, focus, pointer,
and Space fields. The harness nevertheless failed closed after 55 of 56 checks
because it applied the keyboard-aware independent SystemProbe classifier to the
pointer-only sealed action schema. Deliberate macOS, Windows, and stock-Chrome
acceptance never started, the protected publication run was canceled, and no
v0.12.22 Release exists. Its three exact negative files remain only on the
single-parent evidence branch.

Version 0.12.23 separated those classifiers. Its packaged quiet lane passed all
208 checks, and its deliberate lane accepted the exact app-share start receipt
with all 89 checks reached before product dispatch passing. The runner then
reused the pre-handoff persistent-stream frame after 43.807 seconds even though
product action authority expires after three seconds. The helper correctly
refused `computer.click` with HTTP 409 `COMPUTER_STALE_FRAME` before dispatch;
no completion receipt, Windows run, stock-Chrome run, publication, or Release
followed. Its exact ten-file negative record is retained on
`evidence/v0.12.23-macos-app-share-stale-frame-32746618027` under
`evidence/v0.12.23/computer/attempts/withdrawn-9e50811-macos-app-share-stale-frame/`.
This v0.12.33 harness reuses none of those evidence bytes.

The exact v0.12.31 candidate passed its 206-check quiet macOS lane. In the
deliberate lane, the separately authorized exact app/window/button action and
all shared-seat invariants passed, but the harness allowed at most two seconds
to obtain a strictly newer frame after the receipt. That bounded refresh timed
out before product dispatch, so the dual-lane aggregate, Windows, stock Chrome,
publication, tag, and Release never existed. The complete 20-file withdrawn
record is frozen at commit
[`d33a66c65522380d7bdc310e532f5db6575f5f32`](https://github.com/flrngel/local-browser-bridge/commit/d33a66c65522380d7bdc310e532f5db6575f5f32).
Version 0.12.32 preserved those authority checks and built one trusted
candidate with the larger bounded, abortable refresh. Its macOS run passed
source, provenance, package, architecture, signature, and bundle checks, then
failed before permission probes, quiet-seat stabilization, or candidate process
launch because the source-compiled app-share self-test printed an extra
diagnostic line outside its exact success-token contract. It produced no
screenshots, operator markers, or aggregate; Windows, stock Chrome, tagging,
and publication did not follow. Its exact two-file negative record is frozen at
commit
[`9bece01635e4296fbb3ff0f3651100245c5d7729`](https://github.com/flrngel/local-browser-bridge/tree/9bece01635e4296fbb3ff0f3651100245c5d7729/evidence/v0.12.32/computer/attempts/withdrawn-05c565c-macos-app-share-handoff-self-test-failure).
Version 0.12.33 retains the 1,000 ms authority-age limit, exact share/target/
geometry checks, ban on one-shot observation, and larger abortable wait. It
restores and enforces the byte-exact one-line self-test result and reuses no
v0.12.31 or v0.12.32 candidate or evidence byte.

The exact v0.12.11 workflow candidate passed its build and provenance gates but
was withdrawn before execution. Its release receipt could bind only one macOS
result while policy required two fresh, non-mergeable lanes. Windows and
stock-Chrome acceptance never started, publication was canceled, and no
v0.12.11 Release exists. The retained record is
[`withdrawn-414dd7f-macos-dual-lane-receipt-gap`](../../v0.12.11/computer/attempts/withdrawn-414dd7f-macos-dual-lane-receipt-gap/README.md).

The exact v0.12.10 candidate completed 69 assertions and six fixture-only
screenshots, then timed out before the final product action because no
separately authorized pointer movement arrived. Windows and stock-Chrome
acceptance never started, publication was canceled, and no v0.12.10 Release
exists.

The exact tagged v0.12.12 candidate was also withdrawn. Three separately
preserved deliberate-concurrency attempts all stopped before product dispatch:
one launched its final bounded SystemProbe with only the arm deadline's
remaining fraction and surfaced that expected arm expiration as a probe
failure; one saw button/drag/scroll/tablet activity before a clean motion arm;
and one saw input plus user-context contamination while changing the prompt to
`ACTION`. In every attempt `actionDispatched` remained `false`. Those records
stay on their evidence-only branches and are negative history, not passing
bytes.

The exact tagged v0.12.13 candidate was withdrawn after its quiet macOS lane
completed 192 of 193 assertions. Product and fixture cells passed, but the
unchanged whole-run SystemProbe boundary correctly reported unrelated
`mouseMoved`/cursor activity on the shared login seat. That contamination was
not attributed to the helper, the lane was not relabeled, deliberate macOS,
Windows, and stock-Chrome acceptance never started, and publication was
canceled. Version 0.12.33 does not reuse or relabel any prior binary,
screenshot, result, marker, notification, log, or other generated evidence
byte. The negative record is retained only on branch
`evidence/v0.12.13-macos-quiet-pointer-contamination-32695400912` at commit
`bdcc3620e28260e31a3a78bf7e584adf1f0db44e`, under
`evidence/v0.12.13/computer/attempts/withdrawn-7d2692d-macos-quiet-pointer-contamination/`.

Version 0.12.18 passed its complete source matrix, but an independent audit
found that the macOS producer emitted lane-result schema 6 while the publication
verifier still required aggregate schema metadata 5. Protected-tag workflow
`32718436613` was canceled before frozen candidate assembly or execution; no
macOS, Windows, or stock-Chrome candidate lane ran and no Release exists. Its
tag remains protected historical metadata. Version 0.12.33 does not reuse any
v0.12.18 candidate or evidence byte.

The harness talks to the real loopback server API and launches the supplied
`Local Computer Helper.app` executable exactly once. It does not use a mock
connector, a polling screenshot replacement, global HID input, or a second
helper process. Its fixture creates two clearly titled, visible, genuine
`NSWindow` instances in one process from startup: the primary capture/action
target and a magenta same-PID sibling receiver sentinel.

Before invoking either supplied candidate executable—even with `--version`—the
rig verifies the independently handed-off checksum-manifest hash and canonical
format, binds the exact archive checksum, rejects unsafe archive entries,
extracts into fresh scratch storage, and proves both supplied executable byte
hashes equal their archived counterparts. Only then does package inspection
with `lipo`, `codesign`, and `plutil` run; those tools do not execute the
candidate binaries.

After compiling the exact tagged native SystemProbe and confirming its
preexisting Screen Recording, Accessibility, foreground/AX-focus, cursor/HID,
and active-Space oracles, both evidence lanes wait for one 30-second sampled
quiet-seat epoch before invoking either candidate even with `--version`. The
gate samples every 500 ms and requires at least 60 stable
native transitions. Any pointer counter or cursor activity, foreground or AX
focus change, WindowServer front-row change, or active-Space change discards
the entire epoch and starts a new one under the original immutable 30-minute
deadline. An unavailable or unhealthy oracle fails immediately. The gate runs
before the fixture, server, or helper starts and retains no raw pointer data.

The standalone `--prepare-package` mode performs only manifest binding and the
bounded PAX-free ustar extraction. It creates the requested package directory
itself with owner-private permissions, refuses an existing destination, removes
its own partial output on failure, and never inspects or executes candidate
bytes as programs.

## What a passing run proves

- Before either candidate executable, the fixture, server, or helper starts,
  each lane completed its independent 30-second/60-transition native
  quiet-seat epoch. This reduces ambient pre-run contamination; it does not
  reserve the shared login seat, so every existing per-action and whole-run
  non-interruption boundary remains mandatory and fail-closed.
- The server and helper report exactly v0.12.33, are universal
  `arm64`/`x86_64` binaries, and pass strict code-signature checks.
- The supplied executables are byte-for-byte identical to the copies inside
  `local-browser-bridge-v0.12.33-macos-universal.tar.gz`. The archive must be
  bound by a canonical `SHA256SUMS.txt` containing exactly the four v0.12.33
  release assets, and that manifest must match a mandatory SHA-256 supplied
  out of band; a locally rebuilt archive plus a substituted manifest is
  refused. No supplied server or helper code executes before all of those
  checks and the safe extraction/byte-equality proof have succeeded.
- Existing Screen Recording and Accessibility permission are present. The rig
  never requests or changes either permission.
- The exact primary fixture window and a distinct sibling window are both
  enumerated through the authenticated server API with the same PID and their
  independently reported native window IDs. Primary selection remains exact;
  sibling count is never treated as receiver authority.
- Accessibility `setValue` is confirmed by read-back. Generic Accessibility
  invocation remains conservatively `Partial`; the fixture's own counter is
  recorded as separate target-side proof.
- One persistent ScreenCaptureKit `SCStream` keeps the same share authority
  through cadence sampling, a 900 ms background pixel action, and a controlled
  exact-target resize. During the long action an independent probe must observe
  target `AXMainWindow == AXFocusedWindow == primary` with target
  `AXFrontmost=true`, while the user's NSWorkspace PID, raw WindowServer
  PSN/PID, and exact AX main/focused window remain unchanged. Resize evidence waits past the first geometry transition
  for a later acknowledged source frame whose captured-image aspect ratio and
  saved PNG dimensions match the resized exact window.
- After that settled resize frame and screenshot, the runner sends another real
  `computer.click` through the product API. The result must bind to the exact
  resized frame, original share ID, and post-resize source sequence. Only the
  fixture click counter may change functional state; its bounded mouse-move
  delivery counter may also advance as instrumentation. Semantic, focus, and
  resize counters plus independent foreground-process, front-window, and Space
  identities must stay unchanged. Helper preservation is proved separately by
  exact-target delivery provenance and the shared-pointer boundary state.
- The fixture then makes its own background text field first responder without
  becoming the foreground app. A short, bounded native `computer.typeText`
  action must append to that exact field, report `Unverifiable`, and satisfy
  both product and independent non-interruption invariants. Exact fixture
  read-back supplies separate evidence. While the persistent share remains
  active, the runner then waits for a later server-published streamed frame from
  that exact share epoch, proves its top-level `shareId` and `sourceSequence`
  against the sampled transport record, and uses only that frame to authorize
  confirmed Accessibility `setValue` restoration. It never substitutes an
  unbound one-shot observation during the live share and never retries a stale
  mutation. Before and after delivery, both target `AXMainWindow` and
  `AXFocusedWindow` must equal the original sibling.
- Capture metadata reports `macos-screencapturekit-scstream`,
  `nativeStream: true`, the no-suppression system-indicator policy, and
  `programmatic-exact-window` selection.
- Native `sourceSequence` and transport `sequence` advance independently,
  source and transport drop counters remain monotonic, and every later
  transport frame proves acknowledgement of the previous emitted frame.
- The deliberately long action makes the capacity-one native source replace
  frames while input owns the serialized controller. This proves the source
  stays live; transport drops may correctly remain zero when acknowledgements
  keep pace.
- Foreground process, front window, and active Space remain unchanged, both in
  each product action record and in an independent probe. Cursor-position
  equality and HID-system activity are retained only as bounded diagnostics;
  HID-system activity is never called physical-device provenance.
  The probe now sandwiches a read-only foreground `AXFocusedWindow` and
  `AXFrontmost` read between foreground-PID samples; WindowServer ordering is
  retained as a secondary equality check, not trusted as the receiver oracle.
- Before representative primary pixel and native-text actions, an independent
  read-only Accessibility probe confirms that the target app remembers its
  secondary window as `AXFocusedWindow` while a different user app remains
  frontmost. The actions must mutate only the primary fixture counters/field,
  leave sibling text and click counters at zero, and restore the same sibling
  as the target app's focused window before returning. The user's sandwiched
  foreground PID/AX window/`AXFrontmost`, WindowServer front row, cursor, and
  Space must all remain unchanged.
- A live wrong-sibling-at-dispatch negative is explicitly **unproven**. After
  helper preparation, forcing the sibling would require a timing race or a
  production-only hook; making the fixture foreground would interrupt the user.
  The rig uses none of those techniques. Exact receiver mismatch refusal stays
  covered by production unit/source contracts until a deterministic,
  non-activating mechanism exists.
- A real two-second `computer.move` starts under one fresh `callId`. The runner
  waits until the fixture's bounded `mouseMoved` counter proves a newly
  delivered target-routed native event; an exact duplicate must then report
  HTTP 409 `CALL_IN_PROGRESS`. The wait has a failure timeout, never a fixed
  sleep that assumes dispatch. Authenticated `/api/v1/command/cancel` must then
  return 202 for that ID; and the original must settle as HTTP 504
  `COMMAND_OUTCOME_UNKNOWN` with the non-retriable
  `outcome_unknown`/`reobserve` contract. A second cancel is refused, an exact
  retry replays the cached 504 without redispatch, and changed params under the
  same ID are refused as `CALL_ID_REUSED`.
- Explicit cancellation revokes only the owning helper session's share,
  observation, screenshot, pointer, and frame authority. The old screenshot
  URL returns 404 and an idempotent share stop reports `not-active`. Before
  recovery, a click carrying the old frame is rejected by the server as
  `NO_COMPUTER_FRAME`, without helper relay or a recreated surface. An explicit
  one-shot `computer.observe` then recovers the same helper session with a new
  exact-window frame; retrying the pre-cancel frame is still refused as
  `COMPUTER_STALE_FRAME`. Neither refusal changes functional fixture state. The
  expected bounded move instrumentation is reported separately.
- After that cancellation recovery, the runner starts a second persistent
  exact-window share. The successful command response must keep the same helper
  session, name one new active share ID in both its result and public state, and
  replace the recovered one-shot frame with a distinct exact-window one-shot
  publication nested under that new share. The runner then waits for a native
  stream frame whose frame ID differs from both one-shot frames, whose source
  sequence is positive, and whose share ID is exactly the newly authorized ID.
  This is the live regression for the v0.12.2 post-cancellation fresh-share
  refusal; a one-shot observation alone is not accepted as proof.
  While that share is active, the runner sends `SIGTERM` only to its spawned
  fixture process and waits for process exit. The helper must no
  longer enumerate that exact `(process, window)` pair and must emit a terminal
  error for the same share ID. The server must mark the share stopped with
  `capture-error`, clear observation and screenshot authority, return 404 for
  the retired screenshot URL, and remain clear beyond three configured frame
  periods. A click carrying the retired frame must fail at the server as
  `NO_COMPUTER_FRAME`, and an explicit observe of the closed native window must
  fail as `COMPUTER_NO_WINDOW`. An idempotent stop then confirms helper-side
  cleanup without replacing or respawning the helper.
- Helper exit then clears its server session, and server exit closes the
  selected loopback listener. The fixture is already gone because exact-target
  close is now part of the live evidence sequence, not deferred to generic
  teardown.

`systemIndicator: true` is policy evidence that the helper did not suppress
ScreenCaptureKit's indication. An exact-window screenshot cannot prove that a
particular macOS banner was visible. Likewise, this is target-routed background
input in the active login session, not a VM, sandbox, virtual display, separate
login session, or independent input seat.

## Quiet and exact-app-share lanes

Every product action must still prove its exact-target route, dispatch attempt,
target postcondition, helper cursor-preservation claim, and an independently
corroborated shared-seat boundary. Cursor equality and cumulative HID
pointer/keyboard counters are bounded diagnostics; they never identify a person
or physical device, establish atomic app-share-provider identity, or constitute
a continuous monitor. They also cannot rule out transient programmatic changes
or transient focus/window manipulation between endpoint samples.

Both fresh release lanes begin with the same 30-second sampled native quiet
gate with at least 60 stable 500 ms transitions. The default `quiet` lane then
requires every product and independent boundary to stay quiet. The second lane keeps the compatibility name
`deliberate-concurrency`, but its mandatory action is now a separately
authorized exact-app-share click—not shared-pointer movement. Both lanes must
finish with `pointerEvidence.concurrentSharedSeatActivityObserved: false`.

The runner builds and signs a scratch `LBB App Share Handoff.app` with bundle ID
`dev.flrngel.local-browser-bridge.acceptance.app-share`. Its one stable
nonactivating window is titled `LBB macOS Acceptance App Share`; its single
button has accessibility identifier `lbb-app-share-start`. SystemProbe binds
the exact bundle, one window, exact button title and enabled state in `READY`,
then the disabled button in `ARMED`, `ACTION`, and `COMPLETE`. It retains only
bounded booleans, not raw unrelated application identities.

The runner publishes a canonical create-once request, then enables
`START APP-SHARE CHECK`. Through the separately authorized exact-app share,
press that button exactly once and stop all UI use. Do not use the shared
desktop and do not retry. Before accepting the action, the app itself opens the
exact request marker with `O_NOFOLLOW`, verifies its stable ordinary-file
identity and SHA-256, atomically writes a start receipt, and disables the
button. SystemProbe must then record matching foreground app, focused window,
active Space, cursor, and cumulative HID pointer/keyboard counter endpoints.

Only after that start receipt and quiet independent boundary does the runner
open a ten-second `ACTION` interval. It reads the current public share state as
a non-mutating sequence barrier, then waits at most the unreserved portion of
that interval for a strictly newer server-published frame from the same share,
exact target, and settled window/image geometry. Its helper-published
`capturedAt` plus native monotonic `captureAgeMs` must produce a combined
client-side age estimate of at most one second. Five
seconds remain reserved for the action and three seconds for completion. The
runner derives coordinates and product authority only from that bounded-age
post-receipt publication; it does not call a
one-shot `computer.observe`, pause capture, lengthen the three-second frame
lease, or reuse the pre-handoff frame. It then sends the real two-second
target-routed `computer.click`. The exact fixture postcondition must advance
while both the product and independent shared-seat boundaries remain quiet.
The runner then requests `COMPLETE`; the app emits a completion receipt bound
to the request hash, start-receipt hash, and product timestamps. Its
`handoffStateSequenceBound: true` field binds that ordered marker state; it does
not claim uninterrupted observation of the handoff window. The final SystemProbe
must still observe the exact app/window/disabled button and a quiet shared seat.
Success remains impossible until the request, start, and completion files all
exist and the runner closes the scratch app.

The three allowlisted files are:

- `operator/macos-app-share-concurrency-handoff-request.json`;
- `operator/macos-app-share-concurrency-handoff-start.json`; and
- `operator/macos-app-share-concurrency-handoff-complete.json`.

They use marker schema 2. The request binds version, request/expiry times,
runner/app processes, expected bundle/window/button surface, and explicitly
states that it is not product authority. The app-owned receipts claim only the
button and app state they can observe. They do not claim a human, physical
device, shared-seat isolation, or cryptographic identity for the external
Computer Use provider. The schema-8 result records only the bound receipt chain
plus sampled native SystemProbe boundaries and sets
`orchestrationNotProductControl: true`. It also records whether exact same-share
action authority was refreshed after the receipt and remained fresh at dispatch;
both fields are false in the quiet lane and true in a passing deliberate lane.
The request/start/complete chain is
orchestration evidence, not notification-only and not product authority.

`PhysicalPointerHandoff.swift` and the old pointer watcher remain optional
adversarial tools. Their output is structurally excluded from the release
allowlist and cannot replace either mandatory lane.

## Expected screenshots

1. `computer-01-exact-window-observe.png`
2. `computer-02-semantic-set-value.png`
3. `computer-03-semantic-invoke.png`
4. `computer-04-persistent-scstream-start.png`
5. `computer-05-live-share-pixel-action.png`
6. `computer-06-persistent-share-resize.png`

Every image is fetched from `/api/computer/screenshot` only after the public
observation is bound to the fixture's exact PID, native window ID, and title.
The fixture also renders `evidence-lane=quiet` or
`evidence-lane=deliberate-concurrency` inside the captured target and publishes
the same value in its state file. The rig rejects a state mismatch, and the
finalizer requires all twelve lane screenshots to have distinct file SHA-256
digests and distinct canonical decoded-RGBA pixel SHA-256 digests. Changing
only PNG compression therefore cannot disguise replayed pixels across either
lane, and any unexpected metadata or ancillary chunk is rejected.
The machine-readable screenshot manifest records each PNG's dimensions,
SHA-256, frame ID, source sequence, and transport sequence. The runner also
requires all six manifest entries to name the primary window ID and never the
sibling ID. The sibling exists to test routing and is deliberately excluded
from retained screenshots.

Screenshot 06 is intentionally captured before the post-resize click, native
text proof, and cancellation flow. It is the visual proof that the same share
settled on `last=resize size=820x520`; the later action, read-back, replay, and
target-close fail-closed assertions are stronger machine-state/API contracts
and do not require a seventh screenshot. In particular, the close proof records
only bounded process/share/frame identities and API outcomes; it never retains
another target image after native text delivery begins.

## Run

Use a controlled interactive macOS session where the packaged helper already
has Screen Recording and Accessibility permission. The harness never requests
or changes either permission, and it never closes an unrelated application or
window.

The two lanes are sequential and non-mergeable:

1. Run one fresh `quiet` lane synchronously. Its native gate first requires a
   30-second sampled quiet epoch with at least 60 stable 500 ms transitions. Do
   not interact with the pointer, foreground application, windows, or Spaces
   during the gate or product run.
2. Only if the quiet lane passes, run one fresh `deliberate-concurrency` lane.
   Leave the shared seat quiet. When the watcher emits `ACTION REQUIRED`, use a
   separately authorized exact-app share for the named bundle and press
   `START APP-SHARE CHECK` exactly once. Do not use the shared desktop or retry,
   and stop all UI use after the watcher confirms the start receipt.
3. Review all six screenshots from each lane. Only after both result files pass
   and all twelve images are accepted may the finalizer create the aggregate.
   Complete that review and finalization within 30 minutes after the deliberate
   lane finishes; both the local finalizer and publication verifier enforce the
   same interval.

If either lane fails or is interrupted, stop. Preserve that attempt as negative
evidence, withdraw the frozen candidate, and do not retry, resume, combine it
with another lane, or execute Windows/Chrome acceptance. The runner refuses to
overwrite any result, log, operator marker, or screenshot.

The same terminal rule applies to a watcher/runner nonzero exit, mechanical
inventory or hash failure, visual rejection, post-review manifest drift,
finalizer failure, or aggregate mismatch: preserve sanitized negative evidence,
withdraw that exact run/attempt/artifact, and do not retry, resume, merge,
relabel, run Windows/Chrome, or publish. A workflow rerun is a new attempt with
a new artifact, evidence set, and receipt. Only a quiet-epoch reset and a
pre-dispatch `MOVE` re-arm inside their original immutable deadlines are
in-run state transitions rather than retries.

Run every command below from a fresh clean detached checkout of the exact
candidate source commit on `refs/heads/main`. The source checkout and every candidate/evidence directory
must be ordinary, owner-private, non-symlink paths outside the repository.
Supply the run and artifact identifiers from the candidate workflow; a branch
name, local directory, manifest, or previously downloaded artifact is not
authority.

The checked-in candidate binder validates the independent API binding, raw
artifact ZIP size and SHA-256, exact five-file inventory, canonical LF checksum
manifest, all payload hashes, all five GitHub attestations, both exact-attempt
attestation URI fields, GitHub-hosted runner identity, clean detached source,
and its own exact source blob. It never executes
candidate bytes. Extraction and candidate execution are permitted only below
this line and only after the binder passes.

The commands are deliberately split into three phases. Keep the same private
shell open, but never concatenate the phases into one script. Phase 1 stops for
an exact-result and six-image quiet-lane review; Phase 2 cannot start until the
reviewer enters that result's digest. Phase 2 stops again for the deliberate
lane's exact-result and six-image review; Phase 3 requires both entered digests
before it can finalize.

### Phase 1: bind the candidate, run quiet, then stop for review

```bash
set -euo pipefail
umask 077

SOURCE_ROOT="$(git rev-parse --show-toplevel)"
SOURCE_ROOT="$(cd "$SOURCE_ROOT" && pwd -P)"
cd "$SOURCE_ROOT"
[[ "$(pwd -P)" == "$SOURCE_ROOT" ]]

: "${VERSION:=0.12.33}"
: "${WORKFLOW_RUN_ID:?set the exact candidate-workflow run ID}"
: "${WORKFLOW_RUN_ATTEMPT:?set the exact run attempt}"
: "${RELEASE_CANDIDATE_ARTIFACT_ID:?set the exact artifact ID}"
: "${EXPECTED_SOURCE_SHA:?set the exact main source commit}"
: "${PRIVATE_PARENT:?set an existing owner-private ordinary directory}"

[[ "$VERSION" == "0.12.33" ]]
[[ "$WORKFLOW_RUN_ID" =~ ^[1-9][0-9]*$ ]]
[[ "$WORKFLOW_RUN_ATTEMPT" =~ ^[1-9][0-9]*$ ]]
[[ "$RELEASE_CANDIDATE_ARTIFACT_ID" =~ ^[1-9][0-9]*$ ]]
[[ "$EXPECTED_SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]]
[[ -d "$PRIVATE_PARENT" && ! -L "$PRIVATE_PARENT" ]]
PRIVATE_PARENT="$(cd "$PRIVATE_PARENT" && pwd -P)"
[[ "$(stat -f '%HT:%Lp:%u' "$PRIVATE_PARENT")" == "Directory:700:$(id -u)" ]]

RUN_NONCE="$(openssl rand -hex 16)"
CANDIDATE_ROOT="$PRIVATE_PARENT/candidate-$RUN_NONCE"
ATTEMPT_ROOT="$(mktemp -d "$PRIVATE_PARENT/lbb-v0.12.33-macos.XXXXXX")"
SCRATCH_PARENT="$(mktemp -d "$PRIVATE_PARENT/lbb-v0.12.33-scratch.XXXXXX")"
PACKAGE_ROOT="$PRIVATE_PARENT/lbb-v0.12.33-package-$RUN_NONCE"

bash scripts/fetch-verify-release-candidate.sh \
  "$VERSION" \
  "$WORKFLOW_RUN_ID" \
  "$WORKFLOW_RUN_ATTEMPT" \
  "$RELEASE_CANDIDATE_ARTIFACT_ID" \
  "$EXPECTED_SOURCE_SHA" \
  "$CANDIDATE_ROOT"

CANDIDATE_BINDING="$CANDIDATE_ROOT/candidate-binding.json"
RELEASE_CANDIDATE_DIR="$CANDIDATE_ROOT/payload"
EXPECTED_SHA256SUMS_SHA256="$(
  jq -er '.checksumManifestSha256' "$CANDIDATE_BINDING"
)"
RELEASE_CANDIDATE_ARTIFACT_ZIP_SHA256="$(
  jq -er '.artifactZipSha256' "$CANDIDATE_BINDING"
)"
[[ "$(jq -er '.passed' "$CANDIDATE_BINDING")" == "true" ]]

ARCHIVE="$RELEASE_CANDIDATE_DIR/local-browser-bridge-v0.12.33-macos-universal.tar.gz"
MANIFEST="$RELEASE_CANDIDATE_DIR/SHA256SUMS.txt"
[[ ! -e "$PACKAGE_ROOT" && ! -L "$PACKAGE_ROOT" ]]
node evidence/v0.12.33/computer/helper-evidence-rig.mjs \
  --prepare-package \
  "$ARCHIVE" "$MANIFEST" "$EXPECTED_SHA256SUMS_SHA256" \
  "$PACKAGE_ROOT"

SERVER="$PACKAGE_ROOT/local-browser-bridge"
HELPER="$PACKAGE_ROOT/Local Computer Helper.app/Contents/MacOS/local-computer-helper"
QUIET_DIR="$ATTEMPT_ROOT/quiet"
DELIBERATE_DIR="$ATTEMPT_ROOT/deliberate-concurrency"

node evidence/v0.12.33/computer/helper-evidence-rig.mjs \
  "$SERVER" "$HELPER" "$QUIET_DIR" "$SCRATCH_PARENT" \
  "$ARCHIVE" "$MANIFEST" "$EXPECTED_SHA256SUMS_SHA256" \
  "$EXPECTED_SOURCE_SHA" \
  "$WORKFLOW_RUN_ID" "$WORKFLOW_RUN_ATTEMPT" \
  "$RELEASE_CANDIDATE_ARTIFACT_ID" \
  "$RELEASE_CANDIDATE_ARTIFACT_ZIP_SHA256" \
  quiet

QUIET_RESULT="$QUIET_DIR/helper-results.json"
jq -e '
  .schemaVersion == 8 and
  .productVersion == "0.12.33" and
  .status == "passed-release-candidate" and
  .assertions.failed == 0 and
  .assertions.total > 0 and
  .assertions.passed + .assertions.failed == .assertions.total and
  .fixture.evidenceLane == "quiet" and
  .quietSeatStabilization.completed == true and
  .quietSeatStabilization.completedBeforeCandidateExecution == true and
  .quietSeatStabilization.monitoringUnknown == false and
  .pointerEvidence.quietObserved == true and
  .pointerEvidence.concurrentSharedSeatActivityObserved == false and
  .pointerEvidence.unknownObserved == false and
  .appShareHandoff.requested == false and
  .appShareHandoff.requestPublicationAcknowledged == false and
  .appShareHandoff.startReceiptAcknowledged == false and
  .appShareHandoff.completePublicationAcknowledged == false and
  .appShareHandoff.promptClosed == false and
  .appShareHandoff.exactAppBundleObserved == false and
  .appShareHandoff.exactWindowObserved == false and
  .appShareHandoff.exactButtonObserved == false and
  .appShareHandoff.buttonDisabledAfterAction == false and
  .appShareHandoff.acceptanceButtonActionObserved == false and
  .appShareHandoff.appShareSurfaceObservedAtProductBoundaries == false and
  .appShareHandoff.sharedHidInputObserved == null and
  .appShareHandoff.sampledSharedContextUnchanged == false and
  .appShareHandoff.authorityRefreshedAfterReceipt == false and
  .appShareHandoff.authorityFreshAtDispatch == false and
  .appShareHandoff.actionDispatched == false and
  .appShareHandoff.targetPostconditionObserved == false and
  .appShareHandoff.productBoundaryQuiet == false and
  .appShareHandoff.independentBoundaryQuiet == false and
  .appShareHandoff.physicalHumanProvenanceClaimed == false and
  .appShareHandoff.cryptographicToolIdentityClaimed == false and
  .appShareHandoff.orchestrationNotProductControl == true and
  .appShareHandoff.markerNotificationOnly == false and
  .appShareHandoff.markerAcceptedAsProductAuthority == false and
  .appShareHandoff.rawAppIdentityRetainedInResult == false and
  .appShareHandoff.rawPointerDataRetained == false and
  ([.screenshots[].file] == [
    "computer-01-exact-window-observe.png",
    "computer-02-semantic-set-value.png",
    "computer-03-semantic-invoke.png",
    "computer-04-persistent-scstream-start.png",
    "computer-05-live-share-pixel-action.png",
    "computer-06-persistent-share-resize.png"
  ])
' "$QUIET_RESULT" >/dev/null
QUIET_EXPECTED_INVENTORY="$(printf '%s\n' \
  './computer-01-exact-window-observe.png' \
  './computer-02-semantic-set-value.png' \
  './computer-03-semantic-invoke.png' \
  './computer-04-persistent-scstream-start.png' \
  './computer-05-live-share-pixel-action.png' \
  './computer-06-persistent-share-resize.png' \
  './helper-results.json' \
  './helper-rig.log')"
QUIET_ACTUAL_INVENTORY="$(
  cd "$QUIET_DIR" && LC_ALL=C find . -mindepth 1 -print | LC_ALL=C sort
)"
[[ "$QUIET_ACTUAL_INVENTORY" == "$QUIET_EXPECTED_INVENTORY" ]]
(cd "$QUIET_DIR" && \
  jq -er '.screenshots[] | "\(.sha256)  \(.file)"' helper-results.json | \
  shasum -a 256 -c -)
QUIET_REVIEW_MANIFEST="$ATTEMPT_ROOT/quiet-review.sha256"
[[ ! -e "$QUIET_REVIEW_MANIFEST" && ! -L "$QUIET_REVIEW_MANIFEST" ]]
(cd "$QUIET_DIR" && shasum -a 256 \
  computer-01-exact-window-observe.png \
  computer-02-semantic-set-value.png \
  computer-03-semantic-invoke.png \
  computer-04-persistent-scstream-start.png \
  computer-05-live-share-pixel-action.png \
  computer-06-persistent-share-resize.png \
  helper-results.json \
  helper-rig.log) > "$QUIET_REVIEW_MANIFEST"
chmod 600 "$QUIET_REVIEW_MANIFEST"
(cd "$QUIET_DIR" && shasum -a 256 -c "$QUIET_REVIEW_MANIFEST")
QUIET_RESULT_SHA256="$(shasum -a 256 "$QUIET_RESULT" | awk '{ print $1 }')"
printf 'Quiet result SHA-256: %s\n' "$QUIET_RESULT_SHA256"
```

Open and inspect the six exact quiet-lane PNGs. Every image must show only the
primary fixture with `evidence-lane=quiet`, never its sibling or unrelated
pixels. In order, verify: ready with empty/zero counters; the v0.12.33 semantic
value; semantic action 1/complete; persistent stream; click count 1 with
`last=click`; and settled `last=resize` at 820x520. Stop permanently if any
image is wrong, sensitive, stale, or cropped. Only after accepting all six,
enter the printed digest in the same shell:

```bash
printf 'Enter the accepted quiet result SHA-256: '
IFS= read -r QUIET_REVIEWED_RESULT_SHA256
export QUIET_REVIEWED_RESULT_SHA256
(cd "$QUIET_DIR" && shasum -a 256 -c "$QUIET_REVIEW_MANIFEST")
[[ "$QUIET_REVIEWED_RESULT_SHA256" == "$QUIET_RESULT_SHA256" ]]
```

### Phase 2: require quiet review, run deliberate, then stop for review

```bash
: "${QUIET_REVIEWED_RESULT_SHA256:?complete Phase 1 visual review first}"
(cd "$QUIET_DIR" && shasum -a 256 -c "$QUIET_REVIEW_MANIFEST")
[[ "$QUIET_REVIEWED_RESULT_SHA256" == \
  "$(shasum -a 256 "$QUIET_DIR/helper-results.json" | awk '{ print $1 }')" ]]

node evidence/v0.12.33/computer/helper-evidence-rig.mjs \
  "$SERVER" "$HELPER" "$DELIBERATE_DIR" "$SCRATCH_PARENT" \
  "$ARCHIVE" "$MANIFEST" "$EXPECTED_SHA256SUMS_SHA256" \
  "$EXPECTED_SOURCE_SHA" \
  "$WORKFLOW_RUN_ID" "$WORKFLOW_RUN_ATTEMPT" \
  "$RELEASE_CANDIDATE_ARTIFACT_ID" \
  "$RELEASE_CANDIDATE_ARTIFACT_ZIP_SHA256" \
  deliberate-concurrency &
RUNNER_PID=$!

# This watcher relays create-once notifications; it grants no authority.
WATCHER_STATUS=0
node scripts/wait-macos-app-share-concurrency-handoff.mjs \
  --mode watch \
  --evidence-dir "$DELIBERATE_DIR" \
  --runner-pid "$RUNNER_PID" || WATCHER_STATUS=$?
RUNNER_STATUS=0
wait "$RUNNER_PID" || RUNNER_STATUS=$?
[[ "$WATCHER_STATUS" == "0" && "$RUNNER_STATUS" == "0" ]]

DELIBERATE_RESULT="$DELIBERATE_DIR/helper-results.json"
jq -e '
  .schemaVersion == 8 and
  .productVersion == "0.12.33" and
  .status == "passed-release-candidate" and
  .assertions.failed == 0 and
  .assertions.total > 0 and
  .assertions.passed + .assertions.failed == .assertions.total and
  .fixture.evidenceLane == "deliberate-concurrency" and
  .quietSeatStabilization.completed == true and
  .quietSeatStabilization.completedBeforeCandidateExecution == true and
  .quietSeatStabilization.monitoringUnknown == false and
  .pointerEvidence.quietObserved == true and
  .pointerEvidence.concurrentSharedSeatActivityObserved == false and
  .pointerEvidence.unknownObserved == false and
  .appShareHandoff.requested == true and
  .appShareHandoff.requestPublicationAcknowledged == true and
  .appShareHandoff.startReceiptAcknowledged == true and
  .appShareHandoff.completePublicationAcknowledged == true and
  .appShareHandoff.promptClosed == true and
  .appShareHandoff.exactAppBundleObserved == true and
  .appShareHandoff.exactWindowObserved == true and
  .appShareHandoff.exactButtonObserved == true and
  .appShareHandoff.buttonDisabledAfterAction == true and
  .appShareHandoff.acceptanceButtonActionObserved == true and
  .appShareHandoff.appShareSurfaceObservedAtProductBoundaries == true and
  .appShareHandoff.sharedHidInputObserved == false and
  .appShareHandoff.sampledSharedContextUnchanged == true and
  .appShareHandoff.authorityRefreshedAfterReceipt == true and
  .appShareHandoff.authorityFreshAtDispatch == true and
  .appShareHandoff.actionDispatched == true and
  .appShareHandoff.targetPostconditionObserved == true and
  .appShareHandoff.productBoundaryQuiet == true and
  .appShareHandoff.independentBoundaryQuiet == true and
  .appShareHandoff.physicalHumanProvenanceClaimed == false and
  .appShareHandoff.cryptographicToolIdentityClaimed == false and
  .appShareHandoff.orchestrationNotProductControl == true and
  .appShareHandoff.markerNotificationOnly == false and
  .appShareHandoff.markerAcceptedAsProductAuthority == false and
  .appShareHandoff.rawAppIdentityRetainedInResult == false and
  .appShareHandoff.rawPointerDataRetained == false and
  ([.screenshots[].file] == [
    "computer-01-exact-window-observe.png",
    "computer-02-semantic-set-value.png",
    "computer-03-semantic-invoke.png",
    "computer-04-persistent-scstream-start.png",
    "computer-05-live-share-pixel-action.png",
    "computer-06-persistent-share-resize.png"
  ])
' "$DELIBERATE_RESULT" >/dev/null
DELIBERATE_EXPECTED_INVENTORY="$(printf '%s\n' \
  './computer-01-exact-window-observe.png' \
  './computer-02-semantic-set-value.png' \
  './computer-03-semantic-invoke.png' \
  './computer-04-persistent-scstream-start.png' \
  './computer-05-live-share-pixel-action.png' \
  './computer-06-persistent-share-resize.png' \
  './helper-results.json' \
  './helper-rig.log' \
  './operator' \
  './operator/macos-app-share-concurrency-handoff-complete.json' \
  './operator/macos-app-share-concurrency-handoff-request.json' \
  './operator/macos-app-share-concurrency-handoff-start.json')"
DELIBERATE_ACTUAL_INVENTORY="$(
  cd "$DELIBERATE_DIR" && LC_ALL=C find . -mindepth 1 -print | LC_ALL=C sort
)"
[[ "$DELIBERATE_ACTUAL_INVENTORY" == "$DELIBERATE_EXPECTED_INVENTORY" ]]
(cd "$DELIBERATE_DIR" && \
  jq -er '.screenshots[] | "\(.sha256)  \(.file)"' helper-results.json | \
  shasum -a 256 -c -)
DELIBERATE_REVIEW_MANIFEST="$ATTEMPT_ROOT/deliberate-review.sha256"
[[ ! -e "$DELIBERATE_REVIEW_MANIFEST" && ! -L "$DELIBERATE_REVIEW_MANIFEST" ]]
(cd "$DELIBERATE_DIR" && shasum -a 256 \
  computer-01-exact-window-observe.png \
  computer-02-semantic-set-value.png \
  computer-03-semantic-invoke.png \
  computer-04-persistent-scstream-start.png \
  computer-05-live-share-pixel-action.png \
  computer-06-persistent-share-resize.png \
  helper-results.json \
  helper-rig.log \
  operator/macos-app-share-concurrency-handoff-complete.json \
  operator/macos-app-share-concurrency-handoff-request.json \
  operator/macos-app-share-concurrency-handoff-start.json) \
  > "$DELIBERATE_REVIEW_MANIFEST"
chmod 600 "$DELIBERATE_REVIEW_MANIFEST"
(cd "$DELIBERATE_DIR" && shasum -a 256 -c "$DELIBERATE_REVIEW_MANIFEST")
DELIBERATE_RESULT_SHA256="$(shasum -a 256 "$DELIBERATE_RESULT" | awk '{ print $1 }')"
printf 'Deliberate result SHA-256: %s\n' "$DELIBERATE_RESULT_SHA256"
```

Open and inspect the six exact deliberate-lane PNGs. Every image must show only
the primary fixture with `evidence-lane=deliberate-concurrency`, never its
sibling or unrelated pixels. In order, verify: ready with empty/zero counters;
the v0.12.33 semantic value; semantic action 1/complete; persistent stream;
click count 1 with `last=click`; and settled `last=resize` at 820x520. Stop
permanently if any image is wrong, sensitive, stale, or cropped. Only after
accepting all six, enter the printed digest in the same shell. This review and
Phase 3 must finish within the finalizer's 30-minute bound from the deliberate
result's `capturedAt` value.

```bash
printf 'Enter the accepted deliberate result SHA-256: '
IFS= read -r DELIBERATE_REVIEWED_RESULT_SHA256
export DELIBERATE_REVIEWED_RESULT_SHA256
(cd "$DELIBERATE_DIR" && shasum -a 256 -c "$DELIBERATE_REVIEW_MANIFEST")
[[ "$DELIBERATE_REVIEWED_RESULT_SHA256" == "$DELIBERATE_RESULT_SHA256" ]]
```

### Phase 3: require both reviews and finalize create-once

```bash
: "${QUIET_REVIEWED_RESULT_SHA256:?complete Phase 1 visual review first}"
: "${DELIBERATE_REVIEWED_RESULT_SHA256:?complete Phase 2 visual review first}"
(cd "$QUIET_DIR" && shasum -a 256 -c "$QUIET_REVIEW_MANIFEST")
(cd "$DELIBERATE_DIR" && shasum -a 256 -c "$DELIBERATE_REVIEW_MANIFEST")
[[ "$QUIET_REVIEWED_RESULT_SHA256" == \
  "$(shasum -a 256 "$QUIET_DIR/helper-results.json" | awk '{ print $1 }')" ]]
[[ "$DELIBERATE_REVIEWED_RESULT_SHA256" == \
  "$(shasum -a 256 "$DELIBERATE_DIR/helper-results.json" | awk '{ print $1 }')" ]]

QUIET_CANONICAL="$(cd "$QUIET_DIR" && pwd -P)"
DELIBERATE_CANONICAL="$(cd "$DELIBERATE_DIR" && pwd -P)"
AGGREGATE_DIR="$(mktemp -d "$PRIVATE_PARENT/lbb-v0.12.33-aggregate.XXXXXX")"
AGGREGATE_CANONICAL="$(cd "$AGGREGATE_DIR" && pwd -P)"

node scripts/finalize-macos-acceptance.mjs \
  "$QUIET_CANONICAL" \
  "$DELIBERATE_CANONICAL" \
  "$AGGREGATE_CANONICAL"

MACOS_ACCEPTANCE="$AGGREGATE_CANONICAL/macos-acceptance.json"
[[ "$(cd "$AGGREGATE_CANONICAL" && \
  LC_ALL=C find . -mindepth 1 -print | LC_ALL=C sort)" == \
  './macos-acceptance.json' ]]
jq -e \
  --arg quiet_sha256 "$QUIET_REVIEWED_RESULT_SHA256" \
  --arg deliberate_sha256 "$DELIBERATE_REVIEWED_RESULT_SHA256" '
    .schemaVersion == 3 and
    .productVersion == "0.12.33" and
    .status == "passed-release-candidate" and
    .lanes.quiet.resultSha256 == $quiet_sha256 and
    .lanes.deliberateConcurrency.resultSha256 == $deliberate_sha256 and
    .aggregateChecks.laneDirectoriesDisjoint == true and
    .aggregateChecks.exactInventories == true and
    .aggregateChecks.resultsByteDistinct == true and
    .aggregateChecks.passingResultSchemaVersion == 8 and
    .aggregateChecks.inventoryFileCount == 19 and
    .aggregateChecks.screenshotCount == 12 and
    .aggregateChecks.screenshotHashesMatched == true and
    .aggregateChecks.screenshotPixelHashesMatched == true and
    .aggregateChecks.operatorMarkerHashesMatched == true
  ' "$MACOS_ACCEPTANCE" >/dev/null
(cd "$QUIET_DIR" && shasum -a 256 -c "$QUIET_REVIEW_MANIFEST")
(cd "$DELIBERATE_DIR" && shasum -a 256 -c "$DELIBERATE_REVIEW_MANIFEST")
MACOS_ACCEPTANCE_SHA256="$(
  shasum -a 256 "$MACOS_ACCEPTANCE" | awk '{ print $1 }'
)"
printf 'macOS aggregate SHA-256: %s\n' "$MACOS_ACCEPTANCE_SHA256"
```

On macOS, `/tmp` commonly resolves to `/private/tmp`; pass only canonical
`pwd -P` spellings to the finalizer. It rejects linked, non-private,
noncanonical, overlapping, stale, extra-file, mismatched-candidate, same-result,
wrong-lane, unreviewable-screenshot, and malformed-marker inputs. It publishes
`macos-acceptance.json` create-once and never modifies either lane. It also
independently proves that each lane has a bounded forward timeline, all three
deliberate operator markers fall inside that lane, and the deliberate lane
started strictly after the quiet lane finished successfully.

The successful aggregate binds both distinct lane result digests, all twelve
screenshot file and decoded-pixel digests plus dimensions, all three deliberate
operator-marker digests, the
exact source/workflow attempt/artifact/raw-ZIP/manifest/package identity,
and the exact source harness blobs. Copy only its allowlisted sanitized evidence into
the single-parent release-evidence commit. Keep candidate downloads, extracted
packages, scratch data, absolute paths, credentials, and raw identifiers out of
the repository.

The retained macOS subtree is exactly 20 files: one aggregate, eight quiet-lane
files, and eleven deliberate-lane files. Store it at
`evidence/v0.12.33/release/run-RUN_ID-attempt-ATTEMPT/macos/` on
`evidence/v0.12.33-release-run-RUN_ID-attempt-ATTEMPT`; its single commit parent
must be the candidate source and every retained file must be an ordinary `100644`
blob. The aggregate's `inventoryFileCount: 19` counts both lane inventories and
intentionally excludes `macos-acceptance.json` itself. Never retain the
candidate root or binding, attestations, archive, extracted package, scratch
tree, review manifests, absolute paths, credentials, tokens, raw fixture state,
or unrelated screen content.

The runner generates a random bearer token in memory, passes it only in the
server and helper process environments, deletes it from the temporary launch
objects immediately after each spawn, refuses to persist it, and removes the
entire scratch directory in cleanup. A failure still writes a sanitized
machine-readable negative result instead of silently discarding partial
evidence.

The generated native-text suffix and move coordinates are never retained in
results, logs, or screenshots; evidence retains only bounded counts, booleans,
and the fresh non-secret `callId` needed to audit replay identity. The runner
enforces payload exclusion again when serializing both result and log. The text
exists temporarily in process memory and the scratch fixture-state file so the
fixture can provide exact read-back; the scratch directory is removed during
cleanup. The bearer token is also excluded. The cancellation stage uses the
product's normal authenticated endpoint and a naturally long real action—never
a shortened server deadline, test-only dispatch delay, fault hook, or connector
mock.

The sibling field's content is never serialized. The scratch fixture state and
retained result record only its bounded UTF-16 length and click/focus counters;
a passing run requires zero sibling text and pointer mutation. The external AX
probe's raw foreground IDs are reduced to equality booleans in evidence output.
On failure, the same probe receives the exact fixture PID to evaluate sibling
focus, but diagnostics retain only availability, expected-focus, observed-focus,
and equality booleans—never the target PID or either native window ID.

macOS can report the exact-target loss first through window enumeration
(`COMPUTER_NO_WINDOW`) or through the ScreenCaptureKit stop callback
(`COMPUTER_CAPTURE_FAILED`). The rig accepts only those two terminal codes, and
only after it has requested and observed exit of its exact spawned fixture. In
both cases the event must name the second share ID and produce the identical
server authority-clear, stale-frame refusal, and cleanup contract.

After the run, inspect every screenshot, confirm `assertions.failed` is zero,
and compare the recorded archive SHA-256 with the published release asset
before changing this directory's status from candidate to released evidence.

## Withdrawn v0.12.10 exact-candidate result

The exact v0.12.10 candidate passed 69 assertions and retained six reviewed
fixture-only screenshots before the persistent handoff timed out after 300
seconds with no shared-pointer movement. The final post-resize action was not
dispatched. The candidate was not retried; Windows and Chrome acceptance were
never started, the protected publish job was canceled, and no v0.12.10 Release
was created:

- [`withdrawn-de59840-macos-deliberate-pointer-timeout`](../../v0.12.10/computer/attempts/withdrawn-de59840-macos-deliberate-pointer-timeout/README.md)

## Withdrawn v0.12.9 exact-candidate result

The exact v0.12.9 candidate passed its artifact, package, permission, topology,
window-discovery, and initial-observation gates, then failed closed when
`computer.setValue` observed a changed global hardware-cursor sample across the
semantic dispatch. The retained record cannot attribute that change to the
helper rather than external shared-seat activity, so it does not prove the
non-interruption invariant. The candidate was not retried; Windows and Chrome
acceptance were never started, the protected publish job was canceled, and no
v0.12.9 Release was created:

- [`withdrawn-db624da-macos-semantic-hardware-cursor-change`](../../v0.12.9/computer/attempts/withdrawn-db624da-macos-semantic-hardware-cursor-change/README.md)

## Historical v0.12.8 exact-candidate result

The exact v0.12.8 macOS archive passed all 187 assertions and produced six
reviewed screenshots. The same frozen candidate's Windows run passed
protocol-bound helper readiness and delivered one fresh foreground-arm request,
but no click reached the fixture and no received marker was created. It timed
out at `wait-foreground-arm` before the invariant baseline or any product
action. Stock-Chrome acceptance was not started, the protected publish job was
canceled, and no v0.12.8 Release was created. These records are diagnostic
history, not v0.12.33 evidence:

- [`v0.12.8 macOS exact-candidate pass`](../../v0.12.8/computer/README.md)
- [`withdrawn-532d603-windows-foreground-arm-timeout`](../../v0.12.8/computer/attempts/withdrawn-532d603-windows-foreground-arm-timeout/README.md)

## Historical v0.12.7 exact-candidate result

The exact v0.12.7 macOS archive passed all 187 assertions and produced six
reviewed screenshots. The same frozen candidate then reached Windows protocol
readiness and delivered one fresh foreground-arm request, but no human or
scripted click occurred. It timed out with zero mouse-down, mouse-up, and
acknowledgement counts before the invariant baseline or any product action.
Chrome acceptance was not started, the protected publish job was canceled, and
no v0.12.7 Release was created. These records are diagnostic history, not
v0.12.33 evidence:

- [`v0.12.7 macOS exact-candidate pass`](../../v0.12.7/computer/README.md)
- [`withdrawn-0749953-windows-foreground-arm-timeout`](../../v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout/README.md)

## Historical v0.12.6 exact-candidate result

The exact v0.12.6 macOS archive passed all 187 assertions and produced six
reviewed screenshots. The same frozen candidate then failed closed on Windows
after protocol-bound helper readiness and before observation, capture, sharing,
input, or any screenshot because its shown WinForms sentinel did not own the
global foreground. The publish job was canceled and no v0.12.6 Release was
created. These records are diagnostic history, not v0.12.33 evidence:

- [`v0.12.6 macOS exact-candidate pass`](../../v0.12.6/computer/README.md)
- [`withdrawn-397e4b6-windows-foreground-sentinel-timeout`](../../v0.12.6/computer/attempts/withdrawn-397e4b6-windows-foreground-sentinel-timeout/README.md)

## Historical v0.12.5 negative attempt

The exact v0.12.5 candidate run is retained as failed-candidate evidence, not
as a v0.12.33 result:

- [`withdrawn-badda8e-macos-native-text-restore-stale-frame`](../../v0.12.5/computer/attempts/withdrawn-badda8e-macos-native-text-restore-stale-frame/README.md)
  records 82 successful assertions followed by a fail-closed
  `COMPUTER_STALE_FRAME` refusal. Its live-share cleanup requested an unbound
  one-shot observation that a later stream frame correctly superseded before
  mutation dispatch. The same-epoch streamed restore gate in this harness is
  the regression boundary found by that run; production authority checks were
  not relaxed.

## Historical v0.12.2 negative attempt

The v0.12.2 candidate run is preserved byte-for-byte and is not v0.12.33
evidence:

- [`withdrawn-a52d761-post-cancel-fresh-share-refusal`](../../v0.12.2/computer/attempts/withdrawn-a52d761-post-cancel-fresh-share-refusal/README.md)
  proves that the v0.12.2 candidate reached cancellation teardown, one-shot
  recovery, and stale-frame refusal, then failed closed when the subsequent
  `computer.share.start` could not publish fresh share authority. That failure
  is the regression boundary exercised explicitly by this harness.

## Historical v0.12.1 negative attempts

The prior attempts remain byte-for-byte in the v0.12.1 evidence directory.
They are linked for diagnostic history only and are not v0.12.33 results:

- [`withdrawn-98ff6f0-macos-invariant-refusal`](../../v0.12.1/computer/attempts/withdrawn-98ff6f0-macos-invariant-refusal/README.md)
  preserves the first fail-closed run. It is diagnostic history, not release
  evidence and not a passing result.
- [`withdrawn-ffd9f8b-transitional-resize-frame`](../../v0.12.1/computer/attempts/withdrawn-ffd9f8b-transitional-resize-frame/README.md)
  preserves an automated pass rejected by mandatory visual review because its
  resize screenshot still contained transitional pre-resize pixels.
- [`withdrawn-fbdf89c-fixed-scstream-resize-timeout`](../../v0.12.1/computer/attempts/withdrawn-fbdf89c-fixed-scstream-resize-timeout/README.md)
  preserves the strengthened run that exposed the product's fixed-dimension
  SCStream after the target resized. The geometry-bound frame correctly timed
  out instead of accepting stale pixels under new window metadata.
