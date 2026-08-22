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

### Chromium extension

The Manifest V3 extension connects outbound to the server. A mutual-HMAC handshake binds the extension role, nonces, and server-created connector session without putting the raw token in the WebSocket URL.

An explicit browser-control lease attaches `chrome.debugger` to one tab. The held attachment supplies trusted CDP input and causes Chrome to show its browser-owned debugging warning. An isolated content script supplies structured DOM observation and the extension-owned page pill, Stop control, and synthetic pointer.

Cross-origin iframes use recursively verified child CDP sessions on Chromium 125+. They remain children of the single tab attachment and share its count, depth, time, and lease bounds. Input is dispatched through the page target after coordinates are translated into top-level viewport space.

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

- macOS uses ScreenCaptureKit `SCStream` with `SCContentFilter(desktopIndependentWindow:)`, bound to the exact `(PID, CGWindowID)`.
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

macOS semantic actions use Accessibility. Pixel and key delivery use process-targeted routing, including dynamically resolved private facilities where public APIs are insufficient. Windows uses UI Automation and exact-HWND messages where the target framework accepts them. Neither platform silently escalates to global HID or foreground input.

On macOS, the focus oracle identifies the foreground process and its front window; it does not identify the exact focused Accessibility control. Windows additionally records the GUI-thread focus window. A passing before/after comparison does not make dispatch and observation atomic and does not prove that no shorter transient change occurred.

The browser extension remains the preferred actuator for Chromium web content. It has renderer-aware CDP input and browser-owned revocation that generic native window messages cannot reproduce.

## Capture is not isolation

An exact-window stream can avoid recording unrelated windows. Target-routed input can avoid moving the user's pointer or foreground on supported applications. These properties do not create a second desktop, input queue, login, credential store, or security principal.

```text
exact-window capture + target-routed input = shared-session operation with before/after invariant checks
VM / RDP desktop / separate login          = separate environment or input seat
```

PiP automation, virtual displays, VM orchestration, RDP loopback, and separate OS input seats are outside the current architecture. They must be introduced as a distinct backend with explicit credentials, lifecycle, cleanup, and data-sharing policy rather than being aliased to `background-window`.

## Stop and cleanup

- Chrome Cancel, the in-page Stop button, popup release, timeout, target loss, or connector loss revokes the browser lease.
- `computer.share.stop`, helper shutdown, target closure, capture failure, or connector replacement stops the native share and clears frame authority.
- Stopping the server breaks both outbound connector sessions.
- A replacement connector session must negotiate capabilities and obtain fresh observations; it cannot inherit stale authority.

## Release and update flow

The server performs a metadata-only check against the fixed public GitHub Releases API. It never downloads or installs an update. Release artifacts are built as a Windows server executable, Windows helper executable, macOS universal archive with helper app, matching extension ZIP, checksum manifest, and GitHub provenance.

See [Security](../SECURITY.md) for trust details, [Protocol](PROTOCOL.md) for envelopes and commands, and [Limitations](LIMITATIONS.md) for platform-specific boundaries.

## Primary platform references

- [Chrome debugger API](https://developer.chrome.com/docs/extensions/reference/api/debugger)
- [Apple ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)
- [Apple desktop-independent window capture behavior](https://developer.apple.com/videos/play/wwdc2022/10155/)
- [Windows Graphics Capture `CreateForWindow`](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow)
- [Windows UI Automation control patterns](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controlpatternsoverview)
