# Security model

Local Browser Bridge gives an automated system access to pages in a real signed-in browser profile. Treat it like remote-control software even though every transport is local.

Version 0.5.0 enables **Full Access mode by default**. This intentionally removes most action-level safety controls: the allowlist is ignored, sensitive fields can be filled, risky actions and tab closing execute without approval, arbitrary page JavaScript can run, and trusted coordinate/key input is available. Only run it for a local agent you trust. Turn Full Access off in the popup to restore Safe mode.

## Trust boundaries

- The HTTP/WebSocket server binds only to loopback and rejects non-loopback Host headers.
- The extension authenticates with a random token stored outside the repository.
- The server accepts the extension WebSocket only with that token and a `chrome-extension://` Origin.
- The control UI exposes no CORS permission. State changes require a SameSite session cookie, a same-origin `Origin`, and an unpredictable CSRF header.
- Returned tab URLs strip query strings and fragments.
- The bridge control page cannot be selected as a target, preventing recursive self-control.
- The server and control UI are compiled into one Rust binary; no Node.js runtime or package installation is involved.
- The extension contains no remote code, download API, cookie API, native messaging host, telemetry, or update endpoint.

## Release and update trust

- The server makes one bounded HTTPS request to the fixed GitHub Releases API at startup. It sends only a generic product/version `User-Agent`, accepts stable semantic versions and official repository release links, and does not download or install files.
- `--no-update-check` or `LBB_DISABLE_UPDATE_CHECK=1` prevents that request. `--check-updates` performs the same metadata-only check and exits.
- The extension never performs update checks. Unpacked extensions do not silently update on Windows or macOS; the control UI reports a server/extension mismatch so the user can replace both from one release.
- Tagged release builds run on separate GitHub-hosted Windows and macOS workers. Every binary/archive and checksum manifest receives GitHub build provenance, and release immutability prevents later asset or tag replacement.
- Version 0.5.0 artifacts are not yet Microsoft publisher-signed or Apple Developer ID-signed/notarized. Platform warnings are therefore expected for some downloads. Checksums and GitHub provenance detect release tampering but do not replace OS publisher signing or malware notarization.

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
