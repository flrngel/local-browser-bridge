# Session-visible browser control and non-interrupting computer use

Research snapshot: 2026-08-18. Implementation target: version 0.9.0.

## The corrected target

Version 0.6 captured a physical display and injected global input. A live end-to-end run proved that architecture wrong for concurrent use: it moved the person's hardware pointer and typed into whichever application owned focus. Version 0.8 replaced it with exact-window capture, semantic-first action, target-routed background input, and foreground/cursor/desktop invariants.

Version 0.9 adds two properties that must not be conflated:

1. **Session visibility:** a person can tell when browser or helper control is active, see a synthetic pointer in the agent's image stream, and stop control through a trusted surface.
2. **Non-interruption:** desktop input is routed to one observed window while the foreground, focused control, hardware cursor, and active desktop remain unchanged.

Neither property creates an isolated operating-system session. A shared image of a background window is not a VM, remote desktop, virtual display, sandbox, or separate input seat.

## Evidence reviewed

The review used pinned source code, vendor source/documentation, primary papers, empirical public-extension packages, and community reports. Popularity was used only for discovery. Architecture claims come from source or primary documentation; community reports identify failure modes but do not establish platform guarantees.

| Source | Pinned snapshot | Focus relevant to this bridge |
|---|---:|---|
| [Chrome `debugger` API](https://developer.chrome.com/docs/extensions/reference/api/debugger) and [Chromium implementation](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/chrome/browser/extensions/api/debugger/debugger_api.cc) | Chromium main reviewed 2026-08-18 | A held extension debugger attachment creates Chrome's warning UI, keeps the MV3 worker alive on supported Chrome versions, exposes `canceled_by_user`, and fails pending debugger calls when the warning is canceled |
| Public ChatGPT Chrome extension package | `1.2.27259.19709`; ID `hehggadaopoacecdllhhajmbjkdcmajg`; SHA-256 `f9ba06c44525b53a0189d0ad97cf1d457987970063aea0609dec31b1d6782c96` | Empirical benchmark: held debugger sessions, exclusive tab ownership, managed tab group, serialized target work, session/turn/move state, and acknowledged synthetic-cursor arrival |
| Public Claude Chrome extension package | `1.0.85`; ID `fcoeoabgfenejglbffodgkkbkcdhcgfn`; SHA-256 `5c1c1318acf10bb4638be129ae34f9dfe728b867a70c603382f89e66a5d08be3` | Empirical benchmark: clear page indicator, trusted Stop path, heartbeat, page glow, and a simpler CSS synthetic cursor |
| [Cua Driver](https://github.com/trycua/cua/tree/c43f10243856658fe706c08c155a95628fc81248/libs/cua-driver/rust) | `c43f10243856658fe706c08c155a95628fc81248` | Rust exact-window backends plus a session-owned native cursor overlay with Dubins paths, velocity profile, spring settling, click state, themes, click-through windows, and platform-specific rendering |
| [agent-browser](https://github.com/vercel-labs/agent-browser/tree/548b159b30eef119ccf6846c8bc807d0eaa3f6f8) | `548b159b30eef119ccf6846c8bc807d0eaa3f6f8` | Persistent browser sessions, serialized CDP interaction, live screencast input, monotonic frame IDs, latest-frame-wins delivery, and optional renderer acknowledgements |
| [pi-computer-use](https://github.com/injaneity/pi-computer-use/tree/de725835d3b0e3bd13aa8885d6c3f3a9dc23bcdc) | `de725835d3b0e3bd13aa8885d6c3f3a9dc23bcdc` | Immutable state-scoped observations, resource epochs, per-resource serialization, successor state/diffs, checked action outcomes, and an optional non-blocking native ghost cursor on macOS |
| [OSWorld](https://arxiv.org/abs/2404.07972) | 2024 paper | Long-horizon desktop evaluation and environment diversity; its reference execution path uses the physical desktop/cursor and is not a concurrent-user isolation design |
| [Microsoft UFO](https://github.com/microsoft/UFO/tree/96983c73ed09e884a5f1d7ff8936c953b234b684) and [UFO²](https://arxiv.org/abs/2504.14603) | source `96983c7`; 2025 paper / 2026 TMLR | UIA/Win32 automation in shipped source; the paper's RDP-loopback Picture-in-Picture architecture is the strongest reviewed separate-input-session design, but the reviewed public repository still described PiP Desktop as coming soon |
| [UI-TARS](https://github.com/bytedance/UI-TARS/tree/582f3a7ea5d285ee8ed9e2e84048d1ab01453c49) | `582f3a7ea5d285ee8ed9e2e84048d1ab01453c49` | Vision-language grounding and screenshot/action history; this improves the planner/model, while its normal desktop operator remains physical screen input rather than a non-interrupting transport |
| [Temporal UI State Inconsistency / PUSV](https://arxiv.org/abs/2604.18860) | 2026 paper | Formalizes observation-to-action TOCTOU and rechecks local pixels, the global frame, and window identity immediately before action; also shows that visual checks alone miss zero-visual-footprint DOM replacement |
| [Apple High Performance Screen Sharing](https://support.apple.com/guide/mac-help/screen-sharing-type-options-mchl1883115d/mac) | macOS 14+ documentation | Can create virtual displays, but same-user use blanks hardware displays and prevents simultaneous local use; capture or sharing alone does not imply an independent input seat |
| [Power Automate PiP](https://learn.microsoft.com/power-automate/desktop-flows/run-desktop-flows-pip) and [unattended sessions](https://learn.microsoft.com/power-automate/desktop-flows/run-unattended-desktop-flows) | current documentation | Real Windows child/RDP sessions with credential, policy, lifecycle, and cleanup requirements |

Community signals included [real-session bridge discussion on Reddit](https://www.reddit.com/r/ClaudeAI/comments/1v5fz09/browser_bridge_an_mcp_server_that_drives_your/) and issue reports in agent-browser and other automation projects about stale transports, frame/viewport scaling, and pointer offsets. They informed test cases; they are not cited as proof of Chrome or OS behavior.

The ChatGPT and Claude rows are black-box observations of publicly distributed Chrome packages on the date above, not claims about an unpublished server protocol or future versions. Their exact version, extension ID, and hash are recorded so findings are not silently generalized to another package. Both reviewed manifests requested `debugger` and `tabGroups`; neither requested `tabCapture`.

## Three different visibility surfaces

### 1. Chrome's native debugger warning

`chrome.debugger.attach` is the mechanism that makes desktop Chrome display the extension-owned warning, normally rendered as **<extension name> started debugging this browser**. Chromium creates this warning when an extension debugger client attaches, except for explicitly suppressed command-line or policy-installed cases. The warning is browser chrome, not HTML, and a page cannot faithfully recreate it.

Chromium's Cancel path marks the detach reason `canceled_by_user`, fails pending debugger requests, sends `chrome.debugger.onDetach`, and closes the attachment. The infobar delegate is extension-scoped, so canceling it terminates that extension's attached debugger clients. Local Browser Bridge uses one attachment at a time, making the effective boundary one controlled tab.

Attaching only around a click and immediately detaching makes the warning flash or disappear and removes the person's reliable indication of ongoing authority. Version 0.9 therefore holds the attachment for an explicit, expiring lease. Unexpected detach is a hard revocation; there is no synthetic DOM-click fallback.

### 2. The extension's page overlay

The page pill and pointer are content injected by the extension. They communicate product-specific state that Chrome's native warning does not know: controlled tab, turn, pointer sequence, and a trusted Stop action. The overlay uses an isolated closed shadow tree and excludes its own mutations from page-snapshot invalidation.

This is independently stoppable. The Stop button sends an extension-internal message whose sender and tab are validated before the service worker releases the lease. A hostile page cannot call that privileged handler as the extension. Conversely, the pill cannot override Chrome's Cancel action; `onDetach` remains authoritative.

The reviewed Claude package emphasizes this human-facing layer: a persistent status pill, clear Stop action, heartbeat, and visual attention treatment. The reviewed ChatGPT package goes further on controller state and pointer arrival. Version 0.9 combines those product lessons without copying either package's private protocol or source.

### 3. The computer helper's synthetic pointer

The helper pointer is Rust state owned by one helper process. Pixel actions plan a bounded cubic Bézier path with minimum-jerk timing, deliver intermediate window-local moves through the existing exact-window background route, and record an arrival sequence. The last state is composited into exact-window observations and share frames, with an outline, session-derived color, action/ring state, and explicit image/screen coordinates.

It is not the hardware cursor and version 0.9 does not create a native click-through desktop overlay. Consequently, a person looking only at the physical desktop does not see this helper pointer; a person or model looking at the returned exact-window frames does. The current helper's action loop completes the path before it can emit the next captured frame, so shared frames reliably show the settled pointer but are not claimed to reproduce every intermediate animation sample.

The separately observed ChatGPT desktop **using your computer / Esc to cancel** treatment is also not Chrome's debugger warning. It is an operating-system/app-level computer-use surface. Reproducing its semantics would require a native overlay and global trusted cancel integration, which this release does not claim.

## What the benchmark projects prioritize

### Cua: renderer and actuator as one native session

Cua is the strongest open-source pointer-rendering reference reviewed. Its cursor core preserves position, heading, press state, theme, session label, idle fade, and target window. Its path planner chooses a minimum-turning-radius Dubins arc/straight/arc route; motion adds speed floors and spring settling. Platform code renders transparent click-through overlay windows (`NSWindow`, layered/no-activate Win32, or X11 input shaping) and keeps those windows out of input ownership.

Version 0.9 adopts the durable product properties—session ownership, bounded motion, stable style, press/action state, explicit coordinate spaces, and screenshot compositing—but not Cua's overlay implementation or Dubins planner. Local Browser Bridge uses its own bounded cubic candidates and minimum-jerk timing. It must therefore be described as benchmark-informed, not equivalent to Cua's native overlay.

### agent-browser: fresh live transport under backpressure

agent-browser's streaming path distinguishes browser/session lifetime from client lifetime. It assigns monotonically increasing frames, keeps the newest frame instead of building an unbounded backlog, dispatches input independently from frame writes, and optionally permits one in-flight frame until the renderer acknowledges it. Its public issue history also demonstrates why screenshot dimensions and input coordinate metadata must come from the actual frame rather than a configured viewport.

Local Browser Bridge uses a smaller bounded share contract: one exact window, 1–10 FPS, monotonic share sequence, explicit image dimensions and X/Y transport scales, and producer-blocking capture. It does not yet implement renderer acknowledgements or latest-frame replacement. Those are valid future improvements if remote or slow viewers become a supported use case.

### pi-computer-use: immutable state and resource ownership

pi-computer-use treats each observation as immutable state, serializes live work per physical resource, invalidates a base epoch before mutation, records checked outcomes, and creates one successor state. That is more important for correctness than cursor cosmetics. Its native cursor is observational and may be superseded without blocking action delivery.

Local Browser Bridge applies the same class of invariant at narrower layers: WebSocket session and sequence, one browser control lease, observation turn, DOM generation/revision, exact-window frame ID, PID/native-window revalidation, and post-action observation. It does not claim pi-computer-use's complete state store, resource-epoch scheduler, or transaction/diff system.

### UI-TARS and OSWorld: planner quality is not transport isolation

UI-TARS focuses on vision-language grounding, action vocabulary, and prior screenshot/action context. OSWorld focuses on representative, long-horizon task evaluation. Both are essential to end-task quality, but neither makes physical desktop input non-interrupting. This project uses them to shape action coverage and verification, not as evidence for a background-input mechanism.

### PUSV: observe-then-act is a security gap

PUSV demonstrates that an apparently correct screenshot can become unsafe before dispatch. Version 0.9 reduces that gap in complementary ways:

- browser mutations, scroll, and resize invalidate the generation;
- click targets revalidate signature, bounds, connectivity, visibility, and hit-test immediately before CDP dispatch;
- coordinate clicks bind a one-time point token to the exact element and proof;
- desktop actions re-enumerate PID/native-window ownership and geometry immediately before delivery;
- semantic actions re-resolve the exact frame-bound accessibility path and verify an application-owned postcondition when available.

This is partial PUSV coverage. Version 0.9 does not calculate target-patch SSIM or a fresh full-frame visual diff immediately before every action. DOM revalidation catches attacks that pure pixels miss, while a visually identical custom/canvas surface can still change meaning without a semantic signal. The correct claim is defense in depth, not visual atomicity.

## Version 0.9 architecture

```text
browser-only agent
      |
      v
loopback UI / REST API
      |
      v
Rust server -- version + protocol + session + sequence handshake
      |
      +-- Chromium extension
      |      +-- one expiring debugger/tab lease
      |      +-- Chrome-native debugger warning
      |      +-- page pill + trusted Stop + browser synthetic cursor
      |      +-- turn + DOM generation + target proof
      |
      +-- separate native helper
             +-- one exact-window frame authority
             +-- AX/UIA semantic snapshot and action
             +-- target-routed background input
             +-- foreground/focus/hardware-cursor/desktop oracle
             +-- bounded exact-window frame feed
             +-- pointer composited into returned frames
```

The helper opens no listening socket. It authenticates outbound to loopback and exposes only status, share start/status/stop, observe, move, click, drag, scroll, text, key, semantic invoke, and semantic value write. There is no shell, filesystem, process-launch, clipboard, downloader, arbitrary-code, credential, hidden-user, VM-management, or telemetry command.

### Browser lease lifecycle

- Explicit start attaches `chrome.debugger`, enables required CDP domains, persists a lease in extension session storage, starts a heartbeat, and shows the page overlay.
- A normal browser operation may create a lease only when the tab has no hard revocation. Once Chrome or a human Stop control cancels, a durable global pause rejects all remote mutations until a person selects Resume in the extension popup; a remote explicit start cannot clear it.
- One lease owns one tab. Switching targets releases the old attachment first.
- `page.observe` advances the turn and returns control state. Pointer moves advance `moveSequence` and wait for the exact arrival point before the click is committed.
- Page mutation, scroll, or resize invalidates the DOM observation. The bridge's own overlay mutations are ignored.
- Chrome `onDetach`, page Stop, popup release, TTL, heartbeat failure, target close, bridge pause, and disconnect all revoke authority.

### Computer share and pointer lifecycle

- `computer.share.start` binds one share ID to one non-minimized native window and a requested 1–10 FPS rate.
- The helper repeatedly invokes the same bounded exact-window observation path and emits unsolicited `computer.share.frame` events with monotonic sequence metadata.
- The server keeps the latest sanitized computer observation and screenshot. It does not queue an unbounded video history.
- Pointer state is owned by the helper session and remains window-specific. A move to another window reseeds it rather than implying a global desktop location.
- All model-facing pointer/element coordinates use delivered image pixels. OS screen bounds remain separately labeled diagnostic data.
- The hardware cursor is snapshotted as an invariant and must remain unchanged.

## Exact-window backend boundary

The macOS and Windows backends retain the version 0.8 refusal contract:

1. An observation binds one `(pid, native window id)` and captured geometry.
2. The helper re-enumerates that identity immediately before input.
3. Semantic AX/UIA is preferred where the platform exposes a reliable action and postcondition.
4. Pixel input is target-routed to the exact window, never posted as global HID.
5. Foreground process, user focus, hardware cursor, and active desktop are checked around delivery.
6. Unknown or unsupported delivery fails; it never silently activates the app or changes input mode.

macOS uses window capture plus dynamically resolved private SkyLight routing. Only non-minimized windows on the active Space are mutable, and OS changes can break private symbols. Windows uses exact-HWND capture/UIA and background window messages. Elevated, game, secure-input, protected, and custom-rendered surfaces may reject either backend.

## Isolation boundary

The bounded live feed satisfies a viewing requirement: the model and a person can observe one background window without capturing unrelated desktop windows. Target-routed input satisfies a non-interruption requirement on supported applications. Neither satisfies a hostile-workload isolation requirement.

True independent input requires a separately managed environment such as:

- a Windows RDP/child desktop session with credentials, edition/policy checks, resolution management, disconnect semantics, and cleanup;
- a macOS VM or separate login with an explicit data-sharing and credential model;
- a Linux VM/container plus virtual X/Wayland display when application compatibility permits.

UFO²'s RDP-loopback PiP design is the closest reviewed architecture, but its paper is not evidence that the pinned public UFO source ships that complete mode. Apple High Performance Screen Sharing also cannot be treated as a transparent local background seat. An `isolated-session` backend may be added later only as a separate capability with visible prerequisites and lifecycle. It must never be aliased to `background-window` or silently enabled.

## Release acceptance criteria

Version 0.9 evidence must independently prove:

- real Chrome loaded from `chrome://extensions`, with the extension package actually selected;
- native Chrome debugger warning remains visible throughout the lease;
- in-page pill, browser cursor, and trusted Stop are visible and functional;
- Chrome Cancel produces `canceled_by_user`, revokes the lease, blocks fallback, and requires popup Resume followed by a new explicit lease;
- stale WebSocket session/sequence, browser turn/generation/move sequence, and computer frame IDs fail closed;
- each browser command class and each helper action has a machine-readable result plus a screenshot where visual evidence is meaningful;
- exact-window live frames carry monotonic sequence, correct dimensions/scales, share state, and the settled synthetic pointer;
- macOS foreground/focus/hardware-cursor/Space invariants remain unchanged for supported background actions;
- Windows compiles and runs the shared contracts, while representative Windows UIA/background runtime coverage remains explicitly identified if it is not executed on a real Windows host.

Transport success alone is diagnostic evidence. A platform/action combination is supported only after a representative application-owned outcome and the advertised non-interruption invariants are observed.
