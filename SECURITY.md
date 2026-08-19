# Security model

Local Browser Bridge gives an automated system access to pages in a real signed-in browser profile. Treat it like remote-control software even though every transport is local.

Version 0.8.0 enables browser **Full Access mode by default** and adds an optional native computer helper. Full Access intentionally removes most browser action-level safety controls. The computer helper executes its bounded native input commands immediately while it is running; browser Safe mode does not govern desktop applications. Only run either capability for a local agent you trust.

## Trust boundaries

- The HTTP/WebSocket server binds only to loopback and rejects non-loopback Host headers.
- The extension authenticates with a random token stored outside the repository.
- The server accepts the extension WebSocket only with that token and a `chrome-extension://` Origin.
- The control UI exposes no CORS permission. State changes require a SameSite session cookie, a same-origin `Origin`, and an unpredictable CSRF header.
- Returned tab URLs strip query strings and fragments.
- The bridge control page cannot be selected as a target, preventing recursive self-control.
- The server and control UI are compiled into one Rust binary; no Node.js runtime or package installation is involved.
- The extension contains no remote code, download API, cookie API, native messaging host, telemetry, or update endpoint.
- The native computer helper is a separate, visibly connected process. It opens no listener and connects outbound to the server with the shared token and exact private Origin.
- The server intersects helper-advertised capabilities with a fixed allowlist. No shell, filesystem, process-launch, clipboard, downloader, or telemetry method is accepted or implemented.
- Every native input action is bound to the last delivered exact-window frame. The helper rechecks `(pid, native window id)` ownership and geometry immediately before input and rejects stale frames.
- The helper does not post global HID input, move the hardware cursor, leave the user's focused window changed, activate the target application, change the active desktop, or silently fall back to foreground control. It snapshots those invariants around every action and fails closed if they change.
- Browser and native computer actions share one serialization lock so two local callers cannot intentionally interleave mutations through this server.

The random token authenticates local protocol clients; it is not a sandbox. Malware or another user process that can read the token file or operate the authenticated control page may gain the same authority. Operating-system screen-capture and Accessibility permissions remain the final native boundary.

## Release and update trust

- The server makes one bounded HTTPS request to the fixed GitHub Releases API at startup. It sends only a generic product/version `User-Agent`, accepts stable semantic versions and official repository release links, and does not download or install files.
- `--no-update-check` or `LBB_DISABLE_UPDATE_CHECK=1` prevents that request. `--check-updates` performs the same metadata-only check and exits.
- The extension never performs update checks. Unpacked extensions do not silently update on Windows or macOS; the control UI reports a server/extension mismatch so the user can replace both from one release.
- Tagged release builds run on separate GitHub-hosted Windows and macOS workers. Every binary/archive and checksum manifest receives GitHub build provenance, and release immutability prevents later asset or tag replacement.
- Version 0.8.0 artifacts are not yet Microsoft publisher-signed or Apple Developer ID-signed/notarized. The macOS helper app is ad-hoc signed so its bundle is structurally valid, not to claim a verified publisher identity. Platform warnings and permission re-prompts are therefore possible. Checksums and GitHub provenance detect release tampering but do not replace OS publisher signing or malware notarization.

## Modes and human approval

In Full Access mode, all controllable HTTP(S) tabs and optionally file tabs are available; approval and sensitive-field interlocks are bypassed. In Safe mode, only allowlisted HTTP(S) tabs are available, risky click labels and tab closing create a two-minute pending approval, and sensitive fields are blocked. Approval can continue only from the target browser's extension popup because the extension declares no `externally_connectable` pages.

Risk detection in Safe mode is a conservative text heuristic, not a complete policy engine. Sites can use ambiguous labels, icons, canvas UI, or deceptive content. Keep the allowlist narrow when using Safe mode and supervise consequential tasks.

## Sensitive data

- Password, payment-card, and one-time-code fields are rejected only in Safe mode.
- The server never logs fill text.
- Tab URLs returned to the server omit query strings and fragments.
- Screenshot and page text remain in server memory and are not written to disk.
- Browser content is treated as untrusted and inserted into the control UI with `textContent`, not HTML.
- Native exact-window screenshots remain in server memory, are served only from loopback under the control page's same-origin policy, and are replaced on the next capture. They exclude unrelated windows and notifications, although the selected target can itself contain sensitive content.

Full Access can run arbitrary JavaScript in the target page's main world and act with that page's signed-in session. It cannot directly read HttpOnly cookie values, but page-origin requests can still use those cookies. Treat any token holder and any agent that can operate the localhost UI as trusted browser operators.

Native computer input is hybrid semantic/pixel control bound to one exact application window. Semantic refs are tied to the captured frame and re-resolved before use, but a ref still does not detect prompt injection, prove human intent, or make an action harmless. Accessibility and background pixel delivery remain application-framework dependent and can be refused by secure input, protected content, games, custom-rendered controls, or elevated Windows targets. Observe after every action and supervise consequential workflows.

## Native permission rationale

- Screen Recording is required on macOS to capture desktop pixels.
- Accessibility is required on macOS to expose semantic elements, invoke supported actions, set control values, and help route input to an exact process/window. Pixel delivery also dynamically resolves undocumented SkyLight symbols; platform updates can disable that route, in which case the helper reports input unavailable rather than using global HID input.
- Windows input must run in the signed-in interactive session; the helper is not installed as a service and requests no administrator elevation for normal use.
- The macOS release uses a named `.app` bundle because TCC grants attach to application identity. Run that packaged helper rather than copying its inner executable to an arbitrary location.

## Permission rationale

- `tabs`: list, activate, navigate, and capture allowed tabs.
- `scripting` + HTTP(S)/file host permissions: inject the isolated observation/action content script. File access also requires the user-controlled Chrome extension setting.
- `storage`: persist token, port, allowlist, and pending approval.
- `alarms`: reconnect after Manifest V3 service-worker suspension.
- `debugger`: dispatch trusted mouse/key input, insert text, and evaluate page JavaScript, then detach immediately.

## Reporting

Do not include real tokens, screenshots, page text, or authenticated URLs in a public report. Reproduce with the included `/demo` page or mock extension.
