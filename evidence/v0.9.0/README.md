# v0.9.0 evidence index

This directory contains two evidence layers captured on 2026-08-19:

- The original sanitized exploratory run in `browser/results.json`, `computer/results.json`, and screenshots 01–10.
- A frozen, packaged pre-publication run in both `final-results.json` files, browser screenshots 09–11, and computer screenshot 11. It is bound to code commit `dc31363`, the deterministic extension ZIP, and the packaged universal macOS server/helper archive.

The frozen records deliberately keep `finalReleaseVerification` false until the tagged GitHub assets and build attestations have been published, downloaded, and verified. Windows runtime execution is also reported honestly as unavailable on this macOS host; Windows x86_64 check, strict Clippy, and all-target test builds passed.

## Frozen package verification

The exact extension ZIP was extracted and loaded in the user's real Google Chrome 151 through `chrome://extensions`. The packaged macOS server and permission-owning helper were run as separate processes. The machine-readable records are [`browser/final-results.json`](browser/final-results.json) and [`computer/final-results.json`](computer/final-results.json).

| Artifact | What it supports |
| --- | --- |
| `browser/browser-09-final-package-details.jpg` | Real `chrome://extensions` details page: Local Browser Bridge 0.9.0 enabled, developer mode, and reviewed permissions |
| `browser/browser-10-final-native-and-page-control.jpg` | Chrome-native debugger disclosure and Cancel, page pill/Stop, browser cursor, and deterministic demo state visible together |
| `browser/browser-11-final-tool-screenshot.jpg` | Tool screenshot hides page controls and Chrome UI while retaining the model-visible browser cursor |
| `computer/computer-11-final-packaged-background.jpg` | Exact-window packaged-helper capture with the separate native session pointer |

The live browser record includes a successful non-interrupting background click (`skipped_background`, one final CDP move, unchanged foreground and hardware cursor), a foreground animated click, page Stop, popup-only Resume, Chrome Cancel, global remote-mutation refusal while human-paused, remote stop, and service-worker reload/reconnect. The native record includes exact-window observe/move, independent foreground/cursor sampling, 10 FPS sharing with an action during the stream, share stop, and conservative semantic effect classification.

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

All 22 JPEGs were visually reviewed at full size. The bundle was also scanned with filename/type enumeration, JSON parsing, high-risk credential and identity regular expressions, printable-string inspection of every JPEG, and ImageMagick metadata inspection.

No bearer token, cookie, password, API key, private key, email address, account name, Chrome profile path, browsing history, personal URL, or unrelated window was found. The unpacked extension IDs shown in browser files 01 and 09 are non-secret identifiers and do not grant access. JPEG metadata contains only basic JFIF/date and, for the native captures, color-space/pixel-dimension fields; no GPS, author, device, account, or filesystem-path metadata was found.

## Remaining publication verification

The frozen source and local package gates are complete: 120 local tests, 38 extension contracts, strict Rust 1.88 lint, RustSec audit, deterministic ZIP comparison, universal macOS architecture/signature checks, Windows PE/package checks, Linux Rust 1.88 check/lint/tests, Windows x86_64 check/lint/test-build, and macOS x86_64 lint all passed. The remaining publication steps are:

- Publish the immutable `v0.9.0` GitHub release from the frozen tag.
- Download all release assets and `SHA256SUMS.txt`, then verify package contents, hashes, release integrity, and GitHub build attestations.
- Run the packaged server's same-version update check against the published release metadata.
- Change both frozen records to `finalReleaseVerification: true` only after those checks pass.

Windows runtime execution remains a declared coverage limitation, not an implied success.
