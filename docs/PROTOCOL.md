# Bridge protocol

Protocol version: `1`. Package version examples below use `0.11.0`.

## Transport and trust boundary

- HTTP/SSE control surface: `http://127.0.0.1:17373`
- Host validation: every HTTP request and WebSocket upgrade—including `/health`, `/bridge`, and `/computer`—must carry a Host of exactly `127.0.0.1`, `localhost`, or `[::1]`, optionally suffixed with the actual bound port; anything else (wrong port, empty port, malformed brackets, other names) is rejected with HTTP 403 `HOST_REJECTED` before routing and before any authentication, as a DNS-rebinding defense
- Browser-extension WebSocket: `ws://127.0.0.1:17373/bridge` with an exact `chrome-extension://<32-character-id>` Origin
- Computer-helper WebSocket: `ws://127.0.0.1:17373/computer` with exact Origin `lbb-computer-helper://local`
- Connector authentication: token-free mutual HMAC-SHA256 challenge-response; query and Authorization credentials are rejected
- One active extension transport and one active helper transport; a new authenticated connection replaces the old connector of the same type
- Provisional authentication: three-second total deadline, 8 KiB text-message limit, four inbound frames, and four concurrent provisional sockets per connector
- JSON WebSocket message limit: 8 MB
- Command timeout: 15 seconds by default; timeout emits an exact-session cancel and returns `COMMAND_OUTCOME_UNKNOWN`, never a retry-safe success/failure claim
- Server-to-connector queue: 64 messages; saturation returns an overload error instead of growing without bound

The shared token is the raw 32-byte HMAC key after canonical base64url-no-pad decoding. It never appears in a WebSocket URI or header and is never sent on the socket. Origin validation narrows the transport source; mutual proof authenticates both peers and binds every accepted message to one fresh server-created connection session. An unauthenticated provisional socket never attaches to the hub or replaces a ready connector. Browser-control leases and computer frames add narrower action authority inside that transport session.

## Mutual connector authentication

The connector sends only a fresh 32-byte nonce before proving the server. Nonces and proofs use canonical URL-safe base64 without padding (43 characters):

```json
{
  "type": "authHello",
  "authVersion": 1,
  "connector": "browser-extension",
  "clientNonce": "<32 random bytes, base64url-no-pad>"
}
```

The server creates a fresh UUID session and server nonce. Its proof is HMAC-SHA256 over these exact UTF-8 bytes, with LF separators and no trailing LF:

```text
LBB-WS-AUTH-V1
server
browser-extension
<sessionId>
<clientNonce>
<serverNonce>
```

```json
{
  "type": "authChallenge",
  "authVersion": 1,
  "connector": "browser-extension",
  "sessionId": "82b6b311-f71d-4a88-ae07-0b5e7a897815",
  "clientNonce": "<exact client nonce>",
  "serverNonce": "<32 random bytes, base64url-no-pad>",
  "serverProof": "<HMAC-SHA256, base64url-no-pad>"
}
```

The connector verifies the echoed fresh nonce and server proof in constant time before reading browser/native state or emitting a keyed value. Its response proof changes only the role line:

```text
LBB-WS-AUTH-V1
client
browser-extension
<sessionId>
<clientNonce>
<serverNonce>
```

```json
{
  "type": "authResponse",
  "authVersion": 1,
  "connector": "browser-extension",
  "sessionId": "82b6b311-f71d-4a88-ae07-0b5e7a897815",
  "clientNonce": "<exact client nonce>",
  "serverNonce": "<exact server nonce>",
  "clientProof": "<HMAC-SHA256, base64url-no-pad>"
}
```

The server verifies the response in constant time and only then attaches that exact `sessionId` to the connector hub. The helper uses identical envelopes with connector `computer-helper`. Role, connector, session, and both nonces are domain-separated, so a captured proof cannot cross roles, connectors, sessions, or fresh connection attempts.

## Negotiated WebSocket envelope

After mutual authentication succeeds, the server sends the normal welcome for the already-authenticated `sessionId`:

```json
{
  "type": "welcome",
  "protocolVersion": 1,
  "sessionId": "82b6b311-f71d-4a88-ae07-0b5e7a897815",
  "serverVersion": "0.11.0",
  "connector": "browser-extension"
}
```

The connector must validate `protocolVersion`, `serverVersion`, `sessionId`, and `connector` before sending its hello. The browser extension then replies:

```json
{
  "type": "hello",
  "protocolVersion": 1,
  "sessionId": "82b6b311-f71d-4a88-ae07-0b5e7a897815",
  "sequence": 81,
  "controllerSequence": 81,
  "controllerId": "38a72d1f-d124-4335-8f1e-9cb85777df14",
  "connectionId": "2f9ad9af-5bb7-42b3-a77d-a0c83a625792",
  "version": "0.11.0",
  "browser": "Google Chrome",
  "mode": "full-access",
  "capabilities": ["tabs.list", "page.observe", "browser.control.start"]
}
```

`controllerId` is the extension installation's persisted identity. `connectionId` changes on each extension WebSocket attempt. Neither replaces the server-created `sessionId`; the server accepts commands and events only in the current server session.

The computer helper uses the same negotiated envelope and reports its bounded native surface:

```json
{
  "type": "hello",
  "protocolVersion": 1,
  "sessionId": "d559c7b3-56fb-49e6-b661-801cfcb8807f",
  "version": "0.11.0",
  "platform": "macos",
  "architecture": "aarch64",
  "backend": "background-window/skylight+cgwindow",
  "sessionMode": "background-window",
  "inputReady": true,
  "semanticReady": true,
  "capabilities": [
    "computer.status",
    "computer.observe",
    "computer.share.start",
    "computer.click"
  ]
}
```

The server intersects advertised capabilities with its compiled allowlist and sends `helloAck`. `connected` becomes true only after all three compatibility checks pass:

- exact package version;
- exact protocol version;
- exact server-created session ID.

A mismatch yields `EXTENSION_PROTOCOL_MISMATCH` or `COMPUTER_PROTOCOL_MISMATCH`, clears effective capabilities, and blocks commands. A transport connection by itself is not a completed handshake.

## Command, result, and event ordering

Server command:

```json
{
  "id": "e7bd7043-9395-41f7-9c70-d05bfcb0e676",
  "type": "command",
  "protocolVersion": 1,
  "sessionId": "82b6b311-f71d-4a88-ae07-0b5e7a897815",
  "sequence": 17,
  "method": "page.observe",
  "params": { "tabId": 42 }
}
```

Successful result:

```json
{
  "id": "e7bd7043-9395-41f7-9c70-d05bfcb0e676",
  "type": "result",
  "protocolVersion": 1,
  "sessionId": "82b6b311-f71d-4a88-ae07-0b5e7a897815",
  "sequence": 17,
  "ok": true,
  "result": {}
}
```

Failed result:

```json
{
  "id": "e7bd7043-9395-41f7-9c70-d05bfcb0e676",
  "type": "result",
  "protocolVersion": 1,
  "sessionId": "82b6b311-f71d-4a88-ae07-0b5e7a897815",
  "sequence": 17,
  "ok": false,
  "error": {
    "code": "STALE_CONTROL_TURN",
    "message": "Observe the current browser turn before acting"
  }
}
```

Commands are serialized by each connector and have a strictly increasing server sequence. A result must echo the command's exact `id`, `sessionId`, `protocolVersion`, and `sequence`; a stale or cross-connection result resolves as a protocol violation rather than completing a pending call.

If the server deadline expires, it removes the pending result and sends a cancel bound to the same command identity:

```json
{
  "id": "e7bd7043-9395-41f7-9c70-d05bfcb0e676",
  "type": "cancel",
  "protocolVersion": 1,
  "sessionId": "82b6b311-f71d-4a88-ae07-0b5e7a897815",
  "sequence": 17,
  "reason": "command_timeout"
}
```

The connector checks cancellation before side-effect boundaries and during bounded multi-step input, performs best-effort held-input cleanup, and never lets a late result satisfy another request. Because a cancel can race an already dispatched operating-system or CDP event, the client receives `COMMAND_OUTCOME_UNKNOWN` and must observe before making a new decision; it must not automatically retry.

Unsolicited connector events use an independent monotonic `eventSequence`:

```json
{
  "type": "event",
  "protocolVersion": 1,
  "sessionId": "82b6b311-f71d-4a88-ae07-0b5e7a897815",
  "eventSequence": 9,
  "name": "browser.control.revoked",
  "data": {
    "reason": "canceled_by_user",
    "requiresExplicitStart": true
  }
}
```

Duplicate, decreasing, wrong-session, and pre-handshake events are ignored. Supported event names include approval resolution, browser-control start/revocation, and computer-share frame/error notifications.

## Browser-control lease model

There are five deliberately separate freshness values:

| Value | Owner | Changes when | Protects against |
|---|---|---|---|
| WebSocket `sessionId` | Server | Connector socket is replaced or reconnects | Cross-connection messages and stale results |
| `controlSessionId` | Extension | A tab control lease starts | Actions from a released or different controlled tab |
| `turn` | Extension | `page.observe` completes in the lease | Acting from an older observe/act turn |
| `generation` | Content script | A DOM observation is created | Stale element and coordinate references after mutation, scroll, or resize |
| `moveSequence` | Extension | The synthetic browser pointer moves | Actions based on an older pointer state |

`browser.control.start` holds one `chrome.debugger` attachment on one tab. The default lease is five minutes, accepted `ttlMs` values are 15 seconds through 15 minutes, and a ten-second heartbeat verifies the attachment. Holding the attachment has three intentional effects:

1. Chrome keeps its native **Local Browser Bridge started debugging this browser** warning visible.
2. Chrome keeps the Manifest V3 service worker alive while the debugger is attached on supported Chrome versions.
3. Trusted CDP actions cannot silently fall back after debugger loss.

The extension also injects a separate page-owned pill, Stop button, and synthetic pointer. That content overlay is not Chrome's native warning.

Chrome's Cancel action produces `chrome.debugger.onDetach` with `canceled_by_user`. DevTools attachment takeover, target closure, lease expiry, heartbeat failure, bridge pause/disconnect, the in-page Stop button, and the extension popup's **Release control** also end authority. Every unexpected detach is a hard revocation: it hides the overlay, emits `browser.control.revoked`, and requires a new explicit lease. Chrome Cancel, the in-page Stop button, and popup **Release control** additionally persist a global human-pause latch across service-worker and browser restarts. While latched, every remote browser mutation—including `browser.control.start` and tab creation, activation, or closure—is rejected on every tab. Only **Resume** invoked from the extension's own popup can clear that latch; the bridge protocol and dashboard cannot. An in-flight trusted click fails and is never retried with DOM `.click()`.

Strong clients include `controlSessionId`, `turn`, and `moveSequence` returned by the last observation/control status in each action. The content-script `generation` remains mandatory for snapshot-bound DOM and direct-coordinate operations.

Element references embed the observation generation: `page.observe` returns refs shaped like `<generation>.e12`, or `<generation>.f2.e12` for an element inside a merged cross-origin frame. Acting with a ref whose embedded generation has been superseded fails with a coaching `STALE_REF` error before any element lookup, so a stale ref can never silently resolve against a newer snapshot. Legacy bare `e12` refs are still accepted and resolve against the current generation; the explicit `generation` parameter remains authoritative either way. Malformed refs are rejected by the server without being relayed.

## Browser commands

| Method | Parameters | Notes |
|---|---|---|
| `status` | none | Connection, policy mode, allowlist, and control-lease state |
| `browser.control.start` | `tabId`, optional `ttlMs` | Explicitly attaches to one permitted tab and returns lease state |
| `browser.control.status` | none | Returns active lease, turn, pointer sequence, and last revocation |
| `browser.control.stop` | optional `sessionId` | Detaches and releases the current lease; a supplied stale ID is rejected |
| `tabs.list` | none | Full Access returns controllable tabs; Safe mode returns allowlisted tabs; query/fragment removed |
| `tabs.activate` | `tabId` | Focuses a permitted tab |
| `tabs.new` | none | Creates `about:blank` in the named Local Browser Bridge tab group |
| `tabs.close` | `tabId` | Immediate in Full Access; popup approval in Safe mode |
| `page.observe` | `tabId` | Advances `turn`; returns screenshot, text, selected text, interactive refs, generation, and control state |
| `page.navigate` | `tabId`, `url`, optional lease bindings | Full Access permits HTTP(S) and file; Safe mode requires allowlisted HTTP(S) |
| `page.back` / `page.forward` / `page.reload` | `tabId`, optional lease bindings | Standard navigation under the held lease |
| `page.click` | `tabId`, `ref`, `generation`, optional `button`, `clickCount`, `modifiers`, lease bindings | Revalidates identity, geometry, hit target, and generation before trusted CDP input; `button` is left/middle/right, `clickCount` 1–3, `modifiers` a subset of Shift/Control/Alt/Meta |
| `page.hover` | `tabId`, `ref`, `generation`, optional lease bindings | Runs the full click target proof, then moves the trusted pointer to the element center without any press or release |
| `page.fill` | `tabId`, `ref`, `generation`, `text`, optional lease bindings | Full Access permits sensitive fields; Safe mode rejects them |
| `page.select` | `tabId`, `ref`, `generation`, `value`, optional lease bindings | Matches option value or label |
| `page.key` | `tabId`, `generation`, `key`, optional lease bindings | Snapshot-bound key/chord in either chord dialect; Safe mode accepts only bounded navigation keys |
| `page.scroll` | `tabId`, `generation`, `deltaX`, `deltaY`, optional lease bindings | Deltas clamped to ±5000 and invalidate the snapshot |
| `page.waitFor` | `tabId`, one or more of `text`, `textGone`, `urlPrefix`, `mutationQuietMs`, optional `timeoutMs` | Read-only condition wait; needs no control lease and keeps working during a human pause |
| `page.batch` | `tabId`, `generation`, `actions`, optional lease bindings | One to ten sequential click/fill/select/key/scroll steps bound to one generation; stops at the first failure |
| `page.handleDialog` | `tabId`, `accept`, optional `promptText` | Accepts or dismisses the recorded JavaScript dialog under the held lease |
| `page.clickAt` | `tabId`, `generation`, `x`, `y`, optional `button`, `clickCount`, `coordinateSpace`, lease bindings | Full Access only; revalidates the exact point target immediately before trusted input |
| `page.typeText` | `tabId`, `generation`, `text`, optional lease bindings | Full Access only; inserts text into the focused control |
| `page.evaluate` | `tabId`, `expression`, optional lease bindings | Full Access only; awaits a Promise for up to 12 seconds and returns a by-value result |

Open-shadow-root elements participate in observations. A mutation observer invalidates existing refs for page-owned DOM changes; mutations made only by the bridge control overlay are excluded. Cross-origin iframe elements are merged into the same observation; see [Cross-origin frames](#cross-origin-frames).

In Safe mode, a `page.click` that uses a non-left button, a click count above one, or any modifier routes through the existing risky-click approval path; Full Access executes it directly.

### Cross-origin frames

Elements inside a cross-origin (out-of-process) iframe are merged into the same `page.observe` result as the top document, with top-level coordinates, and can be clicked and hovered by ref.

**Ref grammar.** The published element-reference grammar is:

```
top-frame ref ::= [ generation "." ] element
frame ref     ::= generation "." frame "." element
generation    ::= [a-z0-9-]{1,64}
frame         ::= "f" ( [1-9] | 1[0-6] )      ; f1 .. f16
element       ::= "e" [1-9][0-9]{0,3}         ; e1 .. e9999
```

`<generation>` is always the **top** observation generation, so the existing `generation` request parameter and every existing lease binding keep working across the merged observation. A two-segment ref is always `<generation>.<element>`, never `<frame>.<element>` — a generation string that happens to look like `f2` is still a generation. Frame indexes outside `f1`..`f16` are rejected by the server before relay with HTTP 400 and taxonomy `invalid_request`.

**Element provenance.** An element that came from a frame carries four extra keys; a top-document element carries none of them:

| Field | Meaning |
|---|---|
| `frameRef` | The frame's `f<k>` key within this observation |
| `frameId` | Chrome's frame id for that document |
| `frameUrlOrigin` | The frame document's origin |
| `crossOrigin` | Always `true`; the server drops the flag on any element without a valid `frameRef`, so a page cannot forge cross-origin provenance onto a top element |

**Observation additions.** Two fields appear only when the page has something to report about frames — an owner element, an attached target, a merged frame, or a skip. A page with no iframes at all produces an observation byte-identical to one from before frame support:

```jsonc
{
  "frames": [
    {
      "ref": "f1",
      "frameId": "6A1B...",
      "urlOrigin": "https://pay.example.test",
      "crossOrigin": true,
      "depth": 1,
      "offset": { "x": 120, "y": 64 },
      "size": { "width": 380, "height": 220 },
      "elementCount": 12,
      "truncated": false
    }
  ],
  "frameSummary": {
    "supported": true,
    "mode": "cdp-auto-attach",
    "reason": "",
    "ownersSeen": 3,
    "attached": 1,
    "merged": 1,
    "elementsDropped": 0,
    "skipped": [{ "urlOrigin": "https://ads.example.test", "reason": "blank_document" }]
  }
}
```

`elementsTruncated: true` is added whenever the 250-element publication cap dropped an element that survived sanitization. That cap reserves 50 of its 250 slots for elements that came from a merged frame, mirroring the extension's own 400/100 reservation, because the extension appends frame elements *after* the top document's: a flat cap would publish a `frames` list whose `<generation>.f<k>.e<n>` refs appear nowhere in `elements`. Whichever class runs out of entries first hands its unused slots to the other, so the cap is always filled. `frames[].elementCount` is restated as the number of that frame's elements that actually reached `elements`, and `frames[].truncated` is set whenever the extension merged more than that.

`frameSummary.skipped` uses a closed vocabulary: `budget_frames`, `budget_elements`, `budget_time`, `blank_document`, `zero_size`, `offscreen`, `depth_exceeded`, `owner_unresolved`, `agent_install_failed`, `session_probe_failed`, `frame_timeout`, `same_process_frame`, `navigated_during_observation`. Every iframe owner in the top document that never produced its own frame target is reported as `same_process_frame`, so a frame the bridge cannot merge is always visible rather than silently missing.

**Coordinate translation.** `frameOffset(root)` is `(0, 0)` and `frameOffset(F) = frameOffset(parent(F)) + ownerContentTopLeft(F)`, where `ownerContentTopLeft(F)` is the top-left of the content box of `F`'s owner `<iframe>` measured in the parent frame's own viewport. Published element bounds are `frameLocal(element) + frameOffset(frame)`, so the pointer planner, the cursor overlay, and `page.clickAt` keep working in one top-level coordinate space. An element counts as in-viewport only when it is inside its own frame's viewport, inside the frame's visible box on the page, and inside the top-level viewport.

**Supported and refused actions.** `page.click` and `page.hover` accept a frame ref. `page.fill` and `page.select` take a `ref` but refuse a frame-scoped one with `FRAME_ACTION_UNSUPPORTED` (taxonomy `invalid_request`, non-retriable), because it is a stated capability boundary and not a malformed request. `page.key`, `page.scroll`, `page.typeText`, and `page.clickAt` accept no `ref` parameter at all — the server's per-method sanitizer drops any that is sent — so they can only ever act on the focused element or on top-level coordinates, and there is no frame-scoped form of them to refuse. `page.batch` may contain a `page.click` on a frame ref; that step re-runs the full frame proof like any other step, and a frame that navigates mid-batch fails that step with `FRAME_DETACHED` rather than `STALE_SNAPSHOT`.

**Proof.** A frame-scoped click re-runs the same two-phase proof the top document gets, plus a frame proof before and after the pointer moves: the frame's own loader identity; every ancestor frame's loader identity *and* a re-measurement of every ancestor's owner box, because an ancestor that moves shifts the target by the same delta while the target's own owner — measured inside that ancestor — does not move at all; a re-measurement of the frame's own owner box; and an exact hit test proving the owner `<iframe>` is the top element at the translated point. Every re-measurement is a 2 px tolerance on the *same* quantity: the accumulated offset comes from content-box origins, while the hit-test probe's `getBoundingClientRect()` is compared only against the border quad of the same box model, so a bordered or padded iframe is never mistaken for a moved one. Trusted input is always dispatched on the page target at the translated top-level point; nothing is ever dispatched into a child session and the in-frame agent never synthesizes an event.

**New error codes.**

| Code | Taxonomy | Retriable |
|---|---|---|
| `FRAME_DETACHED` | `document_changed` | yes, after a fresh observation |
| `STALE_FRAME_TREE` | `stale_snapshot` | yes, after a fresh observation |
| `FRAME_AGENT_STALE` | `stale_snapshot` | yes, after a fresh observation |
| `FRAME_AGENT_UNAVAILABLE` | `unavailable` | yes |
| `FRAME_AGENT_FAILED` | `unknown` | no |
| `FRAME_ACTION_UNSUPPORTED` | `invalid_request` | no |
| `FRAME_REF_MISROUTED` | `invalid_request` | no |

**Limits.** At most 16 attached frame targets per lease, at most 5 levels of nesting, at most 120 elements per frame, 500 elements total per observation with 400 reserved for the top document when at least one frame merged, 250 of those published with 50 reserved for frame elements, and at most 32 reported skips. A page with more than 400 interactive top-document elements *and* at least one merged frame therefore publishes fewer top elements than it did before frame support; `elementsTruncated` and `frameSummary.elementsDropped` report exactly that.

**Latency.** Observing frames is read-only — no input, no navigation, no value write — so it is bounded rather than trusted: the whole frame pass shares a 4 s budget, every CDP command inside it is bounded by what is left of that budget, and running out costs the frames that did not fit (`frame_timeout` for a frame that stopped answering, `budget_time` for a frame the pass never reached) instead of the lease. A slow third-party iframe can therefore never revoke browser control or fail an observation. A change to the lease itself — the top document navigating, a dialog opening, control being canceled — still fails the observation, because it invalidates all of it and not just one frame's contribution.

**What is and is not proven.** The ref grammar, the sanitizers, the coordinate translation, the merge and budget arithmetic, the fail-closed staleness paths, and the in-frame agent's proofs are covered by the Rust and Node contract suites. Five behaviours are proven only against those harnesses and still require a live-Chrome run before cross-origin merging should be relied on: that `Target.attachedToTarget` reaches the extension for OOPIFs with a populated `sessionId`; that `chrome.debugger.sendCommand({tabId, sessionId})` routes to the child session; that `DOM.getBoxModel`'s content quad is in the parent frame's viewport space; that `Input.dispatchMouseEvent` on the page target hit-tests into the OOPIF renderer; and that the isolated world survives a frame's same-document navigations. Each degrades to a refusal rather than a wrong click. In particular, the first child command of every lease is a discriminating routing probe: the child session must return its own frame id, and a Chrome whose `chrome.debugger` ignores the `sessionId` key answers for the root frame instead, at which point frame support is disabled for the lease (`frameSummary.supported: false`, `reason: "session_routing_unverified"`), every attached record is dropped, and the observation is exactly what it would have been without frame support. `minimum_chrome_version` stays `118`.

### Condition waits

`page.waitFor` requires at least one condition. `text` and `textGone` match a substring of the rendered body text or the document title, `urlPrefix` matches the start of `location.href`, and `mutationQuietMs` (250–5000) requires the document revision—mutations, scrolls, and resizes—to stay unchanged for that long. The content script polls roughly every 250 ms. `timeoutMs` defaults to 5000 and is clamped into 100–12000. Success returns `{satisfied: true, elapsedMs, conditions}` with the final boolean for each requested condition. An unmet wait fails with `WAIT_TIMEOUT` (HTTP 409); the failure is deliberately classified as non-retriable with a `reobserve` hint, because a timed-out wait is a normal outcome that calls for a fresh observation, not a repeat of the same wait. The command never takes or touches a control lease and dispatches no input. While a JavaScript dialog is pending it fails fast with `BLOCKED_BY_DIALOG` like every other renderer-touching command, because the dialog-frozen renderer cannot answer the condition poll.

### Batched actions

`page.batch` accepts one to ten sub-actions drawn only from `page.click`, `page.fill`, `page.select`, `page.key`, and `page.scroll`. Nested batches and every other method are rejected before relay, naming the offending step index. The batch `tabId` and `generation` are authoritative for every step; a step supplying different values is rejected, per-step lease bindings are stripped, and the stored lease is bound exactly once at the top level. Steps run strictly sequentially, each re-running its full per-method proof; the first failing step stops the batch and no later step is dispatched. The result records `{completed, total, perStep, failedIndex?, failedError?}` followed by one automatic observation. Freshness is not weakened: a step that mutates the document invalidates the shared snapshot, so a later snapshot-bound step fails `STALE_SNAPSHOT` naturally—batches suit quiet-page patterns such as fill, fill, click. A JavaScript dialog opened by an earlier step aborts the batch at the next step's index with `BLOCKED_BY_DIALOG` before that step dispatches into the frozen page. In Safe mode a step that would need popup approval fails with `APPROVAL_REQUIRED` instead of queueing an approval mid-batch.

### JavaScript dialogs

When a controlled page opens an `alert`, `confirm`, `prompt`, or `beforeunload` dialog, the extension records it on the lease and the server publishes `pendingDialog {type, message, hasPrompt, at, tabId}` in `/api/state` (`null` when absent). An open dialog freezes the renderer main thread, so any content-script or `Runtime` call against that page would only time out. While a dialog is pending, only `status`, `tabs.list`, `browser.control.status`, `browser.control.stop`, and `page.handleDialog` proceed—each stays off the renderer—and every other browser command, including the read-only `page.observe` and `page.waitFor` and the delayed automatic post-action observation, fails fast with HTTP 409 `BLOCKED_BY_DIALOG` (taxonomy `blocked_by_dialog`) before anything is relayed. The extension enforces the same gate before any content or CDP dispatch, a `page.batch` checks the pending dialog before each sub-step and aborts at that index, a content or CDP timeout that races a dialog opening resolves as `BLOCKED_BY_DIALOG` instead of revoking control, and the heartbeat suspends its `Runtime.evaluate` renderer probe (keeping the browser-side attachment and document checks) while the dialog is pending.

A dialog can also open *after* a command has passed both gates, while an observation is already in flight, or inside the click handler of the very element the agent just acted on. Every boundary that would revoke the lease—document-identity verification at each preparation, dispatch, and completion point, screenshot completion included, the control-indicator boundary, and the held mouse-button or key release that every trusted click and key press runs in its `finally`—rereads the pending-dialog record immediately before revoking: under a dialog the failure resolves as `BLOCKED_BY_DIALOG` and the lease survives, and only a failure observed with no dialog pending still revokes. The observation path rereads it at its start and again after the capture, so a snapshot taken across a dialog is discarded rather than published. A service-worker restart recovers such a lease rather than revoking it: `pendingDialog` is persisted with the lease, the document identity is verified from the browser process, and only the overlay repaint is deferred to the first heartbeat after the dialog is resolved. **No renderer-blocked probe, dispatch, input release, or recovery can therefore revoke the browser-control lease while a dialog is pending, by timeout or by document identity.** The restart's own fail-closed rules are unchanged and still run first: a recovered candidate carrying an unfinished navigation, an unreleased held input, or a document that cannot be verified at all is dropped, dialog or not. What else still ends a lease is what the dialog does not cause: an explicit stop, a human pause, the lease's own TTL expiring, and any failure observed with no dialog pending. Nothing is forgiven permanently: the record is cleared unconditionally once the dialog is resolved, so a document that really did change while the dialog was open is caught by the very next check and revokes then, and a release the dialog blocked is re-dispatched the moment the dialog closes—by `page.handleDialog` or by the user—and revokes then if it is still unacknowledged. Server-side, a relayed observation that comes back `BLOCKED_BY_DIALOG` is logged as a skipped observation and changes no published state.

`page.handleDialog` requires the held lease and a recorded dialog (`NO_PENDING_DIALOG` otherwise); `promptText` (up to 1000 characters) is forwarded only when accepting a prompt, and `accept: false` is the safe default for `beforeunload`. It is bound to the control session but, uniquely, not to an observation `turn` or `moveSequence`: refreshing those needs an observation the dialog itself forbids, and a discarded observation can leave the extension a turn ahead of the published state. Dialog close, lease start or revocation, and connector reconnect all clear the pending state.

### Key chord grammar

`page.key` and `computer.key` accept two chord dialects: canonical (`Meta+L`) and lowercase vendor style (`ctrl+shift+t`, `cmd+l`). The server normalizes both before relaying—modifier aliases resolved (`ctrl`→Control, `cmd`/`command`/`win`/`super`→Meta, `alt`/`option`→Alt), modifiers deduplicated and emitted in a fixed order (Alt, Control, Meta, Shift), and key aliases such as `esc`, `return`, `del`, `space`, and arrow names mapped to their canonical CDP names—so equivalent chords share one spelling and connectors only ever see canonical input. A bare single letter keeps its case verbatim (`j` stays `j`, as v0.9 relayed it, so Gmail-style shortcuts never gain an implied Shift), while a letter inside a modifier chord canonicalizes to uppercase (`shift+j`→`Shift+J`, `ctrl+j`→`Control+J`). Unknown tokens or more than three modifiers are rejected with a coaching `BAD_REQUEST`.

## Native computer commands

| Method | Parameters | Notes |
|---|---|---|
| `computer.status` | none | Platform, backend, target windows, permission/input readiness, pointer state, share state, and current-frame status |
| `computer.share.start` | `windowId`, optional `fps` | Starts a bounded exact-window frame feed; default 4 FPS, accepted range 1–10 |
| `computer.share.status` | none | Returns share ID, window ID, frame sequence, FPS, capture scope, and backpressure policy |
| `computer.share.stop` | none | Stops the active share and returns its ID |
| `computer.observe` | optional `windowId` | Captures one exact application window without including unrelated desktop windows |
| `computer.move` | `frameId`, `x`, `y`, optional `durationMs`, `coordinateSpace` | Routes a bounded synthetic trajectory to the exact window; never moves the hardware cursor |
| `computer.click` | `frameId`, `x`, `y`, optional `button`, `clickCount`, `durationMs`, `coordinateSpace` | Moves the synthetic pointer, then sends exact-window left/middle/right input |
| `computer.drag` | `frameId`, `fromX`, `fromY`, `toX`, `toY`, optional `durationMs`, `coordinateSpace` | Left-button drag; duration is 50–2000 ms |
| `computer.scroll` | `frameId`, `x`, `y`, `deltaX`, `deltaY`, optional `coordinateSpace` | Routes pointer attention, then exact-window scroll with deltas clamped to ±50 |
| `computer.typeText` | `frameId`, `text` | Routes Unicode text to the exact target process/window |
| `computer.key` | `frameId`, `key` | Sends one named key or a bounded chord in either dialect, such as `Meta+L` or `ctrl+shift+t` |
| `computer.invoke` | `frameId`, `elementRef`, optional `action` | Invokes an advertised frame-bound accessibility action and reports an observed postcondition |
| `computer.setValue` | `frameId`, `elementRef`, `value` | Writes through the platform accessibility value pattern and requires read-back or masked-length proof |

An observation is shaped like:

```json
{
  "frame": {
    "id": "6414ca63-6e23-4a13-9358-fffd19cba95d",
    "capturedAt": "2026-08-18T00:00:00Z",
    "windowId": "47782",
    "pid": 51641,
    "appName": "Example App",
    "windowTitle": "Document",
    "imageWidth": 1209,
    "imageHeight": 826,
    "windowX": 180,
    "windowY": 768,
    "windowWidth": 1440,
    "windowHeight": 984,
    "transportScaleX": 0.8395833333,
    "transportScaleY": 0.8394308943,
    "sessionMode": "background-window",
    "deliveryMode": "exact-window-background",
    "pointer": {
      "id": "6bd182be-f4ca-4737-921a-08661110b55f",
      "visible": true,
      "imageX": 637.0,
      "imageY": 312.0,
      "sequence": 4,
      "coordinateSpace": "image-pixels",
      "style": { "theme": "lbb.session-pointer.v1", "hotspot": "tip" }
    },
    "share": {
      "active": true,
      "id": "ca1d7349-6c48-4d15-9cf5-6bd291dce7da",
      "sequence": 12,
      "fps": 4,
      "droppedFrames": 0,
      "ackPaced": true,
      "lastAckedSequence": 11,
      "backpressure": "latest-frame-wins"
    },
    "elements": [
      {
        "ref": "a1",
        "role": "AXButton",
        "name": "Continue",
        "sensitive": false,
        "valueRedacted": false,
        "enabled": true,
        "actions": ["press"],
        "coordinateSpace": "image-pixels",
        "bounds": { "x": 487.0, "y": 262.0, "width": 77.0, "height": 27.0 },
        "screenBounds": { "x": 760.0, "y": 1080.0, "width": 92.0, "height": 32.0 }
      }
    ]
  }
}
```

Agent-supplied coordinates and semantic `bounds` use delivered image pixels. `screenBounds` are diagnostic OS coordinates. The two transport scales are explicit because a resized image can have slightly different X and Y ratios after integer rounding.

### Coordinate spaces

Coordinate commands accept an optional `coordinateSpace`. The default is the existing pixel space (`image` for computer commands, `viewport` for `page.clickAt`); `normalized1000` instead expresses each coordinate as 0–1000 across the current frame. The server converts normalized values against the stored sanitized frame's `imageWidth`/`imageHeight` (or, for `page.clickAt`, the last browser observation's viewport) and clamps to the last addressable pixel—the boundary value 1000 converts to `extent - 1`, because connectors validate coordinates as strictly inside the frame—so connectors only ever receive pixels and `coordinateSpace` is never relayed. Without a stored frame or observation to convert against, the command fails with `NO_COMPUTER_FRAME` or `NO_BROWSER_OBSERVATION` instead of guessing. To let a client pin decisions to an exact frame, `/api/state` exposes a SHA-256 `contentHash` plus decoded `screenshotWidth`/`screenshotHeight` for both the browser observation and the computer observation.

Native password elements are always emitted with `sensitive: true`, `valueRedacted: true`, no `value`, and no `setValue` action. macOS classifies secure AX roles and subroles before reading `AXValue`; Windows reads `CurrentIsPassword` before acquiring a value pattern and treats an unreadable password state as sensitive. The server repeats this redaction when sanitizing helper payloads.

The synthetic pointer is helper-session state, not the hardware cursor. Its bounded cubic Bézier/minimum-jerk trajectory is delivered to the exact window, and its final state is composited into subsequent exact-window PNGs. It is not a native click-through desktop overlay in version 0.11.

`computer.share.frame` events carry the same observation shape with a monotonically increasing share sequence. Frame pacing is negotiated at hello time: a helper that advertises the `computer.share.ack` capability (a feature flag, not a dispatchable method) receives `"shareAck": true` in the server's `helloAck` and switches to ack-paced, latest-frame-wins delivery. The helper then keeps a single-slot mailbox: a newer capture replaces an unemitted frame and increments the share's monotonic `droppedFrames` counter, and the next frame is emitted only after the server acknowledges the previous one. After sanitizing and storing each share frame, the server sends an acknowledgement bound to the session and the exact share sequence:

```json
{
  "type": "eventAck",
  "protocolVersion": 1,
  "sessionId": "d559c7b3-56fb-49e6-b661-801cfcb8807f",
  "name": "computer.share.frame",
  "sequence": 12
}
```

Stale, duplicate, and unknown ack sequences are ignored, and share sequences stay strictly increasing across dropped frames. Share status and the `share` block inside observations report `droppedFrames`, `ackPaced`, `lastAckedSequence`, and `backpressure`: `latest-frame-wins` when pacing is negotiated, `producer-blocking` when either side lacks the capability—in which case behavior is exactly the pre-0.10 timer feed and a legacy helper is never sent a message type it cannot parse. A replaced WebSocket session must renegotiate pacing.

To avoid a 1–10 FPS render/action race, the helper keeps a bounded recent-frame lease: a rendered `frameId` remains usable for at most three seconds only while the current share, PID, native window ID, and complete window geometry still match. Everything else is stale. This feed is repeated exact-window capture, not an OS virtual display, remote-desktop stream, or isolated input session.

The helper re-enumerates the exact `(pid, native window id)` target before input and returns `COMPUTER_STALE_FRAME` if identity or geometry changed. It snapshots foreground process, user focus, hardware cursor, and active desktop around delivery and returns `COMPUTER_BACKGROUND_CONTRACT_VIOLATION` if the non-interruption invariant fails. There is no implicit global-HID or foreground fallback. The server serializes actions and requests a new exact-window observation after successful input.

Legacy `displayId` and display-shaped aliases identify the selected window, not a physical display, and remain deprecated compatibility fields.

## REST API

`POST /api/v1/command` exposes the same allowlisted methods. Bearer-token clients receive server-sanitized results; they never speak the connector WebSocket envelope directly. The localhost UI exchanges the master bridge token for an expiring random `Session` authorization capability and CSRF value. The capability is kept in the exact port origin's `sessionStorage`, not a host-wide localhost cookie. `/api/state`, `/api/events`, `/api/screenshot`, and `/api/computer/screenshot` require that session or a bearer token; mutations additionally require same-origin and CSRF proof.

### Idempotent command replay

`POST /api/v1/command` accepts an optional `callId` (1–128 characters). Admission is atomic: while a command with that `callId` is in flight, a duplicate returns HTTP 409 `CALL_IN_PROGRESS` and nothing is dispatched twice. After completion, the exact final body and HTTP status—success or failure, with `callId` echoed top-level—are cached (256 entries, ten-minute TTL); a replay returns the cached response with `"replayed": true` without touching any connector. If the HTTP client disconnects before a command's outcome is recorded, the action may still execute, so the `callId` is completed with a cached HTTP 504 `COMMAND_OUTCOME_UNKNOWN` failure (taxonomy `outcome_unknown`, hint `reobserve`): retries of that `callId` replay this failure and never re-dispatch, and the caller must observe before acting again. Each registration is fingerprinted over the method and canonical parameters; reusing a `callId` for a different command returns HTTP 409 `CALL_ID_REUSED` (taxonomy `invalid_request`) instead of replaying the other command's outcome. The bridge has exactly one bearer token, so all commands share one replay namespace.

### Error taxonomy

Every failed JSON API response carries a `taxonomy` object next to the untouched legacy `error`:

```json
{
  "error": {
    "code": "STALE_SNAPSHOT",
    "message": "The page changed after the last observation"
  },
  "taxonomy": {
    "code": "stale_snapshot",
    "retriable": true,
    "recoveryHint": "reobserve",
    "prose": "The snapshot or frame you acted on is no longer current; observe again and retry with fresh identifiers."
  }
}
```

The canonical codes are `stale_snapshot`, `stale_ref`, `target_changed`, `out_of_bounds`, `not_interactable`, `obscured`, `document_changed`, `lease_lost`, `needs_user`, `blocked_by_policy`, `blocked_by_dialog`, `sensitive_field`, `outcome_unknown`, `timeout`, `wait_timeout`, `overloaded`, `protocol_mismatch`, `unavailable`, `invalid_request`, and `unknown`. Recovery hints stay within `reobserve`, `wait`, `resume`, `handback`, `reconnect`, and `none`. Every legacy code the server, extension, and helper emit is classified; an unmapped code collapses to `unknown` and is never marked retriable. `wait_timeout` is deliberately non-retriable with a `reobserve` hint. Classification happens only in the server: the connector WebSocket error envelope still carries just `{code, message}`, and `/api/state` activity entries carry human messages without taxonomy.

The native allowlist intentionally contains no shell, filesystem, process-launch, clipboard, downloader, arbitrary-code, credential-store, user-management, or telemetry method.
