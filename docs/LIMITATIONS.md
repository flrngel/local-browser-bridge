# Limitations

Local Browser Bridge provides exact-window capture and best-effort non-interrupting, target-routed input inside the user's current login session. It does not provide a VM, remote desktop, separate login, separate input seat, or security isolation from that user's applications and credentials.

Setup requirements such as browser loading, matching versions, macOS permissions, and the Windows interactive session are documented separately in [Installation](INSTALL.md). This page describes constraints that remain after correct setup.

## Browser limits

- The agent browser must run on the same computer. A cloud-hosted browser cannot reach the user's `127.0.0.1` server.
- Control acts in the selected real browser profile, including its signed-in sessions. A dedicated profile is the clearest way to reduce account exposure.
- Browser-internal pages, extension stores, privileged browser UI, and other debugger-restricted targets are not ordinary controllable web pages.
- Core control supports Chrome and Edge 118+. Recursive cross-origin iframe routing requires Chromium 125+ and is bounded to 16 iframe targets, five levels, and one shared observation budget.
- The debugger lease is exclusive to this extension's controlled tab. Another debugger client, Chrome Cancel, navigation, target loss, or service-worker/connector loss can revoke it.
- Full Access intentionally bypasses most action-level interlocks. Safe mode is heuristic and cannot determine human intent from every icon, canvas, ambiguous label, or hostile page.
- A DOM ref or screenshot can become stale before an action. The bridge revalidates several identities and proofs, but it does not yet calculate a fresh target-patch SSIM and full-frame visual diff before every command.

## Native capture limits

### Shared constraints

- The helper captures one selected native window, not a complete application workflow. A dialog, popover, tooltip, or child window with a different native window ID can be omitted.
- The selected window must be on screen, non-minimized, and have nonzero size when a live share starts.
- Protected or DRM content can be blank. Secure surfaces and some GPU-rendered applications can stop or refuse capture.
- The live feed uses bounded PNG events with a requested 1–10 FPS maximum cadence and a 1,000,000-pixel image cap, not a video codec, WebRTC stream, or audio stream. Actual delivery can be lower when the compositor is change-driven, semantic enumeration or encoding takes longer, acknowledgement pacing applies backpressure, or an action temporarily owns the serialized helper controller.
- On macOS, `computer.observe` remains a one-shot snapshot while `computer.share.start` uses `SCStream`. On Windows, one-shot observation starts the same bounded WGC implementation as live sharing and stops it after one fresh frame. The different lifetimes and shutdown paths still require separate tests.
- The system cursor is excluded. The visible helper pointer is composited into returned images and is not a native desktop cursor overlay.
- Native capture callbacks continue replacing a one-frame source slot while an action runs, but PNG conversion and protocol publication resume after that serialized action completes. Shared frames show the settled helper pointer; they are not guaranteed to reproduce every intermediate pointer-animation sample.

### macOS

The live-share backend uses `SCStream` and a desktop-independent exact-window filter, but the current selector enumerates on-screen windows and rejects minimized targets. Apple documents that ScreenCaptureKit can continue an exact-window stream while the source is occluded, offscreen, or on another Space, and pauses it while minimized. This project does not advertise those broader behaviors until the packaged helper proves them across supported macOS versions. [Apple's exact-window behavior](https://developer.apple.com/videos/play/wwdc2022/10155/).

There is no `SCContentSharingPicker`. The user chooses a window in the bridge control page, after which the helper starts a programmatic exact `(PID, CGWindowID)` stream. macOS still owns its capture indicator and stop affordance, whose appearance varies by release, but that is not a system picker or an independent desktop.

The helper detects a moved or resized exact window on the next complete native frame. It advances a geometry authority epoch, discards queued old-geometry observations, and updates the existing `SCStream` configuration in place for size or display-scale changes without replacing the share lease. Pre-update callbacks are rejected by both a ScreenCaptureKit host-time boundary and the configured pixel dimensions; if no geometry-bound frame arrives within five seconds, the share fails closed. macOS 14 and later can use the filter's point-to-pixel scale before the transport cap; macOS 13 falls back to the validated enumerated window size and can produce lower-resolution Retina frames. Blank, protected, or other non-complete samples can delay resize detection, while the three-second action-frame lease still expires normally.

### Windows

The capture backend is project-owned Windows Graphics Capture using a `CreateFreeThreaded` frame pool on a dedicated MTA owner thread for an exact `(PID, HWND)`. It leaves the normal capture border setting under Windows control and does not request a borderless entitlement. The exact indicator or border depends on Windows version and policy.

The Windows transport currently requests SDR `B8G8R8A8UIntNormalized` frames and converts BGRA8 to PNG. [Microsoft recommends a full `R16G16B16A16_FLOAT` pipeline when HDR is enabled](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture); until that color-management path is implemented, HDR content can look washed out or clipped. This affects color fidelity, not the exact-window identity boundary.

Minimized windows and applications that stop rendering can freeze or stop producing useful frames. Protected content, secure desktops, elevated targets, and some graphics frameworks remain unsupported or unverified.

## Native input limits

### macOS

- Accessibility actions are preferred, but application accessibility trees can be sparse, stale, or incomplete, especially for Electron, canvas, games, and custom controls.
- Pixel/key delivery depends partly on dynamically resolved private SkyLight symbols. A macOS update can rename, gate, or change them. The helper reports the route unavailable rather than falling back to global HID.
- Current mutable targets are limited to non-minimized windows on the active Space. ScreenCaptureKit's ability to capture another Space does not grant cross-Space input.
- Process-targeted keyboard delivery can be ambiguous when one process has multiple eligible windows. The helper refuses the ambiguous route. Prefer semantic `setValue`, keep one eligible target window, or use the browser extension for Chromium web content.
- Native `computer.typeText` is limited to 2,000 UTF-16 code units, paced between each Unicode scalar, and revalidates the sole exact-window destination during delivery. Apple notes that [application frameworks may ignore the Unicode string attached to a Quartz keyboard event](https://developer.apple.com/documentation/coregraphics/cgevent/keyboardsetunicodestring%28stringlength%3Aunicodestring%3A%29), so successful event posting remains delivery evidence rather than a confirmed text postcondition. Prefer semantic `setValue` for a field that exposes it.
- Cross-Space keyboard and pointer input is not claimed. It would require additional private Space discovery/routing, OS-version gating, and invariant tests before release.
- Secure input, protected controls, some Chromium gestures, right-click variants, games, and HID-only engines can reject background events.

### Windows

- UI Automation works only when the target exposes a useful pattern and accepts it without disruptive focus changes. A snapshot visits at most 1,500 Control View nodes, 25 levels, and 500 actionable controls, with a 750 ms traversal budget checked between provider calls. Elements collected before a limit remain usable, while `semanticTruncated` and its closed-vocabulary reason disclose that later controls can be absent.
- Individual UI Automation provider calls cannot be cancelled safely. Windows therefore performs controller work in a disposable supervised worker with a 12-second hard operation deadline. If a provider stalls, the worker is terminated and restarted; an action that crossed its side-effect boundary is reported as outcome-unknown and is never retried automatically.
- Exact-HWND messages are application-framework behavior, not a universal trusted input API. A successful [`PostMessage`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew) call proves only that the message was queued.
- Native `computer.typeText` accepts only Unicode window recipients, uses a documented `WM_CHAR` repeat count of one, and limits each command to 2,000 UTF-16 code units. It posts at most 16 code units before a scheduler pause and checks cancellation at every unit. This caps one command at half of Windows' documented 4,000 minimum posted-message queue limit, but it still cannot prove that a control consumed the queued text; semantic `setValue` is the confirmed route when available.
- Chromium, Electron, WPF, WinUI, GTK, games, canvas, elevated processes, and UIPI boundaries can reject background delivery. Browser web content should use the extension instead.
- The helper does not use global [`SendInput`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) as an automatic fallback. Unsupported actions fail closed.

## Non-interruption is not isolation

On a supported application, target-routed input can leave the user's foreground application, platform focus oracle, hardware cursor, and active desktop unchanged. On macOS that focus oracle is the foreground process plus front window; on Windows it also includes the GUI-thread focus window. These are before/after observations, not an atomic rollback guarantee or proof that no unobserved transient focus change occurred. Both the user and helper still share:

- the same login session and security principal;
- application files, settings, network access, and signed-in accounts;
- operating-system permissions and much of the same clipboard/application state; and
- one underlying WindowServer or Windows desktop environment.

Hostile or destructive workloads require an explicitly managed VM, RDP desktop, separate login, or other sandbox. PiP automation, virtual displays, VM/RDP lifecycle, and separate OS input seats are not included.

## Product comparison boundary

[OpenAI's current documentation](https://learn.chatgpt.com/docs/computer-use) says macOS Computer Use can run a scoped task in the background and says Windows Computer Use runs on the active desktop. OpenAI publishes the macOS Screen Recording and Accessibility prerequisites, but it does not publish its native capture or input implementation. Any claim that Codex specifically uses ScreenCaptureKit, SkyLight, or a particular private symbol is inference, not an official implementation detail.

The closest pinned shared-session comparison is [Cua commit `0213cd8`](https://github.com/trycua/cua/tree/0213cd82fd8f5f35d530e7b3eda5286511bbbc10). Its code and write-ups inform compatibility tests, but this project does not claim Cua feature parity or copy its protocol. Microsoft's [Windows child-session documentation](https://learn.microsoft.com/en-us/windows/win32/termserv/child-sessions) defines the separate-session boundary; exact-window WGC alone does not create that seat.

## Evidence gaps

The following need versioned packaged evidence before they can become supported claims:

- persistent SCStream behavior while fully occluded, moved offscreen, or moved to another Space;
- cross-Space macOS input with unchanged frontmost process/window, hardware cursor, and active Space;
- long-running stream recovery after display sleep, permission changes, resize, target closure, and helper reconnect;
- representative Windows WGC and UIA/background-input runs on real Windows hardware;
- minimized, protected, elevated, secure-desktop, multi-display, mixed-DPI, and child-window behavior;
- private macOS input compatibility across every supported macOS release and architecture; and
- concurrent native agents with deterministic per-window leases and conflict handling.

The checked-in [evidence index](../evidence/) records what was actually run. A code path, unit test, or transport acknowledgement alone is not evidence that an application accepted the action or that the user's desktop remained unchanged.

## Research references

- [Temporal UI State Inconsistency / PUSV](https://arxiv.org/abs/2604.18860) — observation-to-action race defenses
- [UFO²](https://arxiv.org/abs/2504.14603) — isolated virtual desktop as a distinct architecture
- [CaMeLs Can Use Computers Too](https://arxiv.org/abs/2601.09923) — security isolation for computer-use agents
- [Computer-use research](COMPUTER_USE_RESEARCH.md) — pinned implementation and community review
