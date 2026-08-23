# macOS v0.12.6 persistent-share evidence candidate

This directory contains a deterministic evidence harness for the packaged
macOS v0.12.6 server and helper. It is **candidate infrastructure only** until
the harness runs against the exact release-candidate archive and records a
passing `helper-results.json`, `helper-rig.log`, and the six screenshots listed
below. A local pass becomes immutable release evidence only if the same archive
SHA-256 is later published in the v0.12.6 GitHub release.

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

## What the run proves

- The server and helper report exactly v0.12.6, are universal
  `arm64`/`x86_64` binaries, and pass strict code-signature checks.
- The supplied executables are byte-for-byte identical to the copies inside
  `local-browser-bridge-v0.12.6-macos-universal.tar.gz`. The archive must be
  bound by a canonical `SHA256SUMS.txt` containing exactly the four v0.12.6
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
  resize counters plus independent foreground-process, front-window,
  hardware-cursor, and Space identities must stay unchanged.
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
- Foreground process, front window, hardware cursor, and active Space remain
  unchanged, both in each product action record and in an independent probe.
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

## Expected screenshots

1. `computer-01-exact-window-observe.png`
2. `computer-02-semantic-set-value.png`
3. `computer-03-semantic-invoke.png`
4. `computer-04-persistent-scstream-start.png`
5. `computer-05-live-share-pixel-action.png`
6. `computer-06-persistent-share-resize.png`

Every image is fetched from `/api/computer/screenshot` only after the public
observation is bound to the fixture's exact PID, native window ID, and title.
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
has Screen Recording and Accessibility permission. Do not interact with the
foreground app, mouse, or Spaces during the run; an invariant failure is a real
negative result and must be retained. The runner refuses to overwrite any
existing result, log, or expected screenshot. Move a completed or failed
attempt to a separately named evidence directory before starting another one.
The runner deliberately closes its own fixture during the target-close stage;
both same-PID fixture windows therefore exit together. It never closes an
unrelated application or window.

Run from the repository root. `EXPECTED_SHA256SUMS_SHA256` and
`EXPECTED_SOURCE_SHA` must come from the independent release-coordinator
handoff; never calculate either value from the candidate directory and then
trust that same directory. The order below is mandatory: validate the
independent manifest digest first, then the exact five-file inventory, canonical
manifest bytes, all four payload checksums, and all five GitHub attestations.
Only after every trust check passes may any archive be extracted or candidate
executable be invoked.

```bash
set -euo pipefail
: "${RELEASE_CANDIDATE_DIR:?set the frozen candidate directory}"
: "${SCRATCH_PARENT:?set a writable scratch parent}"
: "${EXPECTED_SHA256SUMS_SHA256:?set the independently handed-off manifest SHA-256}"
: "${EXPECTED_SOURCE_SHA:?set the independently handed-off source commit SHA}"

VERSION="0.12.6"
TAG="v$VERSION"
REPOSITORY="flrngel/local-browser-bridge"
MANIFEST="$RELEASE_CANDIDATE_DIR/SHA256SUMS.txt"
ASSETS=(
  "local-browser-bridge-v$VERSION-windows-x86_64.exe"
  "local-computer-helper-v$VERSION-windows-x86_64.exe"
  "local-browser-bridge-v$VERSION-macos-universal.tar.gz"
  "local-browser-bridge-extension-v$VERSION.zip"
)
RELEASE_FILES=("${ASSETS[@]}" "SHA256SUMS.txt")

[[ "$EXPECTED_SHA256SUMS_SHA256" =~ ^[0-9a-f]{64}$ ]]
[[ "$EXPECTED_SOURCE_SHA" =~ ^[0-9a-f]{40}$ ]]
[[ "$(shasum -a 256 "$MANIFEST" | awk '{print $1}')" == \
  "$EXPECTED_SHA256SUMS_SHA256" ]]

EXPECTED_LISTING="$(printf '%s\n' "${RELEASE_FILES[@]}" | LC_ALL=C sort)"
ACTUAL_LISTING="$(find "$RELEASE_CANDIDATE_DIR" -mindepth 1 -maxdepth 1 \
  -exec basename {} \; | LC_ALL=C sort)"
[[ "$ACTUAL_LISTING" == "$EXPECTED_LISTING" ]]
for NAME in "${RELEASE_FILES[@]}"; do
  [[ -f "$RELEASE_CANDIDATE_DIR/$NAME" && \
    ! -L "$RELEASE_CANDIDATE_DIR/$NAME" && \
    -s "$RELEASE_CANDIDATE_DIR/$NAME" ]]
done

[[ "$(wc -l < "$MANIFEST" | tr -d ' ')" == "4" ]]
awk \
  -v first="${ASSETS[0]}" \
  -v second="${ASSETS[1]}" \
  -v third="${ASSETS[2]}" \
  -v fourth="${ASSETS[3]}" '
  BEGIN {
    expected[1] = first
    expected[2] = second
    expected[3] = third
    expected[4] = fourth
    valid = 1
  }
  {
    hash = substr($0, 1, 64)
    separator = substr($0, 65, 2)
    name = substr($0, 67)
    if (length(hash) != 64 || hash !~ /^[0-9a-f]+$/ ||
        separator != "  " || name != expected[NR] ||
        length($0) != 66 + length(expected[NR])) {
      valid = 0
    }
  }
  END { exit !(valid && NR == 4) }
' "$MANIFEST"

(cd "$RELEASE_CANDIDATE_DIR" && shasum -a 256 -c SHA256SUMS.txt)
for NAME in "${RELEASE_FILES[@]}"; do
  gh attestation verify "$RELEASE_CANDIDATE_DIR/$NAME" \
    -R "$REPOSITORY" \
    --source-ref "refs/tags/$TAG" \
    --source-digest "$EXPECTED_SOURCE_SHA" \
    --signer-workflow "$REPOSITORY/.github/workflows/deploy.yml" \
    --deny-self-hosted-runners
done

# Extraction and candidate execution are permitted only below this line.
bash scripts/verify-release-assets.sh "$VERSION" "$RELEASE_CANDIDATE_DIR"
PACKAGE_ROOT="$(mktemp -d "$SCRATCH_PARENT/lbb-v0.12.6-package.XXXXXX")"
trap 'rm -rf -- "$PACKAGE_ROOT"' EXIT
tar -xzf "$RELEASE_CANDIDATE_DIR/local-browser-bridge-v0.12.6-macos-universal.tar.gz" \
  -C "$PACKAGE_ROOT"

node evidence/v0.12.6/computer/helper-evidence-rig.mjs \
  "$PACKAGE_ROOT/local-browser-bridge" \
  "$PACKAGE_ROOT/Local Computer Helper.app/Contents/MacOS/local-computer-helper" \
  evidence/v0.12.6/computer \
  "$SCRATCH_PARENT" \
  "$RELEASE_CANDIDATE_DIR/local-browser-bridge-v0.12.6-macos-universal.tar.gz" \
  "$RELEASE_CANDIDATE_DIR/SHA256SUMS.txt" \
  "$EXPECTED_SHA256SUMS_SHA256"
```

`EXPECTED_SHA256SUMS_SHA256` must be the 64-character lowercase SHA-256 of the
frozen candidate manifest obtained through the independent candidate handoff,
not recomputed from the directory being tested. The runner records the
expected and actual manifest hashes, canonical four-entry-set proof, and the
archive-entry binding in both passing and failing machine-readable results.
The outer gate is deliberately broader: it validates all candidate assets and
their provenance before the first extraction. The rig then repeats the
manifest/archive binding and byte-compares both supplied macOS executables with
its own fresh archive extraction before it invokes either binary.

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

## Historical v0.12.5 negative attempt

The exact v0.12.5 candidate run is retained as failed-candidate evidence, not
as a v0.12.6 result:

- [`withdrawn-badda8e-macos-native-text-restore-stale-frame`](../../v0.12.5/computer/attempts/withdrawn-badda8e-macos-native-text-restore-stale-frame/README.md)
  records 82 successful assertions followed by a fail-closed
  `COMPUTER_STALE_FRAME` refusal. Its live-share cleanup requested an unbound
  one-shot observation that a later stream frame correctly superseded before
  mutation dispatch. The same-epoch streamed restore gate in this harness is
  the regression boundary found by that run; production authority checks were
  not relaxed.

## Historical v0.12.2 negative attempt

The v0.12.2 candidate run is preserved byte-for-byte and is not v0.12.6
evidence:

- [`withdrawn-a52d761-post-cancel-fresh-share-refusal`](../../v0.12.2/computer/attempts/withdrawn-a52d761-post-cancel-fresh-share-refusal/README.md)
  proves that the v0.12.2 candidate reached cancellation teardown, one-shot
  recovery, and stale-frame refusal, then failed closed when the subsequent
  `computer.share.start` could not publish fresh share authority. That failure
  is the regression boundary exercised explicitly by this harness.

## Historical v0.12.1 negative attempts

The prior attempts remain byte-for-byte in the v0.12.1 evidence directory.
They are linked for diagnostic history only and are not v0.12.6 results:

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
