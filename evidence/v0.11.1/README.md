# Version 0.11.1 boundary evidence

Recorded 2026-08-20 to close the live-evidence gaps left after v0.11.1. This
bundle intentionally preserves four separate evidence runs instead of
presenting them as one release result:

| Run | Result | Evidence class |
|---|---:|---|
| Published v0.11.1 Chromium extension | **17 of 19** | Exact published extension ZIP in a disposable Chrome for Testing profile; exposes a real nested-frame defect |
| Local v0.11.2 Chromium extension candidate | **22 of 22** | Unpublished worktree package; repair validation only, not release proof |
| Published v0.11.1 stock-Chrome native Cancel | **11 of 11** | Exact published extension ZIP in a disposable stock Chrome 151 profile; browser-owned warning clicked through an exact native AX target |
| Published v0.11.1 macOS server and helper | **32 of 32** | Exact published universal archive, exercised through the helper transport and a deterministic native fixture |

The published v0.11.1 assets were downloaded from the immutable release and
checked against `SHA256SUMS.txt`; all release assets were also independently
verified with GitHub build provenance before these runs. The machine-readable
records retain the hashes of the exact browser ZIP, server, helper, and macOS
archive used. Attestation verification is an acquisition step outside the
helper rig and is not inferred from `helper-results.json` alone.

## Published browser result: 17 of 19

[`browser/frame-results.json`](browser/frame-results.json) and
[`browser/frame-rig.log`](browser/frame-rig.log) are the unedited result and
transcript from the exact published `local-browser-bridge-extension-v0.11.1.zip`
in Google Chrome for Testing 152.0.7977.54. The ZIP SHA-256 is
`9beaf8635115a9cfe672767d675106e1b826e274780d0cfc9e9be60ede3c75f0`.

The live run proved that:

- a same-process, same-origin iframe is reported with the explicit
  `same_process_frame` skip reason;
- a depth-one out-of-process iframe is attached and merged, including its text
  field;
- attempting `page.fill` with a frame element ref fails closed with HTTP 400,
  `FRAME_ACTION_UNSUPPORTED`, taxonomy `invalid_request`, and `retriable: false`;
- the routing diagnostic fails closed with `supported: false`, reason
  `session_routing_unverified`, zero frame refs, and an active control lease.

It also found a real v0.11.1 defect. The nested depth-two OOPIF was present as a
Chrome target and visibly rendered, but the bridge attached and merged only the
depth-one child. The failed checks are `depth-two-frame-merged` and
`depth-two-button-visible`. Version 0.11.1 therefore does **not** support its
claimed nested-frame boundary beyond depth one. The screenshot
[`browser/00-published-frame-observation.png`](browser/00-published-frame-observation.png)
shows the fixture at the failed observation point.

The negative routing result is deliberately diagnostic. The rig copies the
input extension and performs exactly one evidence-only replacement so child
commands omit `sessionId`; its hash and the before/after source lines are
recorded in the JSON. It is not evidence that an unmodified browser produced
this result. In particular, Chrome's extension API schema in versions 118–124
does not expose the `sessionId` member on `chrome.debugger.Debuggee`; the member
was added in Chrome 125. That history motivates conservative compatibility
handling, but this fault injection is **not** a live Chrome 118–124 result or a
known legacy-Chrome failure.

## Local candidate result: 22 of 22

[`browser/frame-candidate-results-v0.11.2.json`](browser/frame-candidate-results-v0.11.2.json)
and
[`browser/frame-candidate-rig-v0.11.2.log`](browser/frame-candidate-rig-v0.11.2.log)
record the same matrix against a locally packaged v0.11.2 candidate. This is
explicitly an **unpublished developer candidate**, not an immutable release
artifact and not proof of a shipped v0.11.2 release.

The candidate recursively attaches nested frame targets. Both depth-one and
depth-two OOPIFs were merged, the same-origin skip remained explicit, frame
fill stayed fail-closed, and the depth-two button received a trusted coordinate
click. The grandchild fixture independently reported `grandchild-click:true`.
The evidence-only routing fault still produced `supported: false` with
`session_routing_unverified` while preserving the lease.

The candidate screenshots are:

- [`browser/01-candidate-v0.11.2-frame-observation.png`](browser/01-candidate-v0.11.2-frame-observation.png), showing both frame depths in the observation;
- [`browser/02-candidate-v0.11.2-depth-two-click.png`](browser/02-candidate-v0.11.2-depth-two-click.png), showing the trusted grandchild click landed.

Those image files were given stable evidence-index names after the run. The
generated basenames retained inside the candidate JSON are the original rig
output names.

## Published stock-Chrome native Cancel result: 11 of 11

[`browser/native-warning-results.json`](browser/native-warning-results.json),
[`browser/native-warning-click.json`](browser/native-warning-click.json), and
[`browser/native-warning-cleanup.json`](browser/native-warning-cleanup.json)
record the exact published v0.11.1 server and extension in stock Google Chrome
151.0.7922.138. Chrome 151 ignores `--load-extension`, so the rig installed the
unpacked payload from the checksum-verified release ZIP into a fresh disposable
profile through the browser-target CDP `Extensions.loadUnpacked` command. This
is an ephemeral test install, not a claim that the installation survives a
browser restart.

The first exact-window screenshot,
[`browser/10-native-warning-and-page-pill.png`](browser/10-native-warning-and-page-pill.png),
shows the browser-owned **"Local Browser Bridge" started debugging this
browser** warning and **Cancel** button at the same time as the separate
in-page control pill and **Stop** button. The native UI tool bound itself to the
dedicated Chrome PID, found exactly one matching `AXButton` with description
`Cancel` at `{x: 784, y: 129, width: 68, height: 32}`, and posted a real
CoreGraphics click at its center `(818, 145)`. It did not search or click the
user's existing Chrome process.

That click produced `chrome.debugger.onDetach` reason `canceled_by_user`, made
the lease inactive, latched the global human pause, removed the page pill, and
left a remote restart refused with HTTP 423 `HUMAN_CONTROL_PAUSED` and taxonomy
`needs_user`. The second exact-window screenshot,
[`browser/11-native-after-cancel.png`](browser/11-native-after-cancel.png),
shows both the browser warning and page pill gone. The hardware cursor was
returned to its pre-run coordinates before that screenshot. The disposable
Chrome remained foreground solely for the after-state capture; once it exited,
the pre-run user Chrome PID was foreground again. The cleanup record also
confirms that the dedicated profile, CDP endpoint, server endpoint, Chrome
process, and server process were absent. No bearer token is retained in this
bundle, and no OS permission prompt was requested or approved.

## Published macOS helper result: 32 of 32

[`computer/helper-results.json`](computer/helper-results.json) and
[`computer/helper-rig.log`](computer/helper-rig.log) record the exact published
v0.11.1 server and `Local Computer Helper.app` from
`local-browser-bridge-v0.11.1-macos-universal.tar.gz`. The archive SHA-256 is
`83bbb5b8408dabc8fe7a407941ff6c43c6a846a3e74bc1c06b6502860d29ebd5`.

The run proved the following on macOS 26.5.1, arm64:

- both executables reported 0.11.1, contained arm64 and x86_64 slices, passed
  strict code-signature verification, and matched the published checksum;
- the packaged helper completed its protocol handshake and advertised
  acknowledged live sharing;
- exact-window observation returned a screenshot and four Accessibility
  elements from the deterministic fixture;
- semantic `setValue` and `invoke` operations were confirmed by target-side
  postconditions;
- a background pixel click advanced the fixture counter while foreground app,
  keyboard focus, hardware cursor, and macOS Space remained unchanged;
- the pixel operation remained conservatively classified `Unverifiable`; the
  deterministic fixture counter is independent evidence for this target, not a
  general upgrade of pixel-effect confidence;
- 10 FPS exact-window sharing remained acknowledgement-paced with
  `latest-frame-wins` backpressure, advanced its sequence, recorded dropped
  frames, and stopped cleanly;
- terminating the helper revoked the computer session and cleared the current
  observation.

Screen Recording and Accessibility permissions were already granted on this
machine. The rig did not request, approve, or modify either permission.
Screenshots are
[`computer/computer-01-packaged-observe.png`](computer/computer-01-packaged-observe.png)
and
[`computer/computer-02-share-action.png`](computer/computer-02-share-action.png).

## Reproduction commands

The browser driver and deterministic fixtures are checked in at
[`browser/frame-boundaries-rig.mjs`](browser/frame-boundaries-rig.mjs) and
[`browser/fixtures/`](browser/fixtures/). It accepts an exact unpacked extension
directory and the ZIP from which it came. Use separate output directories for
the published and candidate runs because the driver uses generated basenames:

```sh
node evidence/v0.11.1/browser/frame-boundaries-rig.mjs \
  "$SERVER_BIN" "$PUBLISHED_EXTENSION_DIR" "$CHROME_BIN" \
  "$PUBLISHED_OUTPUT_DIR" "$SCRATCH_DIR" "$PUBLISHED_EXTENSION_ZIP" \
  published

node evidence/v0.11.1/browser/frame-boundaries-rig.mjs \
  "$SERVER_BIN" "$CANDIDATE_EXTENSION_DIR" "$CHROME_BIN" \
  "$CANDIDATE_OUTPUT_DIR" "$SCRATCH_DIR" "$CANDIDATE_EXTENSION_ZIP" \
  candidate
```

The published invocation is expected to exit nonzero because it faithfully
records the two v0.11.1 nested-frame failures. The candidate invocation should
exit zero. Each invocation creates disposable Chrome profiles, binds the
fixtures only to loopback, removes its injected extension copy, and stops its
child processes.

The stock-Chrome driver and native target helper are
[`browser/native-warning-rig.mjs`](browser/native-warning-rig.mjs) and
[`browser/native-warning-ui-tool.swift`](browser/native-warning-ui-tool.swift).
The driver prints a `READY native-cancel` boundary after proving the extension
and page state; the operator then inspects and clicks the unique PID-scoped
native button with the compiled helper. This explicit split prevents a browser
protocol command from standing in for the browser-chrome click being tested.

The native driver and fixture sources are checked in under
[`computer/`](computer/). Run it with the server and helper extracted from the
same published archive:

```sh
node evidence/v0.11.1/computer/helper-evidence-rig.mjs \
  "$PUBLISHED_SERVER_BIN" "$PUBLISHED_HELPER_BIN" \
  evidence/v0.11.1/computer "$SCRATCH_PARENT" \
  "$PUBLISHED_MACOS_ARCHIVE" "$SHA256SUMS_FILE"
```

The helper rig uses a fresh bearer token, refuses to persist it, selects a free
loopback port, compiles the two deterministic Swift probes, and removes its
scratch directory and child processes in cleanup.

## Boundaries still not proven

- **Extension popup Release control:** no v0.11.x run here presses the popup's
  Release control and verifies the resulting state.
- **Browser-restart persistence after native Cancel:** the stock-Chrome run
  proves the live global pause and its remote restart refusal, but its
  browser-target unpacked installation is intentionally ephemeral and the run
  does not claim persistence across a full Chrome restart.
- **Windows UI Automation runtime:** Windows artifacts have build and package
  coverage elsewhere, but this bundle contains no execution on a real
  interactive Windows desktop. The macOS helper run cannot substitute for it.

The v0.11.1 browser failure and the v0.11.2 candidate success remain separate
on purpose: the former proves the released defect, while the latter validates
the local repair without claiming that repair has been released.
