# Non-interrupting computer-use research and architecture

Research snapshot: 2026-08-18

## Correction to the original implementation

Version 0.6 captured a physical display with xcap and injected global input with Enigo. A real end-to-end run proved that this was the wrong architecture: the helper moved the user's pointer and typed into whichever application currently owned focus. A frame ID protected only monitor geometry; it did not bind input to an application window. That implementation has been removed rather than retained as an automatic fallback.

Version 0.7 defines a stricter product invariant:

1. Observation is bound to one exact `(pid, native window id)` target.
2. Input is delivered only through a target-addressed background route.
3. The foreground process, user's focused window/control, hardware cursor, and active desktop must not change.
4. A route that cannot prove those properties returns a structured error. It never retries through global HID input, target activation, `SetForegroundWindow`, or a desktop/Space switch.
5. The server captures the target window again after every action so the caller can verify the result.

## Evidence reviewed

The review used current source code, vendor documentation, research papers, and community reports. Repository popularity is only a discovery signal; architecture claims below come from code or primary documentation.

| Project or source | Pinned snapshot | What the implementation actually establishes |
|---|---:|---|
| [Cua Driver](https://github.com/trycua/cua) | `c43f10243856658fe706c08c155a95628fc81248` | Current Rust code implements exact-window capture and background delivery on macOS and Windows, with explicit background/foreground modes and fail-closed target proofs |
| [Microsoft UFO](https://github.com/microsoft/UFO) | `96983c73ed09e884a5f1d7ff8936c953b234b684` | Strong Windows UIA/Win32 automation and RDP-safe capture fallbacks; the repository still labels Picture-in-Picture Desktop “coming soon” |
| [UFO² paper](https://arxiv.org/abs/2504.14603) | 2025 paper / 2026 TMLR | Describes an RDP-loopback PiP architecture that isolates agent and human input sessions; this is a design result, not proof that the current UFO repository ships it |
| [Agent Workspace Linux](https://github.com/agent-sh/agent-workspace-linux) | current snapshot | Runs a hidden Xvfb/Openbox/xdotool workspace, a clean example of true separate-display isolation on Linux |
| [Apple High Performance Screen Sharing](https://support.apple.com/guide/mac-help/screen-sharing-type-options-mchl1883115d/mac) | macOS 14+ | Can create one or two virtual displays, but a same-user connection blanks hardware displays and prevents local use; it does not satisfy this product's simultaneous-use invariant |
| [Power Automate PiP](https://learn.microsoft.com/power-automate/desktop-flows/run-desktop-flows-pip) | current documentation | Windows child sessions and preview virtual desktops provide real separation but require Power Automate, policy prerequisites, and administrator setup |
| [Power Automate unattended](https://learn.microsoft.com/power-automate/desktop-flows/run-unattended-desktop-flows) | current documentation | Microsoft creates, manages, and releases an RDP user session; credentials and session lifecycle are part of the security boundary |
| [ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit/scscreenshotmanager) | current SDK | Supports window-scoped capture independent of desktop occlusion; capture alone does not create isolated input |
| [OSWorld](https://arxiv.org/abs/2404.07972) | 369 desktop tasks | Desktop-agent correctness requires multi-step application verification, not merely a callable click primitive |
| [Tactile](https://arxiv.org/abs/2607.14443) | 2026 paper | Semantic actions and application structure improve reliability beyond screenshot-only control |

Community reports about separate macOS Screen Sharing logins and Windows RDP sessions were used only to identify operational questions. They are not treated as proof of an API or security property.

## What the current code focuses on

### Cua: exact target identity and no silent escalation

Cua's current macOS background-input policy carries one `(pid, CGWindowID)` from observation through dispatch and postcondition. It gathers fresh WindowServer ownership, AX window membership, minimized/hidden state, same-process keyboard ambiguity, and element ancestry. Unknown facts do not unlock a route. Its route ladder prioritizes semantic AX and exact browser mechanisms, then window-local pointer delivery and PID keyboard delivery; the background mode never silently activates the app or posts global HID input.

The relevant macOS mechanisms are:

- window-scoped capture with ScreenCaptureKit;
- PID-routed events through `SLEventPostToPid`;
- window-local coordinates through `CGEventSetWindowLocation`;
- window identity fields carried on the event;
- focus-without-raise through `SLPSPostEventRecordTo`;
- post-action verification against the exact target.

These are undocumented SkyLight interfaces. They are practical for a separately distributed helper but create an explicit OS-compatibility risk and are unsuitable for App Store assumptions.

On Windows, Cua defaults to background PostMessage/UIA delivery and returns `background_unavailable` for framework/event pairs known to drop it. Foreground `SendInput` is a separate explicit caller choice. Its window capture uses PrintWindow and Windows.Graphics.Capture fallbacks, and its delivery code guards against self-activation. The important pattern is the refusal boundary, not a claim that one actuator works for every Windows UI framework.

### UFO and PiP: paper architecture versus shipped code

The UFO² paper's RDP-loopback desktop is the strongest Windows session-isolation design reviewed: the agent and user receive separate input sessions. The current open-source UFO tree, however, contains RDP-safe PrintWindow capture paths while its README still marks PiP as in development. This bridge therefore does not advertise UFO PiP as an available dependency.

True session isolation also changes product setup. It needs a Windows edition that supports the required session technology, credentials or a managed identity, local policy changes, resolution/session lifecycle management, and deterministic cleanup. Hiding those requirements inside a permissive localhost helper would create a credential and persistence system far beyond the current bounded authority.

### Apple Screen Sharing virtual displays: not simultaneous local use

Apple's High Performance Screen Sharing is often described as a virtual-display solution. Apple documents that when the connection authenticates as the currently logged-in user, hardware displays are blanked and nobody else can use the Mac. It requires another Apple-silicon Mac, macOS 14 or later, high bandwidth, UDP ports, and an authenticated Screen Sharing connection. It is useful for private remote operation, but it is not a local background-control primitive and fails the user's “keep using my screen” requirement in the common same-user configuration.

A separate macOS login or a Virtualization.framework guest can provide true isolation, but it does not automatically contain the user's already-installed app state. It also requires credentials or a guest OS image and a clear data-sharing policy. Version 0.7 does not create hidden users, store login credentials, or install a VM.

## Version 0.7 architecture

```text
browser-only agent
      |
      v
loopback control page/API --- Rust server --- Chromium extension ---> browser tabs
                                  |
                                  +--- authenticated helper process
                                             |
                                             +--- exact-window capture
                                             +--- target-routed background input
                                             +--- foreground/cursor/desktop oracle
```

The separate helper remains the permission-owning process and opens no listening socket. It authenticates outbound to the loopback server with the shared random token and the private Origin `lbb-computer-helper://local`. Its fixed command allowlist still contains only status, observe, move, click, drag, scroll, type text, and key chord. There is no shell, filesystem, process-launch, clipboard, downloader, arbitrary-code, credential, user-management, or telemetry method.

### macOS backend

- xcap enumerates shareable on-screen windows and captures one exact CGWindowID without exposing unrelated windows or notifications.
- The frame stores the window owner PID, CGWindowID, bounds, and delivered image dimensions.
- Before every action, the helper re-enumerates the target and rejects changed ownership or geometry.
- Mouse and scroll events carry screen coordinates, window-local coordinates, target PID, and target window fields, then post to that PID through SkyLight.
- Keyboard events post to the exact PID after a focus-without-raise record for the exact window. Multiple eligible windows in the same process are rejected because PID-scoped delivery would otherwise be ambiguous.
- The helper snapshots the front process PSN, user's front window, real cursor location, and active Space before and after dispatch. Focus-without-raise is restored to the prior front window after dispatch. Any change produces `COMPUTER_BACKGROUND_CONTRACT_VIOLATION`.
- Only non-minimized windows on the active Space are mutable in v0.7. Secure input, protected video, games, and OS/framework changes may refuse delivery.

### Windows backend

- xcap enumerates top-level HWNDs and captures one exact HWND using its Windows capture backend.
- Background mouse messages are routed to the deepest eligible child window at the target point; keyboard messages go to the target GUI thread's focused control only when its root is the requested top-level window.
- A temporary `WS_EX_NOACTIVATE` guard prevents the target top-level window from activating itself while handling a message.
- The helper verifies HWND ownership immediately before every action and snapshots `GetForegroundWindow`, the user's GUI-thread focus HWND, and `GetCursorPos` around delivery.
- Chromium, WPF, WinUI, elevated, game, and protected surfaces may ignore Win32 messages. They fail rather than using `SendInput` or `SetForegroundWindow`.

## Verification performed

The macOS end-to-end fixture is a real AppKit window launched without activation. It records every event to a machine-readable state file and redraws its visible state. The test uses only the shipped server REST command, the shipped computer-helper WebSocket, and exact-window screenshots.

The exercised matrix is:

| Action | Target state proof | Non-interruption proof |
|---|---|---|
| observe | Window-only PNG captured while fixture was backgrounded | Target reported `focused: false` |
| move | Routed move completed | Foreground, user focus, cursor, and Space unchanged |
| click | Click counter incremented | Foreground, user focus, cursor, and Space unchanged |
| drag | Drag counter incremented across 18 delivered steps | Foreground, user focus, cursor, and Space unchanged |
| scroll | Scroll accumulator changed by the requested delta | Foreground, user focus, cursor, and Space unchanged |
| Unicode text | State became `background-✓` | Foreground, user focus, cursor, and Space unchanged |
| named key | State appended `[enter]` | Foreground, user focus, cursor, and Space unchanged |

The fixture source is `tests/fixtures/macos/BackgroundFixture.swift`. Per-action screenshots are generated under ignored `target/background-e2e/` during the local run. Cross-platform CI and local verification still compile the same Rust version for Windows x86_64 and macOS arm64/x86_64.

## Remaining work

AX on macOS and UIA on Windows are the next reliability layer. They should expose semantic elements and verified value/action mutations before attempting another pixel route. A managed `isolated-session` backend can be added later as a separate capability with explicit credentials, OS prerequisites, visible lifecycle, and cleanup; it must not be presented as a transparent implementation detail of `background-window`.

The private macOS interfaces and framework-specific Windows behavior require continued release-by-release testing. No researched model runtime, planner, or remote protocol was embedded. The implementation follows the published architecture patterns and platform behavior while keeping its own protocol and bounded authority.
