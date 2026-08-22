# macOS v0.12.1 persistent-share evidence candidate

This directory contains a deterministic evidence harness for the packaged
macOS v0.12.1 server and helper. It is **candidate infrastructure only** until
the harness runs against the exact release-candidate archive and records a
passing `helper-results.json`, `helper-rig.log`, and the six screenshots listed
below. A local pass becomes immutable release evidence only if the same archive
SHA-256 is later published in the v0.12.1 GitHub release.

The harness talks to the real loopback server API and launches the supplied
`Local Computer Helper.app` executable exactly once. It does not use a mock
connector, a polling screenshot replacement, global HID input, or a second
helper process.

## What the run proves

- The server and helper report exactly v0.12.1, are universal
  `arm64`/`x86_64` binaries, and pass strict code-signature checks.
- The supplied executables are byte-for-byte identical to the copies inside
  `local-browser-bridge-v0.12.1-macos-universal.tar.gz`, whose hash must match
  `SHA256SUMS.txt`.
- Existing Screen Recording and Accessibility permission are present. The rig
  never requests or changes either permission.
- The exact fixture window is observed through the authenticated server API.
- Accessibility `setValue` is confirmed by read-back. Generic Accessibility
  invocation remains conservatively `Partial`; the fixture's own counter is
  recorded as separate target-side proof.
- One persistent ScreenCaptureKit `SCStream` keeps the same share authority
  through cadence sampling, a 900 ms background pixel action, and a controlled
  exact-target resize.
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
- Share stop revokes frame authority, helper exit clears its server session,
  and server exit closes the selected loopback listener.

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
SHA-256, frame ID, source sequence, and transport sequence.

## Run

Use a controlled interactive macOS session where the packaged helper already
has Screen Recording and Accessibility permission. Do not interact with the
foreground app, mouse, or Spaces during the run; an invariant failure is a real
negative result and must be retained. The runner refuses to overwrite any
existing result, log, or expected screenshot. Move a completed or failed
attempt to a separately named evidence directory before starting another one.

```sh
mkdir -p "$SCRATCH_PARENT/v0.12.1-package"
tar -xzf "$RELEASE_CANDIDATE_DIR/local-browser-bridge-v0.12.1-macos-universal.tar.gz" \
  -C "$SCRATCH_PARENT/v0.12.1-package"

node evidence/v0.12.1/computer/helper-evidence-rig.mjs \
  "$SCRATCH_PARENT/v0.12.1-package/local-browser-bridge" \
  "$SCRATCH_PARENT/v0.12.1-package/Local Computer Helper.app/Contents/MacOS/local-computer-helper" \
  evidence/v0.12.1/computer \
  "$SCRATCH_PARENT" \
  "$RELEASE_CANDIDATE_DIR/local-browser-bridge-v0.12.1-macos-universal.tar.gz" \
  "$RELEASE_CANDIDATE_DIR/SHA256SUMS.txt"
```

The runner generates a random bearer token in memory, passes it only in the
server and helper process environments, deletes it from the temporary launch
objects immediately after each spawn, refuses to persist it, and removes the
entire scratch directory in cleanup. A failure still writes a sanitized
machine-readable negative result instead of silently discarding partial
evidence.

After the run, inspect every screenshot, confirm `assertions.failed` is zero,
and compare the recorded archive SHA-256 with the published release asset
before changing this directory's status from candidate to released evidence.

## Retained negative attempts

- [`attempts/withdrawn-98ff6f0-macos-invariant-refusal`](attempts/withdrawn-98ff6f0-macos-invariant-refusal/README.md)
  preserves the first fail-closed run. It is diagnostic history, not release
  evidence and not a passing result.
