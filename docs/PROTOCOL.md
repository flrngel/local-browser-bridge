# Bridge protocol

## Transport

- HTTP/SSE control surface: `http://127.0.0.1:17373`
- Extension WebSocket: `ws://127.0.0.1:17373/bridge?token=...`
- WebSocket handshake requirements: exact token and an Origin beginning with `chrome-extension://`
- One active extension connection; a new authenticated connection replaces the old one
- JSON message size limit: 8 MB
- Command timeout: 15 seconds by default

## WebSocket envelopes

Extension hello:

```json
{
  "type": "hello",
  "version": "0.4.0",
  "browser": "Google Chrome",
  "mode": "full-access",
  "capabilities": ["tabs.list", "page.observe"]
}
```

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

The REST API uses the same methods through `POST /api/v1/command` with the bridge token as a Bearer token. The localhost UI uses its same-origin session and CSRF token.
