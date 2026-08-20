# Version 0.10.0 evidence

Recorded 2026-08-20 against the **published v0.10.0 release artifacts**, not a
developer build. The server binary came from
`local-browser-bridge-v0.10.0-macos-universal.tar.gz` and the extension from
`local-browser-bridge-extension-v0.10.0.zip`, both downloaded from the immutable
GitHub release, checksum-verified against `SHA256SUMS.txt`, and verified with
`gh attestation verify` (GitHub build provenance, exit 0).

Machine-readable results: [`browser/results.json`](browser/results.json).
Driver used to produce them: [`browser/evidence-rig.mjs`](browser/evidence-rig.mjs).
Raw transcript: [`browser/rig.log`](browser/rig.log).

**28 of 33 live checks passed. The five failures are one real defect, described
below, that only a live browser could expose.**

## Environment

| Item | Value |
|---|---|
| Server | packaged `local-browser-bridge 0.10.0` (macOS universal) |
| Extension | packaged `Local Browser Bridge 0.10.0`, loaded unpacked |
| Matrix browser | Google Chrome for Testing 152.0.7977.54, isolated profile |
| Indicator browser | Google Chrome 151.0.7922.138 (stock), isolated profile |
| Target page | the bridge's own `/demo` route, the only bridge-origin page the extension permits |

Two browsers were used deliberately. Stock Chrome 151 no longer honours
`--load-extension`, so the feature matrix ran in Chrome for Testing, which does
accept it. Chrome for Testing suppresses Chrome's own debugger infobar, so the
indicator evidence was captured separately in stock Chrome, where the packaged
extension was installed through the real **Load unpacked** flow.

Every profile was disposable and empty. No personal tab, account, or window
appears in any image.

## What was proven

| Scenario | Evidence |
|---|---|
| Native Chrome warning and page pill are independent, simultaneous surfaces | `browser/01-stock-chrome-native-warning-and-page-pill.png` — stock Chrome's own `"Local Browser Bridge" started debugging this browser` infobar with **Cancel**, above the extension's `Local Browser Bridge is using this tab · turn 0 · move 0 · Stop` pill |
| Epoch-embedded refs (0.10) | `refs.epoch-embedded`: every ref is `<generation>.eN`, e.g. `mt0yxbi2-fip667jr.e1`; `browser/02-observation-epoch-refs.jpg` |
| Condition waits (0.10) | `page.waitFor.satisfied` returned `{satisfied:true, elapsedMs:0}`; a miss returned `WAIT_TIMEOUT` with taxonomy `wait_timeout`/`reobserve`, and the lease stayed active |
| Trusted input, not synthetic DOM clicks | `page.click.trusted-event-observed`: the demo page recorded `coordinate:true`, i.e. `event.isTrusted === true` |
| Hover moves the pointer without clicking (0.10) | `page.hover` succeeded while the page's last-action state stayed `none` |
| Modifier and multi-button clicks (0.10) | `page.click.modifiers` with `Shift` dispatched and landed |
| Off-screen refusal then scroll recovery | `TARGET_OUT_OF_VIEWPORT` until `page.scroll` brought the target in; then `inViewport: true` |
| Batch actions (0.10) | `page.batch` reported `completed: 2` and the page held `{"name":"Ada","color":"blue"}`; `browser/03-batch-applied.jpg` |
| Normalized coordinates (0.10) | `page.clickAt` with `coordinateSpace: normalized1000` clicked the converted point |
| Idempotent callId (0.10) | identical replay returned `replayed: true` with a byte-identical result; reuse for a different command returned `409 CALL_ID_REUSED` |
| Error taxonomy (0.10) | a stale ref returned legacy `STALE_REF` plus `{code: stale_ref, retriable: true, recoveryHint: reobserve}` |
| Host-header hardening (0.10) | raw socket with `Host: evil.example` → `HTTP/1.1 403 Forbidden`; `Host: 127.0.0.1:17373` → `200 OK` |
| Release integrity | SHA-256 manifest check, archive/format inspection, and `gh attestation verify` all passed |

## Defect found by this run

**JavaScript dialogs still revoke the browser-control lease.** The 0.10 dialog
interception is defeated by the post-action observation path.

Reproduced identically in Chrome for Testing 152 and stock Chrome 151:

1. A page opens `confirm()` shortly after an action completes.
2. The extension does see it — the activity log records
   `JavaScript confirm dialog opened; browser commands are blocked until page.handleDialog resolves it`.
3. But the already-scheduled post-action auto-observe runs against the
   dialog-frozen renderer, its document-identity check fails, and the lease is
   hard-revoked with `document_changed:screenshot completion`.
4. `pendingDialog` therefore never becomes visible to a client, `page.observe`
   returns `CONTROL_REVOKED` instead of `BLOCKED_BY_DIALOG`, and
   `page.handleDialog` answers `NO_PENDING_DIALOG` — leaving the dialog open on
   the page for a human to dismiss.

`browser/06-dialog-revokes-lease-defect.png` shows the end state: the confirm
dialog is open while both the native Chrome infobar and the in-page pill have
disappeared, because the lease is gone.

The 0.10 work hardened the timeout paths (`CONTENT_TIMEOUT`, CDP timeouts, the
heartbeat probe) against pending dialogs, but this path fails through
`DOCUMENT_CHANGED` rather than a timeout, so it was not covered. The contract
tests pass because they inject `pendingDialog` before exercising the gate; the
live ordering — dialog opens *after* the observe is already in flight — is the
case only a real browser produces.

Tracked for the next release together with the remaining boundaries in
[../../docs/SOTA_AUDIT.md](../../docs/SOTA_AUDIT.md).

## Not proven here

- **Chrome Cancel and the in-page Stop button.** Both require a real mouse
  click on browser-owned or page-owned UI. The automation host lacks macOS
  assistive access, so no click could be synthesised. Their revocation paths
  remain covered by the 0.9 evidence and by the contract suite.
- **Desktop computer-use helper.** This bundle covers the browser surface only.
- **Cross-origin iframes.** Still unimplemented in 0.10; see the audit.
