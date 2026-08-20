# Version 0.11.0 evidence

Recorded 2026-08-20 in a real browser, against the **published v0.11.0 release artifacts** downloaded from the immutable GitHub release, checksum-verified against `SHA256SUMS.txt` and verified with `gh attestation verify` (exit 0 for all four assets). Two runs, **32 of 32 live checks passed**:

| Run | Checks | Records |
|---|---|---|
| Cross-origin frames and the dialog/lease repair | 21 of 21 | [`browser/results.json`](browser/results.json), [`browser/rig.log`](browser/rig.log), driver [`browser/evidence-rig.mjs`](browser/evidence-rig.mjs) |
| The in-page Stop button, clicked for real | 11 of 11 | [`browser/stop-results.json`](browser/stop-results.json), [`browser/stop-rig.log`](browser/stop-rig.log), driver [`browser/stop-button-rig.mjs`](browser/stop-button-rig.mjs) |

This bundle exists to settle three questions earlier bundles left open: whether
the dialog defect 0.10 found is really fixed, whether the new cross-origin
frame support works outside its test harness, and whether the in-page Stop
button — unproven since 0.9 because it needs a real click on page-owned UI —
actually stops the bridge.

## Environment

| Item | Value |
|---|---|
| Server | packaged `local-browser-bridge 0.11.0` from `local-browser-bridge-v0.11.0-macos-universal.tar.gz` |
| Extension | packaged `Local Browser Bridge 0.11.0` from `local-browser-bridge-extension-v0.11.0.zip`, loaded unpacked |
| Browser | Google Chrome for Testing 152.0.7977.54, disposable profile |
| Parent page | `http://localhost:8099/parent.html` ([fixture](browser/fixtures/parent.html)) |
| Child frame | `http://127.0.0.1:8100/child.html` ([fixture](browser/fixtures/child.html)) |
| Stop-button page | the server's own `http://127.0.0.1:17373/demo` |

`localhost` and `127.0.0.1` are different sites, so Chrome puts the child in its
own renderer process. The frame under test is a genuine out-of-process iframe,
not a same-origin one.

The Stop-button run used the same packaged server binary and extension in a
second disposable profile. Its two screenshots are cropped to the top of the
screen so that an unrelated macOS window in the corner stays out of the
repository; nothing inside the browser window is cropped or edited.

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

## The in-page Stop button, proven with a real click

Since 0.9 this row had stayed unproven for one reason: the Stop button is
page-owned UI inside a closed shadow root, and a DOM `.click()` on it would
prove nothing — the bridge's own code could have produced it. This run clicks it
the way a person does, with `Input.dispatchMouseEvent` at the pill's viewport
coordinates, and no scripted call to the button at all. **11 of 11 checks
passed** ([`browser/stop-results.json`](browser/stop-results.json)).

The shadow root is closed, so the driver takes the pill's position from its
**host element's** bounding rect in the page main world — `{x: 851, y: 12,
width: 417, height: 37}` — and aims inside the right edge, at **(1242, 31)**.
That is a measurement, not an entry point: the closed root is never opened and
the button is never called.

| Claim | Evidence |
|---|---|
| The click was a real input event | `stop.click-dispatched`: `Input.dispatchMouseEvent` at `(1242, 31)`, explicitly not a DOM `.click()` |
| The lease was released | `stop.lease-revoked`: `active: false`, `reason: released_by_user` |
| A human Stop latches the global pause | `stop.human-pause-latched`: `humanPaused: true` |
| Remote control cannot restart itself | `stop.remote-restart-refused`: `browser.control.start` refused with `HUMAN_CONTROL_PAUSED`, taxonomy `needs_user` |
| Nor can it observe | `stop.remote-mutation-refused`: `page.observe` refused with the same code |
| The page overlay is gone | `stop.page-overlay-removed`: the host element is no longer in the DOM |

[`browser/20-before-stop.png`](browser/20-before-stop.png) shows the pill
reading **Local Browser Bridge is using this tab · turn 2 · move 0** with its
Stop button; [`browser/21-after-stop.png`](browser/21-after-stop.png) is the
same viewport after the click, with the pill gone.

Separately proven in the same run: **the bridge refuses to click its own Stop
button**. `page.clickAt` aimed at the pill answers `CONTROL_UI_OCCLUSION`
("release control with the visible Stop button or choose another point")
— check `bridge.refuses-to-click-its-own-stop`. The two halves belong together:
a human input event stops the bridge, and the bridge's own input events cannot.

### A defect this run found: the HTTP status of a human pause

Confirmed fixed on the packaged **0.11.1** build by re-running the same rig:
both refusals now answer **`423 Locked`** with taxonomy `needs_user`, and all
11 checks still pass ([`browser/stop-results-0.11.1.json`](browser/stop-results-0.11.1.json)).
The original `stop-results.json` keeps its `500`s verbatim as the record of the
defect rather than being edited to match the repaired code.


Both refusals above came back as **HTTP 500** with `HUMAN_CONTROL_PAUSED`
(visible in `stop-results.json`), which tells a REST client the local server
faulted and invites a retry, when the only thing that resolves the state is a
person pressing **Resume**. Version 0.11.1 derives the status from the error
taxonomy instead, so this exact response is now **423 Locked** with taxonomy
`needs_user` and hint `handback`. The 500s recorded in this bundle are the
0.11.0 behaviour, kept as the record of the defect.

## Still not proven here

- **Chrome's Cancel button** needs a real mouse click on *browser chrome*. No
  dispatched page input can reach it — `Input.dispatchMouseEvent` is delivered
  to a renderer, and the Cancel button belongs to the browser process — so this
  run's technique cannot prove it and it stays outstanding. Covered by the 0.9
  bundle and the contract suite.
- **The extension popup's Release control button**, the other human release
  surface, which this run did not press. Only the in-page Stop button was
  clicked here.
- **Chrome's native debugger infobar** is absent from this Chrome for Testing
  run. That is not proof that Chrome for Testing intrinsically suppresses the
  warning. It was captured separately against stock Chrome 151 for 0.10 and the
  attachment logic is unchanged in 0.11; see
  [../v0.10.0/browser/01-stock-chrome-native-warning-and-page-pill.png](../v0.10.0/browser/01-stock-chrome-native-warning-and-page-pill.png).
- **Nested and same-process frames**, and fill/select inside a frame, which 0.11
  deliberately refuses with `FRAME_ACTION_UNSUPPORTED`.
- **The desktop computer-use helper**, which this bundle does not cover.
