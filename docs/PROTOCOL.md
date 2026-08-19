# Bridge protocol

Protocol version: `1`. Package version examples below use `0.9.0`.

## Transport and trust boundary

- HTTP/SSE control surface: `http://127.0.0.1:17373`
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
  "serverVersion": "0.9.0",
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
  "version": "0.9.0",
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
  "version": "0.9.0",
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
| `page.click` | `tabId`, `ref`, `generation`, optional lease bindings | Revalidates identity, geometry, hit target, and generation before trusted CDP input |
| `page.fill` | `tabId`, `ref`, `generation`, `text`, optional lease bindings | Full Access permits sensitive fields; Safe mode rejects them |
| `page.select` | `tabId`, `ref`, `generation`, `value`, optional lease bindings | Matches option value or label |
| `page.key` | `tabId`, `generation`, `key`, optional lease bindings | Snapshot-bound key/chord; Safe mode accepts only bounded navigation keys |
| `page.scroll` | `tabId`, `generation`, `deltaX`, `deltaY`, optional lease bindings | Deltas clamped to ±5000 and invalidate the snapshot |
| `page.clickAt` | `tabId`, `generation`, `x`, `y`, optional `button`, `clickCount`, lease bindings | Full Access only; revalidates the exact point target immediately before trusted input |
| `page.typeText` | `tabId`, `generation`, `text`, optional lease bindings | Full Access only; inserts text into the focused control |
| `page.evaluate` | `tabId`, `expression`, optional lease bindings | Full Access only; awaits a Promise for up to 12 seconds and returns a by-value result |

Open-shadow-root elements participate in observations. A mutation observer invalidates existing refs for page-owned DOM changes; mutations made only by the bridge control overlay are excluded. Cross-origin iframe semantic merging is not implemented.

## Native computer commands

| Method | Parameters | Notes |
|---|---|---|
| `computer.status` | none | Platform, backend, target windows, permission/input readiness, pointer state, share state, and current-frame status |
| `computer.share.start` | `windowId`, optional `fps` | Starts a bounded exact-window frame feed; default 4 FPS, accepted range 1–10 |
| `computer.share.status` | none | Returns share ID, window ID, frame sequence, FPS, capture scope, and backpressure policy |
| `computer.share.stop` | none | Stops the active share and returns its ID |
| `computer.observe` | optional `windowId` | Captures one exact application window without including unrelated desktop windows |
| `computer.move` | `frameId`, `x`, `y`, optional `durationMs` | Routes a bounded synthetic trajectory to the exact window; never moves the hardware cursor |
| `computer.click` | `frameId`, `x`, `y`, optional `button`, `clickCount`, `durationMs` | Moves the synthetic pointer, then sends exact-window left/middle/right input |
| `computer.drag` | `frameId`, `fromX`, `fromY`, `toX`, `toY`, optional `durationMs` | Left-button drag; duration is 50–2000 ms |
| `computer.scroll` | `frameId`, `x`, `y`, `deltaX`, `deltaY` | Routes pointer attention, then exact-window scroll with deltas clamped to ±50 |
| `computer.typeText` | `frameId`, `text` | Routes Unicode text to the exact target process/window |
| `computer.key` | `frameId`, `key` | Sends one named key or a bounded chord such as `Meta+L` or `Control+L` |
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
      "backpressure": "producer-blocking"
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

Native password elements are always emitted with `sensitive: true`, `valueRedacted: true`, no `value`, and no `setValue` action. macOS classifies secure AX roles and subroles before reading `AXValue`; Windows reads `CurrentIsPassword` before acquiring a value pattern and treats an unreadable password state as sensitive. The server repeats this redaction when sanitizing helper payloads.

The synthetic pointer is helper-session state, not the hardware cursor. Its bounded cubic Bézier/minimum-jerk trajectory is delivered to the exact window, and its final state is composited into subsequent exact-window PNGs. It is not a native click-through desktop overlay in version 0.9.

`computer.share.frame` events carry the same observation shape with a monotonically increasing share sequence. Capture is producer-blocking and reports zero queued-frame drops because the helper does not build an unbounded frame backlog. To avoid a 1–10 FPS render/action race, the helper keeps a bounded recent-frame lease: a rendered `frameId` remains usable for at most three seconds only while the current share, PID, native window ID, and complete window geometry still match. Everything else is stale. This feed is repeated exact-window capture, not an OS virtual display, remote-desktop stream, or isolated input session.

The helper re-enumerates the exact `(pid, native window id)` target before input and returns `COMPUTER_STALE_FRAME` if identity or geometry changed. It snapshots foreground process, user focus, hardware cursor, and active desktop around delivery and returns `COMPUTER_BACKGROUND_CONTRACT_VIOLATION` if the non-interruption invariant fails. There is no implicit global-HID or foreground fallback. The server serializes actions and requests a new exact-window observation after successful input.

Legacy `displayId` and display-shaped aliases identify the selected window, not a physical display, and remain deprecated compatibility fields.

## REST API

`POST /api/v1/command` exposes the same allowlisted methods. Bearer-token clients receive server-sanitized results; they never speak the connector WebSocket envelope directly. The localhost UI exchanges the master bridge token for an expiring random `Session` authorization capability and CSRF value. The capability is kept in the exact port origin's `sessionStorage`, not a host-wide localhost cookie. `/api/state`, `/api/events`, `/api/screenshot`, and `/api/computer/screenshot` require that session or a bearer token; mutations additionally require same-origin and CSRF proof.

The native allowlist intentionally contains no shell, filesystem, process-launch, clipboard, downloader, arbitrary-code, credential-store, user-management, or telemetry method.
