# Security model

Local Browser Bridge gives an automated system access to pages in a real signed-in browser profile. Treat it like remote-control software even though every transport is local.

## Trust boundaries

- The HTTP/WebSocket server binds only to loopback and rejects non-loopback Host headers.
- The extension authenticates with a random token stored outside the repository.
- The server accepts the extension WebSocket only with that token and a `chrome-extension://` Origin.
- The control UI exposes no CORS permission. State changes require a SameSite session cookie, a same-origin `Origin`, and an unpredictable CSRF header.
- The extension returns only allowlisted tabs. It strips URL query strings and fragments before returning URLs.
- The bridge control page cannot be selected as a target, preventing recursive self-control.

## Human approval

Risky click labels and tab closing create a two-minute pending approval. Execution can continue only from the target browser's extension popup. The localhost web agent cannot invoke popup messages because the extension declares no `externally_connectable` pages.

Risk detection is a conservative text heuristic, not a complete policy engine. Sites can use ambiguous labels, icons, canvas UI, or deceptive content. Keep the allowlist narrow and supervise consequential tasks.

## Sensitive data

- Password, payment-card, and one-time-code fields are rejected by the fill command.
- The server never logs fill text.
- Tab URLs returned to the server omit query strings and fragments.
- Screenshot and page text remain in server memory and are not written to disk.
- Browser content is treated as untrusted and inserted into the control UI with `textContent`, not HTML.

The extension can still read visible content and screenshots from explicitly allowed sites. Do not allow a site unless the task needs it.

## Permission rationale

- `tabs`: list, activate, navigate, and capture allowed tabs.
- `scripting` + HTTP(S) host permissions: inject the isolated observation/action content script.
- `storage`: persist token, port, allowlist, and pending approval.
- `alarms`: reconnect after Manifest V3 service-worker suspension.
- `debugger`: dispatch trusted mouse/key input and detach immediately. It is not used for cookies, network bodies, or storage extraction.

## Reporting

Do not include real tokens, screenshots, page text, or authenticated URLs in a public report. Reproduce with the included `/demo` page or mock extension.
