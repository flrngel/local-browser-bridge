# Security model

Local Browser Bridge gives an automated system access to pages in a real
signed-in browser profile, and optionally to one native desktop window and a
native shell. Treat it like remote-control software, even though every
transport is local. This page is the model to accept before running it; the
underlying implementation invariants that back these claims are in
[docs/internals/security-invariants.md](docs/internals/security-invariants.md).

## Threat model, in five points

1. **Trust boundary is loopback, not the internet.** The server binds only to
   `127.0.0.1` and rejects any other `Host` header before authentication runs.
   It has no cloud relay and no remote attack surface by design — the risk is
   local: another process, user, or page on this same machine.
2. **The bearer token is the master key.** Anyone who holds it can dispatch
   any command, including shell commands if enabled, and can cancel any
   in-flight command from any client (there is no per-client cancel scope).
3. **Full Access is the default.** It removes most action-level interlocks —
   sensitive-field entry, click approvals, arbitrary main-world JavaScript —
   and trusts the connected agent with the signed-in browser session. Safe
   mode narrows this to an allowlist plus approvals; it is a heuristic, not a
   policy engine.
4. **The computer helper and shell share your session, not an isolated one.**
   Native input is best-effort target-routed input at one application window
   — it is cooperative with the person at the keyboard, not a separate
   desktop or input queue. Shell access, when enabled, is full current-user
   command execution with no sandbox.
5. **A human always has an out.** Chrome's own debugging warning and Cancel
   button, the in-page Stop control, and the extension popup's Release
   control/Resume all work independently of the API and cannot be bypassed
   by a compromised or malicious agent process.

## What each credential grants

| Credential | Grants | Rotate by |
|---|---|---|
| Bearer token | Full command dispatch (browser, computer, shell if enabled), and cancellation of any in-flight command | Restart the server without `LBB_TOKEN`/an existing token file, or delete `~/.local-browser-bridge/token` |
| Dashboard session + CSRF | Same as bearer, scoped to one browser tab's `sessionStorage`, obtained by exchanging the token | Expires after 12 hours idle; close the dashboard tab |
| Agent Fetch capability URL | Same command authority as the bearer token, via GET only | Rotating the bearer token also revokes this, since it is derived from the token |

All three are credentials. Never paste them into logs, screenshots, issue
reports, or a page you do not trust.

## Full Access vs. Safe mode

Full Access (default) can act in any controllable tab, enter sensitive text,
and run arbitrary JavaScript in the page's main world; it cannot read
`HttpOnly` cookie values directly, but page-origin requests can still use
them. Safe mode restricts control to an allowlist, blocks sensitive fields,
and requires one-time popup approval for risky clicks and tab closes — its
risk detection is a conservative text heuristic that can miss ambiguous
labels or canvas UI, so keep the allowlist narrow and supervise consequential
tasks either way. Switch modes in the extension popup. A pending approval can
only be granted or rejected from that same target browser's own extension
popup, is bound to the exact server session and extension connection that
queued it, and is discarded — clearing its badge — if the token, port, or
mode rotates before someone acts on it. Neither mode, nor a
`page.observe`/`computer.observe` semantic ref, detects prompt injection or
proves human intent — page and window content reaching the agent this way is
untrusted and can try to steer it, so treat every observation as data, not
instruction, and re-observe after each action rather than trusting a plan
made before it.

## Shell is full user authority

`--enable-shell` / `LBB_ENABLE_SHELL=1` grants an authenticated client the
same command authority as the signed-in user: read, modify, launch, or delete
anything that account can touch. It is off by default, bounded in size/time/
output, and never logs command text — but it is not a sandbox, not scoped to
a directory, and not confined by the browser or computer-use model. See
[Shell](docs/SHELL.md).

## The computer helper is a shared seat

Native input can avoid moving the shared hardware cursor and can leave the
foreground application unchanged before/after a supported action. On macOS,
focus-capable input may still briefly use a private, transient
`AXFrontmost` lease — the machine-readable capability record reports
`activatesTargetApplication: true` on macOS for exactly this reason, and
these before/after samples are not proof of zero visible or focus-state
interruption. None of this creates a second desktop, input queue, or
security principal; see [Limitations](docs/LIMITATIONS.md) for the full
argument. The helper exposes no shell, filesystem, clipboard, or
process-launch method, and the server independently intersects whatever
capabilities a connected helper advertises against a fixed allowlist — a
compromised or modified helper cannot grant itself a method the server does
not already recognize.

## Sensitive data

Password, payment-card, and one-time-code fields are rejected only in Safe
mode. The server never logs fill text or shell command text. Tab URLs are
returned with query strings and fragments stripped. Screenshots and page text
live only in server memory, never on disk, and are served only to an
authenticated session or bearer client. Native accessibility password values
are never read or writable through semantic control.

## Extension permissions

The extension requests exactly six Chrome permissions, each for one purpose:

| Permission | Why |
|---|---|
| `tabs` | List, activate, navigate, and identify controllable tabs |
| `scripting` | Inject the isolated observation/action content script into HTTP(S)/file pages |
| `storage` | Persist the token, port, allowlist, and any pending Safe-mode approval |
| `alarms` | Reconnect the transport, expire leases, and recover the connection after the service worker is suspended |
| `tabGroups` | Visibly group tabs the bridge created; grouping never grants authority |
| `debugger` | Hold one explicit controlled-tab lease so Chrome shows its native debugging warning, then dispatch trusted input and evaluate page JavaScript |

It declares no `cookies`, `downloads`, or `nativeMessaging` permission, ships
no remote code (its content-security-policy is `script-src 'self'`), and
sends no telemetry.

## Release trust

Windows executables are not yet Microsoft publisher-signed; the macOS package
is ad-hoc signed but not Developer ID-signed or notarized — expect a
SmartScreen or Gatekeeper warning, and keep both protections enabled. Tagged
releases build on separate GitHub-hosted workers with build provenance and an
immutable checksum manifest; verify both before running a downloaded binary.
See [Verify a release](docs/VERIFY_RELEASE.md) for the exact commands.

## Reporting

Do not include real tokens, screenshots, page text, or authenticated URLs in
a public report. Reproduce with the included `/demo` page or a mock
extension, and open a private security advisory on GitHub rather than a
public issue for anything exploitable.
