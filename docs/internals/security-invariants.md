# Security invariants

Implementation-level detail behind [Security](../../SECURITY.md), for anyone
auditing the code or implementing a compatible connector. Every claim here is
backed by a Rust or extension contract test.

## Transport and authentication

- The HTTP/WebSocket server binds only to loopback and rejects non-loopback
  `Host` headers.
- The sole bearer token authorizes both command dispatch and exact `callId`
  cancellation. Any token holder can cancel any currently in-flight bearer
  command, so `callId` is an idempotency/request identity, not a secret or a
  separate authorization capability; there is no global or unauthenticated
  cancel.
- Connector WebSockets carry no bearer, query, or raw token. The extension
  and helper use a three-second, size-bounded mutual HMAC-SHA256 handshake
  that binds role, connector, a fresh client nonce, a fresh server nonce, and
  the server-created session ID. A connector verifies server proof before
  sending its own proof or reading browser/native state; the server attaches
  it to the active hub only after constant-time client-proof verification.
- The extension additionally requires an exact Chrome-extension Origin, and
  the helper requires exact Origin `lbb-computer-helper://local`. Provisional
  unauthenticated sockets are concurrency-bounded and cannot replace an
  active connector.
- The control URL places the master token only in a fragment, which the page
  removes immediately before exchanging it for an expiring random session
  capability kept in the dashboard's port-specific `sessionStorage`. No
  localhost cookie is used or exposed to other ports. State, event, and
  screenshot reads require that session or a bearer token; state changes
  additionally require a same-origin `Origin` and an unpredictable CSRF
  header. No API exposes CORS permission.
- Returned tab URLs strip query strings and fragments. The bridge control
  page cannot be selected as a target, preventing recursive self-control.
- The GET-only Agent Fetch URL contains a domain-separated capability derived
  from the master token. It shares POST's command replay and requires
  `callId` for actions, rejects browser-declared cross-site use, and returns
  no-store/no-referrer headers.

## Token storage

The extension authenticates with a canonical URL-safe encoding of 32 random
bytes stored outside the repository. Empty, weak, malformed, or permissive
persisted tokens are rotated atomically. Only the exact computed default
`.local-browser-bridge` parent under the current user's absolute profile path
is bridge-managed and may be created or permission-hardened; missing or
invalid profile metadata fails closed instead of falling back to the working
directory, and a matching name elsewhere is still custom. Every custom parent
is validated as already private and is never created or rewritten. Unix
requires current-user mode `0700` on the parent and mode `0600` on a
single-link token opened without following symlinks; special entries are
inspected nonblocking before replacement. Windows requires a protected
TokenUser-only DACL on both paths, rejects a reparse-point final parent and
reparse children, multiply linked token files, alternate-stream/device-name
ambiguity, and resolves each exact-case child through a retained parent
handle. A private typed capability retains the exact write-through temporary
handle across flush and atomic native `NtSetInformationFile(FileRenameInformation)`
replacement relative to that parent; every pre-commit cleanup marks that same
internally created handle for deletion rather than reopening a pathname.

The extension popup persists its copy of that token in
trusted-context-only `chrome.storage.local`; disabling Bridge control pauses
the connector but does not erase the credential. **Clear saved token** is
restricted to the extension's own popup URL, synchronously invalidates the
controller identity, cancels not-yet-dispatched work, revokes control,
disconnects, clears any pending approval and its badge, removes the storage
entry, then verifies both the not-configured state and
`tokenConfigured: false` before reporting success.

## Outcome-unknown recovery

A controlled-page command with an unknown outcome cannot reuse earlier
observation authority. The server synchronously latches exact-session
recovery before waking an explicit cancel, before a dropped HTTP handler
releases the shared action lock (including no-`callId` and legacy dashboard
actions), and on any post-dispatch connector outcome-unknown result. It
removes the public observation and screenshot and refuses later page
mutations until an explicit `page.observe` succeeds. This boundary does not
depend on the connector queue delivering its cancel. The extension separately
advances and persists the lease turn, clears its frame snapshot at
cancellation and again at the final queue barrier, and revokes the otherwise
preserved lease if persistence fails.

Canceled `tabs.activate`, `tabs.new`, and `tabs.close` calls remain
outcome-unknown but do not consume DOM generation, screenshot, element-ref,
or browser-control-turn authority. Never replace their `callId` and retry:
reconcile with `tabs.list` first.

## Native computer authority

- One-shot native observation and live sharing are separate. Live sharing
  binds a persistent ScreenCaptureKit stream on macOS or Windows Graphics
  Capture session on Windows to one exact `(process, native window)` target,
  disables the system cursor, and forwards bounded PNG frames through a
  single latest-frame slot under a requested 1–10 FPS cap.
- Every native input action is bound to an exact-window frame. A
  share-derived frame remains usable for at most three seconds only while
  `(share id, pid, native window id, geometry)` still exactly matches the
  current share; all other stale frames fail closed.
- Native action proof has three separate layers: a sealed exact-target
  route, an operating-system API acceptance signal where one exists, and a
  target-owned postcondition. Only the postcondition can confirm the
  requested effect.
- The helper does not intentionally post global HID input, request a
  hardware-cursor mutation, leave the platform's foreground/window-focus
  oracle changed after an accepted action, switch the OS-front process,
  raise the target with `AXRaise`, change the active desktop/Space, or
  silently fall back to foreground control. macOS focus-capable input may
  nevertheless use a private, transient target `AXFrontmost` focus lease:
  the saved user's AX state is released, the exact target becomes
  `AXFrontmost=true`, and both are restored while WindowServer's user-front
  process/window remains unchanged. The machine contract therefore reports
  `activatesTargetApplication: true` on macOS. On Windows there is no
  explicit target-activation API in the backend. These samples are not an
  atomic rollback guarantee or proof that no shorter visible or
  focus-state interruption occurred.
- `cursorPositionUnchanged` is a diagnostic sample of one shared global
  pointer, not action-source authority. HID-system activity can come from a
  physical device, virtual HID, remote input, or another platform route; it
  is never physical-device provenance. Unknown route or monitor state fails
  closed.
- The macOS target-event route depends on undocumented, unsupported private
  SkyLight interfaces. Source and packaged Mach-O audits forbid known global
  cursor/HID APIs and freeze the expected targeted symbol set, but they
  cannot turn a private interface into a supported Apple contract.

## Control indicators (no in-page presence)

The extension injects nothing into the controlled page — there is no page
DOM, shadow root, or CSS surface for a hostile document to spoof, hide, or
race. The only visible, page-independent signals that a tab is under remote
control are Chrome's own "Local Browser Bridge started debugging this
browser" warning and Cancel button (authoritative, and the sole browser-owned
signal), the named **Local Browser Bridge** tab group the extension places
the controlled tab in for the duration of the lease, and **Release
control**/**Resume** in the extension popup. All three exist entirely outside
the page's own rendering and script execution context, so none of them
depend on the same experimental CDP DOM methods, animation-frame timing, or
proof budgets that an in-page indicator would need.

## Cross-origin frame attachment (blast radius)

The controlled-tab lease enables `Target.setAutoAttach` on the page target,
then repeats it on a child session only after a routing probe proves that
the session returns its own frame ID:

- Any auto-attached target whose `targetInfo.type` is not `iframe` is
  `Target.detachFromTarget`ed immediately, as is any target arriving without
  a matching lease or live parent session, past the 16-frame/five-level
  caps, or for a blank document.
- Child sessions never get their own debugger attachment; they live under
  the single `chrome.debugger.attach` for the leased tab, and the existing
  `chrome.debugger.detach` tears every one of them down.
- The in-frame agent runs in a dedicated isolated world
  (`Page.createIsolatedWorld`, `grantUniveralAccess: false`), never the
  page's main world, and is read-only: no click, focus, event dispatch,
  value write, or extension messaging surface, and it never synthesizes
  input. Trusted input is always dispatched on the page target at a
  translated top-level point.
- Frame observation is read-only and bounded to a shared 4-second budget; a
  slow or unresponsive third-party iframe skips (`frame_timeout`,
  `budget_time`) rather than revoking browser control.

## Release provenance

Tagged release builds run on separate GitHub-hosted Windows and macOS
workers. Every binary/archive and checksum manifest receives GitHub build
provenance, and release immutability prevents later asset or tag
replacement. Both executables embed the project license and generated
notices for the exact locked production dependency graph behind
`--licenses`. See [Verify a release](../VERIFY_RELEASE.md) for the exact
verification commands.
