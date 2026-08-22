# Session-visible browser control and non-interrupting computer use

Research snapshot: 2026-08-22. Historical version references below identify when a design property entered the project. [Capabilities](CAPABILITIES.md) and [Limitations](LIMITATIONS.md) are authoritative for the current implementation boundary.

## The corrected target

Version 0.6 captured a physical display and injected global input. A live end-to-end run proved that architecture wrong for concurrent use: it moved the person's hardware pointer and typed into whichever application owned focus. Version 0.8 replaced it with exact-window capture, semantic-first action, target-routed background input, and foreground/cursor/desktop invariants.

The corrected design separates two properties that must not be conflated:

1. **Session visibility:** a person can tell when browser or helper control is active, see a synthetic pointer in the agent's image stream, and stop control through a trusted surface.
2. **Non-interruption:** desktop input is routed to one observed window while the platform-specific foreground/window-focus oracle, hardware cursor, and active desktop remain unchanged.

Neither property creates an isolated operating-system session. A shared image of a background window is not a VM, remote desktop, virtual display, sandbox, or separate input seat.

Windows therefore has two honest architectural modes, even when a product eventually presents them through one UI:

| Mode | Capture and input boundary | Can operate an already-open main-session app? | Independent real input seat? |
|---|---|---:|---:|
| `shared-window` | Exact-HWND WGC plus a capability router across UIA, verified window messages, and browser CDP | Yes, only for proven routes | No |
| `isolated-child-session` | Loopback RDP child session with its own helper, browser profile, focus, cursor, and input queue | No; it launches a separate app instance | Yes |

WGC solves exact-window pixels. UIA solves supported semantic control patterns. `PostMessageW` queues HWND messages but does not create trusted physical input or prove application acceptance. Windows child sessions solve the independent-seat problem, with edition/policy, one-active-child, lifecycle, profile, and cleanup constraints. A child session is also not a hostile-workload sandbox because it inherits the user's identity and access; use Windows Sandbox or a VM for that boundary.

## Evidence reviewed

The review used pinned source code, vendor source/documentation, primary papers, empirical public-extension packages, and community reports. Popularity was used only for discovery. Architecture claims come from source or primary documentation; community reports identify failure modes but do not establish platform guarantees.

| Source | Pinned snapshot | Focus relevant to this bridge |
|---|---:|---|
| [Chrome `debugger` API](https://developer.chrome.com/docs/extensions/reference/api/debugger) and [Chromium implementation](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/chrome/browser/extensions/api/debugger/debugger_api.cc) | Chromium main reviewed 2026-08-18 | A held extension debugger attachment creates Chrome's warning UI, keeps the MV3 worker alive on supported Chrome versions, exposes `canceled_by_user`, and fails pending debugger calls when the warning is canceled |
| [OpenAI Computer Use documentation](https://learn.chatgpt.com/docs/computer-use) and [Codex announcement](https://openai.com/index/codex-for-almost-everything/) | official pages reviewed 2026-08-20 | Documents macOS background behavior, Screen Recording and Accessibility prerequisites, and Windows foreground behavior; it does not disclose the native API implementation, so ScreenCaptureKit or SkyLight attribution remains inference |
| Public ChatGPT Chrome extension package | `1.2.27259.19709`; ID `hehggadaopoacecdllhhajmbjkdcmajg`; SHA-256 `f9ba06c44525b53a0189d0ad97cf1d457987970063aea0609dec31b1d6782c96` | Empirical benchmark: held debugger sessions, exclusive tab ownership, managed tab group, serialized target work, session/turn/move state, and acknowledged synthetic-cursor arrival |
| Public Claude Chrome extension package | `1.0.85`; ID `fcoeoabgfenejglbffodgkkbkcdhcgfn`; SHA-256 `5c1c1318acf10bb4638be129ae34f9dfe728b867a70c603382f89e66a5d08be3` | Empirical benchmark: clear page indicator, trusted Stop path, heartbeat, page glow, and a simpler CSS synthetic cursor |
| [Cua](https://github.com/trycua/cua/tree/0213cd82fd8f5f35d530e7b3eda5286511bbbc10) | `0213cd82fd8f5f35d530e7b3eda5286511bbbc10` | Interactive-session daemon, UIA CacheRequest, UIA/MSAA routing, framework-specific refusals, checked outcomes, and repository-owned Windows fixtures; shared-session background routing remains distinct from an isolated input seat |
| [winappCli](https://github.com/microsoft/winappCli/tree/2280dfd4f628451a0c729f8fe40cd39a1f93be64) | `2280dfd4f628451a0c729f8fe40cd39a1f93be64` | Stable Windows selectors, key-message construction, WGC `CreateFreeThreaded`, RangeValue/LegacyIAccessible fallbacks, and explicit foreground/locked-session limits |
| [Interceptor](https://github.com/Hacker-Valley-Media/Interceptor/tree/227482c2b7535fdc508479bc5e995f35b737732c) | `227482c2b7535fdc508479bc5e995f35b737732c` | Current macOS source combines AX-first action, per-PID `CGEvent` delivery, ScreenCaptureKit streams, first-frame waiting, late-frame rejection, and a private WindowServer still-image fallback. Its continuous app capture selects the first matching window and publishes no source sequence or dropped-frame count, so it is evidence for the component techniques rather than exact-window authority or freshness parity |
| [OpenKosmos](https://github.com/microsoft/open-kosmos/tree/9e45cc51bae33d65e412824ab5915606f99ae038) | `9e45cc51bae33d65e412824ab5915606f99ae038` | Visible click-through native cursor, fresh foreground-app confirmation gates, action audit, cancellation checks, and multi-display coordinate handling. Its computer-use path captures displays and drives the physical input seat, so its overlay and consent design are relevant while its transport is intentionally not the non-interrupting exact-window mode |
| [Browser Control](https://github.com/anomalyco/browser-control/tree/f12441cb4de55967f2c5ce31ae05b5a3a2875dac) | `f12441cb4de55967f2c5ce31ae05b5a3a2875dac` | Real-profile extension relay with reconnect alarms, attached-target inventory, per-session target ownership, managed groups, debugger-detach reason forwarding, page status, and a synthetic cursor. The reviewed source does not turn Chrome Cancel into a durable global human-pause latch or expose a trusted page Stop equivalent |
| [Browy](https://github.com/BrowyHQ/browy/tree/2f71ee907c5f571ff759ca9c0cb2f9c880366a5c) | `2f71ee907c5f571ff759ca9c0cb2f9c880366a5c` | Real-profile MV3/native-host bridge with session-to-tab multiplexing, alarm-driven orphan debugger cleanup, and opener-popup attachment. Its `onDetach` path only forgets the tab, and its visible Stop cancels model generation rather than latching browser input authority |
| [Apple ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit) and [exact-window session](https://developer.apple.com/videos/play/wwdc2022/10155/) | official documentation reviewed 2026-08-20 | Desktop-independent exact-window streams, occlusion/offscreen behavior, minimized-stream pause, child-window boundary, frame metadata, and bounded surface queues; capture does not supply a second input seat |
| [Windows Graphics Capture `CreateForWindow`](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow) | official documentation reviewed 2026-08-20 | Exact-HWND capture item and OS-owned capture indication; this is capture, not universal background input |
| [agent-browser](https://github.com/vercel-labs/agent-browser/tree/548b159b30eef119ccf6846c8bc807d0eaa3f6f8) | `548b159b30eef119ccf6846c8bc807d0eaa3f6f8` | Persistent browser sessions, serialized CDP interaction, live screencast input, monotonic frame IDs, latest-frame-wins delivery, and optional renderer acknowledgements |
| [pi-computer-use](https://github.com/injaneity/pi-computer-use/tree/de725835d3b0e3bd13aa8885d6c3f3a9dc23bcdc) | `de725835d3b0e3bd13aa8885d6c3f3a9dc23bcdc` | Immutable state-scoped observations, resource epochs, per-resource serialization, successor state/diffs, checked action outcomes, and an optional non-blocking native ghost cursor on macOS |
| [OSWorld](https://arxiv.org/abs/2404.07972) | 2024 paper | Long-horizon desktop evaluation and environment diversity; its reference execution path uses the physical desktop/cursor and is not a concurrent-user isolation design |
| [OSWorld 2.0](https://arxiv.org/abs/2606.29537) | 2026 paper | Adds streaming interaction, dynamic environments, hidden-state recovery, partial scoring, and safety reports; it reinforces continuous observation and explicit verification, but still does not define a background-input transport |
| [GUI vs. CLI: Execution Bottlenecks](https://arxiv.org/abs/2606.24551) | 2026 paper | A matched 440-task study finds that verifier-guided skill augmentation outperforms either raw screen-only GUI control or the original skill set, reinforcing semantic-first routes plus application-owned postconditions rather than a single universal actuator |
| [WindowsWorld](https://arxiv.org/abs/2604.27776) | 2026 paper | Cross-application Windows workflows with intermediate inspection and final verifiers; it strengthens the case for multi-step recovery and state verification but does not establish background-input correctness |
| [Microsoft UFO](https://github.com/microsoft/UFO/tree/96983c73ed09e884a5f1d7ff8936c953b234b684) and [UFO²](https://arxiv.org/abs/2504.14603) | source `96983c7`; 2025 paper / 2026 TMLR | UIA/Win32 automation in shipped source; the paper's RDP-loopback Picture-in-Picture architecture is the strongest reviewed separate-input-session design, but the reviewed public repository still described PiP Desktop as coming soon |
| [UI-TARS](https://github.com/bytedance/UI-TARS/tree/582f3a7ea5d285ee8ed9e2e84048d1ab01453c49) | `582f3a7ea5d285ee8ed9e2e84048d1ab01453c49` | Vision-language grounding and screenshot/action history; this improves the planner/model, while its normal desktop operator remains physical screen input rather than a non-interrupting transport |
| [Temporal UI State Inconsistency / PUSV](https://arxiv.org/abs/2604.18860) | 2026 paper | Formalizes observation-to-action TOCTOU and rechecks local pixels, the global frame, and window identity immediately before action; also shows that visual checks alone miss zero-visual-footprint DOM replacement |
| [CaMeLs Can Use Computers Too](https://arxiv.org/abs/2601.09923) | 2026 v3 | Shows why continuous UI observation and prompt-injection isolation are separate system-design problems |
| [Apple High Performance Screen Sharing](https://support.apple.com/guide/mac-help/screen-sharing-type-options-mchl1883115d/mac) | macOS 14+ documentation | Can create virtual displays, but same-user use blanks hardware displays and prevents simultaneous local use; capture or sharing alone does not imply an independent input seat |
| [Power Automate PiP](https://learn.microsoft.com/power-automate/desktop-flows/run-desktop-flows-pip) and [unattended sessions](https://learn.microsoft.com/power-automate/desktop-flows/run-unattended-desktop-flows) | current documentation | Real Windows child/RDP sessions with credential, policy, lifecycle, and cleanup requirements |

Community signals included [real-session bridge discussion on Reddit](https://www.reddit.com/r/ClaudeAI/comments/1v5fz09/browser_bridge_an_mcp_server_that_drives_your/), [macOS exact-window capture expectations](https://www.reddit.com/r/macapps/comments/1f5hraj/screen_recording_software_with_possibility_to/), and issue reports in agent-browser and other automation projects about stale transports, frame/viewport scaling, and pointer offsets. [Cua issue #1467](https://github.com/trycua/cua/issues/1467) records a physical macOS 26.4.1 ScreenCaptureKit failure despite both permissions, while current [Apple developer reports](https://developer.apple.com/forums/tags/screencapturekit) include signing-identity permission resets, display-sleep failures, and stream regressions. These reports informed failure and recovery tests; they are not cited as platform guarantees.

The ChatGPT and Claude package rows are black-box observations of publicly distributed Chrome packages on the date above, not claims about an unpublished server protocol or future versions. Their exact version, extension ID, and hash are recorded so findings are not silently generalized to another package. Both reviewed manifests requested `debugger` and `tabGroups`; neither requested `tabCapture`. OpenAI's official product behavior is cited separately and does not confirm a particular native implementation.

## Three different visibility surfaces

### 1. Chrome's native debugger warning

`chrome.debugger.attach` is the mechanism that makes desktop Chrome display the extension-owned warning, normally rendered as **<extension name> started debugging this browser**. Chromium creates this warning when an extension debugger client attaches, except for explicitly suppressed command-line or policy-installed cases. The warning is browser chrome, not HTML, and a page cannot faithfully recreate it.

Chromium's Cancel path marks the detach reason `canceled_by_user`, fails pending debugger requests, sends `chrome.debugger.onDetach`, and closes the attachment. The infobar delegate is extension-scoped, so canceling it terminates that extension's attached debugger clients. Local Browser Bridge uses one attachment at a time, making the effective boundary one controlled tab.

Attaching only around a click and immediately detaching makes the warning flash or disappear and removes the person's reliable indication of ongoing authority. The bridge therefore holds the attachment for an explicit, expiring lease. Unexpected detach is a hard revocation; there is no synthetic DOM-click fallback.

### 2. The extension's page overlay

The page pill and pointer are content injected by the extension. They communicate product-specific state that Chrome's native warning does not know: controlled tab, turn, pointer sequence, and a trusted Stop action. The overlay uses an isolated closed shadow tree, randomized public host plus private marker per document, and shadow-important resets for critical visual host, pseudo-element, and backdrop properties; JavaScript attribute/ancestor checks handle renderer-side accessibility. Snapshot invalidation excludes only the retained host and exact closed-shadow-owned objects, so a copied public ID, matching ancestor, or page-owned light child cannot hide a page mutation. Lease start and every same-tab reuse require the host to remain the direct document-element child, then reopen it before bounded render/layout/computed-style plus document/closed-shadow hit tests; capture begin/end separately acknowledge intentional hiding and restoration. The Chrome process resolves exact `:root`, pins the marker's innermost closed-shadow host, rejects an intermediate light/shadow wrapper, checks initial and fresh-final host/root ancestry and accessibility-critical attributes, checks raw ordered top-layer membership and later same-root ancestry, and resolves five `ignorePointerEventsNone` hit-test points through that host. A 1.5-second/512-step proof budget, top-layer revision seqlock, separate pre-render-request content-loss generation, and absolute three-second dirty-event/loss deadline address passive and continuously renewed coverage without treating the bridge's own re-top events as content loss.

This is independently stoppable. A `document_start` isolated guard sees trusted Stop pointer/click activation before later page listeners, and the content handler rechecks the exact session plus document/closed-shadow Stop ownership before the service worker releases the lease. A hostile page cannot call that privileged handler as the extension. The watchdog attempts a sample every 500 ms only while its earlier acknowledgement is idle; the browser acknowledgement is bounded at two seconds and the service-worker dirty deadline is three seconds plus scheduling/transport timing, so the design does not claim a 500 ms revocation guarantee. Two animation-frame opportunities (or the 250 ms fallback), browser DOM ancestry, and browser hit tests are still not compositor or physical-pixel proof, and a hostile page can race after the final check and before the next CDP action; no atomic browser primitive joins those events. The experimental DOM dependencies are pinned to Chrome 140+ and live conformance-tested. Conversely, the pill cannot override Chrome's Cancel action; `onDetach` remains authoritative and page-independent.

The reviewed Claude package emphasizes this human-facing layer: a persistent status pill, clear Stop action, heartbeat, and visual attention treatment. The reviewed ChatGPT package goes further on controller state and pointer arrival. The bridge combines those product lessons without copying either package's private protocol or source.

### 3. The computer helper's synthetic pointer

The helper pointer is Rust state owned by one helper process. Pixel actions plan a bounded cubic Bézier path with minimum-jerk timing, deliver intermediate window-local moves through the existing exact-window background route, and record an arrival sequence. The last state is composited into exact-window observations and share frames, with an outline, session-derived color, action/ring state, and explicit image/screen coordinates.

It is not the hardware cursor, and the helper does not create a native click-through desktop overlay. Consequently, a person looking only at the physical desktop does not see this helper pointer; a person or model looking at the returned exact-window frames does. The current helper's action loop completes the path before it can emit the next captured frame, so shared frames reliably show the settled pointer but are not claimed to reproduce every intermediate animation sample.

The separately observed ChatGPT desktop **using your computer / Esc to cancel** treatment is also not Chrome's debugger warning. It is an operating-system/app-level computer-use surface. Reproducing its semantics would require a native overlay and global trusted cancel integration, which this release does not claim.

## What the benchmark projects prioritize

### Cua: renderer and actuator as one native session

Cua is the strongest open-source pointer-rendering reference reviewed. Its cursor core preserves position, heading, press state, theme, session label, idle fade, and target window. Its path planner chooses a minimum-turning-radius Dubins arc/straight/arc route; motion adds speed floors and spring settling. Platform code renders transparent click-through overlay windows (`NSWindow`, layered/no-activate Win32, or X11 input shaping) and keeps those windows out of input ownership.

Local Browser Bridge adopts the durable product properties—session ownership, bounded motion, stable style, press/action state, explicit coordinate spaces, and screenshot compositing—but not Cua's overlay implementation or Dubins planner. It uses its own bounded cubic candidates and minimum-jerk timing and must therefore be described as benchmark-informed, not equivalent to Cua's native overlay.

### agent-browser: fresh live transport under backpressure

agent-browser's streaming path distinguishes browser/session lifetime from client lifetime. It assigns monotonically increasing frames, keeps the newest frame instead of building an unbounded backlog, dispatches input independently from frame writes, and optionally permits one in-flight frame until the renderer acknowledges it. Its public issue history also demonstrates why screenshot dimensions and input coordinate metadata must come from the actual frame rather than a configured viewport.

Local Browser Bridge uses a smaller bounded share contract: one exact window, a requested 1–10 FPS maximum cadence, monotonic share sequence, explicit image dimensions and X/Y transport scales, and PNG events capped at 1,000,000 pixels. Persistent ScreenCaptureKit and Windows Graphics Capture callbacks park only the latest native frame. When the connector negotiates acknowledgement pacing, one encoded frame remains in flight and newer pending frames replace older ones with an explicit dropped-frame count. This bounds memory without claiming a guaranteed frame rate, video-stream latency, or fan-out.

### pi-computer-use: immutable state and resource ownership

pi-computer-use treats each observation as immutable state, serializes live work per physical resource, invalidates a base epoch before mutation, records checked outcomes, and creates one successor state. That is more important for correctness than cursor cosmetics. Its native cursor is observational and may be superseded without blocking action delivery.

Local Browser Bridge applies the same class of invariant at narrower layers: WebSocket session and sequence, one browser control lease, observation turn, DOM generation/revision, exact-window frame ID, PID/native-window revalidation, and post-action observation. It does not claim pi-computer-use's complete state store, resource-epoch scheduler, or transaction/diff system.

### Interceptor and OpenKosmos: component techniques are not the authority boundary

Interceptor's current macOS source independently validates several implementation choices used here: AX before pixels, `CGEvent.postToPid` for background delivery, ScreenCaptureKit for native frames, an explicit wait for the first frame, and rejection of callbacks after stop. It also shows the limitation of stopping at the component level. Its continuous app mode selects the first window owned by a named application, caches a latest JPEG without a source sequence or dropped-frame proof, and routes synthetic input to a PID rather than proving the exact window receiver. Local Browser Bridge therefore keeps the exact `(PID, native window id)` capability, monotonic source sequence, bounded replacement accounting, receiver proof, and foreground/focus invariants instead of treating a successful per-PID post as an accepted exact-window action.

OpenKosmos prioritizes a different product surface: a persistent click-through cursor on the physical desktop, one-time action confirmations, a fresh foreground-app allowlist check, cancellation checkpoints, and an audit trail. Those are strong visibility and consent references. Its actuator still uses the shared physical input seat and may focus the target, so adopting its overlay would not make its transport non-interrupting. Version 0.12.2 keeps returned-frame pointer evidence and target-routed input; a trusted native Stop/Esc surface and a physical-desktop overlay remain explicitly open deltas rather than inferred from capture.

### Current real-profile browser relays: lifecycle recovery without durable handback

Browser Control and Browy confirm that the dominant 2026 real-profile architecture remains a Manifest V3 extension connected to a local process, with `chrome.debugger` held on selected tabs, reconnect repair, target ownership, and cleanup of orphaned attachments. Browser Control additionally forwards the exact debugger detach reason and maintains page status and grouping; Browy multiplexes sessions and inherits opener popups. Neither reviewed snapshot promotes Chrome Cancel into a durable, global mutation pause that only a trusted human surface can clear. Local Browser Bridge retains that stronger handback boundary: `canceled_by_user`, page Stop, or popup Release revokes the lease and refuses all remote mutations until popup Resume.

### UI-TARS and OSWorld: planner quality is not transport isolation

UI-TARS focuses on vision-language grounding, action vocabulary, and prior screenshot/action context. OSWorld focuses on representative, long-horizon task evaluation. OSWorld 2.0 further emphasizes streaming state, dynamic environments, constraint tracking, hidden-state recovery, and verification rather than optimistic completion. These properties shape the bridge's persistent streams, explicit state epochs, application-owned postconditions, and evidence runner, but none makes physical desktop input non-interrupting or creates a separate input seat.

### PUSV: observe-then-act is a security gap

PUSV demonstrates that an apparently correct screenshot can become unsafe before dispatch. The bridge reduces that gap in complementary ways:

- browser mutations, scroll, and resize invalidate the generation;
- click targets revalidate signature, bounds, connectivity, visibility, and hit-test immediately before CDP dispatch;
- coordinate clicks bind a one-time point token to the exact element and proof;
- desktop actions re-enumerate PID/native-window ownership and geometry immediately before delivery;
- semantic actions re-resolve the exact frame-bound accessibility path and verify an application-owned postcondition when available.

This is partial PUSV coverage. The bridge does not calculate target-patch SSIM or a fresh full-frame visual diff immediately before every action. DOM revalidation catches attacks that pure pixels miss, while a visually identical custom/canvas surface can still change meaning without a semantic signal. The correct claim is defense in depth, not visual atomicity.

## Current architecture

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
             +-- platform foreground/window-focus + cursor/desktop oracle
             +-- persistent native exact-window frame source
             +-- bounded PNG event transport
             +-- pointer composited into returned frames
```

The helper opens no listening socket. It authenticates outbound to loopback and exposes only status, share start/status/stop, observe, move, click, drag, scroll, text, key, semantic invoke, and semantic value write. There is no shell, filesystem, process-launch, clipboard, downloader, arbitrary-code, credential, hidden-user, VM-management, or telemetry command.

### Browser lease lifecycle

- Explicit start attaches `chrome.debugger`, enables required CDP domains, persists a lease in extension session storage, starts a heartbeat, and shows the page overlay.
- A normal browser operation may create a lease only when the tab has no hard revocation. Once Chrome or a human Stop control cancels, a durable global pause rejects all remote mutations until a person selects Resume in the extension popup; a remote explicit start cannot clear it.
- One lease owns one tab. Switching targets releases the old attachment first.
- `page.observe` advances the turn and returns control state. Pointer moves advance `moveSequence` and wait for the exact arrival point before the click is committed.
- Page mutation, scroll, or resize invalidates the DOM observation. Only exact retained host/closed-shadow-owned mutations are ignored; public-ID lookalikes and page-owned light DOM are ordinary page mutations.
- Chrome `onDetach`, page Stop, popup release, TTL, heartbeat failure, target close, bridge pause, and disconnect all revoke authority.

### Computer share and pointer lifecycle

- `computer.share.start` binds one share ID to one non-minimized native window and a requested 1–10 FPS maximum cadence.
- The live-share path starts a persistent ScreenCaptureKit `SCStream` on macOS or a project-owned Windows Graphics Capture `CreateFreeThreaded` frame pool on a dedicated MTA owner thread. macOS one-shot observation remains a separate snapshot path; Windows one-shot observation starts the same bounded WGC implementation and proves its own shutdown after one frame.
- Apple documents [`SCStream.updateConfiguration`](https://developer.apple.com/documentation/screencapturekit/scstream/updateconfiguration%28_%3Acompletionhandler%3A%29) as the uninterrupted way to change a running stream. The macOS path uses it for size or display-scale changes while preserving the share ID, and accepts the new geometry epoch only after a strictly newer WindowServer display time and the configured pixel dimensions both match. Position-only moves advance the same authority boundary without recreating the stream.
- Native callbacks keep only the newest accepted exact-window frame; the helper composites its pointer and emits bounded `computer.share.frame` PNG events with monotonic sequence metadata.
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
5. The platform-specific foreground/window-focus oracle, hardware cursor, and active desktop are checked before and after delivery.
6. Unknown or unsupported delivery fails; it never silently switches to global/foreground input. The macOS machine contract explicitly discloses the transient target `AXFrontmost` focus lease used by supported focus-capable routes.

macOS one-shot observation uses the snapshot backend, while live sharing uses a persistent desktop-independent ScreenCaptureKit exact-window stream. The pinned [`screencapturekit` 8.0.1 bridge](https://github.com/doom-fish/screencapturekit-rs/blob/2a9f13bcbeadb0aabc5596f0ff3d2ba71da8c1d0/swift-bridge/Sources/CoreMedia/CoreMedia.swift#L26-L38) casts Apple's numeric frame-status attachment directly to a Swift enum, whereas [Apple's canonical sample](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos) decodes the raw integer first. The helper therefore reads the raw CFNumber as a compatibility fallback and accepts only `Complete`, retaining a fail-closed status gate. That dependency also guards the base `updateConfiguration` method at macOS 14 even though Apple exposes it from 12.3; the repository vendors the exact 8.0.1 source with a documented one-line availability correction at the product's macOS 13 floor.

The packaged persistent-share probe exposed a receiver distinction that raw window cardinality misses. On the tested macOS build, starting `SCStream` added a 66-by-20 capture indicator inside the selected title bar. WindowServer reported that surface as same-PID, on-screen, shareable, layer 0, and Accessibility mapped it as a non-minimized `AXWindow`/`AXDialog`. From the retained probe evidence, we infer that neither raw sibling count nor the AX-intersection approach in [Cua `df57e610`](https://github.com/trycua/cua/commit/df57e610) can distinguish that surface from a real application window on this build. Title, size, subrole, and location are also compositor heuristics rather than keyboard authority.

The implementation now separates eligibility, inactive sibling selection, and receiver authorization. An independent `CGWindowListOptionAll` inventory must contain one exact owner/layer/geometry row and explicitly report the selected window on screen; required `AXChildren` and `AXWindows` reads must map that ID to a non-minimized top-level element. Sibling cardinality never authorizes input. The packaged two-window probes also found that an application-level `AXFocusedWindow` write is unavailable/no-op on this build and that writing window `AXFocused=false` is not a valid reverse operation. Window [`AXMain`](https://developer.apple.com/documentation/applicationservices/kaxmainattribute) is the useful public selector, but Apple does not define “main” as sufficient proof of key focus. The helper therefore retains the exact prior and requested AX windows, requires both `AXMain` attributes settable before any mutation, admits only a baseline where target `AXMainWindow == AXFocusedWindow`, performs the requested `AXMain` selection while the user is still fully active, and bounded-polls exact main+focused read-back with the target still inactive. After private process-focus preparation and a minimum settle, the app's [`AXFocusedWindow`](https://developer.apple.com/documentation/applicationservices/kaxfocusedwindowattribute), `AXMainWindow`, and `AXFrontmost` must describe the exact requested active target. Text additionally resolves [`AXFocusedUIElement`](https://developer.apple.com/documentation/applicationservices/kaxfocuseduielementattribute) through `_AXUIElementGetWindow`, `AXWindow`, `AXTopLevelUIElement`, or bounded parent ancestry and repeats that proof after event construction immediately before every scalar key-down. A receiver loss after any earlier scalar is outcome-unknown and is never safe to auto-retry.

Different-process preparation is a dual-state lease. It captures the user's front ProcessSerialNumber/PID and exact main/focused AX window plus the target app's prior exact main/focused AX window, and requires that the second user-window sample still equal the original snapshot rather than silently adopting a newly focused sibling. It resolves each target window through `CGSMainConnectionID → SLSGetWindowOwner → SLSGetConnectionPSN` and refuses the old PID-only fallback, so requested and prior IDs must bind the same exact target connection. Packaged probes exposed the private-focus state transitions on the tested build: after inactive AXMain selection, posting the saved-user `Defocus` record leaves WindowServer's front PSN/PID unchanged but changes that app's `AXFrontmost` from true to false; posting target `Focus` then makes the exact requested target `AXFrontmost=true` while the saved user remains WindowServer-front. Preparation records require the exact current target phase, target PSN/raw destination, and a final front-PSN → exact user AX window/expected-frontmost → front-PSN sample both before and after dispatch accounting. Restore cleanup runs after outcome accounting has already begun; each cleanup record instead repeats the exact target/raw/user authorization once immediately before its write. Every following phase is bounded-polled rather than inferred from the SkyLight return code. An unexpected active target is never cleaned up when the invocation proves that target Focus was not posted.

Restoration first polls requested-active → requested-inactive. A genuine AppKit field editor can keep the requested window as the native receiver even after a normal prior Focus record, so AXMain alone cannot reliably reverse a post-text key-window split. The retained live probe established a bounded cleanup: Focus the exact prior; accept only exact active requested-or-prior; if requested remains active, send the target-only paired make-key records for prior; prove target `AXMainWindow == AXFocusedWindow == prior` and `AXFrontmost=true`; then Defocus prior, prove prior inactive, and finally return the user's exact front window to `AXFrontmost=true`. The record bytes match the make-key subprimitive in [Cua `b296ec9`](https://github.com/trycua/cua/blob/b296ec9cbf460f360cf8ea2e203c26bcc92614b0/libs/cua-driver/rust/crates/platform-macos/src/input/skylight.rs#L618-L659) and [yabai `dd84572`](https://github.com/koekeishiya/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1269-L1322), but the authorization contract is this project's own: unlike yabai's full focus operation and Cua's foreground `make_exact_window_key`, Local Browser Bridge never calls `_SLPSSetFrontProcessWithOptions`, never performs `AXRaise`, and uses the pair only during exact target cleanup under the unchanged-user lease. This mechanism and its non-interruption behavior are empirical findings on the retained tested build, not a public Apple guarantee.

The fixed restore deadline is divided into earlier target-work and user-Focus-authorization cutoffs, a target-independent user-restoration poll, and a retained safe-retry slice. Cleanup failure dominates the original action result. If target state becomes unknown, emergency user compensation intentionally performs no target read or write: it can post saved-user Focus only after proving the unchanged front PSN/PID, exact saved window with `AXFrontmost=false`, raw user restorability, and the same deadline. Target uncertainty still makes the command outcome unknown, but it cannot consume the time reserved to restore and prove the exact user owner. A same-app foreground sibling mismatch refuses before any focus-capable pixel or keyboard side effect. Before each key-down or new focus-capable pointer mutation, the released user owner and exact requested-active target receiver are re-proved; target receiver proof is the final AX read before keyboard dispatch. Long drag repeats the owner proof before each drag event, while an already-posted down event still receives unconditional release. The unrelated foreground app is inspected through read-only AX access; the existing Chromium manual-Accessibility opt-in is limited to the selected target. Per-call timeouts, collection caps, ancestry limits, and whole-proof deadlines fail closed. Synchronous `CGWindowListCopyWindowInfo` cannot be interrupted while in flight, so its result is checked against the same absolute deadline after return and a late inventory authorizes no record. These sandwiches narrow rather than eliminate the sub-sample TOCTOU because no public atomic API binds the final AX proof to the following private event post. The current selector still requires an on-screen, non-minimized active-Space target, and macOS changes can break the private symbols. ScreenCaptureKit's capture reach must not be generalized into cross-Space input.

The general macOS before/after oracle compares the frontmost process and front window. Focus-preparing routes add an exact Accessibility-window lease and refuse restoration if the user changed even to another window of the same foreground process. Windows additionally compares the foreground and focused HWNDs. These checks cannot make focus proof and event posting atomic, prove that no shorter transient change occurred, or roll back an event the target already processed.

Windows live sharing uses a persistent exact-HWND WGC session, while input uses UIA and background window messages. A successfully queued window message does not prove the application acted. Minimized, elevated, game, secure-input, protected, and custom-rendered surfaces may reject either backend. Neither platform silently falls back to foreground/global input.

## Isolation boundary

The bounded live feed satisfies a viewing requirement: the model and a person can observe one background window without capturing unrelated desktop windows. Target-routed input satisfies a non-interruption requirement on supported applications. Neither satisfies a hostile-workload isolation requirement.

True independent input requires a separately managed environment such as:

- a Windows RDP/child desktop session with credentials, edition/policy checks, resolution management, disconnect semantics, and cleanup;
- a macOS VM or separate login with an explicit data-sharing and credential model;
- a Linux VM/container plus virtual X/Wayland display when application compatibility permits.

UFO²'s RDP-loopback PiP design is the closest reviewed architecture, but its paper is not evidence that the pinned public UFO source ships that complete mode. Apple High Performance Screen Sharing also cannot be treated as a transparent local background seat. An `isolated-session` backend may be added later only as a separate capability with visible prerequisites and lifecycle. It must never be aliased to `background-window` or silently enabled.

## Release acceptance criteria

Release evidence must independently prove:

- real Chrome loaded from `chrome://extensions`, with the extension package actually selected;
- native Chrome debugger warning remains visible throughout the lease;
- in-page pill, browser cursor, and trusted Stop are visible and functional;
- Chrome Cancel produces `canceled_by_user`, revokes the lease, blocks fallback, and requires popup Resume followed by a new explicit lease;
- stale WebSocket session/sequence, browser turn/generation/move sequence, and computer frame IDs fail closed;
- each browser command class and each helper action has a machine-readable result plus a screenshot where visual evidence is meaningful;
- exact-window live frames carry monotonic sequence, correct dimensions/scales, share state, and the settled synthetic pointer;
- macOS frontmost-process/window, hardware-cursor, and Space invariants remain unchanged for supported background actions;
- Windows compiles and runs the shared contracts, while representative Windows UIA/background runtime coverage remains explicitly identified if it is not executed on a real Windows host.

Transport success alone is diagnostic evidence. A platform/action combination is supported only after a representative application-owned outcome and the advertised non-interruption invariants are observed.
