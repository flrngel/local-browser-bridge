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

The extension also injects a separate extension-owned in-page pill, Stop button, and synthetic pointer. That content overlay is not Chrome's native warning. Both its public host identifier and a private marker retained inside the closed shadow root are freshly randomized for each document. Shadow-host rules use important resets for critical display, position, visibility, opacity, filter, mask, clip, transform, containment, pseudo-element, and backdrop properties; exact accessibility state and absence of an active View Transition are required. Lease start, same-tab action reuse, and capture restoration reopen the host and require bounded render/layout/computed-style plus document/closed-shadow hit tests (capture begin instead acknowledges the deliberately hidden pill). Chrome gets two animation-frame opportunities when it schedules them, with a 250 ms timeout fallback; neither is compositor or physical-pixel proof.

The renderer requires the genuine host to remain the direct child of `document.documentElement`. The service worker independently resolves exact `:root`, pins the private marker's first/innermost closed-shadow host, and requires that host's immediate browser-process parent to equal the root element; an ordinary wrapper or outer page-owned open/closed shadow root is rejected. It requires unique host membership in raw ordered `DOM.getTopLayerElements`, rejects any later node whose bounded ancestry resolves to the same root document, and—outside intentional capture—requires five `DOM.getNodeForLocation(ignorePointerEventsNone:true)` samples through that host and frame. Initial and fresh-final phases repeat exact host/root ancestry and both elements' `hidden`/`inert`/ARIA-critical attributes. A root `DOM.topLayerElementsUpdated` revision seqlock accepts the bridge's own clean re-top events only when covered by the final list; a separate content-loss generation captured before the renderer request changes only for loss/mismatch signals and must remain unchanged through browser proof. Every CDP call shares a 1.5-second deadline plus 512 ancestry-work budget. Chromium currently concatenates local documents' top-layer lists, so unrelated child-document entries remain allowed while later entries from the controlled root do not. `DOM.getTopLayerElements`, the DOM search helpers, and `DOM.getNodeForLocation` are experimental protocol dependencies; the supported floor is Chrome/Edge 140 and release acceptance conformance-tests their exact behavior. See Chrome's [per-document LIFO top-layer description](https://developer.chrome.com/blog/top-layer-devtools), the [CDP DOM methods](https://chromedevtools.github.io/devtools-protocol/tot/DOM/), and Chromium's [multi-document implementation](https://chromium.googlesource.com/chromium/src/+/HEAD/third_party/blink/renderer/core/inspector/inspector_dom_agent.cc).

The content watchdog attempts a sample every 500 ms only when no prior re-top/browser acknowledgement is active; its browser message is bounded at two seconds. Every root top-layer event records a monotonic revision without changing the content-loss generation; every accepted indicator-loss or content identity/proof mismatch records that separate monotonic generation. A proof clears only the exact covered revision/content-loss pair, so a newer same-revision loss retains the absolute three-second service-worker dirty deadline plus scheduler/transport timing—even while an extension-owned re-top/proof is in progress. Authorized navigation, a browser-native dialog, and intentional capture suspend ordinary input under their own completion/rebind boundaries; they do not reset a dirty timestamp. The extension validates sender tab and exact lease session before honoring loss or proof messages. A JavaScript dialog is the sole pre-action renderer exception because it freezes the renderer; `page.handleDialog` resolves it in the browser process, after which the page proof is restored before ordinary input resumes.

These checks narrow but cannot eliminate a page-owned UI race: a hostile document can alter CSS, accessibility, View Transition, top-layer, or compositor state after the final browser-process proof and before the following trusted CDP dispatch or next sample. Render/layout, ancestry, and browser-process point hits are not atomically bound to input and cannot prove physical pixels. Chrome's browser-owned warning and Cancel action remain the page-independent authority and handback surface throughout the lease, and popup **Release control** remains available.

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
| `tabs.new` | optional `url` | Creates a policy-approved URL, or `about:blank` when omitted, in the named Local Browser Bridge tab group |
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

Omit `tabs.new.url` only when the client wants a blank lifecycle tab for later reconciliation. When the client intends to control the new tab immediately, it should pass a complete policy-approved URL: [Chrome does not permit extension code in a top-level `about:` frame](https://developer.chrome.com/docs/extensions/reference/api/extensionTypes#type-InjectDetails), so an isolated `about:blank` cannot host this extension's visible control surface. The server accepts at most one 4,096-character string and drops unrelated parameters. Before `chrome.tabs.create` can run, the extension rechecks the current Full Access or Safe-mode settings with the same URL policy used by navigation, rejects its own control surface (with the existing `/demo` exception), and passes Chrome the canonical allowed URL. Invalid or blocked URLs create no tab. The result remains the non-URL metadata `{ tabId, groupId, bridgeCreated }`; use `tabs.list` to reconcile an outcome-unknown creation without exposing a query string or fragment. While Chrome is still committing the navigation, `tabs.list` uses the policy-checked pending URL instead of briefly reporting the transitional blank URL.

A Safe-mode approval record carries the exact authenticated server session, its `server:<sessionId>` control owner, and the extension-generated connection ID. Only one unexpired approval may be live; a second request fails `APPROVAL_ALREADY_PENDING` instead of replacing the popup card. Only the trusted popup can atomically claim and clear the record while that exact WebSocket is ready. A stale popup click is non-destructive and immediately refreshes the card from current extension state. The approved action then runs outside the identity queue but inside the worker-wide browser-action queue with a registered session-bound cancellation context, so token, port, enable, policy, lifecycle, or unexpected-socket rotation can invalidate/cancel deferred work instead of waiting behind dispatch. Approval/rejection is refused after any binding change; pending storage and the `?` badge are cleared before reconnect, and installation never restores an old approval. Cancellation prevents a later side-effect dispatch but cannot roll back a Chrome mutation that already crossed its dispatch boundary; that existing outcome-unknown rule is unchanged.

Open-shadow-root elements participate in observations. A mutation observer invalidates existing refs for page-owned DOM changes. It excludes only mutations whose changed objects are the exact retained bridge host or exact closed-shadow-owned objects; a copied public host ID, an ancestor match, and page-owned light DOM—even under the genuine host—do not suppress invalidation. Target revalidation also rejects an observed element that has since become an exact excluded object. Cross-origin iframe elements are merged into the same observation; see [Cross-origin frames](#cross-origin-frames).

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

`frameSummary.skipped` uses a closed vocabulary: `budget_frames`, `budget_elements`, `budget_time`, `blank_document`, `zero_size`, `offscreen`, `depth_exceeded`, `owner_unresolved`, `agent_install_failed`, `session_probe_failed`, `frame_timeout`, `same_process_frame`, `navigated_during_observation`. Every iframe owner in the top document that never produced its own frame target is reported as `same_process_frame`. Recursively attached OOPIFs that exceed the supported depth are reported as `depth_exceeded` and detached. This does not claim complete reporting for an in-process iframe nested inside an attached child: the frame agent does not publish nested owner elements, so that deferred case has no frame-scoped ref and may be absent from the summary.

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

**What is and is not proven.** The ref grammar, sanitizers, recursive session lifecycle, depth and time bounds, coordinate translation, merge arithmetic, staleness paths, and in-frame proofs are covered by the Rust and Node contract suites. The published 0.11.0 live bundle confirmed the five CDP behaviours required for one genuine depth-one OOPIF in Chrome for Testing 152: child-session event delivery and command routing, parent-viewport box coordinates, page-target input hit-testing into the child renderer, and isolated-world survival through the click ([../evidence/v0.11.0/README.md](../evidence/v0.11.0/README.md)).

The exact published 0.11.1 boundary run added a live top-level `same_process_frame` skip and a live `FRAME_ACTION_UNSUPPORTED` refusal, but failed 2 of 19 checks because the root-only auto-attach did not discover a depth-two OOPIF. Version 0.11.2 recursively arms each verified child session and handles child-originated target lifecycle events. A packaged local candidate then passed 22 of 22 checks: two OOPIF levels merged with accumulated offsets and a depth-two click landed with `event.isTrusted === true` ([../evidence/v0.11.1/README.md](../evidence/v0.11.1/README.md)). That candidate is developer-build evidence, not immutable release proof. Isolated-world survival across a frame's own same-document navigation and nested in-process-frame reporting remain harness-only or deferred.

The first command to every child remains a discriminating routing probe: it must return that child's own frame ID. If a browser or intermediary strips `sessionId` and returns the root tree, frame support is disabled for the lease with `reason: "session_routing_unverified"`, every attached record is dropped, and the observation stays top-document-only. The checked-in negative run is an explicitly labelled fault injection; no released Chrome version is known to accept and silently ignore that field. Chrome 118–124 did not expose child-session routing in the extension API schema and rejected the child command, while Chrome 125 added the routed form. Version 0.12.65 declares Chrome 140 as its minimum because that line also supports restricting persisted local extension storage to trusted contexts, so every supported browser has the routed form.

### Condition waits

`page.waitFor` requires at least one condition. `text` and `textGone` match a substring of the rendered body text or the document title, `urlPrefix` matches the start of `location.href`, and `mutationQuietMs` (250–5000) requires the document revision—mutations, scrolls, and resizes—to stay unchanged for that long. The content script polls roughly every 250 ms. `timeoutMs` defaults to 5000 and is clamped into 100–12000. Success returns `{satisfied: true, elapsedMs, conditions}` with the final boolean for each requested condition. An unmet wait fails with `WAIT_TIMEOUT` (HTTP 409); the failure is deliberately classified as non-retriable with a `reobserve` hint, because a timed-out wait is a normal outcome that calls for a fresh observation, not a repeat of the same wait. The command never takes or touches a control lease and dispatches no input. While a JavaScript dialog is pending it fails fast with `BLOCKED_BY_DIALOG` like every other renderer-touching command, because the dialog-frozen renderer cannot answer the condition poll.

### Batched actions

`page.batch` accepts one to ten sub-actions drawn only from `page.click`, `page.fill`, `page.select`, `page.key`, and `page.scroll`. Nested batches and every other method are rejected before relay, naming the offending step index. The batch `tabId` and `generation` are authoritative for every step; a step supplying different values is rejected, per-step lease bindings are stripped, and the stored lease is bound exactly once at the top level. Steps run strictly sequentially, each re-running its full per-method proof; the first failing step stops the batch and no later step is dispatched. The result records `{completed, total, perStep, failedIndex?, failedError?}` followed by one automatic observation. Freshness is not weakened: a step that mutates the document invalidates the shared snapshot, so a later snapshot-bound step fails `STALE_SNAPSHOT` naturally—batches suit quiet-page patterns such as fill, fill, click. A JavaScript dialog opened by an earlier step aborts the batch at the next step's index with `BLOCKED_BY_DIALOG` before that step dispatches into the frozen page. In Safe mode a step that would need popup approval fails with `APPROVAL_REQUIRED` instead of queueing an approval mid-batch.

### JavaScript dialogs

When a controlled page opens an `alert`, `confirm`, `prompt`, or `beforeunload` dialog, the extension records it on the lease and the server publishes `pendingDialog {type, message, hasPrompt, at, tabId}` in `/api/state` (`null` when absent). An open dialog freezes the renderer main thread, so any content-script or `Runtime` call against that page would only time out. While a dialog is pending, only `status`, `tabs.list`, `browser.control.status`, `browser.control.stop`, and `page.handleDialog` proceed—each stays off the renderer—and every other browser command, including the read-only `page.observe` and `page.waitFor` and the delayed automatic post-action observation, fails fast with HTTP 409 `BLOCKED_BY_DIALOG` (taxonomy `blocked_by_dialog`) before anything is relayed. The extension enforces the same gate before any content or CDP dispatch, a `page.batch` checks the pending dialog before each sub-step and aborts at that index, a content or CDP timeout that races a dialog opening resolves as `BLOCKED_BY_DIALOG` instead of revoking control, and the heartbeat suspends its `Runtime.evaluate` renderer probe (keeping the browser-side attachment and document checks) while the dialog is pending.

A dialog can also open *after* a command has passed both gates, while an observation is already in flight, or inside the click handler of the very element the agent just acted on. Every boundary that would revoke the lease—document-identity verification at each preparation, dispatch, and completion point, screenshot completion included, the control-indicator boundary, and the held mouse-button or key release that every trusted click and key press runs in its `finally`—rereads the pending-dialog record immediately before revoking: under a dialog the failure resolves as `BLOCKED_BY_DIALOG` and the lease survives, and only a failure observed with no dialog pending still revokes. The observation path rereads it at its start and again after the capture, so a snapshot taken across a dialog is discarded rather than published. A service-worker restart recovers such a lease rather than revoking it: `pendingDialog` is persisted with the lease, the document identity is verified from the browser process, and only the overlay repaint is deferred to the first heartbeat after the dialog is resolved. **No renderer-blocked probe, dispatch, input release, or recovery can therefore revoke the browser-control lease while a dialog is pending, by timeout or by document identity.** The restart's own fail-closed rules are unchanged and still run first: a recovered candidate carrying an unfinished navigation, an unreleased held input, or a document that cannot be verified at all is dropped, dialog or not. What else still ends a lease is what the dialog does not cause: an explicit stop, a human pause, the lease's own TTL expiring, and any failure observed with no dialog pending. Nothing is forgiven permanently: the record is cleared unconditionally once the dialog is resolved, so a document that really did change while the dialog was open is caught by the very next check and revokes then, and a release the dialog blocked is re-dispatched the moment the dialog closes—by `page.handleDialog` or by the user—and revokes then if it is still unacknowledged. Server-side, a relayed observation that comes back `BLOCKED_BY_DIALOG` is logged as a skipped observation and changes no published state.

`page.handleDialog` requires the held lease and a recorded dialog (`NO_PENDING_DIALOG` otherwise); `promptText` (up to 1000 characters) is forwarded only when accepting a prompt, and `accept: false` is the safe default for `beforeunload`. It is bound to the control session but, uniquely, not to an observation `turn` or `moveSequence`: refreshing those needs an observation the dialog itself forbids, and a discarded observation can leave the extension a turn ahead of the published state. Dialog close, lease start or revocation, and connector reconnect all clear the pending state.

### Key chord grammar

`page.key` and `computer.key` share only the server-side chord syntax, not one delivery capability. Both accept canonical (`Meta+L`) and lowercase vendor-style (`ctrl+shift+t`, `cmd+l`) input. The server resolves modifier aliases (`ctrl`→Control, `cmd`/`command`/`win`/`super`→Meta, `alt`/`option`→Alt), deduplicates and orders modifiers as Alt, Control, Meta, Shift, and canonicalizes aliases such as `esc`, `return`, `del`, `space`, and arrow names. A bare single ASCII letter preserves its case (`j` remains `j`); an ASCII letter in a chord is uppercased (`ctrl+j`→`Control+J`). The shared fallback accepts exactly one Unicode scalar that is neither a control nor whitespace character. It does not promise that the scalar is visibly printable: Unicode format characters can pass. Literal `+` is reserved as the chord separator and cannot be sent as a literal key, and whitespace is accepted only through a named alias such as `Space`. Unknown tokens and more than three modifiers fail with `BAD_REQUEST` before relay.

`page.key` then uses Chromium CDP. It accepts the named set `Tab`, `Enter`, `Escape`, `Backspace`, `ArrowLeft`, `ArrowUp`, `ArrowRight`, `ArrowDown`, `PageUp`, `PageDown`, `End`, `Home`, `Space`, `Delete`, `Insert`, `ContextMenu`, `CapsLock`, `PrintScreen`, and `Pause`; `F1`–`F12`; ASCII letters and digits; and a shared-fallback scalar only when it occupies exactly one UTF-16 code unit. Safe mode further restricts keys to its bounded navigation allowlist. Use `page.typeText` for text, especially a non-BMP character: the server can normalize one non-BMP Unicode scalar, but the extension deliberately rejects a key token that occupies two UTF-16 code units. CDP virtual-key mappings for `ContextMenu`, `CapsLock`, `PrintScreen`, and `Pause` are 93, 20, 44, and 19 respectively.

`computer.key` is narrower and platform-specific. macOS maps Control, Alt/Option, Shift, and Meta/Command modifiers; the navigation/editing keys through `Delete`, `Home`/`End`, `PageUp`/`PageDown`, arrows, `F1`–`F12`, ASCII letters/digits, and the punctuation `= - ] ' ; \\ , / . [`. Windows maps Control, Alt, and Shift plus the same navigation/editing keys, `F1`–`F12`, ASCII letters/digits, and ``; = , - . / ` [ \\ ] '``. Although the shared grammar recognizes Meta/Win on Windows, the exact-window backend rejects every Windows-key chord, as well as `Alt+Tab`, `Alt+Escape`, `Control+Escape`, and `Control+Alt+Delete`, because those are global or secure shortcuts. Names accepted only by `page.key` and other unmapped native tokens fail per action with `COMPUTER_BACKGROUND_UNAVAILABLE`; use `computer.typeText` for text.

