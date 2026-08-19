# Bridge protocol

## Transport

- HTTP/SSE control surface: `http://127.0.0.1:17373`
- Extension WebSocket: `ws://127.0.0.1:17373/bridge?token=...`
- WebSocket handshake requirements: exact token and an Origin beginning with `chrome-extension://`
- Computer-helper WebSocket: `ws://127.0.0.1:17373/computer?token=...`
- Computer-helper handshake requirements: exact token and exact Origin `lbb-computer-helper://local`
- One active extension connection; a new authenticated connection replaces the old one
- One active computer-helper connection; a new authenticated helper replaces the old one
- JSON message size limit: 8 MB
- Command timeout: 15 seconds by default

## WebSocket envelopes

Extension hello:

```json
{
  "type": "hello",
  "version": "0.8.0",
  "browser": "Google Chrome",
  "mode": "full-access",
  "capabilities": ["tabs.list", "page.observe"]
}
```

Computer-helper hello:

```json
{
  "type": "hello",
  "version": "0.8.0",
  "platform": "macos",
  "architecture": "aarch64",
  "backend": "background-window/skylight+cgwindow",
  "sessionMode": "background-window",
  "inputReady": true,
  "semanticReady": true,
  "capabilities": ["computer.status", "computer.observe", "computer.click"]
}
```

The server keeps only capabilities present in its compiled computer allowlist. The helper version must exactly match the server version; a mismatch clears its effective capabilities and returns `COMPUTER_VERSION_MISMATCH` for attempted actions. Browser and computer connectors use the same command/result envelope but separate sockets, connection state, and screenshot storage.

Server command:

```json
{
  "id": "uuid",
  "type": "command",
  "method": "page.observe",
  "params": { "tabId": 42 }
}
```

Extension result:

```json
{
  "id": "uuid",
  "type": "result",
  "ok": true,
  "result": {}
}
```

Extension error:

```json
{
  "id": "uuid",
  "type": "result",
  "ok": false,
  "error": { "code": "SITE_BLOCKED", "message": "example.com is not in the extension allowlist" }
}
```

Unsolicited approval event:

```json
{
  "type": "event",
  "name": "approval.resolved",
  "data": { "id": "uuid", "method": "page.click", "ok": true, "result": {} }
}
```

## Commands

| Method | Parameters | Notes |
|---|---|---|
| `status` | none | Connection, mode, and allowlist status |
| `tabs.list` | none | Full Access returns controllable tabs; Safe returns allowlisted tabs; query/fragment removed |
| `tabs.activate` | `tabId` | Focuses a permitted tab |
| `tabs.new` | none | Creates `about:blank` |
| `tabs.close` | `tabId` | Immediate in Full Access; popup approval in Safe mode |
| `page.observe` | `tabId` | Screenshot + text + selected text + interactive refs |
| `page.navigate` | `tabId`, `url` | Full Access permits HTTP(S) and file; Safe requires allowlisted HTTP(S) |
| `page.back` / `page.forward` / `page.reload` | `tabId` | Standard navigation |
| `page.click` | `tabId`, `ref`, `generation` | Trusted input when available; Safe mode asks approval for risky labels |
| `page.fill` | `tabId`, `ref`, `generation`, `text` | Full Access permits sensitive fields; Safe mode rejects them |
| `page.select` | `tabId`, `ref`, `generation`, `value` | Matches option value or label |
| `page.key` | `tabId`, `generation`, `key` | Snapshot-bound key/chord; Full Access accepts keys such as `Meta+A`, Safe mode accepts navigation keys |
| `page.scroll` | `tabId`, `deltaX`, `deltaY` | Deltas clamped to ±5000 |
| `page.clickAt` | `tabId`, `generation`, `x`, `y`, optional `button`, `clickCount` | Full Access only; snapshot-bound trusted viewport-coordinate click |
| `page.typeText` | `tabId`, `generation`, `text` | Full Access only; snapshot-bound insertion into the focused control |
| `page.evaluate` | `tabId`, `expression` | Full Access only; evaluates page JavaScript, awaits Promise up to 12 seconds, returns a by-value result |

Refs are scoped to an observation `generation`. A page mutation or a new observation invalidates prior refs by design.

## Native computer commands

| Method | Parameters | Notes |
|---|---|---|
| `computer.status` | none | Platform, exact-window background backend, target windows, permission/input readiness, and current-frame status |
| `computer.observe` | optional `windowId` | Captures the requested application window without capturing unrelated desktop windows |
| `computer.move` | `frameId`, `x`, `y` | Sends a window-local move event; never moves the hardware cursor |
| `computer.click` | `frameId`, `x`, `y`, optional `button`, `clickCount` | Left/middle/right, one to three clicks |
| `computer.drag` | `frameId`, `fromX`, `fromY`, `toX`, `toY`, optional `durationMs` | Left-button drag; duration is 50–2000 ms |
| `computer.scroll` | `frameId`, `x`, `y`, `deltaX`, `deltaY` | Sends window-local scroll events with deltas clamped to ±50 |
| `computer.typeText` | `frameId`, `text` | Routes Unicode text to the exact target process/window |
| `computer.key` | `frameId`, `key` | Sends one named key or a bounded chord such as `Meta+L` or `Control+L` |
| `computer.invoke` | `frameId`, `elementRef`, optional `action` | Invokes an advertised frame-bound accessibility action and reports an observed postcondition |
| `computer.setValue` | `frameId`, `elementRef`, `value` | Writes through the platform accessibility value pattern and requires value read-back or masked-length proof |

`computer.observe` returns a private screenshot data URL to the server plus metadata shaped like:

```json
{
  "frame": {
    "id": "uuid",
    "capturedAt": "2026-08-18T00:00:00Z",
    "windowId": "47782",
    "pid": 51641,
    "appName": "Example App",
    "windowTitle": "Document",
    "imageWidth": 1209,
    "imageHeight": 826,
    "windowX": 180,
    "windowY": 768,
    "windowWidth": 720,
    "windowHeight": 492,
    "sessionMode": "background-window",
    "deliveryMode": "exact-window-background",
    "semanticMode": "macos-accessibility",
    "semanticAvailable": true,
    "semanticError": null,
    "elements": [
      {
        "ref": "a1",
        "role": "AXButton",
        "name": "Continue",
        "value": null,
        "enabled": true,
        "actions": ["press"],
        "bounds": { "x": 760, "y": 1080, "width": 92, "height": 32 }
      }
    ]
  }
}
```

Coordinates use the delivered image dimensions, not assumed physical pixels. The helper maps them into the captured window's local and screen coordinate spaces. Semantic refs are valid only for that frame. If the platform accessibility snapshot is unavailable, the screenshot still succeeds with `semanticAvailable: false`, no semantic refs, and a bounded `semanticError`.

The helper retains only the latest frame, re-enumerates the exact `(pid, native window id)` target immediately before input, and returns `COMPUTER_STALE_FRAME` if ownership or geometry changed. It snapshots the foreground process, user's focused window/control, hardware cursor, and active desktop around delivery and returns `COMPUTER_BACKGROUND_CONTRACT_VIOLATION` if the non-interruption invariant fails. There is no implicit global-HID or foreground fallback. The server serializes all actions and requests a new exact-window observation after every successful input action.

Version 0.8 still accepts `displayId` and returns legacy display-shaped aliases for compatibility with v0.6 clients. They identify the selected window, not a physical display, and are deprecated.

The native allowlist intentionally contains no shell, filesystem, process-launch, clipboard, downloader, or arbitrary-code method.

The REST API uses the same methods through `POST /api/v1/command` with the bridge token as a Bearer token. The localhost UI uses its same-origin session and CSRF token.
