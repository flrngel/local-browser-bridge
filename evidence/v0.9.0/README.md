# v0.9.0 evidence index

This directory contains sanitized, exploratory evidence captured on 2026-08-19 while version 0.9.0 was under development. It is not final release verification: the captures are not bound to a frozen release commit, packaged extension ZIP, signed platform binaries, or a release-asset checksum manifest.

## Real Chrome exploration

The files in `browser/` are cropped screenshots from Google Chrome and the deterministic local demo page. `browser/results.json` records only facts visible in those screenshots and their SHA-256 hashes; it is not a command transcript.

| Artifact | What it visibly supports | Important limit |
| --- | --- | --- |
| `browser-01-real-chrome-extension.jpg` | Chrome's extension card shows Local Browser Bridge 0.9.0 enabled | Does not establish that a release ZIP, rather than an unpacked development tree, was loaded |
| `browser-02-native-held-debugger.jpg` | Chrome's native debugger disclosure and Cancel control are visible above the local demo | Does not prove the result of pressing Cancel |
| `browser-03-native-and-page-indicators.jpg` | The native disclosure and the page-level status pill/Stop control are visible together | Does not prove Stop, revocation, or post-stop refusal |
| `browser-04-trusted-click-pointer.jpg` | The demo shows populated fields, a greeting result, and a visible pointer | A still image does not prove pointer-path timing, trusted-event provenance, or the command response |
| `browser-05-long-pointer-click-at.jpg` | The offscreen target is visible and the demo action log reads `bottom-click` | No raw `clickAt` result or stale-target preflight record is present |
| `browser-06-type-text.jpg` | The direct-input field visibly contains `keyboard-sota` | No raw `typeText` result is present |
| `browser-07-key.jpg` | Same visible state as `browser-06-type-text.jpg` | This file is byte-for-byte identical to file 06 and is not independent proof of a key action |
| `browser-08-scroll.jpg` | The viewport is scrolled to the bottom target and the demo action log is visible | No raw scroll result is present |

These screenshots demonstrate that the development extension was visible in real Chrome and that Chrome's native debugger disclosure could coexist with the page indicator. They do not establish a continuous, reproducible end-to-end run from a release package.

## Native computer exploration

The files in `computer/` show the dedicated macOS fixture captured as an exact window. `computer/results.json` is the sanitized exploratory result record produced by that run.

| Artifact | What it visibly supports | Important limit |
| --- | --- | --- |
| `computer-01-observe.jpg` | Initial exact-window fixture state | A screenshot alone does not prove capture isolation |
| `computer-02-move.jpg` | Composited session pointer over the fixture | Does not independently measure the hardware cursor |
| `computer-03-click.jpg` | Fixture state changed to `clicks=1` and `last=click` | The JSON correctly classifies pixel delivery as `Unverifiable`, not confirmed |
| `computer-04-drag.jpg` | Fixture drag counter and `last=drag` state | Same exploratory-run limitation |
| `computer-05-scroll.jpg` | Fixture scroll value `-7` and `last=scroll` state | Same exploratory-run limitation |
| `computer-06-type-text.jpg`, `computer-07-key.jpg` | Final text-field state after the exploratory text/key sequence | The two files are byte-for-byte identical and do not separately prove both actions; the JSON contains the distinct responses |
| `computer-08-set-value.jpg` | Semantic field contains `semantic-value` | The JSON records a confirmed `value-confirmed` postcondition |
| `computer-09-invoke.jpg` | Fixture shows `Semantic action complete` | The JSON records a confirmed `element-state-changed` postcondition |
| `computer-10-live-share-action.jpg` | Exact-window frame with the composited session pointer during the share run | Share start/status/stop and frame sequence are recorded only in the exploratory JSON |

The native result record includes macOS delivery invariants, action timing, frame IDs, semantic postconditions, share lifecycle, and a before/after foreground and hardware-cursor sample. Pixel actions remain explicitly `Unverifiable`; invariant delivery evidence is not treated as target-effect confirmation.

## Privacy review

All 18 JPEGs were visually reviewed at full size and as contact sheets. The bundle was also scanned with filename/type enumeration, JSON parsing, high-risk credential and identity regular expressions, printable-string inspection of every JPEG, and ImageMagick metadata inspection.

No bearer token, cookie, password, API key, private key, email address, account name, Chrome profile path, browsing history, personal URL, or unrelated window was found. The only Chrome identifier shown is the development extension ID in file 01; it is a non-secret identifier and does not grant access. JPEG metadata contains only basic JFIF/date and, for the native captures, color-space/pixel-dimension fields; no GPS, author, device, account, or filesystem-path metadata was found.

## Required final release verification

Before this directory can be described as final release evidence, all of the following still need fresh proof from a frozen release commit:

- Run the complete formatting, strict lint, Rust test, extension contract, manifest, shell, version, language, secret, and deterministic-package checks on the frozen tree.
- Build and verify the macOS and Windows release binaries plus the extension ZIP; record SHA-256 values, target architecture, archive contents, and GitHub provenance/attestation results.
- Load the packaged extension through `chrome://extensions` and record a continuous server-to-extension run with sanitized machine-readable browser responses.
- Exercise Chrome Cancel and the page Stop/Resume controls, proving revocation and refusal of subsequent or late mutations.
- Record stale snapshot, target mutation/occlusion, protocol mismatch, replay, timeout/cancel, and reconnect/session-replacement failures with zero unintended side effects.
- Repeat native runtime verification on both macOS and Windows, including share start/status/stop, session teardown, action cancellation, held-input cleanup, and independently sampled foreground/focus/cursor/Space-or-desktop invariants.
- Verify update discovery/download UX against the published same-version multi-platform release assets.
- Re-run the privacy/secret scan on the final evidence and release artifacts.
