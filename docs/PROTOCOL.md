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
  "version": "0.6.0",
  "browser": "Google Chrome",
  "mode": "full-access",
  "capabilities": ["tabs.list", "page.observe"]
}
```

Computer-helper hello:

```json
{
  "type": "hello",
  "version": "0.6.0",
  "platform": "macos",
  "architecture": "aarch64",
  "backend": "xcap+enigo",
  "inputReady": true,
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
| `page.key` | `tabId`, `key` | Full Access accepts keys/chords such as `Meta+A`; Safe mode accepts navigation keys |
| `page.scroll` | `tabId`, `deltaX`, `deltaY` | Deltas clamped to ±5000 |
| `page.clickAt` | `tabId`, `x`, `y`, optional `button`, `clickCount` | Full Access only; trusted viewport-coordinate click |
| `page.typeText` | `tabId`, `text` | Full Access only; inserts arbitrary text into the focused control |
| `page.evaluate` | `tabId`, `expression` | Full Access only; evaluates page JavaScript, awaits Promise up to 12 seconds, returns a by-value result |

Refs are scoped to an observation `generation`. A page mutation or a new observation invalidates prior refs by design.

## Native computer commands

| Method | Parameters | Notes |
|---|---|---|
| `computer.status` | none | Platform, architecture, backend, displays, permission/input readiness, and current-frame status |
| `computer.observe` | optional `displayId` | Captures the requested display or primary display and returns PNG plus frame/display metadata |
| `computer.move` | `frameId`, `x`, `y` | Moves the native pointer to image-space coordinates |
| `computer.click` | `frameId`, `x`, `y`, optional `button`, `clickCount` | Left/middle/right, one to three clicks |
| `computer.drag` | `frameId`, `fromX`, `fromY`, `toX`, `toY`, optional `durationMs` | Left-button drag; duration is 50–2000 ms |
| `computer.scroll` | `frameId`, `x`, `y`, `deltaX`, `deltaY` | Moves to image-space point, then scrolls each axis with deltas clamped to ±50 |
| `computer.typeText` | `frameId`, `text` | Types Unicode text into the focused native control |
| `computer.key` | `frameId`, `key` | Sends one named key or a bounded chord such as `Meta+L` or `Control+L` |

`computer.observe` returns a private screenshot data URL to the server plus metadata shaped like:

```json
{
  "frame": {
    "id": "uuid",
    "capturedAt": "2026-08-18T00:00:00Z",
    "displayId": "1",
    "displayIndex": 0,
    "displayName": "Built-in Display",
    "imageWidth": 1333,
    "imageHeight": 750,
    "screenX": 0,
    "screenY": 0,
    "screenWidth": 2560,
    "screenHeight": 1440,
    "scaleFactor": 2.0,
    "rotation": 0.0
  }
}
```

Coordinates use the delivered image dimensions, not assumed physical pixels. The helper maps them into the captured display's screen space. It retains only the latest frame, re-enumerates the display immediately before input, and returns `COMPUTER_STALE_FRAME` if the frame ID or display identity/geometry/scale/rotation changed. The server serializes all actions and requests a new computer observation after every successful input action.

The native allowlist intentionally contains no shell, filesystem, process-launch, clipboard, downloader, or arbitrary-code method.

The REST API uses the same methods through `POST /api/v1/command` with the bridge token as a Bearer token. The localhost UI uses its same-origin session and CSRF token.
