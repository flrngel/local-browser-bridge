## REST API

`POST /api/v1/command` exposes the same allowlisted methods. Bearer-token clients receive server-sanitized results; they never speak the connector WebSocket envelope directly. The localhost UI exchanges the master bridge token for an expiring random `Session` authorization capability and CSRF value. The capability is kept in the exact port origin's `sessionStorage`, not a host-wide localhost cookie. `/api/state`, `/api/events`, `/api/screenshot`, and `/api/computer/screenshot` require that session or a bearer token; mutations additionally require same-origin and CSRF proof.

### Agent Fetch GET-only API

`GET /api/v1/fetch/{capability}/{method}` exposes the same command dispatcher
for agents whose only local networking primitive is a basic Web Fetch GET. The
dashboard and server startup output provide the complete private Agent Fetch
base URL. Its capability is a domain-separated SHA-256 derivation of the master
bridge token: disclosing it does not disclose the master token, rotating the
master token revokes it, and it is compared in constant time. It is still a
credential with command authority and must be protected like the master token.

Top-level query keys become method parameters. Canonical numbers, `true`,
`false`, and `null` become JSON scalars; other values remain strings. Prefix a
value with `str:` to force a numeric-looking string or `json:` for one explicit
JSON value. Alternatively, put one URL-encoded JSON object in `params`; it
cannot be mixed with direct keys.

```text
GET {AGENT_FETCH_BASE_URL}/status
GET {AGENT_FETCH_BASE_URL}/tabs.list
GET {AGENT_FETCH_BASE_URL}/page.navigate?callId=navigate-1&url=https%3A%2F%2Fexample.com
GET {AGENT_FETCH_BASE_URL}/page.click?callId=click-1&params=%7B%22ref%22%3A%22g4.e2%22%2C%22generation%22%3A%22g4%22%7D
```

Every method except `status`, `tabs.list`, `browser.control.status`,
`page.waitFor`, `computer.status`, `computer.share.status`, and `shell.status`
requires `callId`. The GET transport uses the same atomic admission, cached
result, outcome-unknown fencing, and replay namespace as POST, so retrying the
exact URL cannot dispatch the action twice. Reusing its `callId` with different
parameters returns `CALL_ID_REUSED`. Cancel an in-flight GET with
`GET {AGENT_FETCH_BASE_URL}/cancel?callId=...`.

The server sends `Cache-Control: no-store` and `Referrer-Policy: no-referrer`,
keeps request logging disabled, rejects a browser-declared cross-site fetch,
and still accepts only the exact loopback Host. The URL can nevertheless be
retained by the calling agent, browser history, proxies, screenshots, or
diagnostics outside the bridge. Do not place secrets in query parameters and
prefer the bearer POST API when the client supports headers and request bodies.

### Local shell methods

`shell.status` reports whether shell authority is enabled and the native shell
set. `shell.run` is accepted only when the server was started with
`--enable-shell` or `LBB_ENABLE_SHELL=1`.

```json
{
  "method": "shell.run",
  "callId": "shell-42",
  "params": {
    "command": "pwd",
    "shell": "default",
    "cwd": "/path/to/worktree",
    "timeoutMs": 30000
  }
}
```

Windows supports `powershell` (the default) and `cmd`; macOS supports `zsh`
(the default) and `sh`. The command is non-interactive with null stdin. Each
stream is retained up to 1 MiB while excess bytes are drained and marked
truncated; commands are limited to 16 KiB and 120 seconds. Timeout terminates
the process tree and returns `timedOut: true`. A nonzero exit is a completed
command result, not a transport error. The activity log records only completion
status, never the command, working directory, stdout, or stderr.

Shell authority is full current-user code execution. It is not part of the
computer helper, does not inherit exact-window restrictions, and is not a
sandbox or approval system.

### Idempotent command replay

`POST /api/v1/command` accepts an optional `callId` (1–128 characters). Admission is atomic: while a command with that `callId` is in flight, a duplicate returns HTTP 409 `CALL_IN_PROGRESS` and nothing is dispatched twice. After completion, the exact final body and HTTP status—success or failure, with `callId` echoed top-level—are cached (256 entries, ten-minute TTL); a replay returns the cached response with `"replayed": true` without touching any connector. If the HTTP client disconnects before a command's outcome is recorded, the action may still execute, so the `callId` ultimately becomes a cached HTTP 504 `COMMAND_OUTCOME_UNKNOWN` failure (taxonomy `outcome_unknown`, hint `reobserve`): retries of that `callId` replay this failure and never re-dispatch, and the caller must observe before acting again. For an owner-bound computer mutation, handler teardown first marks the replay entry interrupted while its action future still owns the global action lock. The entry remains 409 `CALL_IN_PROGRESS` until the exact-session public-state quarantine is installed; only then can the 504 enter the replay cache. A different `callId` already waiting on the action lock must help finish that quarantine before reading frame authority or relaying, and therefore receives `NO_COMPUTER_FRAME` for the revoked old frame rather than consuming it. A request without `callId`, including legacy `/api/action`, has no public replay identity, so the server assigns an internal request UUID and registers its exact helper owner before relay. The registration remains active through response publication; handler Drop marks it interrupted before releasing the action lock, and a fresh computer waiter helps finish the same exact-session quarantine before using state. Thus the no-`callId` form remains fail-closed across both mid-action and post-action/pre-response disconnects, but it cannot offer idempotent replay. If teardown happens after the async runtime is no longer available, an owner-bound entry stays pending for a later waiter instead of exposing unsafe authority; shutdown discards that server instance and both interruption registries together. Each `callId` registration is fingerprinted over the method and canonical parameters; reusing a `callId` for a different command returns HTTP 409 `CALL_ID_REUSED` (taxonomy `invalid_request`) instead of replaying the other command's outcome. The bridge has exactly one bearer token, so all commands share one replay namespace.

### Explicit command cancellation

`POST /api/v1/command/cancel` accepts JSON `{ "callId": "..." }` under the same loopback Host, bearer-token, Origin, body-size, and JSON Content-Type boundary as the command endpoint. Only a currently in-flight command that was atomically registered with that exact `callId` is cancellable. Acceptance returns HTTP 202 `{ "ok": true, "callId": "...", "cancellationRequested": true }`; a missing, completed, or already-canceled ID returns HTTP 409 `CALL_NOT_IN_PROGRESS`. There is no unauthenticated or global cancel.

```http
POST /api/v1/command/cancel HTTP/1.1
Authorization: Bearer <bridge-token>
Content-Type: application/json

{"callId":"agent-request-42"}
```

```json
{"ok":true,"callId":"agent-request-42","cancellationRequested":true}
```

Cancellation and normal completion linearize under the replay registry's mutex. If cancellation wins, the entry is synchronously fenced as interrupted, browser recovery is latched before the exact action future can release the action lock, and one session/id/sequence-bound connector cancel is emitted if dispatch occurred. Authority settlement then runs through the same single-claimer path used for handler disconnect: an aborted settler releases its claim so another waiter can finish, and the replayable HTTP 504 `COMMAND_OUTCOME_UNKNOWN` is stored only after every bound browser and computer fail-closed boundary is installed. The original receives that 504; the same payload replays it without redispatch, while a different payload remains `CALL_ID_REUSED`. The 202 confirms a cancellation request, not rollback or proof that no side effect occurred. Canceled computer mutations immediately invalidate the owning helper session's published share, frame, pointer, and screenshot authority; a replacement helper session is never cleared. Browser cancellation preserves the user's browser-control lease when durable freshness advancement succeeds.

For browser control, the server does not depend on the bounded connector queue accepting the cancel or on the extension returning a freshness event. At the same cancellation boundary it latches recovery for the exact extension session, clears the published browser observation and screenshot, and refuses controlled-page mutations before relay with `NO_BROWSER_OBSERVATION`. The same quarantine runs when an HTTP handler disappears after dispatch (with a `callId`, without one, or through legacy `/api/action`) and whenever the connector reports a post-dispatch `COMMAND_OUTCOME_UNKNOWN`, including its own timeout. A dropped handler installs the gate before releasing the shared action lock; a registered retry remains `CALL_IN_PROGRESS` until asynchronous public-state cleanup completes, then replays 504. Only a successful, caller-requested `page.observe` for that session clears the latch. Read-only status, `tabs.list`, `page.waitFor`, human `browser.control.stop`, `browser.control.start`, and the dialog escape `page.handleDialog` remain available while recovering. The extension independently advances and durably stores the lease turn, clears `frameSnapshot` immediately and again at its final serialized queue barrier, and publishes the updated public control state before its next command. The visible lease therefore survives when safe, but an old generation or turn cannot authorize another mutation even if the cancel envelope or event is delayed or lost.

The explicit browser freshness set is `browser.control.start`, `page.observe`, and every controlled `page.*` command except `page.waitFor`. `page.observe` is read-only but advances the turn before later capture awaits; `browser.control.start` can renew or establish a lease before returning; the remaining entries can cross a renderer or CDP side-effect boundary. `status`, `browser.control.status`, `tabs.list`, and `page.waitFor` neither consume nor advance controlled-page freshness. `tabs.activate`, `tabs.new`, and `tabs.close` are browser-process mutations but do not consume a lease turn, DOM generation, screenshot, or element ref, so they are not allowed to clear or advance unrelated controlled-page authority. Their canceled result is still outcome-unknown and must never be retried under a fresh `callId`; use `tabs.list` to reconcile tab state first. `tabs.new` commits every created tab's bridge provenance before group decoration, and a late creation holds the global browser-action queue—including trusted popup-approved dispatch—until provenance persistence and canceled-command freshness finalization finish; this also keeps an omitted-URL `about:blank` visible to Safe-mode reconciliation. The original `callId` remains non-dispatching, so no retry behavior is inferred from a stale observation.

If a canceled `page.handleDialog` actually closed the dialog but both its result and `page.dialogClosed` event were lost, the recovery-safe retry can return `NO_PENDING_DIALOG`. For the exact current extension session, that browser-process answer clears the stale public dialog gate but does not clear freshness recovery; the caller must then run `page.observe`. This prevents a lost close event from deadlocking the only recovery path.

Computer revocation and publication linearize under the public-state write lock. Once the gate is installed for helper session S, a queued or late `computer.share.frame` from S is acknowledged when pacing was negotiated but cannot republish its share, observation, pointer, or screenshot. Until explicit recovery, computer mutations fail server-side with HTTP 409 `NO_COMPUTER_FRAME`; `computer.status`, `computer.share.status`, recovery through `computer.observe` or `computer.share.start`, and the safe inactive `computer.share.stop` remain available. Status methods are descriptive only: while gated, both public state and their returned REST result retain the fail-closed share status instead of echoing a stale active reply. Recovery requires an explicit successful `computer.observe` whose one-shot frame carries no share authority, or an explicit successful `computer.share.start` followed by its first exact-ID observation. The latter authorizes only its exact new share ID; old and mismatched frame IDs remain quarantined, an exact-ID share error can stop it, and only a raw `active: false` `computer.share.stop` result can publish safe inactive state. Tests and packaged evidence rigs must distinguish these two refusal layers: an old-frame mutation before same-session recovery is rejected by the server as `NO_COMPUTER_FRAME` with zero helper relay; after an explicit share-free observation recovers the gate, retrying that retired frame reaches the helper's frame lease check and is rejected as `COMPUTER_STALE_FRAME`. The gate remains for that helper session after recovery and resets at the helper connection/handshake boundary. If a canceled Windows action replaces its disposable worker, the replacement session does not inherit the old session's gate; it nevertheless starts without observation dimensions, so a normalized-coordinate mutation returns `NO_COMPUTER_FRAME` until explicit observe. That Windows no-frame refusal is distinct from the macOS evidence rig's same-session authority-gate refusal. Retired share IDs are retained up to 256 epochs; saturation returns HTTP 503 `COMPUTER_SHARE_SESSION_EXHAUSTED` (`unavailable`, recovery hint `reconnect`) rather than evicting a revoked ID. In that intentionally extreme case, `reconnect` means establish a new helper WebSocket session: relaunch the macOS helper, or restart the Windows helper if its supervisor has not already replaced the worker after transport loss.

The identity model follows the same request-scoped shape used by MCP's canceled-request notification and LSP's `$/cancelRequest`: cancellation names one caller request ID and never means “stop everything.” `callId` remains this REST API's idempotency/cancellation identity; the connector's internal UUID plus session and sequence remain separate transport identity and are never supplied by the caller.

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

#### HTTP status of a connector failure

When a failure comes back from a connector—the browser extension or the computer helper—its HTTP status is **derived from its taxonomy class**, so the status and the `taxonomy` object can never disagree and no connector state is ever reported as a local server fault. The class table:

| Taxonomy class | HTTP status | What the status tells the client |
| --- | --- | --- |
| `invalid_request` | 400 Bad Request | The request itself is wrong; fix the parameters |
| `blocked_by_policy`, `sensitive_field` | 403 Forbidden | A standing refusal; do not retry the same request |
| `stale_snapshot`, `stale_ref`, `target_changed`, `out_of_bounds`, `not_interactable`, `obscured`, `document_changed`, `lease_lost`, `blocked_by_dialog`, `wait_timeout` | 409 Conflict | The world moved under the request; observe again (or resolve the dialog) and retry |
| `needs_user` | 423 Locked | A human holds the lock and only a human can release it; hand back instead of retrying |
| `protocol_mismatch`, `unknown` | 502 Bad Gateway | The connector answered with something the server cannot trust or read |
| `overloaded`, `unavailable` | 503 Service Unavailable | The connector is missing or shedding load; reconnect or wait |
| `timeout`, `outcome_unknown` | 504 Gateway Timeout | The connector never delivered an outcome; observe before acting again |

A connector failure therefore never answers 500: an unclassified code is the connector's fault (502), not the bridge's. So a human using Chrome's **Cancel** action or the extension popup's **Release control** makes commands answer **423 `HUMAN_CONTROL_PAUSED` with taxonomy `needs_user`** and hint `handback`, not a 500 that would read as a retriable server error.

Three connector codes deliberately answer a narrower status than their class, because the class is right about the recovery and the status is more precise about the cause: `BAD_COORDINATES` answers 400 (the number itself is wrong, not the observation), `COMPUTER_PERMISSION_REQUIRED` answers 403 (a missing operating-system permission is a standing refusal, not a lock a handback resumes), and `NO_PENDING_DIALOG` answers 409 (the request is well formed; only the page state does not match). There are no other exceptions.

Statuses the server produces for its **own** refusals are explicit: 400 `BAD_REQUEST`, 401 `UNAUTHORIZED`, 403 `HOST_REJECTED`/`CSRF_REJECTED`/`ORIGIN_REJECTED`, 404 `NOT_FOUND`/`NO_SCREENSHOT`/`NO_COMPUTER_SCREENSHOT`, 409 `CALL_IN_PROGRESS`/`CALL_ID_REUSED`/`CALL_NOT_IN_PROGRESS`/`EXTENSION_PROTOCOL_MISMATCH`/`EXTENSION_CAPABILITY_UNAVAILABLE`/`COMPUTER_PROTOCOL_MISMATCH`/`COMPUTER_CAPABILITY_UNAVAILABLE`/`BLOCKED_BY_DIALOG`/`STALE_SCREENSHOT`/`NO_COMPUTER_FRAME`/`NO_BROWSER_OBSERVATION`, 413 `BODY_TOO_LARGE`, 415 `UNSUPPORTED_MEDIA_TYPE`, 429 `AUTH_BUSY`, 502 `COMPUTER_INVALID_OBSERVATION`, 503 `EXTENSION_HANDSHAKE_PENDING`/`COMPUTER_HANDSHAKE_PENDING`/`EXTENSION_DISCONNECTED`/`COMPUTER_DISCONNECTED`/`COMPUTER_SHARE_SESSION_EXHAUSTED`, and the 504 `COMMAND_OUTCOME_UNKNOWN` cached for an interrupted or canceled `callId`. The only 500 the API can still answer is `INVALID_SANITIZER_STATE`, a broken internal invariant of the server itself, which is exactly what a 500 should mean.

The native allowlist intentionally contains no shell, filesystem, process-launch, clipboard, downloader, arbitrary-code, credential-store, user-management, or telemetry method.
