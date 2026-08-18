# Security model

Local Browser Bridge gives an automated system access to pages in a real signed-in browser profile. Treat it like remote-control software even though every transport is local.

Version 0.2.0 enables **Full Access mode by default**. This intentionally removes most action-level safety controls: the allowlist is ignored, sensitive fields can be filled, risky actions and tab closing execute without approval, arbitrary page JavaScript can run, and trusted coordinate/key input is available. Only run it for a local agent you trust. Turn Full Access off in the popup to restore Safe mode.

## Trust boundaries

- The HTTP/WebSocket server binds only to loopback and rejects non-loopback Host headers.
- The extension authenticates with a random token stored outside the repository.
- The server accepts the extension WebSocket only with that token and a `chrome-extension://` Origin.
- The control UI exposes no CORS permission. State changes require a SameSite session cookie, a same-origin `Origin`, and an unpredictable CSRF header.
- Returned tab URLs strip query strings and fragments.
- The bridge control page cannot be selected as a target, preventing recursive self-control.

## Modes and human approval

In Full Access mode, all controllable HTTP(S) tabs and optionally file tabs are available; approval and sensitive-field interlocks are bypassed. In Safe mode, only allowlisted HTTP(S) tabs are available, risky click labels and tab closing create a two-minute pending approval, and sensitive fields are blocked. Approval can continue only from the target browser's extension popup because the extension declares no `externally_connectable` pages.

Risk detection in Safe mode is a conservative text heuristic, not a complete policy engine. Sites can use ambiguous labels, icons, canvas UI, or deceptive content. Keep the allowlist narrow when using Safe mode and supervise consequential tasks.

## Sensitive data

- Password, payment-card, and one-time-code fields are rejected only in Safe mode.
- The server never logs fill text.
- Tab URLs returned to the server omit query strings and fragments.
- Screenshot and page text remain in server memory and are not written to disk.
- Browser content is treated as untrusted and inserted into the control UI with `textContent`, not HTML.

Full Access can run arbitrary JavaScript in the target page's main world and act with that page's signed-in session. It cannot directly read HttpOnly cookie values, but page-origin requests can still use those cookies. Treat any token holder and any agent that can operate the localhost UI as trusted browser operators.

## Permission rationale

- `tabs`: list, activate, navigate, and capture allowed tabs.
- `scripting` + HTTP(S)/file host permissions: inject the isolated observation/action content script. File access also requires the user-controlled Chrome extension setting.
- `storage`: persist token, port, allowlist, and pending approval.
- `alarms`: reconnect after Manifest V3 service-worker suspension.
- `debugger`: dispatch trusted mouse/key input, insert text, and evaluate page JavaScript, then detach immediately.

## Reporting

Do not include real tokens, screenshots, page text, or authenticated URLs in a public report. Reproduce with the included `/demo` page or mock extension.
