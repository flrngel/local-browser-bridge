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
| List, activate, create, navigate, reload, and close tabs | Available | Chrome or Edge 118+ |
| Observe viewport pixels, text, selection, and interactive elements | Available | Regular HTTP(S) pages; file pages require the browser's file-URL permission |
| Click, hover, fill, select, scroll, type, and send key chords | Available | One explicit debugger-backed lease on one tab |
| Main-world JavaScript evaluation | Available | Full Access, or the applicable Safe-mode policy |
| Same-process frame observation | Available | Chrome or Edge 118+ |
| Recursive cross-origin iframe observation and trusted point input | Available | Chrome or Edge 125+; bounded to 16 iframe targets and five levels |
| Dialog handling, condition waits, and bounded action batches | Available | Commands remain tied to the current lease and observation epoch |
| Browser-owned warning and extension-owned page indicator | Available | Chrome owns the debugger warning; the extension owns its pill and Stop button |
| Full Access | Available and default | Broad authority over regular pages in the selected profile |
| Safe mode | Available | Site allowlist, sensitive-field blocking, and selected one-time approvals |

The extension never replaces trusted debugger input with an untrusted page-generated click. Chrome Cancel, the in-page **Stop** button, popup release, lease expiry, tab closure, or connector loss revokes control.

## Native application control

The optional helper is a separate Rust process. It connects outbound to the loopback server and exposes a fixed computer-use command set; it does not expose shell, filesystem, clipboard, process-launch, download, or telemetry methods.

| Capability | macOS | Windows |
|---|---|---|
| One-shot exact-window observation | Available through the existing exact-window snapshot backend | Available through the existing exact-window snapshot backend |
| Persistent exact-window live share | ScreenCaptureKit `SCStream` bound to an exact `(PID, CGWindowID)` | Free-threaded Windows Graphics Capture bound to an exact `(PID, HWND)` |
| Live-share transport | Requested 1–10 FPS maximum cadence, PNG frames capped at 1,000,000 pixels, monotonic sequence, bounded latest-frame slot, optional acknowledgement pacing | Same protocol and bounds |
| Captured cursor | System cursor disabled; helper cursor composited into returned frames | Same |
| Semantic observation/action | macOS Accessibility | Windows UI Automation |
| Background pixel/key route | Process/window-targeted route using dynamically resolved macOS facilities | UIA plus exact-HWND background messages where the application accepts them |
| Automatic foreground fallback | Not included | Not included |
| Physical pointer movement or global HID input | Not included | Not included |

The live-share target is selected in the Local Browser Bridge control page. The helper starts the native stream programmatically for that exact process/window pair. It does **not** present `SCContentSharingPicker` on macOS or the Windows system capture picker.

The operating system still owns capture lifecycle UI. macOS can show its current screen-capture indicator and stop affordance; Windows can show its normal WGC capture border or indicator. Their exact appearance varies by OS version and policy, so the project does not promise a particular banner or wording.

## Setup prerequisites

These are setup conditions, not product limitations:

- Every installed component must use the same release version.
- Browser control requires an unpacked Manifest V3 extension loaded from `chrome://extensions` or `edge://extensions`.
- Core browser control requires Chromium 118+; cross-origin frame routing requires Chromium 125+.
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
- [Pinned Cua macOS implementation](https://github.com/trycua/cua/tree/9a61050e3474fc9488d7adc85184299f02514d0e/libs/cua-driver/rust/crates/platform-macos)

See [Computer-use research](COMPUTER_USE_RESEARCH.md) for the complete source comparison and the distinction between capture, input routing, and isolation.
