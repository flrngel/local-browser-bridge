## Native computer commands

| Method | Parameters | Notes |
|---|---|---|
| `computer.status` | none | Platform, backend, target windows, permission/input readiness, pointer state, share state, and current-frame status |
| `computer.share.start` | `windowId`, optional `fps` | Starts one persistent OS exact-window capture stream; default maximum cadence 4 FPS, accepted range 1–10 |
| `computer.share.status` | none | Returns the capture backend/OS-indication policy, source and transport sequences/drop counts, exact target, requested FPS cap, and backpressure policy |
| `computer.share.stop` | none | Stops the active share and returns its ID |
| `computer.observe` | optional `windowId` | Captures one exact application window without including unrelated desktop windows |
| `computer.move` | `frameId`, `x`, `y`, optional `durationMs`, `coordinateSpace` | Routes a bounded synthetic trajectory to the exact window; never moves the hardware cursor |
| `computer.click` | `frameId`, `x`, `y`, optional `button`, `clickCount`, `durationMs`, `coordinateSpace` | Moves the synthetic pointer, then sends exact-window left/middle/right input |
| `computer.drag` | `frameId`, `fromX`, `fromY`, `toX`, `toY`, optional `durationMs`, `coordinateSpace` | Left-button drag; duration is 50–2000 ms |
| `computer.scroll` | `frameId`, `x`, `y`, `deltaX`, `deltaY`, optional `coordinateSpace` | Routes pointer attention, then exact-window scroll with deltas clamped to ±50 |
| `computer.typeText` | `frameId`, `text` | Routes 1–2,000 UTF-16 code units (excluding U+0000) to the exact target process/window through a paced, cooperatively cancellable native action |
| `computer.key` | `frameId`, `key` | Sends one platform-mapped key/chord from the native subsets above; unsupported/global tokens fail closed |
| `computer.invoke` | `frameId`, `elementRef`, optional `action` | Invokes an advertised frame-bound accessibility action and reports an observed postcondition |
| `computer.setValue` | `frameId`, `elementRef`, `value` | Writes through the platform accessibility value pattern and requires read-back or masked-length proof |

The `computer.typeText` bound is intentionally separate from the 100,000-character limits on browser `page.typeText` and semantic `computer.setValue`. Native macOS delivery emits targeted keyboard events, while Windows emits one `WM_CHAR` per UTF-16 code unit; neither primitive is a bulk string setter. The helper repeats the HTTP sanitizer's UTF-16 validation, checks cancellation between dispatched units, paces the platform queue, and stops the native dispatch loop after 2,500 ms. On macOS, event construction is followed immediately by an exact `AXFocusedWindow` and focused-element ownership proof before every scalar key-down; key-up is still attempted unconditionally once key-down posting begins. If that receiver proof is lost after an earlier scalar, or cancellation/deadline occurs after the first native mutation, the result is `COMPUTER_OUTCOME_UNKNOWN`, all observation-derived computer authority is revoked, and the client must observe again without automatically retrying. Successful native text remains `effect: Unverifiable`; use `computer.setValue` when an advertised accessibility value pattern can provide exact read-back, or `page.typeText` for controlled browser content.

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
    "semanticMode": "windows-ui-automation",
    "semanticAvailable": true,
    "semanticTruncated": false,
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

Windows UI Automation observations preserve every actionable element collected before a bounded traversal stops. In that case `semanticAvailable` remains `true`, `semanticTruncated` is `true`, and `semanticTruncationReason` is exactly one of `node_budget`, `depth_budget`, `actionable_budget`, `deadline`, or `provider_error`. The reason is omitted when `semanticTruncated` is `false`. A root/setup failure instead reports semantic data as unavailable rather than presenting an empty result as a complete tree.

### Coordinate spaces

Coordinate commands accept an optional `coordinateSpace`. The default is the existing pixel space (`image` for computer commands, `viewport` for `page.clickAt`); `normalized1000` instead expresses each coordinate as 0–1000 across the current frame. The server converts normalized values against the stored sanitized frame's `imageWidth`/`imageHeight` (or, for `page.clickAt`, the last browser observation's viewport) and clamps to the last addressable pixel—the boundary value 1000 converts to `extent - 1`, because connectors validate coordinates as strictly inside the frame—so connectors only ever receive pixels and `coordinateSpace` is never relayed. Without a stored frame or observation to convert against, the command fails with `NO_COMPUTER_FRAME` or `NO_BROWSER_OBSERVATION` instead of guessing. To let a client pin decisions to an exact frame, `/api/state` exposes a SHA-256 `contentHash` plus decoded `screenshotWidth`/`screenshotHeight` for both the browser observation and the computer observation.

Native password elements are always emitted with `sensitive: true`, `valueRedacted: true`, no `value`, and no `setValue` action. macOS classifies secure AX roles and subroles before reading `AXValue`; Windows reads `CurrentIsPassword` before acquiring a value pattern and treats an unreadable password state as sensitive. The server repeats this redaction when sanitizing helper payloads.

The synthetic pointer is helper-session state, not the hardware cursor. Its bounded cubic Bézier/minimum-jerk trajectory is delivered to the exact window, and its final state is composited into subsequent exact-window PNGs. It is not a native click-through desktop overlay.

`computer.share.start` creates one persistent native source bound to the selected `(pid, native window id)`: ScreenCaptureKit `SCStream` on macOS or a project-owned Windows Graphics Capture `CreateFreeThreaded` frame pool on a dedicated MTA owner thread. The helper advertises `computer.capture.native-stream.v1` for this implementation. The system cursor is excluded, and the bridge uses each platform's default capture-indication behavior without requesting a borderless or hidden mode. `systemIndicator: true` means indication was not suppressed by the helper; it is not runtime proof that a particular banner or border was visible. Selection is programmatic from the authenticated control page; there is no native system content picker.

Share lifecycle authority is parsed from the raw helper result rather than the sanitized status representation. A successful start must be an object containing boolean `active: true` and a nonempty `id` of at most 100 characters. It does not commit until the server obtains a first `computer.observe` result from the same connector UUID whose observation carries that exact share ID. A successful stop must be an object containing boolean `active: false`; a missing, null, string, or presentation-defaulted field is not teardown proof.

The server arms start validation immediately before the exact-session `computer.share.start` dispatch and keeps it armed through the first exact-ID observation. If the owning task is canceled or dropped while either call is pending, the guard revokes the originating WebSocket through the out-of-band shutdown signal. A malformed/rejected lifecycle result or failed first observation first removes public frame/share authority, then transfers `computer.share.stop` to a detached exact-session cleanup task before awaiting it. Dropping the caller can discard only its join handle, not the cleanup task. Cleanup accepts only raw `active: false`; otherwise it revokes the originating transport. A replacement helper is never selected or cleared by this path.

The native callback owns a capacity-one latest-frame slot. Each accepted source frame has a monotonic `sourceSequence` and callback timestamp; replacement increments `sourceDroppedFrames`. The helper converts only the newest available native image into the regular observation shape, caps it at 1,000,000 pixels, composites its synthetic pointer, and PNG-encodes it. The requested 1–10 FPS value is a maximum cadence rather than a guaranteed rate. Source capture continues while an input command owns the controller, although protocol conversion and publication resume only after that command releases the controller; returned frames promise settled synthetic-pointer state, not every intermediate animation sample.

`computer.share.frame` events carry the observation with a separate monotonically increasing transport `sequence`. Frame pacing is negotiated at hello time: a helper that advertises `computer.share.ack` (also a feature flag, not a dispatchable method) receives `"shareAck": true` in the server's `helloAck` and switches to ack-paced, latest-frame-wins delivery. The encoded transport keeps its own single-slot mailbox: a newer converted frame replaces an unemitted frame and increments `transportDroppedFrames` (also retained as the compatibility field `droppedFrames`), and the next frame is emitted only after the server acknowledges the previous one. After validating each share frame, the server sends an acknowledgement bound to the session, exact share lease, and transport sequence. A well-formed frame quarantined by the outcome-unknown authority gate is still acknowledged so an old mailbox cannot deadlock, but it is not stored or published:

```json
{
  "type": "eventAck",
  "protocolVersion": 1,
  "sessionId": "d559c7b3-56fb-49e6-b661-801cfcb8807f",
  "name": "computer.share.frame",
  "shareId": "3c10ac31-ce03-4db2-93c4-f4ec0b152ec0",
  "sequence": 12
}
```

Stale, duplicate, unknown, and wrong-share acknowledgements are ignored, and both sequence domains stay strictly increasing across dropped frames within one share lease. Asynchronous `computer.share.error` events carry the producing `shareId`; the server clears authority only when that ID is the exact currently published or post-revocation approved epoch. An unbound or old-share error is ignored, so a queued failure cannot clear a newer share. Share status and the `share` block report `captureBackend`, `nativeStream`, `systemIndicator`, `selectionMode`, `sourceSequence`, `sourceDroppedFrames`, `sequence`, `transportDroppedFrames`, `ackPaced`, `lastAckedSequence`, and `backpressure`. `latest-frame-wins` is used when pacing is negotiated; a helper without the acknowledgement feature retains producer-timed transport and is never sent an unknown message. A replaced WebSocket session must start a new native stream and renegotiate pacing.

On macOS, either intentional server shutdown or unexpected transport loss terminates the helper process without synchronously waiting for `SCStream` teardown; the user must relaunch it. An explicit `computer.share.stop` does not terminate the helper. On Windows, transport loss terminates only the disposable worker and the still-running supervisor starts replacements with backoff.

To avoid a live-share render/action race, the helper keeps a bounded recent-frame lease: a rendered `frameId` remains usable for at most three seconds only while the current share ID, PID, native window ID, and complete window geometry still match. Everything else is stale. A native exact-window stream still shares the user's login and input environment; it is not an OS virtual display, remote-desktop session, VM, sandbox, or independent input seat.

When capture, semantic discovery, and PNG conversion have already aged a
published share frame by at least one second, the helper defers the next
serialized share pump for one bounded 500 ms action-admission interval. A
queued command still has to present the exact unexpired frame and all normal
share, target, pointer, and geometry bindings. This scheduling interval keeps a
new conversion from consuming the caller's remaining lease; it does not extend
or renew frame authority.

The exact v0.12.23 deliberate-concurrency run demonstrated this refusal boundary without weakening it: its app-share start receipt and 89/89 completed assertions passed, but the harness reused a pre-handoff frame after 43.807 seconds and the helper returned HTTP 409 `COMPUTER_STALE_FRAME` before dispatch. Version 0.12.24 retained the three-second helper lease and added a release-harness wait, within the already reserved handoff deadline, for a strictly newer streamed frame whose share ID, exact target, and complete window/image geometry match the pre-refresh authority; only that successor may derive the product click. Version 0.12.68 retains that contract. A timeout or mismatch fails closed; there is no retry with the stale frame and no relaxed helper freshness limit.

The macOS exact-app handoff reserves 18 seconds from its create-once start
receipt through product action and create-once completion receipt. The app,
runner, read-only watcher, receipt reader, and aggregate finalizer enforce that
same bound. Receipt identity, request/start hashes, exact prompt process,
canonical timestamp order, disabled-button state, and shared-seat invariants
remain mandatory; aligning the deadline does not authorize retries or accept a
late, changed, or copied receipt.

The helper re-enumerates the exact `(pid, native window id)` target before input and returns `COMPUTER_STALE_FRAME` if identity or geometry changed. macOS keyboard eligibility uses an independent all-window inventory, requires a known on-screen exact owner/layer/geometry row and a non-minimized AX top-level mapping, and does not treat same-PID sibling count as receiver authority. ScreenCaptureKit can add a same-PID layer-0 `AXDialog` for its title-bar indicator, so dispatch instead requires the app's exact `AXFocusedWindow`; text also requires the focused element to resolve to that window.

A different-process focus-preparing action captures the user-front app's exact main/focused AX window and the target app's prior exact main/focused AX window without adopting a later sibling switch. A distinct target sibling is admitted only when the target's original main and focused IDs agree, both prior and requested exact windows have a settable `AXMain`, and WindowServer binds both IDs to the same PSN. While the user is still fully active, the helper sets the requested exact window's `AXMain` and bounded-polls target `AXMainWindow == AXFocusedWindow == requested` with target `AXFrontmost=false`. It never writes the application-level `AXFocusedWindow`. The private route then moves through saved user active/target requested inactive → saved user released/target requested inactive → saved user released/target requested active. WindowServer's saved user PSN/PID remains front throughout; the saved app has `AXFrontmost=false` while released, and the exact target has `AXFrontmost=true` only in its private active phases.

Restoration defocuses and proves requested inactive. For a distinct prior sibling, it posts an exact prior Focus and accepts only a bounded exact active requested-or-prior observation; when AppKit remains on requested, a target-only paired make-key record commits and proves `AXMainWindow == AXFocusedWindow == prior`. The prior is then defocused and proved inactive before saved-user Focus. This route does not call `_SLPSSetFrontProcessWithOptions`, perform `AXRaise`, or raise/change Space. Preparation records require exact raw/AX target-phase evidence ending with a saved-user PSN → AX window/frontmost → PSN sample both before and after dispatch accounting. Restore cleanup runs after accounting and repeats one exact target/raw/user authorization immediately before every cleanup record. Each write is followed by a bounded phase poll, with at least 50 ms settle before input. Cleanup failure dominates the original action error. Restoration reserves distinct target-work, user-Focus-authorization, target-independent user-proof, and safe-retry deadline slices; exact user restoration is proved before final target inspection. Emergency user compensation is the deliberate exception to target-phase proof: it performs no target read or write and may Focus only the unchanged saved user after proving exact PSN/PID, exact AX window with `AXFrontmost=false`, raw user restorability, and the original deadline; the target result remains unknown. Before each keyboard down or new focus-capable pointer event, the released user owner and exact requested-active target receiver are re-proved; the exact target receiver is the final AX proof before keyboard posting. A same-process foreground sibling mismatch refuses before focus-capable pixel or keyboard dispatch. Accessibility reads of the unrelated front app are read-only; only the selected target can receive the one-time Chromium AX opt-in.

On macOS the general oracle sandwiches read-only exact front-app AX focus, global cursor sampling, pointer-activity counters, and active Space between stable foreground ProcessSerialNumber/PID samples; it never derives focus from the first same-PID compositor row. The focus lease additionally refuses to overwrite restoration if the user's exact front AX window changed. Every synchronous WindowServer inventory shares the absolute proof deadline and is rejected if it returns late, although `CGWindowListCopyWindowInfo` itself cannot be interrupted in flight. Windows compares foreground and focused HWND identities plus the input desktop and reports only the pointer-monitor signals that its runtime can obtain conservatively. The helper returns `COMPUTER_BACKGROUND_CONTRACT_VIOLATION` if the required foreground, focus, desktop, target-route, or pointer-attribution proof becomes unknown or violated. Its message reports only a closed-vocabulary action stage and failed invariant names, such as `stage=clickDispatch;failedInvariants=sharedPointerBoundaryCorroborated`; it never exposes raw process, window, cursor, desktop, or sibling AX metadata. These bounded samples are not transactional rollback, cannot make the final receiver proof plus private event post atomic, and cannot prove no shorter transient change occurred. There is no implicit global-HID or foreground fallback. The server serializes actions and requests a new exact-window observation after successful input.

Every mutation result keeps three proof layers separate:

1. **Sealed exact-target route.** `inputDelivery` records the resolved route, support level, exact-target binding, dispatch attempt, and explicit negatives for shared-seat, global-HID, and hardware-cursor-mutation use. This is local helper provenance; it is not a receipt from the operating system or target.
2. **Operating-system API acceptance.** `osAcceptanceSignalAvailable` says whether the selected API has a synchronous return signal, and `osAcceptanceObserved` records that signal when available. A successful AX/UIA return or queued Windows message proves only that API boundary. The private macOS `SLEventPostToPid` call returns `void`, so its dispatch attempt is recorded with no acceptance signal and must never be described as a delivery receipt.
3. **Target postcondition.** Only an application-owned read-back, state change, or other allowlisted postcondition can support `effect: "Confirmed"`. Dispatch and invariant evidence never confirm the target effect.

The macOS pixel/key route reports `supportLevel: "privateUnsupported"` because its SkyLight interfaces are private and unsupported even when runtime resolution and packaged-artifact auditing succeed. Semantic Accessibility and Windows UI Automation routes report their documented support level. An unavailable, unsealed, or changed route fails closed.

A representative macOS semantic action result can contain:

```json
{
  "effect": "Confirmed",
  "invariants": {
    "foregroundUnchanged": true,
    "userFocusUnchanged": true,
    "cursorPositionUnchanged": false,
    "sharedPointerActivityObserved": true,
    "hidSystemPointerActivityObserved": true,
    "rawInputPointerActivityObserved": false,
    "injectedPointerActivityObserved": false,
    "pointerActivityMonitorHealthy": true,
    "sharedPointerBoundaryCorroborated": true,
    "sharedPointerBoundaryState": "corroborated",
    "hardwareCursorPreservedByHelper": true,
    "helperGlobalPointerPreservation": "confirmed",
    "sharedPointerActivityState": "contaminated",
    "spaceUnchanged": true,
    "inputDelivery": {
      "route": "macosAccessibility",
      "supportLevel": "publicDocumented",
      "exactTargetBound": true,
      "dispatchAttemptRecorded": true,
      "osAcceptanceSignalAvailable": true,
      "osAcceptanceObserved": true,
      "sharedInputSeatUsed": false,
      "globalHidInputUsed": false,
      "hardwareCursorMutationRequested": false
    }
  }
}
```

`cursorPositionUnchanged` is diagnostic. A false value identifies a coordinate delta, not its source. `sharedPointerActivityObserved` is the platform-neutral activity bit. On macOS, `hidSystemPointerActivityObserved` covers movement, drag, button, scroll, and tablet counter activity; physical devices, virtual HID, remote-session input, and other platform routing can all contribute, so it is never physical-input provenance. On Windows, `rawInputPointerActivityObserved` comes from message-only `RIDEV_INPUTSINK` Raw Input and `injectedPointerActivityObserved` comes from a minimal dedicated-thread `WH_MOUSE_LL` injected-flag epoch. Only counters and health are retained—never device IDs, coordinates, or input contents. Raw Input and hook signals still cannot prove a human or physical device and can have integrity-level, remote-session, or virtual-input blind spots. Windows can also silently remove a timed-out low-level hook without notification, so `pointerActivityMonitorHealthy` reports successful initialization and a usable sampled epoch, not guaranteed continuous hook delivery. `sharedPointerBoundaryCorroborated` and `sharedPointerBoundaryState` report whether the sampled boundary was sufficient to reason conservatively. `helperGlobalPointerPreservation` is `confirmed`, `unknown`, or `violated` from the sealed route plus that boundary. `sharedPointerActivityState` is `quiet`, `contaminated`, or `unknown`; `contaminated` truthfully permits unrelated concurrent pointer activity without blaming it on the helper. An unavailable monitor, implausible/reset counter epoch, uncorroborated delta, or unsealed route yields `unknown` and fails closed. None of these diagnostics confirms target effect or a physical actor.

Each successful mutation also includes an `actionId`, structured `evidence`, and `timings`. `resolveMs` ends when frame/target/action resolution is complete. `dispatchMs` ends immediately after the final native side effect in a compound action. `verifyMs` covers exact postcondition reads, invariant sampling, and result finalization after that boundary. `totalMs` spans the complete helper action. A later side effect, such as the click after a pointer approach, supersedes the earlier boundary so preparation is never mislabeled as final verification.

Legacy `displayId` and display-shaped aliases identify the selected window, not a physical display, and remain deprecated compatibility fields.

