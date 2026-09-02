# Browser control

How the browser-control lease, observations, and cross-origin frames work.
Method signatures are in [API reference](API_REFERENCE.md#browser-methods-action_methods-srcserverrs);
this page explains the model behind them. For the wire-level detail (exact CDP
calls, timing budgets, proof mechanics), see
[internals/PROTOCOL.md](internals/PROTOCOL.md#browser-control-lease-model).

## Enablement

Requires the unpacked Manifest V3 extension loaded into Chrome or Edge 140+
(`chrome://extensions` or `edge://extensions`, **Developer mode**, **Load
unpacked**) and connected to the running server with a matching token. No
server flag is needed — browser control is available whenever the extension is
connected.

## Full Access vs. Safe mode

Set in the extension popup.

| | Full Access (default) | Safe mode |
|---|---|---|
| Tabs | All controllable HTTP(S) and file tabs | Only allowlisted HTTP(S) tabs |
| Sensitive fields (`page.fill`) | Allowed | Blocked |
| Risky clicks, tab close | Immediate | Two-minute popup approval |
| `page.clickAt`, `page.typeText`, `page.evaluate` | Allowed | Not available |
| Key input | Full chord grammar | Bounded navigation keys only |

Safe mode's risk detection is a conservative text heuristic, not a policy
engine — it can miss ambiguous labels, icons, or canvas UI. Keep the allowlist
narrow and supervise consequential tasks either way.

## The lease model

`browser.control.start` attaches one `chrome.debugger` session to one tab.
While held:

- Chrome shows its own **Local Browser Bridge started debugging this browser**
  warning — the authoritative, page-independent indicator. Nothing is
  injected into the page itself. It cannot be hidden and its **Cancel**
  action always ends the lease.
- The controlled tab is placed in a named **Local Browser Bridge** tab group
  (visible in the tab strip) for the duration of the lease, and the
  extension popup shows **Release control** for the same tab.
- The default lease is 5 minutes; `ttlMs` accepts 15 seconds through 15
  minutes, with a 10-second heartbeat verifying the attachment.

Five freshness values are tracked separately, each invalidated by a different
event:

| Value | Invalidated by |
|---|---|
| `sessionId` (WebSocket) | Extension reconnects or is replaced |
| `controlSessionId` (lease) | Lease starts, stops, or is revoked |
| `turn` | Each `page.observe` |
| `generation` (DOM snapshot) | Navigation, mutation, scroll, resize |
| `moveSequence` (pointer) | Each trusted pointer move the extension dispatches |

Anything that ends the lease — Chrome **Cancel**, popup **Release control**,
TTL expiry, tab closure, or connector loss — is a hard revocation: the next
action fails and a fresh `browser.control.start` is required.

**Human pause is different from revocation.** Chrome **Cancel** on the
debugging warning, and **Release control** in the popup, also latch a global
pause that survives service-worker and browser restarts. While latched,
every remote mutation — including a new `browser.control.start` — returns
423 `HUMAN_CONTROL_PAUSED`. Only clicking **Resume** in the extension's own
popup clears it.

## Observe, act, re-observe

`page.observe` returns a screenshot, page text, selection, and a list of
interactive elements. Each element has a `ref` such as `g3.e12` that embeds
the observation `generation`; acting against a superseded generation fails
`STALE_REF` before any lookup, rather than silently resolving against a newer
page. Every mutating action re-observes automatically, so the returned ref is
fresh for the next call.

`page.batch` runs 1–10 sequential `page.click`/`fill`/`select`/`key`/`scroll`
steps bound to one `generation`, stopping at the first failure — useful for
"fill, fill, click" sequences without a round trip per step.

## JavaScript dialogs

An `alert`/`confirm`/`prompt`/`beforeunload` dialog freezes the page's
renderer. While one is pending, every renderer-touching command (including
read-only `page.observe`) fails 409 `BLOCKED_BY_DIALOG`; only `status`,
`tabs.list`, `browser.control.status`, `browser.control.stop`, and
`page.handleDialog` proceed. `page.handleDialog` accepts or dismisses it
(`{"accept": true, "promptText": "..."}` optional); the lease survives a
dialog — see [internals/PROTOCOL.md](internals/PROTOCOL.md#javascript-dialogs)
for the exact revocation-safety argument.

## Cross-origin frames

Elements inside a same-origin or cross-origin (out-of-process) `<iframe>` are
merged into the same `page.observe` result, with top-level coordinates, and
can be clicked/hovered by ref. A frame-scoped ref looks like
`<generation>.f2.e12`. Bounds: 16 attached frames, 5 levels of nesting, 250
published elements per observation (50 reserved for frame elements). `page.fill`
and `page.select` refuse a frame-scoped ref with `FRAME_ACTION_UNSUPPORTED`
(a stated capability boundary, not a bug); `page.key`, `page.scroll`,
`page.typeText`, and `page.clickAt` accept no `ref` at all. Requires Chrome or
Edge 140+. Full ref grammar, coordinate translation, and the frame-proof
mechanics: [internals/PROTOCOL.md](internals/PROTOCOL.md#cross-origin-frames).

## Key chord grammar

`page.key` accepts canonical (`Meta+L`) and lowercase vendor-style
(`ctrl+shift+t`) chords, the named set `Tab`, `Enter`, `Escape`, `Backspace`,
arrow keys, `PageUp`/`PageDown`, `Home`/`End`, `Space`, `Delete`, `Insert`,
`ContextMenu`, `CapsLock`, `PrintScreen`, `Pause`, `F1`–`F12`, ASCII
letters/digits, and one non-control/non-whitespace Unicode scalar. Use
`page.typeText` for text, especially anything outside the Basic Multilingual
Plane. This grammar is shared with (but narrower- and platform-specific for)
`computer.key` — see [Computer use](COMPUTER_USE.md#keys).

## Safety notes

- Full Access can run arbitrary JavaScript in the page's main world and act
  with that page's signed-in session. It cannot read `HttpOnly` cookie values
  directly, but page-origin requests can still use them.
- The extension never replaces trusted debugger input with an untrusted
  page-generated click, and browser content is inserted into the control UI
  as text, never HTML.
- Password, payment-card, and one-time-code fields are rejected only in Safe
  mode; the server never logs fill text.

See [Security](../SECURITY.md) for the full trust model and
[Troubleshooting](TROUBLESHOOTING.md) for connection problems.
