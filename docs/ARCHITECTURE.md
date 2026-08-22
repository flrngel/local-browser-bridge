# Architecture

Local Browser Bridge is a local coordinator between a browser-based agent, a Chromium extension, and an optional native computer helper. The server has no cloud relay and always binds to `127.0.0.1`.

```text
same-machine agent browser
          |
          v
authenticated control page / REST API
          |
          v
standalone Rust server
    |                         |
    v                         v
Chromium extension       native helper process
    |                         |
    v                         v
leased browser tab       exact application window
```

## Processes and trust boundaries

### Rust server

The server embeds the control UI, REST API, SSE state feed, and connector WebSocket endpoints in one compiled executable. It owns command validation, serialization, connector version checks, sanitized state, and the latest browser and computer observations.

The authenticated control URL uses a fragment-held master token that is exchanged for a port-specific dashboard session capability. State-changing dashboard requests also require same-origin and CSRF checks. The server rejects non-loopback Host headers and exposes no CORS permission.

Persisted-token access is a capability-bound transaction. Default storage requires an absolute current-user profile path and never falls back to the working directory. Unix retains the validated parent directory descriptor and performs nonblocking child opens, atomic replacement, verification, and cleanup with descriptor-relative operations. Windows retains a no-follow parent handle plus no-delete leases on its pathname ancestors, and checks the directory's volume-and-file identity before and after its path-based child operations. Custom token parents are validation-only; only the exact computed default directory is managed or hardened.

### Chromium extension

The Manifest V3 extension connects outbound to the server. A mutual-HMAC handshake binds the extension role, nonces, and server-created connector session without putting the raw token in the WebSocket URL. Before its first storage read, the service worker restricts `chrome.storage.local` to Chrome's `TRUSTED_CONTEXTS`, so injected content scripts cannot read the persisted bridge token or control state through the extension storage API.

An explicit browser-control lease attaches `chrome.debugger` to one tab. The held attachment supplies trusted CDP input and causes Chrome to show its browser-owned debugging warning. An isolated content script supplies structured DOM observation and the extension-owned page pill, Stop control, and synthetic pointer. The public host and a private marker retained inside its closed shadow root are randomized per document; shadow-important rules reset critical visual host, pseudo-element, and backdrop properties. Snapshot invalidation excludes only the retained host and exact objects owned by that closed shadow root: the public ID, ancestor selectors, and page-owned light children confer no exclusion. JavaScript attribute/ancestor checks and the independent browser-process proof handle accessibility state. Control start, reuse, capture restoration, and passive checks first re-top and perform bounded render/layout/computed-style plus document/closed-shadow hit tests.

The renderer requires the host to remain the direct child of `document.documentElement`. The service worker independently resolves that exact `:root`, pins the private marker's innermost closed-shadow host, and requires the host's immediate browser-process parent to equal the root element. Within a shared 1.5 s/512-ancestry-work proof it samples `DOM.getTopLayerElements`, rejects any later node resolving to the same document, checks initial and fresh-final host/root ancestry and `hidden`/`inert`/ARIA-critical attributes, and—outside intentional capture—requires five `DOM.getNodeForLocation(ignorePointerEventsNone:true)` paint-order hits through that host and frame. A top-layer event revision seqlock accepts a clean final state after the bridge's own re-top events; a separate content-loss generation captured before the renderer request changes only for loss/mismatch signals and rejects same-revision loss across the entire proof. The content watchdog attempts a sample every 500 ms when its previous acknowledgement is idle; browser acknowledgement is bounded at 2 s, and a root top-layer event or indicator loss not cleared by its exact proof has an absolute 3 s service-worker deadline plus scheduler/transport timing. Navigation, native dialogs, and intentional capture suspend ordinary input and must rebind/reprove at their completion boundary. The DOM methods are experimental Chrome 140+ dependencies and the proof is neither compositor/physical-pixel proof nor atomic with later input, so Chrome's browser-owned warning and Cancel remain authoritative.

Cross-origin iframes use recursively verified child CDP sessions on supported Chromium 140+ browsers. They remain children of the single tab attachment and share its count, depth, time, and lease bounds. Input is dispatched through the page target after coordinates are translated into top-level viewport space.

### Native computer helper

The helper is a separate compiled process because operating-system capture and input permissions are materially different from browser-extension authority. It opens no listener and connects outbound to the server with its own connector role and Origin.

The helper owns native window enumeration, exact-window observations, persistent live sharing, semantic accessibility state, target-routed input, and the computer synthetic cursor. Stopping it removes native application authority without stopping browser control.

On Windows, the executable users start is also a supervisor. It launches the same version-matched executable in a hidden worker mode, places that worker in a kill-on-close Job Object, and restarts it after a transport loss, an unknown action outcome, an unconfirmed capture shutdown, or a hard operation deadline. This is process-failure containment inside the same interactive desktop, not a sandbox or separate input seat. macOS keeps the existing single helper process because its Accessibility and Screen Recording grants are attached to the packaged app identity.

## Browser state and authority

Browser actions are tied to several changing identities:

```text
connector session -> control lease -> observation turn -> DOM generation/ref
                                           |
                                           -> pointer move sequence
```

Navigation, document mutation, scroll, resize, tab loss, debugger detach, human Stop, and lease expiry invalidate the applicable state. A stale reference or sequence fails instead of being reinterpreted against a newer page.

Full Access and Safe mode are action policies inside this authority boundary. They do not change connector authentication or make a signed-in browser profile a sandbox.

## Native capture paths

One-shot observation and live sharing are deliberately separate paths.

### One-shot observation

`computer.observe` takes an exact-window snapshot through the existing platform snapshot backend. The result binds a fresh frame ID to the target PID, native window ID, geometry, image dimensions, semantic elements, and pointer state. Native input must refer to this current frame authority.

### Persistent live sharing

`computer.share.start` validates one selected, non-minimized window and starts a persistent native capture source:

- macOS uses ScreenCaptureKit `SCStream` with `SCContentFilter(desktopIndependentWindow:)`, bound to the exact `(PID, CGWindowID)`. Window geometry changes advance a frame-authority epoch and use `SCStream.updateConfiguration` on the same stream for new pixel dimensions; queued and in-flight pre-update frames cannot cross that epoch.
- Windows uses a project-owned Windows Graphics Capture `CreateFreeThreaded` frame pool on a dedicated MTA owner thread bound to the exact `(PID, HWND)`.

Both sources disable capture of the system cursor. Native callbacks publish only the newest accepted frame into a one-slot handoff, so a slow consumer cannot create an unbounded native-frame backlog. The helper then composites its window-scoped synthetic pointer and emits PNG `computer.share.frame` events with the requested 1–10 FPS as a maximum cadence, not a guaranteed delivery rate. Delivered images are capped at 1,000,000 pixels.

The native producer remains live while an input action owns the serialized controller, but semantic enumeration, PNG conversion, and protocol publication resume only after the action completes. On Windows, share pumping runs outside the transport loop under the disposable worker's hard deadline, so a stalled UI Automation provider cannot wedge authentication, cancellation, or worker replacement. This preserves a bounded newest frame without promising every intermediate synthetic-pointer animation sample.

When acknowledgement pacing is negotiated, only one encoded frame is in flight. A newer frame replaces an unsent frame, increments `droppedFrames`, and keeps the sequence monotonic. Older helpers retain the bounded producer-timed contract rather than receiving an unknown acknowledgement message.

The operating system owns capture lifecycle indication. The helper does not suppress macOS capture UI or request borderless WGC. Selection still occurs in the bridge control page: neither native system content picker is implemented.

## Native input paths

Every native action revalidates the exact target before delivery.

1. Resolve the frame-bound `(PID, native window ID)` and geometry.
2. Prefer a semantic Accessibility or UI Automation operation when a reliable action exists.
3. Otherwise use the platform's exact process/window background route.
4. Verify an application-owned postcondition when the framework exposes one.
5. Compare the platform-specific foreground/window-focus oracle, hardware pointer, and active desktop before and after delivery.
6. Fail closed if identity, support, or invariants cannot be proved.

macOS semantic actions use Accessibility. Pixel and key delivery use process-targeted routing, including dynamically resolved private facilities where public APIs are insufficient. A focus-preparing route uses an AX-backed dual-window lease: the unrelated front app is read without enabling Chromium Accessibility, the selected target may receive the existing one-time opt-in, and both apps' exact focused windows are stabilized and restored. This is a transient target activation at the Accessibility `AXFrontmost` level, even though it does not switch WindowServer's OS-front process/window. Windows uses UI Automation and exact-HWND messages where the target framework accepts them. Neither platform silently escalates to global HID or foreground input.

On macOS, the general focus oracle samples the foreground ProcessSerialNumber/PID, the same app's read-only exact AX focused and main window, hardware cursor, and active Space, then re-samples AX and the foreground identity before accepting the snapshot. It never substitutes the first same-PID WindowServer row, which can be a ScreenCaptureKit indicator. A different-process lease is admitted only when the saved user's main and focused IDs agree, the target's original main and focused IDs agree, both exact target windows expose a writable `AXMain`, and the modern WindowServer owner-connection lookup binds both IDs to the same target PSN. While the saved user is still fully active, the helper selects the requested sibling through its retained exact-window `AXMain` element and polls target main+focused read-back without raising it or making it `AXFrontmost` yet. It then uses explicit private-focus phases: saved user active/target requested inactive; saved user released/target requested inactive; and saved user released/target requested active. WindowServer's user-front PSN/PID stays unchanged while the saved app's `AXFrontmost` is false and the exact target's `AXFrontmost` is true in the private active phase.

Restoration first defocuses the requested target and proves it inactive. For a distinct prior sibling, an exact prior Focus record is bounded-polled; if AppKit still reports the requested receiver, a target-only paired make-key record commits the prior exact main/focused receiver, which is proved before the prior is defocused. The helper then restores and proves the saved user. This subprimitive never calls `_SLPSSetFrontProcessWithOptions`, performs `AXRaise`, or writes the application-level `AXFocusedWindow`. Normal preparation records receive the exact raw/AX target-phase and saved-user PSN/window authorization both before and after dispatch accounting; already-accounted cleanup records repeat one target-first/user-last authorization immediately before each record. Emergency saved-user compensation deliberately omits target reads when the target is unknown; it may post only after proving the unchanged front PSN/PID, exact saved user window with `AXFrontmost=false`, raw user restorability, and deadline, and still returns an unknown outcome for target restoration. Cleanup failure dominates the original action result. The saved user is independently proved restored before any final target verification, with fixed deadline slices retaining a user-only recovery opportunity. Text resolves the focused AX element to the target window immediately before every scalar key-down. Windows additionally records the GUI-thread focus window. A passing before/after comparison does not make receiver proof and dispatch atomic and does not prove that no shorter transient change occurred.

The browser extension remains the preferred actuator for Chromium web content. It has renderer-aware CDP input and browser-owned revocation that generic native window messages cannot reproduce.

## Capture is not isolation

An exact-window stream can avoid recording unrelated windows. Target-routed input can avoid moving the user's pointer and can preserve the OS-front process/window before and after supported actions. On macOS that does not exclude the disclosed transient target `AXFrontmost` lease or prove zero interruption. These properties do not create a second desktop, input queue, login, credential store, or security principal.

```text
exact-window capture + target-routed input = shared-session operation with before/after invariant checks
VM / RDP desktop / separate login          = separate environment or input seat
```

PiP automation, virtual displays, VM orchestration, RDP loopback, and separate OS input seats are outside the current architecture. They must be introduced as a distinct backend with explicit credentials, lifecycle, cleanup, and data-sharing policy rather than being aliased to `background-window`.

## Stop and cleanup

- Chrome Cancel, the in-page Stop button, popup release, timeout, target loss, or connector loss revokes the browser lease.
- `POST /api/v1/command/cancel` is bearer- and `callId`-scoped. It drops the exact action future and sends one original-session connector cancel; it preserves the browser lease when safe and reports the original call as outcome-unknown. For a started controlled-page command, the server independently latches exact-session recovery and removes observation/screenshot authority before returning 202, so a dropped connector cancel cannot reopen the old turn. The same exact-session fence runs before a disconnected HTTP handler releases the action lock and on post-dispatch connector outcome-unknown errors, including no-`callId` and legacy dashboard requests. The extension synchronously advances and persists its turn, clears its frame snapshot both immediately and at the final queue barrier, and serializes that persistence before the next command. Its global browser-action tail also waits for late Chrome reconciliation (including durable `tabs.new` provenance) and freshness finalization before a socket or popup-approved action can enter; explicit `page.observe` is the only normal controlled-page recovery.
- `computer.share.stop`, helper shutdown, target closure, capture failure, or connector replacement stops the native share and clears frame authority.
- Canceling a computer mutation immediately clears only the owning helper session's published share/frame/pointer/screenshot authority; replacement-session state survives.
- Stopping the server breaks both outbound connector sessions.
- A replacement connector session must negotiate capabilities and obtain fresh observations; it cannot inherit stale authority.

## Release and update flow

The server performs a metadata-only check against the fixed public GitHub Releases API. It accepts only a canonical stable release marked immutable by GitHub and never downloads or installs an update. Release artifacts are built as a Windows server executable, Windows helper executable, macOS universal archive with helper app, matching extension ZIP, checksum manifest, and GitHub provenance. Project and locked dependency licenses are embedded in both executables; the macOS archive and extension package also carry their applicable notice files.

See [Security](../SECURITY.md) for trust details, [Protocol](PROTOCOL.md) for envelopes and commands, and [Limitations](LIMITATIONS.md) for platform-specific boundaries.

## Primary platform references

- [Chrome debugger API](https://developer.chrome.com/docs/extensions/reference/api/debugger)
- [Chrome extension storage access levels](https://developer.chrome.com/docs/extensions/reference/api/storage/StorageArea#method-setAccessLevel)
- [Chromium implementation of trusted access for local and sync storage](https://chromium.googlesource.com/chromium/src.git/+/a8f1f337c692360aaec9470a0a91f965011d37a3)
- [Apple ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)
- [Apple desktop-independent window capture behavior](https://developer.apple.com/videos/play/wwdc2022/10155/)
- [Windows Graphics Capture `CreateForWindow`](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow)
- [Windows UI Automation control patterns](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controlpatternsoverview)
