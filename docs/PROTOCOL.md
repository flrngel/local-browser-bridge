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
  "version": "0.1.0",
  "browser": "Google Chrome",
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
| `status` | none | Connection and allowlist status |
| `tabs.list` | none | Returns allowlisted tabs only; query/fragment removed |
| `tabs.activate` | `tabId` | Focuses a permitted tab |
| `tabs.new` | none | Creates `about:blank` |
| `tabs.close` | `tabId` | Always requires popup approval |
| `page.observe` | `tabId` | Screenshot + text + selected text + interactive refs |
| `page.navigate` | `tabId`, `url` | Destination must be HTTP(S) and allowlisted |
| `page.back` / `page.forward` / `page.reload` | `tabId` | Standard navigation |
| `page.click` | `tabId`, `ref`, `generation` | Trusted input when available; risky labels require approval |
| `page.fill` | `tabId`, `ref`, `generation`, `text` | Sensitive fields rejected |
| `page.select` | `tabId`, `ref`, `generation`, `value` | Matches option value or label |
| `page.key` | `tabId`, `key` | Fixed navigation-key allowlist only |
| `page.scroll` | `tabId`, `deltaX`, `deltaY` | Deltas clamped to ±5000 |

Refs are scoped to an observation `generation`. A page mutation or a new observation invalidates prior refs by design.
