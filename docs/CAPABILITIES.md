# Capabilities

This page states what the current code can do, what the user must configure, and what still lacks release-grade evidence. A capability is not a security boundary: browser control uses the selected signed-in profile, and native control uses the selected window in the user's current login session.

## Status terms

- **Available** means the implementation and protocol exist.
- **Prerequisite** means the feature is available only after the listed browser or operating-system setup.
- **Evidence gap** means the code exists but the exact platform, application, or packaged-release path has not yet been proven by the checked-in live evidence.
- **Not included** means clients must not infer the capability from a nearby feature.

## Browser control

| Capability | Status | Requirement or boundary |
|---|---|---|
| List, activate, create at a policy-approved URL, navigate, reload, and close tabs | Available | Chrome or Edge 140+; omit the creation URL only for a blank lifecycle tab |
| Observe viewport pixels, text, selection, and interactive elements | Available | Regular HTTP(S) pages; file pages require the browser's file-URL permission |
| Click, hover, fill, select, scroll, type, and send key chords | Available | One explicit debugger-backed lease on one tab |
| Browser key subset | Available | Documented CDP named keys, F1–F12, ASCII letters/digits, and one non-control/non-whitespace BMP scalar other than the reserved `+`; Safe mode is narrower |
| Main-world JavaScript evaluation | Available | Full Access, or the applicable Safe-mode policy |
| Same-process frame observation | Available | Chrome or Edge 140+ |
| Recursive cross-origin iframe observation and trusted point input | Available | Chrome or Edge 140+; bounded to 16 iframe targets and five levels |
| Dialog handling, condition waits, and bounded action batches | Available | Commands remain tied to the current lease and observation epoch |
| Browser-owned warning and extension-owned page indicator | Available | Chrome owns the authoritative warning/Cancel; the page pill combines a direct-root-child/innermost-host-bound private marker, initial/final host/root accessibility checks, five browser-process point hits, bounded top-layer ancestry, separate top-layer revision/content-loss generations, a 500 ms sampling attempt, and an absolute 3 s dirty-proof deadline |
| Full Access | Available and default | Broad authority over regular pages in the selected profile |
| Safe mode | Available | Site allowlist, sensitive-field blocking, and selected one-time approvals |
| Per-command API cancellation | Available | Bearer-authenticated in-flight `callId`; returns outcome-unknown and requires observation, never automatic retry |
| Saved-token removal | Available | Trusted popup action revokes control, disconnects, removes the token from extension storage, and verifies the cleared state |

The extension never replaces trusted debugger input with an untrusted page-generated click. Chrome Cancel, the in-page **Stop** button, popup release, lease expiry, tab closure, or connector loss revokes control.

Canceling one bearer API command stops only that command context and preserves the user's browser-control lease when it can be kept safely. Cancellation is cooperative and can race a dispatched side effect, so the original command is completed as outcome-unknown rather than reported as rolled back. A controlled-page command with an unknown outcome immediately clears the server's observation/screenshot and latches exact-session recovery; later page mutations are refused until an explicit `page.observe` succeeds, even if the extension never receives the cancel. This also covers a disconnected caller with or without `callId`, legacy dashboard actions, and connector timeouts. The extension advances and persists the lease turn before its next queued command, re-clears frame state at the queue barrier, and revokes the lease on persistence failure.

Every primary browser command and follow-up tab list or page observation is bound to the exact extension-session UUID selected before dispatch. A replacement extension never receives work that belonged to the old session.

## Native application control

The optional helper is a separate Rust process. It connects outbound to the loopback server and exposes a fixed computer-use command set; it does not expose shell, filesystem, clipboard, process-launch, download, or telemetry methods.

| Capability | macOS | Windows |
|---|---|---|
| One-shot exact-window observation | Available through the exact-window snapshot backend | Starts the same bounded, exact-HWND Windows Graphics Capture backend used by live sharing and stops it after one fresh frame |
| Persistent exact-window live share | ScreenCaptureKit `SCStream` bound to an exact `(PID, CGWindowID)` | Project-owned Windows Graphics Capture `CreateFreeThreaded` frame pool on a dedicated MTA owner thread, bound to an exact `(PID, HWND)` |
| Live-share transport | Requested 1–10 FPS maximum cadence, PNG frames capped at 1,000,000 pixels, monotonic sequence, bounded latest-frame slot, optional acknowledgement pacing | Same protocol and bounds |
| Captured cursor | System cursor disabled; helper cursor composited into returned frames | Same |
| Semantic observation/action | macOS Accessibility | Windows UI Automation |
| Background pixel/key route | Process/window-targeted route using dynamically resolved macOS facilities | UIA plus exact-HWND background messages where the application accepts them |
| Native key subset | Navigation/editing keys, F1–F12, ASCII letters/digits, mapped US-keyboard punctuation, and Control/Alt/Shift/Meta modifiers | Navigation/editing keys, F1–F12, ASCII letters/digits, mapped punctuation, and Control/Alt/Shift; Windows/global and secure chords fail closed |
| Readiness signal | Current permission and complete focus/input snapshot must be readable | Non-Session-0 process plus readable input desktop, foreground/focus HWNDs, and cursor; provider acceptance is still per action |
| Target-activation disclosure | Focus-capable input may use and restore a transient exact-target `AXFrontmost` lease; no `AXRaise`, OS-front-process switch, or Space switch | No explicit target-activation or foreground API; provider behavior is still checked before/after |
| Foreground/focus invariant | WindowServer-front process/window and saved user AX state must match before/after; no zero-transient guarantee | Foreground and GUI-thread focus HWNDs must match before/after; no zero-transient guarantee |
| Helper transport lifecycle | Intentional or unexpected server-transport loss exits the helper; relaunch is required. Explicit share stop stays in process | The launcher supervises disposable workers and restarts them after transport loss. Explicit share stop stays in process |
| Automatic foreground fallback | Not included | Not included |
| Physical pointer movement or global HID input | Not included | Not included |

The live-share target is selected in the Local Browser Bridge control page. The helper starts the native stream programmatically for that exact process/window pair. It does **not** present `SCContentSharingPicker` on macOS or the Windows system capture picker.

The operating system still owns capture lifecycle UI. macOS can show its current screen-capture indicator and stop affordance; Windows can show its normal WGC capture border or indicator. Their exact appearance varies by OS version and policy, so the project does not promise a particular banner or wording.

Every primary computer command and follow-up observation is bound to the exact helper-session UUID selected before dispatch. Share start is accepted only from a raw `{ "active": true, "id": "..." }` result followed by a first observation carrying that exact ID; share stop requires raw `{ "active": false }`. Rejected lifecycle results are quarantined and cleaned up by an exact-session task that survives caller cancellation. If stop cannot be proven, the server revokes only the originating WebSocket through a queue-independent shutdown signal; a replacement helper is never used for that cleanup.

## Setup prerequisites

These are setup conditions, not product limitations:

- Every installed component must use the same release version.
- Browser control requires an unpacked Manifest V3 extension loaded from `chrome://extensions` or `edge://extensions`.
- Browser control requires Chromium 140+. Cross-origin child-session routing first appeared in Chromium 125, but version 0.12.4 retains the overall floor so persisted extension storage can be restricted to trusted contexts.
- The complete macOS archive requires macOS 13+. Live sharing additionally requires Screen Recording permission for the packaged helper application.
- macOS semantic control and supported input routes require Accessibility permission.
- Windows native control must run in the signed-in interactive session, not Session 0 or a service.
- The selected native window must be on screen, non-minimized, and have a nonzero capturable area when sharing starts.

See [Installation](INSTALL.md) for the user flow.

## Not included

- A native content picker or OS-managed per-window consent dialog
- Picture-in-Picture automation on a second desktop
- A virtual display, VM, RDP loopback, separate login, or separate OS input seat
- Security isolation from the user's current account, credentials, applications, clipboard, or files
- Guaranteed background input for every application framework
- Audio capture or video/WebRTC transport
- A native desktop cursor overlay; the helper cursor appears in returned images

## Evidence status

- The browser evidence under [`evidence/v0.11.1`](../evidence/v0.11.1/README.md) separates published 0.11.1 results from a local 0.11.2 recursive-frame candidate.
- The published native evidence proves the earlier exact-window observation, semantic actions, background input invariants, and bounded share protocol. It does not by itself prove the newer persistent SCStream or WGC source.
- Persistent native-stream code must receive packaged, version-specific live proof on both operating systems before its implementation status is treated as release proof.
- Cross-Space macOS capture/input, minimized-window capture, protected content, elevated Windows targets, and a broad application compatibility matrix remain unproven and are not advertised.

Transport success is diagnostic evidence. A native action is supported only when a representative application-owned result and the advertised platform-specific foreground/window-focus, pointer, and desktop invariants are also observed. Successful snapshots are post-dispatch evidence, not transactional rollback or proof that no shorter transient change occurred.

## Primary references

- [Chrome debugger API](https://developer.chrome.com/docs/extensions/reference/api/debugger)
- [Apple: Take ScreenCaptureKit to the next level](https://developer.apple.com/videos/play/wwdc2022/10155/)
- [Microsoft: create a Windows Graphics Capture item for a window](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow)
- [Pinned Cua implementation](https://github.com/trycua/cua/tree/0213cd82fd8f5f35d530e7b3eda5286511bbbc10)

See [Computer-use research](COMPUTER_USE_RESEARCH.md) for the complete source comparison and the distinction between capture, input routing, and isolation.
