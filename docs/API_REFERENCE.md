# API reference

Every HTTP and WebSocket route the server exposes (`src/server.rs`), every
callable method, and the full error taxonomy. This is the source of truth for
the [Agent Skill](../skills/local-browser-bridge/SKILL.md) references. For a
guided first integration, read
[Agent integration](AGENT_INTEGRATION.md) instead; come back here for exact
parameters and response shapes.

All requests, including `/health`, must carry a `Host` of exactly `127.0.0.1`,
`localhost`, or `[::1]` (optionally with the bound port) or they are rejected
with 403 `HOST_REJECTED` before routing or authentication.

## Route table

| Route | Verb | Auth | Purpose |
|---|---|---|---|
| `/health` | GET | none (Host only) | Liveness probe; see below |
| `/api/session` | GET | bearer | Exchange the bearer token for a dashboard session + CSRF token |
| `/api/state` | GET | session or bearer | Full state snapshot (also pushed over `/api/events`) |
| `/api/screenshot` | GET | session or bearer | Latest browser screenshot (PNG) |
| `/api/computer/screenshot` | GET | session or bearer | Latest computer-helper screenshot (PNG) |
| `/api/events` | GET (SSE) | session or bearer | Server-sent events, one per state change, plus a periodic heartbeat |
| `/api/action` | POST | session + CSRF | Legacy dashboard-only command dispatch; agents should use `/api/v1/command` |
| `/api/update/check` | POST | session + CSRF | Trigger the dashboard's on-demand update check |
| `/api/v1/command` | POST | bearer | Primary command dispatch for agents |
| `/api/v1/command/cancel` | POST | bearer | Cancel one in-flight `callId` |
| `/api/v1/fetch/{key}/cancel` | GET | Agent Fetch capability | Cancel one in-flight `callId` via GET |
| `/api/v1/fetch/{key}/{method}` | GET | Agent Fetch capability | Agent Fetch dispatch (see below) |
| `/bridge` | WS | mutual HMAC handshake, `chrome-extension://` origin | Browser extension transport |
| `/computer` | WS | mutual HMAC handshake, `lbb-computer-helper://local` origin | Computer helper transport |
| everything else | GET | none | Static dashboard assets, including `/demo` |

"session" means the expiring capability issued by `/api/session`, kept in the
dashboard's `sessionStorage`. "bearer" means `Authorization: Bearer <token>`.
State-changing session routes additionally require a same-origin `Origin`
header and the CSRF token from `/api/session`.

### `GET /health`

No authentication beyond the Host check. Returns:

```json
{"computerConnected":false,"extensionConnected":false,"ok":true,"shellEnabled":true,"version":"0.12.68"}
```

Use this as the readiness probe before sending any command.

### `GET /api/session`

Exchanges a bearer token for a short-lived dashboard session:

```json
{"csrfToken":"...","expiresAfterIdleSeconds":43200,"ok":true,"sessionToken":"..."}
```

Agents that already hold the bearer token do not need this; it exists for the
dashboard UI, whose page fragment holds the token only long enough to trade it
for a session.

### `GET /api/state`

Returns `{"ok":true,"state":{...}}`. The `state` object's keys, verified
against a running server:

| Key | Meaning |
|---|---|
| `connected` | Extension connector is attached |
| `computerConnected` | Computer helper connector is attached |
| `extension` | Extension hello/status details, or `null` |
| `computer` | Helper hello/status details, or `null` |
| `tabs` | Known tabs (Full Access) or allowlisted tabs (Safe mode) |
| `targetTabId` | Currently observed/controlled tab, or `null` |
| `observation` | Latest browser observation summary, or `null` |
| `computerObservation` | Latest computer observation summary, or `null` |
| `browserControl` | Active browser-control lease state, or `null` |
| `pendingDialog` | `{type, message, hasPrompt, at, tabId}` for an open JavaScript dialog, or `null` |
| `shell` | `shell.status` result (see below) |
| `agentFetch` | `{baseUrl, enabled, requiresCallIdForActions}` |
| `update` | Update-check state: `{checkedAt, currentVersion, latestVersion, message, releaseUrl, status}` |
| `activity` | Recent activity-log entries (human messages, no taxonomy, no secrets) |
| `revision` | Monotonic revision number; also the payload of each `/api/events` push |

`/api/state` also exposes a SHA-256 `contentHash` plus `screenshotWidth`/
`screenshotHeight` inside `observation` and `computerObservation`, so a client
can pin decisions to an exact frame.

### `GET /api/events` (SSE)

The first event on the stream is always `event: state`. After that, each
push is named for what changed — `approval`, `connection`, `dialog`, `error`,
`hello`, `observation`, `shell`, `tabs`, `update`, `warning`,
`computer-error`, and other change-specific names — never `state` again. An
`EventSource` client that only registers
`addEventListener("state", …)` will see exactly one event and then go quiet;
use `source.onmessage` (or listen for every name you care about) to catch
every push. Every event's `data` is `{"revision":N}` regardless of its name;
poll `/api/state` after receiving one if you need the full object. The
connection also carries a `: heartbeat` comment every 15 seconds so a client
can tell an idle stream from a dead one.

### `GET /api/screenshot`, `GET /api/computer/screenshot`

Return the latest PNG screenshot bytes, or 404 `NO_SCREENSHOT` /
`NO_COMPUTER_SCREENSHOT` when none has been captured yet.

### `POST /api/action`, `POST /api/update/check`

Dashboard-internal endpoints. `/api/action` is the legacy predecessor of
`/api/v1/command`, kept only for the dashboard UI; agents should use
`/api/v1/command` instead, which has the idempotency, cancellation, and Agent
Fetch guarantees described below. Both require a dashboard session and CSRF
token, not a bare bearer token — a bearer-only request gets 403
`CSRF_REJECTED`.

## `POST /api/v1/command`

```json
{"method": "tabs.list", "callId": "optional-id", "params": {"...": "..."}}
```

Requires `Authorization: Bearer <token>` and `Content-Type: application/json`.
Returns `{"ok": true, "result": {...}, "state": {...}}` on success, or
`{"ok": false, "error": {...}, "taxonomy": {...}}` on failure (see
[Error taxonomy](#error-taxonomy)). Either shape carries a `callId` key only
when the request supplied one. See
[Agent integration](AGENT_INTEGRATION.md) for `callId` semantics, replay, and
cancellation.

`POST /api/v1/command/cancel` takes `{"callId": "..."}` and returns 202
`{"ok":true,"callId":"...","cancellationRequested":true}`, or 409
`CALL_NOT_IN_PROGRESS` if nothing with that ID is in flight.

## Agent Fetch (`GET /api/v1/fetch/{key}/{method}`)

For agents whose only local networking primitive is a plain GET. `{key}` is a
domain-separated derivation of the bearer token — a separate credential, not
the token itself — shown as "Agent Fetch base URL" on the dashboard and in the
server's startup log. Base URL shape:

```text
http://127.0.0.1:{port}/api/v1/fetch/{key}
```

Top-level query keys become method parameters. Canonical numbers, `true`,
`false`, and `null` become JSON scalars; everything else stays a string.
Prefix a value with `str:` to force a numeric-looking string, or `json:` for
an explicit JSON value. Alternatively pass one URL-encoded JSON object as
`params`; it cannot be combined with direct keys (400 `BAD_REQUEST` if both are
present).

```text
GET {BASE}/status
GET {BASE}/tabs.list
GET {BASE}/page.navigate?callId=nav-1&tabId=12&url=https%3A%2F%2Fexample.com
GET {BASE}/page.click?callId=click-1&params=%7B%22tabId%22%3A12%2C%22ref%22%3A%22g4.e2%22%2C%22generation%22%3A%22g4%22%7D
GET {BASE}/cancel?callId=nav-1
```

Every method except `status`, `tabs.list`, `browser.control.status`,
`page.waitFor`, `computer.status`, `computer.share.status`, and `shell.status`
requires `callId`. The GET transport shares POST's atomic admission, replay
cache, and outcome-unknown fencing, so retrying the exact URL cannot dispatch
the action twice; reusing a `callId` with different parameters returns
`CALL_ID_REUSED`. Query keys the method does not recognize are silently
ignored rather than rejected — except on `{BASE}/cancel`, which accepts only
`callId` and 400s on anything else. An ignored key still counts toward
the `callId`'s replay fingerprint, though: add or drop one between retries of
the same `callId` and the second request 409s with `CALL_ID_REUSED` even
though the recognized parameters are identical.

Responses carry `Cache-Control: no-store` and `Referrer-Policy: no-referrer`,
a browser-declared cross-site fetch is rejected, and only the exact loopback
Host is accepted. The URL is still a credential-bearing string that can be
retained by browser history, proxies, or diagnostics — prefer bearer POST when
the client supports headers and bodies.

## Browser methods (`ACTION_METHODS`, `src/server.rs`)

25 methods, dispatched by both `POST /api/v1/command` and Agent Fetch. A
method that takes `tabId` falls back to `state.targetTabId` (the tab your
last `page.observe` or `browser.control` call touched) when it is omitted,
and 400s `Select a target tab first` if there is no fallback either.

| Method | Parameters | Notes |
|---|---|---|
| `status` | none | Connection, policy mode, allowlist, and control-lease state |
| `browser.control.start` | `tabId`, optional `ttlMs` | Explicitly attaches to one permitted tab and returns lease state |
| `browser.control.status` | none | Returns active lease, turn, pointer sequence, and last revocation |
| `browser.control.stop` | optional `sessionId` | Detaches and releases the current lease; a supplied stale ID is rejected |
| `tabs.list` | none | Full Access returns controllable tabs; Safe mode returns allowlisted tabs; query/fragment removed |
| `tabs.activate` | `tabId` | Focuses a permitted tab |
| `tabs.new` | optional `url` | Creates a policy-approved URL, or `about:blank` when omitted |
| `tabs.close` | `tabId` | Immediate in Full Access; popup approval in Safe mode |
| `page.observe` | `tabId` | Advances `turn`; returns screenshot, text, selection, interactive refs, generation, control state |
| `page.navigate` | `tabId`, `url`, optional lease bindings | Full Access permits HTTP(S) and file; Safe mode requires allowlisted HTTP(S) |
| `page.back` / `page.forward` / `page.reload` | `tabId`, optional lease bindings | Standard navigation under the held lease |
| `page.click` | `tabId`, `ref`, `generation`, optional `button`, `clickCount`, `modifiers`, lease bindings | Revalidates identity, geometry, hit target, generation before trusted CDP input |
| `page.hover` | `tabId`, `ref`, `generation`, optional lease bindings | Same target proof, then moves the pointer without pressing |
| `page.fill` | `tabId`, `ref`, `generation`, `text`, optional lease bindings | Full Access permits sensitive fields; Safe mode rejects them |
| `page.select` | `tabId`, `ref`, `generation`, `value`, optional lease bindings | Matches option value or label |
| `page.key` | `tabId`, `generation`, `key`, optional lease bindings | Snapshot-bound key/chord; Safe mode accepts only bounded navigation keys |
| `page.scroll` | `tabId`, `generation`, `deltaX`, `deltaY`, optional lease bindings | Deltas clamped to ±5000; invalidates the snapshot |
| `page.clickAt` | `tabId`, `generation`, `x`, `y`, optional `button`, `clickCount`, `coordinateSpace`, lease bindings | Full Access only |
| `page.typeText` | `tabId`, `generation`, `text`, optional lease bindings | Full Access only; inserts text into the focused control |
| `page.evaluate` | `tabId`, `expression`, optional lease bindings | Full Access only; awaits a Promise up to 12s, returns a by-value result |
| `page.waitFor` | `tabId`, one or more of `text`, `textGone`, `urlPrefix`, `mutationQuietMs`, optional `timeoutMs` (default 5,000, clamped to 100–12,000) | Read-only; needs no lease; keeps working during a human pause |
| `page.batch` | `tabId`, `generation`, `actions`, optional lease bindings | 1–10 sequential click/fill/select/key/scroll steps bound to one generation |
| `page.handleDialog` | `tabId`, `accept`, optional `promptText` | Accepts or dismisses the recorded JavaScript dialog |

Details on the lease model, `ref`/`generation`, cross-origin frames, and key
chord grammar live in [Browser control](BROWSER_CONTROL.md) and, for the full
wire-level specification, [internals/PROTOCOL.md](internals/PROTOCOL.md).

## Computer methods (`COMPUTER_METHODS`, `src/computer_protocol.rs`)

13 methods, available only while the computer helper is connected:

| Method | Parameters | Notes |
|---|---|---|
| `computer.status` | none | Platform, backend, target windows, permission/input readiness, pointer state, share state, current-frame status |
| `computer.share.start` | `windowId`, optional `fps` | Starts one persistent capture stream; default max cadence 4 FPS, range 1–10 |
| `computer.share.status` | none | Capture backend/OS-indication policy, sequences/drop counts, target, FPS cap, backpressure |
| `computer.share.stop` | none | Stops the active share and returns its ID |
| `computer.observe` | optional `windowId` (or `displayId`, accepted as an alias for it) | Captures one exact application window (one-shot) |
| `computer.move` | `frameId`, `x`, `y`, optional `durationMs`, `coordinateSpace` | Bounded synthetic trajectory; never moves the hardware cursor |
| `computer.click` | `frameId`, `x`, `y`, optional `button`, `clickCount`, `durationMs`, `coordinateSpace` | Moves the synthetic pointer, then sends exact-window input |
| `computer.drag` | `frameId`, `fromX`, `fromY`, `toX`, `toY`, optional `durationMs`, `coordinateSpace` | Left-button drag; 50–2000ms |
| `computer.scroll` | `frameId`, `x`, `y`, `deltaX`, `deltaY`, optional `coordinateSpace` | Deltas clamped to ±50 |
| `computer.typeText` | `frameId`, `text` | 1–2,000 UTF-16 code units, paced and cancellable |
| `computer.key` | `frameId`, `key` | One platform-mapped key/chord; unsupported/global tokens fail closed |
| `computer.invoke` | `frameId`, `elementRef`, optional `action` | Invokes an advertised accessibility action |
| `computer.setValue` | `frameId`, `elementRef`, `value` | Writes through the accessibility value pattern; requires read-back or masked-length proof |

Full capture/input model, proof layers, and platform limits:
[Computer use](COMPUTER_USE.md).

## Shell methods (`SHELL_METHODS`, `src/shell.rs`)

Only reachable when the server was started with `--enable-shell` or
`LBB_ENABLE_SHELL=1`. See [Shell](SHELL.md) for setup, limits, and a warning
about what this grants.

| Method | Parameters | Notes |
|---|---|---|
| `shell.status` | none | Always available; reports whether shell authority is enabled and the platform's shell set |
| `shell.run` | `command`, optional `shell`, `cwd`, `timeoutMs` | 403 `SHELL_DISABLED` when authority is off |

`shell.status` result shape (verified):

```json
{"availableShells":["zsh","sh"],"defaultShell":"zsh","enabled":true,"interactive":false,"maxCommandBytes":16384,"maxOutputBytesPerStream":1048576,"maxTimeoutMs":120000,"platform":"macos"}
```

`shell.run` result shape (verified, `command: "echo hello; echo oops >&2; exit 3"`):

```json
{"durationMs":2,"exitCode":3,"shell":"zsh","stderr":"oops\n","stderrTruncated":false,"stdout":"hello\n","stdoutTruncated":false,"timedOut":false}
```

## Error taxonomy

Every failed JSON response carries a `taxonomy` object next to the legacy
`error`:

```json
{
  "error": {"code": "STALE_SNAPSHOT", "message": "The page changed after the last observation"},
  "taxonomy": {
    "code": "stale_snapshot",
    "retriable": true,
    "recoveryHint": "reobserve",
    "prose": "The snapshot or frame you acted on is no longer current; observe again and retry with fresh identifiers."
  }
}
```

Canonical taxonomy codes: `stale_snapshot`, `stale_ref`, `target_changed`,
`out_of_bounds`, `not_interactable`, `obscured`, `document_changed`,
`lease_lost`, `needs_user`, `blocked_by_policy`, `blocked_by_dialog`,
`sensitive_field`, `outcome_unknown`, `timeout`, `wait_timeout`, `overloaded`,
`protocol_mismatch`, `unavailable`, `invalid_request`, `unknown`. Recovery
hints: `reobserve`, `wait`, `resume`, `handback`, `reconnect`, `none`. An
unmapped code collapses to `unknown` and is never retriable.

HTTP status is derived from the taxonomy class for every connector-originated
failure, so status and taxonomy never disagree:

| Taxonomy class | HTTP status | Meaning |
|---|---|---|
| `invalid_request` | 400 | The request itself is wrong; fix the parameters |
| `blocked_by_policy`, `sensitive_field` | 403 | A standing refusal; do not retry the same request |
| `stale_snapshot`, `stale_ref`, `target_changed`, `out_of_bounds`, `not_interactable`, `obscured`, `document_changed`, `lease_lost`, `blocked_by_dialog`, `wait_timeout` | 409 | The world moved under the request; observe again (or resolve the dialog) and retry |
| `needs_user` | 423 | A human holds the lock; hand back instead of retrying |
| `protocol_mismatch`, `unknown` | 502 | The connector answered with something the server cannot trust |
| `overloaded`, `unavailable` | 503 | The connector is missing or shedding load; reconnect or wait |
| `timeout`, `outcome_unknown` | 504 | The connector never delivered an outcome; observe before acting again |

Three codes deliberately answer a narrower status than their class:
`BAD_COORDINATES` answers 400 (the number is wrong, not the observation),
`COMPUTER_PERMISSION_REQUIRED` answers 403 (a missing OS permission is a
standing refusal), and `NO_PENDING_DIALOG` answers 409 (the request is
well-formed; only the page state does not match).

Codes the server produces for its own refusals, verified against a running
server where noted:

| Status | Codes |
|---|---|
| 400 | `BAD_REQUEST` |
| 401 | `UNAUTHORIZED` (verified) |
| 403 | `HOST_REJECTED` (verified) / `CSRF_REJECTED` (verified) / `ORIGIN_REJECTED` (verified) / `SHELL_DISABLED` (verified) |
| 404 | `NOT_FOUND` (verified) / `NO_SCREENSHOT` (verified) / `NO_COMPUTER_SCREENSHOT` |
| 409 | `CALL_IN_PROGRESS` / `CALL_ID_REUSED` (verified) / `CALL_NOT_IN_PROGRESS` (verified) / `EXTENSION_PROTOCOL_MISMATCH` / `EXTENSION_CAPABILITY_UNAVAILABLE` / `COMPUTER_PROTOCOL_MISMATCH` / `COMPUTER_CAPABILITY_UNAVAILABLE` / `BLOCKED_BY_DIALOG` / `STALE_SCREENSHOT` / `NO_COMPUTER_FRAME` / `NO_BROWSER_OBSERVATION` |
| 413 | `BODY_TOO_LARGE` |
| 415 | `UNSUPPORTED_MEDIA_TYPE` (verified) |
| 429 | `AUTH_BUSY` |
| 502 | `COMPUTER_INVALID_OBSERVATION` |
| 503 | `EXTENSION_HANDSHAKE_PENDING` (verified) / `COMPUTER_HANDSHAKE_PENDING` (verified) / `EXTENSION_DISCONNECTED` / `COMPUTER_DISCONNECTED` / `COMPUTER_SHARE_SESSION_EXHAUSTED` / `SHELL_UNAVAILABLE` |
| 504 | `COMMAND_OUTCOME_UNKNOWN` |
| 500 | `INVALID_SANITIZER_STATE` / `SHELL_FAILED` / `INTERNAL_ERROR` — a broken internal invariant, never a connector's fault |

A connector failure never answers 500: an unclassified connector code is the
connector's fault (502), not the bridge's.
