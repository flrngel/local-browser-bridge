# Version 0.11.0 evidence

Recorded 2026-08-20 in a real browser, against the **published v0.11.0 release artifacts** downloaded from the immutable GitHub release, checksum-verified against `SHA256SUMS.txt` and verified with `gh attestation verify` (exit 0 for all four assets). **21 of 21 live checks passed**
([`browser/results.json`](browser/results.json),
[`browser/rig.log`](browser/rig.log), driver
[`browser/evidence-rig.mjs`](browser/evidence-rig.mjs)).

This bundle exists to settle two questions the 0.10 bundle left open: whether
the dialog defect it found is really fixed, and whether the new cross-origin
frame support works outside its test harness.

## Environment

| Item | Value |
|---|---|
| Server | packaged `local-browser-bridge 0.11.0` from `local-browser-bridge-v0.11.0-macos-universal.tar.gz` |
| Extension | packaged `Local Browser Bridge 0.11.0` from `local-browser-bridge-extension-v0.11.0.zip`, loaded unpacked |
| Browser | Google Chrome for Testing 152.0.7977.54, disposable profile |
| Parent page | `http://localhost:8099/parent.html` ([fixture](browser/fixtures/parent.html)) |
| Child frame | `http://127.0.0.1:8100/child.html` ([fixture](browser/fixtures/child.html)) |

`localhost` and `127.0.0.1` are different sites, so Chrome puts the child in its
own renderer process. The frame under test is a genuine out-of-process iframe,
not a same-origin one.

## Cross-origin frames now work end to end

Previously the audit recorded five CDP behaviours as harness-proven only. This
run exercises all of them together in a live browser.

| Claim | Evidence |
|---|---|
| OOPIFs are discovered and merged | `frames.observed`: one frame, `crossOrigin: true`, `urlOrigin http://127.0.0.1:8100`, `depth 1`, `offset {x:250, y:123}`, `mode cdp-auto-attach` |
| Frame elements join the observation | `frames.elements-merged`: the child's button and text field appear as `…f1.e1` and `…f1.e2` with their own origin |
| Frame ref grammar | `frames.ref-grammar`: `mt1487ip-940w0uxx.f1.e1` |
| Coordinates are translated into the top-level viewport | the child button reports `bounds {x:266, y:187}`, its frame-local position plus the frame offset |
| **A trusted click really lands inside the cross-origin frame** | `frames.trusted-click-landed-in-child`: the child document recorded **`child-click:true`**, i.e. `event.isTrusted === true` inside the OOPIF |

[`browser/11-cross-origin-click.png`](browser/11-cross-origin-click.png) shows
the moment: the synthetic cursor is drawn over the child button, the child
prints `child-click:true`, and the control pill reads `turn 3 · move 1`.

Because the click landed on the intended target, the assumptions behind it are
confirmed in practice: `Target.attachedToTarget` reaches `chrome.debugger` with
a usable `sessionId`, `sendCommand({tabId, sessionId})` routes to the child,
`DOM.getBoxModel`'s content quad is in the parent's viewport space, root-session
`Input.dispatchMouseEvent` hit-tests into the OOPIF renderer, and the isolated
world survived long enough to prove the target.

## The 0.10 dialog defect is fixed

The 0.10 bundle found that a JavaScript dialog revoked the browser-control
lease through `document_changed:screenshot completion`, leaving
`page.handleDialog` unusable. The identical scenario now behaves correctly:

| Step | 0.10.0 | 0.11.0 |
|---|---|---|
| `pendingDialog` visible to the client | never (`null`) | `{type: confirm, message: "bridge dialog regression", hasPrompt: false}` |
| `page.observe` during the dialog | `CONTROL_REVOKED` | `409 BLOCKED_BY_DIALOG`, taxonomy `blocked_by_dialog` |
| Lease while the dialog is open | revoked | **still active** |
| `page.handleDialog` | `NO_PENDING_DIALOG` | succeeds, lease still active |
| `page.observe` afterwards | impossible | works, `turn 6` |
| Authorized navigation afterwards | — | works |

[`browser/12-dialog-open-lease-alive.png`](browser/12-dialog-open-lease-alive.png)
shows the dialog open while the control pill is still present, which is exactly
what the 0.10 screenshot could not show.

Fixing it required guarding four separate revocation routes, three of which the
contract suite never exercised: the document-identity check at every boundary
including screenshot completion, the control-overlay acknowledgement, the
held-input release after a click whose own handler opened the dialog, and the
service-worker restart recovery path. The guarantee is deliberately narrow: a
pending dialog suppresses *revocation*, never a check. A document change that
happens while a dialog is open is still caught by the first check after the
dialog is resolved.

## Still not proven here

- **Chrome Cancel and the in-page Stop button** need a real mouse click on
  browser- or page-owned UI; the automation host has no macOS assistive access.
  Covered by the 0.9 bundle and the contract suite.
- **Chrome's native debugger infobar** is suppressed by Chrome for Testing. It
  was captured separately against stock Chrome 151 for 0.10 and the attachment
  logic is unchanged in 0.11; see
  [../v0.10.0/browser/01-stock-chrome-native-warning-and-page-pill.png](../v0.10.0/browser/01-stock-chrome-native-warning-and-page-pill.png).
- **Nested and same-process frames**, and fill/select inside a frame, which 0.11
  deliberately refuses with `FRAME_ACTION_UNSUPPORTED`.
- **The desktop computer-use helper**, which this bundle does not cover.
