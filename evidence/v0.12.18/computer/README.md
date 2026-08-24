# macOS v0.12.18 persistent-share evidence candidate

This directory defines a fresh deterministic evidence harness for the packaged
macOS v0.12.18 server and helper candidate. It is protocol infrastructure, not
passing evidence; no v0.12.18 packaged run exists yet.

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
canceled. Version 0.12.18 does not reuse or relabel any prior binary,
screenshot, result, marker, notification, log, or other generated evidence
byte. The negative record is retained only on branch
`evidence/v0.12.13-macos-quiet-pointer-contamination-32695400912` at commit
`bdcc3620e28260e31a3a78bf7e584adf1f0db44e`, under
`evidence/v0.12.13/computer/attempts/withdrawn-7d2692d-macos-quiet-pointer-contamination/`.

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
and active-Space oracles, both evidence lanes wait for one continuous
30-second quiet-seat epoch before invoking either candidate even with
`--version`. The gate samples every 500 ms and requires at least 60 stable
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
- The server and helper report exactly v0.12.18, are universal
  `arm64`/`x86_64` binaries, and pass strict code-signature checks.
- The supplied executables are byte-for-byte identical to the copies inside
  `local-browser-bridge-v0.12.18-macos-universal.tar.gz`. The archive must be
  bound by a canonical `SHA256SUMS.txt` containing exactly the four v0.12.18
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

## Pointer evidence lanes

Every product action must report an exact-target delivery route, a recorded
dispatch attempt, no shared-seat/global-HID/cursor-mutation route, confirmed
helper global-pointer preservation, and a corroborated shared-pointer boundary.
Cursor equality plus generic and platform-specific activity observations are
diagnostic inputs to that boundary; none identifies a human or physical device.

Before either lane begins product execution, the native quiet-seat gate above
must complete. This applies to `deliberate-concurrency` too: that lane contains
required quiet cells before the orange `MOVE` prompt, so ambient motion must not
consume its one allowed attempt. Intentional movement starts only after those
quiet cells and the separately verified prompt handoff.

The default `quiet` lane requires every product and independent evidence cell
to report a healthy monitor, `sharedPointerActivityState: "quiet"`, unchanged
sampled cursor position, and no generic shared-pointer activity. Unexpected
shared-seat activity contaminates this release lane without being blamed on the
helper.

The `deliberate-concurrency` lane is the one-shot release-candidate mode. A
separately authorized participant introduces shared-seat pointer activity while
at least one product evidence cell is active; the rig itself never emits global
input. After the first quiet cells, the runner owns a separate, nonactivating
notification process. Its mouse-transparent panel has only the fixed states
`WAITING`, orange `MOVE`, orange `ACTION`, and green `COMPLETE`; it never becomes key or
frontmost and contains no input handler. An independent SystemProbe must bind
the exact prompt PID, fixed state title, on-screen window, and nonactivating
state before the runner publishes the request notification or dispatches the
product command.

When the orange panel says `MOVE THE POINTER NOW`, move the shared pointer
continuously without clicking and keep moving until the panel turns green. The
runner waits up to 300 seconds and requires three consecutive `mouseMoved`
counter advances spanning at least 500 ms. Before any product request exists,
a button, drag, scroll, tablet, foreground, focus, or Space change resets the
clean-motion arm to a fresh cumulative-counter baseline under the same absolute
300-second deadline. Every `MOVE` sample is checked for all of those context
changes, not only HID-counter progress. If contamination is observed while the
prompt changes to `ACTION RUNNING` or in the final independent sample immediately
before dispatch, the prompt returns to `MOVE` and requires a new clean arm. The
state machine returns that final clean sample directly as the product-action
baseline; it does not take another pre-dispatch sample outside the retry loop. No
such reset is allowed once dispatch is in flight: unknown monitoring or any
post-dispatch disallowed input still fails closed. A SystemProbe timeout is
treated as ordinary arm expiration only when the original absolute arm deadline
has actually elapsed; earlier probe failures remain fatal.

After one clean transition the runner takes a new independent baseline and
immediately starts one bounded two-second target-routed `computer.click`. Keep
moving without clicking until the panel turns green. Entering `ACTION` starts a
separate 10-second hard grace, capped by a 310-second total prompt lifetime; the
prompt exits at the earlier deadline even after completion.
Both the product result and the independent post-action sample must show fresh
post-baseline contamination, and the independent interval must contain
`mouseMoved` progress with every button, drag, scroll, and tablet counter
stable. Pre-arm motion cannot satisfy either boundary.
The same run must contain at least one `quiet` cell and at least one
`contaminated` cell, keep the helper route and preservation claims valid, and
contain no `unknown` cell. HID-system activity remains a shared-seat signal,
never proof of a human or particular physical device.

The runner atomically publishes create-once notifications at
`operator/macos-pointer-concurrency-handoff-request.json` and
`operator/macos-pointer-concurrency-handoff-complete.json`. Both bind
marker `schemaVersion: 1`, `kind`, `productVersion`, `requestId`, canonical UTC
`createdAt`, `runnerPid`, `promptPid`, `requestDelivered`, `panelOnScreen`,
`panelNonactivating`, `notificationOnly`, and `acceptedAsAuthority` in that
order. The completion notification then adds `sustainedMotionSamples`,
`sustainedMotionSpanMilliseconds`, `productBoundaryContaminated`, and
`independentBoundaryContaminated`, then `clickFreeMotionObserved`. That final
boolean is true only when click-free movement advances while arming, across the
product action, and through the independently observed green completion state.
The PID fields
exist only in these operator notifications so an external watcher can bind the
live handoff; they are not
copied into the result or log. The runner never reads an external
acknowledgement and never accepts either notification as authority. It publishes
completion only after both contamination gates pass, then closes the prompt;
a successful result is impossible until both publications exist and prompt
teardown has completed.

The retained schema-v5 result contains only bounded booleans, counts, and state
enums for this boundary. Raw cursor coordinates, raw platform activity
counters, prompt titles, prompt PIDs, and local paths are never written to the
result, log, or failure diagnostics. A failure before command dispatch records
stage `waitDeliberatePointerActivity` and `actionDispatched: false`. Schema v5
uses a tri-state dispatch field: `false` before the command request, `null`
while its outcome is in flight or unknown, and `true` only after a returned
product result reports a recorded dispatch attempt. A returned result without
that proof fails closed and cannot publish completion.

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

1. Run one fresh `quiet` lane synchronously. Its native gate first requires 30
   continuous quiet seconds. Do not interact with the pointer, foreground
   application, windows, or Spaces during the gate or product run.
2. Only if the quiet lane passes, run one fresh `deliberate-concurrency` lane.
   Leave the seat quiet while its own native gate and initial quiet cells run.
   Move the pointer continuously without clicking only while the orange prompt
   requests it, and stop when the prompt turns green.
3. Review all six screenshots from each lane. Only after both result files pass
   and all twelve images are accepted may the finalizer create the aggregate.
   Complete that review and finalization within 30 minutes after the deliberate
   lane finishes; both the local finalizer and publication verifier enforce the
   same interval.

If either lane fails or is interrupted, stop. Preserve that attempt as negative
evidence, withdraw the frozen candidate, and do not retry, resume, combine it
with another lane, or execute Windows/Chrome acceptance. The runner refuses to
overwrite any result, log, operator marker, or screenshot.

Run every command below from a fresh clean detached checkout of the annotated
`v0.12.18` tag. The source checkout and every candidate/evidence directory
must be ordinary, owner-private, non-symlink paths outside the repository.
Supply the run and artifact identifiers from the waiting tag workflow; a branch
name, local directory, manifest, or previously downloaded artifact is not
authority.

The checked-in candidate binder validates the independent API binding, raw
artifact ZIP size and SHA-256, exact five-file inventory, canonical LF checksum
manifest, all payload hashes, all five GitHub attestations, both exact-attempt
attestation URI fields, GitHub-hosted runner identity, clean detached source,
annotated tag object, and its own tagged source blob. It never executes
candidate bytes. Extraction and candidate execution are permitted only below
this line and only after the binder passes.

```bash
set -euo pipefail
umask 077

: "${VERSION:=0.12.18}"
: "${WORKFLOW_RUN_ID:?set the exact tag-workflow run ID}"
: "${WORKFLOW_RUN_ATTEMPT:?set the exact run attempt}"
: "${RELEASE_CANDIDATE_ARTIFACT_ID:?set the exact artifact ID}"
: "${EXPECTED_SOURCE_SHA:?set the exact tagged source commit}"
: "${EXPECTED_TAG_OBJECT_SHA:?set the annotated tag object SHA}"
: "${PRIVATE_PARENT:?set an existing owner-private ordinary directory}"

[[ "$VERSION" == "0.12.18" ]]
[[ "$WORKFLOW_RUN_ID" =~ ^[1-9][0-9]*$ ]]
[[ "$WORKFLOW_RUN_ATTEMPT" =~ ^[1-9][0-9]*$ ]]
[[ "$RELEASE_CANDIDATE_ARTIFACT_ID" =~ ^[1-9][0-9]*$ ]]
[[ "$EXPECTED_SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]]
[[ "$EXPECTED_TAG_OBJECT_SHA" =~ ^[0-9a-f]{40}$ ]]
[[ -d "$PRIVATE_PARENT" && ! -L "$PRIVATE_PARENT" ]]

RUN_NONCE="$(openssl rand -hex 16)"
CANDIDATE_ROOT="$PRIVATE_PARENT/candidate-$RUN_NONCE"
ATTEMPT_ROOT="$(mktemp -d "$PRIVATE_PARENT/lbb-v0.12.18-macos.XXXXXX")"
SCRATCH_PARENT="$(mktemp -d "$PRIVATE_PARENT/lbb-v0.12.18-scratch.XXXXXX")"
PACKAGE_ROOT="$PRIVATE_PARENT/lbb-v0.12.18-package-$RUN_NONCE"

bash scripts/fetch-verify-release-candidate.sh \
  "$VERSION" \
  "$WORKFLOW_RUN_ID" \
  "$WORKFLOW_RUN_ATTEMPT" \
  "$RELEASE_CANDIDATE_ARTIFACT_ID" \
  "$EXPECTED_SOURCE_SHA" \
  "$EXPECTED_TAG_OBJECT_SHA" \
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

ARCHIVE="$RELEASE_CANDIDATE_DIR/local-browser-bridge-v0.12.18-macos-universal.tar.gz"
MANIFEST="$RELEASE_CANDIDATE_DIR/SHA256SUMS.txt"
[[ ! -e "$PACKAGE_ROOT" && ! -L "$PACKAGE_ROOT" ]]
node evidence/v0.12.18/computer/helper-evidence-rig.mjs \
  --prepare-package \
  "$ARCHIVE" "$MANIFEST" "$EXPECTED_SHA256SUMS_SHA256" \
  "$PACKAGE_ROOT"

SERVER="$PACKAGE_ROOT/local-browser-bridge"
HELPER="$PACKAGE_ROOT/Local Computer Helper.app/Contents/MacOS/local-computer-helper"
QUIET_DIR="$ATTEMPT_ROOT/quiet"
DELIBERATE_DIR="$ATTEMPT_ROOT/deliberate-concurrency"

node evidence/v0.12.18/computer/helper-evidence-rig.mjs \
  "$SERVER" "$HELPER" "$QUIET_DIR" "$SCRATCH_PARENT" \
  "$ARCHIVE" "$MANIFEST" "$EXPECTED_SHA256SUMS_SHA256" \
  "$EXPECTED_SOURCE_SHA" "$EXPECTED_TAG_OBJECT_SHA" \
  "$WORKFLOW_RUN_ID" "$WORKFLOW_RUN_ATTEMPT" \
  "$RELEASE_CANDIDATE_ARTIFACT_ID" \
  "$RELEASE_CANDIDATE_ARTIFACT_ZIP_SHA256" \
  quiet

# Inspect quiet/helper-results.json and all six quiet PNGs now.
# Stop permanently if any assertion or visual review fails.

node evidence/v0.12.18/computer/helper-evidence-rig.mjs \
  "$SERVER" "$HELPER" "$DELIBERATE_DIR" "$SCRATCH_PARENT" \
  "$ARCHIVE" "$MANIFEST" "$EXPECTED_SHA256SUMS_SHA256" \
  "$EXPECTED_SOURCE_SHA" "$EXPECTED_TAG_OBJECT_SHA" \
  "$WORKFLOW_RUN_ID" "$WORKFLOW_RUN_ATTEMPT" \
  "$RELEASE_CANDIDATE_ARTIFACT_ID" \
  "$RELEASE_CANDIDATE_ARTIFACT_ZIP_SHA256" \
  deliberate-concurrency &
RUNNER_PID=$!

# This watcher relays create-once notifications; it grants no authority.
WATCHER_STATUS=0
node scripts/wait-macos-pointer-concurrency-handoff.mjs \
  --mode watch \
  --evidence-dir "$DELIBERATE_DIR" \
  --runner-pid "$RUNNER_PID" || WATCHER_STATUS=$?
RUNNER_STATUS=0
wait "$RUNNER_PID" || RUNNER_STATUS=$?
[[ "$WATCHER_STATUS" == "0" && "$RUNNER_STATUS" == "0" ]]

# Inspect deliberate-concurrency/helper-results.json and all six lane PNGs now.
# Stop permanently if any assertion or visual review fails.

QUIET_CANONICAL="$(cd "$QUIET_DIR" && pwd -P)"
DELIBERATE_CANONICAL="$(cd "$DELIBERATE_DIR" && pwd -P)"
AGGREGATE_DIR="$(mktemp -d "$PRIVATE_PARENT/lbb-v0.12.18-aggregate.XXXXXX")"
AGGREGATE_CANONICAL="$(cd "$AGGREGATE_DIR" && pwd -P)"

node scripts/finalize-macos-acceptance.mjs \
  "$QUIET_CANONICAL" \
  "$DELIBERATE_CANONICAL" \
  "$AGGREGATE_CANONICAL"
```

On macOS, `/tmp` commonly resolves to `/private/tmp`; pass only canonical
`pwd -P` spellings to the finalizer. It rejects linked, non-private,
noncanonical, overlapping, stale, extra-file, mismatched-candidate, same-result,
wrong-lane, unreviewable-screenshot, and malformed-marker inputs. It publishes
`macos-acceptance.json` create-once and never modifies either lane. It also
independently proves that each lane has a bounded forward timeline, both
deliberate operator markers fall inside that lane, and the deliberate lane
started strictly after the quiet lane finished successfully.

The successful aggregate binds both distinct lane result digests, all twelve
screenshot file and decoded-pixel digests plus dimensions, both deliberate
operator-marker digests, the
exact source/tag/workflow attempt/artifact/raw-ZIP/manifest/package identity,
and the tagged harness blobs. Copy only its allowlisted sanitized evidence into
the single-parent release-evidence commit. Keep candidate downloads, extracted
packages, scratch data, absolute paths, credentials, and raw identifiers out of
the repository.

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
history, not v0.12.18 evidence:

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
v0.12.18 evidence:

- [`v0.12.7 macOS exact-candidate pass`](../../v0.12.7/computer/README.md)
- [`withdrawn-0749953-windows-foreground-arm-timeout`](../../v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout/README.md)

## Historical v0.12.6 exact-candidate result

The exact v0.12.6 macOS archive passed all 187 assertions and produced six
reviewed screenshots. The same frozen candidate then failed closed on Windows
after protocol-bound helper readiness and before observation, capture, sharing,
input, or any screenshot because its shown WinForms sentinel did not own the
global foreground. The publish job was canceled and no v0.12.6 Release was
created. These records are diagnostic history, not v0.12.18 evidence:

- [`v0.12.6 macOS exact-candidate pass`](../../v0.12.6/computer/README.md)
- [`withdrawn-397e4b6-windows-foreground-sentinel-timeout`](../../v0.12.6/computer/attempts/withdrawn-397e4b6-windows-foreground-sentinel-timeout/README.md)

## Historical v0.12.5 negative attempt

The exact v0.12.5 candidate run is retained as failed-candidate evidence, not
as a v0.12.18 result:

- [`withdrawn-badda8e-macos-native-text-restore-stale-frame`](../../v0.12.5/computer/attempts/withdrawn-badda8e-macos-native-text-restore-stale-frame/README.md)
  records 82 successful assertions followed by a fail-closed
  `COMPUTER_STALE_FRAME` refusal. Its live-share cleanup requested an unbound
  one-shot observation that a later stream frame correctly superseded before
  mutation dispatch. The same-epoch streamed restore gate in this harness is
  the regression boundary found by that run; production authority checks were
  not relaxed.

## Historical v0.12.2 negative attempt

The v0.12.2 candidate run is preserved byte-for-byte and is not v0.12.18
evidence:

- [`withdrawn-a52d761-post-cancel-fresh-share-refusal`](../../v0.12.2/computer/attempts/withdrawn-a52d761-post-cancel-fresh-share-refusal/README.md)
  proves that the v0.12.2 candidate reached cancellation teardown, one-shot
  recovery, and stale-frame refusal, then failed closed when the subsequent
  `computer.share.start` could not publish fresh share authority. That failure
  is the regression boundary exercised explicitly by this harness.

## Historical v0.12.1 negative attempts

The prior attempts remain byte-for-byte in the v0.12.1 evidence directory.
They are linked for diagnostic history only and are not v0.12.18 results:

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
