# Session-visible browser control and non-interrupting computer use

Research snapshot: 2026-08-23. Historical version references below identify when a design property entered the project. [Capabilities](CAPABILITIES.md) and [Limitations](LIMITATIONS.md) are authoritative for the current implementation boundary.

## The corrected target

Version 0.6 captured a physical display and injected global input. A live end-to-end run proved that architecture wrong for concurrent use: it moved the person's hardware pointer and typed into whichever application owned focus. Version 0.8 replaced it with exact-window capture, semantic-first action, target-routed background input, and foreground/cursor/desktop invariants.

The corrected design separates two properties that must not be conflated:

1. **Session visibility:** a person can tell when browser or helper control is active, see a synthetic pointer in the agent's image stream, and stop control through a trusted surface.
2. **Non-interruption:** desktop input uses a sealed exact-target route while the platform-specific foreground/window-focus and active-desktop boundaries remain stable. Shared pointer movement is measured as concurrent activity rather than attributed to the helper from two coordinates alone.

Neither property creates an isolated operating-system session. A shared image of a background window is not a VM, remote desktop, virtual display, sandbox, or separate input seat. Same-session use is cooperative; true independent concurrency requires another session/desktop or a VM.

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
| [BackgroundComputerUse](https://github.com/actuallyepic/background-computer-use/tree/52116acfe0f2f57174f5e0166881abe944cb6eeb) | `52116acfe0f2f57174f5e0166881abe944cb6eeb` | Local macOS exact-window API with AX projection, window-scoped private event posting, post-action state rereads, a state token, and a click-through cursor overlay. Its reviewed capture is a one-shot deprecated `CGWindowListCreateImage` path rather than a persistent sequenced stream, and its state token does not hash pixel content |
| [DSH Computer Use](https://github.com/ZRui-C/dsh-computer-use/tree/0b0a0844018b56a6a8e95aefea6529004b8341c4) | `0b0a0844018b56a6a8e95aefea6529004b8341c4` | Text-first AX plus Vision OCR, snapshot-scoped refs with stale refusal, fresh post-action observations, targeted SkyLight/CoreGraphics input, and a click-through cursor overlay. Its macOS capture uses one-shot `SCScreenshotManager`, and it explicitly refuses the unsafe Stage Manager thumbnail geometry case |
| [Browser Bridge](https://github.com/vitalysim/browser-bridge/tree/5d88bab5b4402c5e33e3e0c0665609346fc17bd1) | `5d88bab5b4402c5e33e3e0c0665609346fc17bd1` (`0.16.0`) | Real-profile MV3 relay with 68 tools and a watch-mode timeline: epoch-bound cursors, explicit gaps/drops/blind health, popup following, and network/console causality. Its source and documentation also state the important debugger-ownership constraint: one `chrome.debugger` client owns a tab, so DevTools or another debugger-based controller cannot inspect the same active lease |
| [ParaDesk](https://github.com/sinpoce/ParaDesk/tree/dc840fac488374923b315a3fe8e6f3ee9060b964) | `dc840fac488374923b315a3fe8e6f3ee9060b964` | Source-available Windows child-session reference using `WTSEnableChildSessions(true)` and loopback RDP ActiveX `ConnectToChildSession=true`. It creates a genuinely separate input queue but requires Pro/Enterprise/Education, administrator setup, a reboot, RDP listener/firewall/credential-delegation changes, a separate app/browser instance, and still provides no security isolation from the same user account |
| [Apple ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit) and [exact-window session](https://developer.apple.com/videos/play/wwdc2022/10155/) | official documentation reviewed 2026-08-20 | Desktop-independent exact-window streams, occlusion/offscreen behavior, minimized-stream pause, child-window boundary, frame metadata, and bounded surface queues; capture does not supply a second input seat |
| [Windows Graphics Capture `CreateForWindow`](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow) | official documentation reviewed 2026-08-20 | Exact-HWND capture item and OS-owned capture indication; this is capture, not universal background input |
| [Windows Raw Input](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input), [`RegisterRawInputDevices`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerrawinputdevices), [`LowLevelMouseProc`](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc), and [`MSLLHOOKSTRUCT`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-msllhookstruct) | official documentation reviewed 2026-08-23 | `RIDEV_INPUTSINK` can deliver background `WM_INPUT`; the low-level record exposes injected flags but must return immediately, needs a message loop, and can be silently removed after a timeout with no notification. These are activity diagnostics, not continuous-capture or physical-user proof |
| [agent-browser](https://github.com/vercel-labs/agent-browser/tree/548b159b30eef119ccf6846c8bc807d0eaa3f6f8) | `548b159b30eef119ccf6846c8bc807d0eaa3f6f8` | Persistent browser sessions, serialized CDP interaction, live screencast input, monotonic frame IDs, latest-frame-wins delivery, and optional renderer acknowledgements |
| [pi-computer-use](https://github.com/injaneity/pi-computer-use/tree/de725835d3b0e3bd13aa8885d6c3f3a9dc23bcdc) | `de725835d3b0e3bd13aa8885d6c3f3a9dc23bcdc` | Immutable state-scoped observations, resource epochs, per-resource serialization, successor state/diffs, checked action outcomes, and an optional non-blocking native ghost cursor on macOS |
| [OSWorld](https://arxiv.org/abs/2404.07972) | 2024 paper | Long-horizon desktop evaluation and environment diversity; its reference execution path uses the physical desktop/cursor and is not a concurrent-user isolation design |
| [OSWorld 2.0](https://arxiv.org/abs/2606.29537) | 2026 paper | Adds streaming interaction, dynamic environments, hidden-state recovery, partial scoring, and safety reports; it reinforces continuous observation and explicit verification, but still does not define a background-input transport |
| [GUI vs. CLI: Execution Bottlenecks](https://arxiv.org/abs/2606.24551) | 2026 paper | A matched 440-task study finds that verifier-guided skill augmentation outperforms either raw screen-only GUI control or the original skill set, reinforcing semantic-first routes plus application-owned postconditions rather than a single universal actuator |
| [WindowsWorld](https://arxiv.org/abs/2604.27776) | 2026 paper | Cross-application Windows workflows with intermediate inspection and final verifiers; it strengthens the case for multi-step recovery and state verification but does not establish background-input correctness |
| [Microsoft UFO](https://github.com/microsoft/UFO/tree/96983c73ed09e884a5f1d7ff8936c953b234b684) and [UFO²](https://arxiv.org/abs/2504.14603) | source `96983c7`; 2025 paper / 2026 TMLR | UIA/Win32 automation in shipped source; the paper's RDP-loopback Picture-in-Picture architecture is the strongest reviewed separate-input-session design, but the reviewed public repository still described PiP Desktop as coming soon |
| [UI-TARS](https://github.com/bytedance/UI-TARS/tree/582f3a7ea5d285ee8ed9e2e84048d1ab01453c49) | `582f3a7ea5d285ee8ed9e2e84048d1ab01453c49` | Vision-language grounding and screenshot/action history; this improves the planner/model, while its normal desktop operator remains physical screen input rather than a non-interrupting transport |
| [Temporal UI State Inconsistency / PUSV](https://arxiv.org/abs/2604.18860) | 2026 paper | Formalizes observation-to-action TOCTOU and rechecks local pixels, the global frame, and window identity immediately before action; also shows that visual checks alone miss zero-visual-footprint DOM replacement |
| [Visual Confused Deputy](https://arxiv.org/abs/2603.14707) | 2026 paper | Treats mis-grounded or raced GUI authorization as a security failure rather than only a task failure, and evaluates an independent visual-plus-intent guardrail outside the agent's own perception loop |
| [AgentCIBench](https://arxiv.org/abs/2606.23189) and [released benchmark](https://github.com/UKPLab/arxiv2026-agentcibench) | 2026 paper and code | Measures cross-application contextual-integrity failures: visual co-location, ambiguous-task oversharing, and recipient misalignment. Exact-window capture prevents unrelated pixels from entering one frame, but it does not by itself prevent an authorized agent from reading one context and disclosing it in another |
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

Debugger ownership is exclusive at the tab boundary. While Local Browser Bridge holds its lease, a second debugger-based controller cannot attach to that tab. The v0.12.31 acceptance protocol therefore releases every Chrome Computer Use surface before browser candidate execution and never uses Chrome MCP through independent review. The deliberate macOS lane is narrower: one separately authorized app share is bound only to the non-product acceptance app, and its single button action cannot inspect or control the product target. The exact candidate-bound helper uses only the authenticated loopback API for stock-Chrome UI, the native picker, the extension popup, exact-window sharing, native input, and retained screenshots; the Local Browser Bridge API executes the browser-method matrix. A separate reviewer reads only immutable digest-bound exported image files, and preflight/postflight records attest that no competing browser-control surface was used or resumed. A screenshot made by a competing debugger client is not valid active-lease evidence.

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

OpenKosmos prioritizes a different product surface: a persistent click-through cursor on the physical desktop, one-time action confirmations, a fresh foreground-app allowlist check, cancellation checkpoints, and an audit trail. Those are strong visibility and consent references. Its actuator still uses the shared physical input seat and may focus the target, so adopting its overlay would not make its transport non-interrupting. Version 0.12.31 keeps returned-frame pointer evidence and target-routed input; a trusted native Stop/Esc surface and a physical-desktop overlay remain explicitly open deltas rather than inferred from capture.

### Current real-profile browser relays: lifecycle recovery without durable handback

Browser Control and Browy confirm that the dominant 2026 real-profile architecture remains a Manifest V3 extension connected to a local process, with `chrome.debugger` held on selected tabs, reconnect repair, target ownership, and cleanup of orphaned attachments. Browser Control additionally forwards the exact debugger detach reason and maintains page status and grouping; Browy multiplexes sessions and inherits opener popups. Neither reviewed snapshot promotes Chrome Cancel into a durable, global mutation pause that only a trusted human surface can clear. Local Browser Bridge retains that stronger handback boundary: `canceled_by_user`, page Stop, or popup Release revokes the lease and refuses all remote mutations until popup Resume.

Browser Bridge 0.16.0 pushes the passive-observation side further. Its watch mode keeps an epoch in the cursor, treats service-worker restart, disconnect, eviction, unscriptable pages, and lost watches as explicit gaps, follows tabs opened by the watched tab, and ties network/error records back to the user action that caused them. Those are useful future browser-observability benchmarks. They do not replace this project's authority checks: the reviewed source records debugger detach but does not expose a trusted global handback latch comparable to Local Browser Bridge's human pause, and its much broader cookie, storage, filesystem, request-replay, interception, and fuzzing surface would materially enlarge this project's default authority.

### BackgroundComputerUse and DSH: freshness and verification over cursor cosmetics

BackgroundComputerUse and DSH both prioritize semantic state plus application-owned rereads after action. BackgroundComputerUse adds a compact state token derived from window metadata, AX projection, focus/selection, and image dimensions; DSH makes every ref snapshot-scoped, rejects a stale snapshot, and returns a fresh bounded observation after exactly one action. DSH also combines AX with Vision OCR for semantic gaps and refuses a Stage Manager shelf-thumbnail geometry mismatch before asking ScreenCaptureKit to capture it.

The useful adoption is the policy, not their code: stale state never authorizes a guess, each action produces or requires a successor observation, and visual-only state needs pixels because a semantic token can remain unchanged. Version 0.12.31 carries layered observation/share identities, exact receiver proof, application-owned semantic postconditions, persistent native source sequences, dropped-frame accounting, and route-versus-pointer attribution. It does not claim BackgroundComputerUse's or DSH's native click-through overlay, broader semantic inventory, or OCR fusion; unlike both reviewed one-shot capture paths, its live-share contract is a persistent SCStream/WGC stream. BackgroundComputerUse's reviewed random-port loopback server also exposes no bearer-authentication or exact-Host gate, so it is a component benchmark rather than a transport-security reference.

### ParaDesk: a separate input seat is a different product mode

ParaDesk is the strongest reviewed running reference for genuinely independent Windows input. It enables a Windows child session and connects an RDP ActiveX surface to that local session; Windows supplies a separate window station, desktop, focus, cursor, and input queue. That is categorically stronger non-interruption than posting messages into a background HWND.

It is not a drop-in upgrade to `shared-window`: setup changes RDP service, firewall, default-credential delegation, and child-session policy; it needs administrator approval, a reboot, a supported Windows edition, and a separate application/browser instance; only one child session is supported; and the same account/files mean it is not a security sandbox. ParaDesk is also PolyForm Noncommercial source-available, not OSI open source. Local Browser Bridge therefore keeps `isolated-child-session` as a future explicit backend rather than relabeling its current exact-window transport or silently making system policy changes.

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

Visual Confused Deputy further shows why the verifier must not be only the same model that selected the action: target appearance and the stated reason for acting can fail independently. AgentCIBench adds a separate privacy boundary: an agent can correctly operate every window and still violate contextual integrity by carrying nearby or cross-application personal data into the wrong recipient context. Local Browser Bridge's exact-window images, safe-mode approvals, and deterministic authority gates reduce accidental capture and unauthorized dispatch; they are not an independent intent classifier, data-flow policy, or recipient-aware disclosure monitor. Those remain measured security deltas rather than being hidden under a generic “safe” label.

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
- Apple's [`desktopIndependentWindow` filter](https://developer.apple.com/documentation/screencapturekit/sccontentfilter/init(desktopindependentwindow:)) supplies exact-window capture, while [`showsCursor`](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/showscursor) controls cursor inclusion. Neither API creates another input seat or attributes shared-pointer motion.
- Apple documents [`SCStream.updateConfiguration`](https://developer.apple.com/documentation/screencapturekit/scstream/updateconfiguration%28_%3Acompletionhandler%3A%29) as the uninterrupted way to change a running stream. The macOS path uses it for size or display-scale changes while preserving the share ID, and accepts the new geometry epoch only after a strictly newer WindowServer display time and the configured pixel dimensions both match. Position-only moves advance the same authority boundary without recreating the stream.
- Native callbacks keep only the newest accepted exact-window frame; the helper composites its pointer and emits bounded `computer.share.frame` PNG events with monotonic sequence metadata.
- The server keeps the latest sanitized computer observation and screenshot. It does not queue an unbounded video history.
- Pointer state is owned by the helper session and remains window-specific. A move to another window reseeds it rather than implying a global desktop location.
- All model-facing pointer/element coordinates use delivered image pixels. OS screen bounds remain separately labeled diagnostic data.
- The global cursor sample is diagnostic. Sealed route provenance and a healthy platform activity boundary decide whether the helper's global-pointer preservation is confirmed, unknown, or violated; shared activity is reported separately.
- The v0.12.20 release-only deliberate-concurrency design used a separate
  runner-owned pointer prompt and sustained HID advances. That design is
  historical; its physical-pointer components now provide only optional
  adversarial coverage and cannot satisfy v0.12.31 publication.
- Version 0.12.22 replaced that dependency with a nonactivating acceptance app
  and one exact app-share button. Version 0.12.23 retained the surface while
  separating sealed action-pointer evidence from keyboard-aware independent
  system evidence. Version 0.12.24 added a strictly newer
  same-share/same-target/same-geometry frame after the app-share `ACTION` receipt
  and within the reserved deadline before deriving click authority; version
  0.12.31 retains that boundary. The
  create-once request/start/complete chain is orchestration evidence, not a
  notification-only signal and not product authority. Product authority and
  effect proof remain the authenticated bridge request, sealed exact-target
  route, and target-owned postcondition.

## Exact-window backend boundary

The macOS and Windows backends retain the version 0.8 refusal contract:

1. An observation binds one `(pid, native window id)` and captured geometry.
2. The helper re-enumerates that identity immediately before input.
3. Semantic AX/UIA is preferred where the platform exposes a reliable action and postcondition.
4. Pixel input is target-routed to the exact window, never posted as global HID.
5. The action separately records sealed route provenance, any operating-system API acceptance signal, and an application-owned postcondition.
6. The platform-specific foreground/window-focus, shared-pointer-attribution, and active-desktop boundaries are checked around delivery.
7. Unknown or unsupported delivery fails; it never silently switches to global/foreground input. The macOS machine contract explicitly discloses the transient target `AXFrontmost` focus lease used by supported focus-capable routes.

macOS one-shot observation uses the snapshot backend, while live sharing uses a persistent desktop-independent ScreenCaptureKit exact-window stream. The pinned [`screencapturekit` 8.0.1 bridge](https://github.com/doom-fish/screencapturekit-rs/blob/2a9f13bcbeadb0aabc5596f0ff3d2ba71da8c1d0/swift-bridge/Sources/CoreMedia/CoreMedia.swift#L26-L38) casts Apple's numeric frame-status attachment directly to a Swift enum, whereas [Apple's canonical sample](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos) decodes the raw integer first. The helper therefore reads the raw CFNumber as a compatibility fallback and accepts only `Complete`, retaining a fail-closed status gate. That dependency also guards the base `updateConfiguration` method at macOS 14 even though Apple exposes it from 12.3; the repository vendors the exact 8.0.1 source with a documented one-line availability correction at the product's macOS 13 floor.

The packaged persistent-share probe exposed a receiver distinction that raw window cardinality misses. On the tested macOS build, starting `SCStream` added a 66-by-20 capture indicator inside the selected title bar. WindowServer reported that surface as same-PID, on-screen, shareable, layer 0, and Accessibility mapped it as a non-minimized `AXWindow`/`AXDialog`. From the retained probe evidence, we infer that neither raw sibling count nor the AX-intersection approach in [Cua `df57e610`](https://github.com/trycua/cua/commit/df57e610) can distinguish that surface from a real application window on this build. Title, size, subrole, and location are also compositor heuristics rather than keyboard authority.

The implementation now separates eligibility, inactive sibling selection, and receiver authorization. An independent `CGWindowListOptionAll` inventory must contain one exact owner/layer/geometry row and explicitly report the selected window on screen; required `AXChildren` and `AXWindows` reads must map that ID to a non-minimized top-level element. Sibling cardinality never authorizes input. The packaged two-window probes also found that an application-level `AXFocusedWindow` write is unavailable/no-op on this build and that writing window `AXFocused=false` is not a valid reverse operation. Window [`AXMain`](https://developer.apple.com/documentation/applicationservices/kaxmainattribute) is the useful public selector, but Apple does not define “main” as sufficient proof of key focus. The helper therefore retains the exact prior and requested AX windows, requires both `AXMain` attributes settable before any mutation, admits only a baseline where target `AXMainWindow == AXFocusedWindow`, performs the requested `AXMain` selection while the user is still fully active, and bounded-polls exact main+focused read-back with the target still inactive. After private process-focus preparation and a minimum settle, the app's [`AXFocusedWindow`](https://developer.apple.com/documentation/applicationservices/kaxfocusedwindowattribute), `AXMainWindow`, and `AXFrontmost` must describe the exact requested active target. Text additionally resolves [`AXFocusedUIElement`](https://developer.apple.com/documentation/applicationservices/kaxfocuseduielementattribute) through `_AXUIElementGetWindow`, `AXWindow`, `AXTopLevelUIElement`, or bounded parent ancestry and repeats that proof after event construction immediately before every scalar key-down. A receiver loss after any earlier scalar is outcome-unknown and is never safe to auto-retry.

Different-process preparation is a dual-state lease. It captures the user's front ProcessSerialNumber/PID and exact main/focused AX window plus the target app's prior exact main/focused AX window, and requires that the second user-window sample still equal the original snapshot rather than silently adopting a newly focused sibling. It resolves each target window through `CGSMainConnectionID → SLSGetWindowOwner → SLSGetConnectionPSN` and refuses the old PID-only fallback, so requested and prior IDs must bind the same exact target connection. Packaged probes exposed the private-focus state transitions on the tested build: after inactive AXMain selection, posting the saved-user `Defocus` record leaves WindowServer's front PSN/PID unchanged but changes that app's `AXFrontmost` from true to false; posting target `Focus` then makes the exact requested target `AXFrontmost=true` while the saved user remains WindowServer-front. Preparation records require the exact current target phase, target PSN/raw destination, and a final front-PSN → exact user AX window/expected-frontmost → front-PSN sample both before and after dispatch accounting. Restore cleanup runs after outcome accounting has already begun; each cleanup record instead repeats the exact target/raw/user authorization once immediately before its write. Every following phase is bounded-polled rather than inferred from the SkyLight return code. An unexpected active target is never cleaned up when the invocation proves that target Focus was not posted.

Restoration first polls requested-active → requested-inactive. A genuine AppKit field editor can keep the requested window as the native receiver even after a normal prior Focus record, so AXMain alone cannot reliably reverse a post-text key-window split. The retained live probe established a bounded cleanup: Focus the exact prior; accept only exact active requested-or-prior; if requested remains active, send the target-only paired make-key records for prior; prove target `AXMainWindow == AXFocusedWindow == prior` and `AXFrontmost=true`; then Defocus prior, prove prior inactive, and finally return the user's exact front window to `AXFrontmost=true`. The record bytes match the make-key subprimitive in [Cua `b296ec9`](https://github.com/trycua/cua/blob/b296ec9cbf460f360cf8ea2e203c26bcc92614b0/libs/cua-driver/rust/crates/platform-macos/src/input/skylight.rs#L618-L659) and [yabai `dd84572`](https://github.com/koekeishiya/yabai/blob/dd845723416f5fe92af49fad5ebab00369e07edd/src/window_manager.c#L1269-L1322), but the authorization contract is this project's own: unlike yabai's full focus operation and Cua's foreground `make_exact_window_key`, Local Browser Bridge never calls `_SLPSSetFrontProcessWithOptions`, never performs `AXRaise`, and uses the pair only during exact target cleanup under the unchanged-user lease. This mechanism and its non-interruption behavior are empirical findings on the retained tested build, not a public Apple guarantee.

The fixed restore deadline is divided into earlier target-work and user-Focus-authorization cutoffs, a target-independent user-restoration poll, and a retained safe-retry slice. Cleanup failure dominates the original action result. If target state becomes unknown, emergency user compensation intentionally performs no target read or write: it can post saved-user Focus only after proving the unchanged front PSN/PID, exact saved window with `AXFrontmost=false`, raw user restorability, and the same deadline. Target uncertainty still makes the command outcome unknown, but it cannot consume the time reserved to restore and prove the exact user owner. A same-app foreground sibling mismatch refuses before any focus-capable pixel or keyboard side effect. Before each key-down or new focus-capable pointer mutation, the released user owner and exact requested-active target receiver are re-proved; target receiver proof is the final AX read before keyboard dispatch. Long drag repeats the owner proof before each drag event, while an already-posted down event still receives unconditional release. The unrelated foreground app is inspected through read-only AX access; the existing Chromium manual-Accessibility opt-in is limited to the selected target. Per-call timeouts, collection caps, ancestry limits, and whole-proof deadlines fail closed. Synchronous `CGWindowListCopyWindowInfo` cannot be interrupted while in flight, so its result is checked against the same absolute deadline after return and a late inventory authorizes no record. These sandwiches narrow rather than eliminate the sub-sample TOCTOU because no public atomic API binds the final AX proof to the following private event post. The current selector still requires an on-screen, non-minimized active-Space target, and macOS changes can break the private symbols. ScreenCaptureKit's capture reach must not be generalized into cross-Space input.

The general macOS before/after oracle compares the frontmost process and front window. Focus-preparing routes add an exact Accessibility-window lease and refuse restoration if the user changed even to another window of the same foreground process. Windows additionally compares the foreground and focused HWNDs. These checks cannot make focus proof and event posting atomic, prove that no shorter transient change occurred, or roll back an event the target already processed.

Windows live sharing uses a persistent exact-HWND WGC session, while input uses UIA and background window messages. A successfully queued window message does not prove the application acted. The pointer boundary uses one process-owned message-only Raw Input sink as its primary activity epoch and a minimal dedicated-thread `WH_MOUSE_LL` callback only to count generic and injected-flag epochs. Microsoft explicitly recommends Raw Input for asynchronous monitoring and documents that a timed-out low-level hook can be silently removed without notification. `pointerActivityMonitorHealthy` therefore proves initialization and a readable sampled epoch, not continuous hook coverage. Minimized, elevated, game, secure-input, protected, and custom-rendered surfaces may reject either backend. Neither platform silently falls back to foreground/global input.

## Pointer attribution and action proof

The exact v0.12.9 packaged macOS attempt exposed why two global coordinates are not an ownership proof. Its first semantic `computer.setValue` used Accessibility, 40 precondition assertions had passed, and the retained result then observed a cursor-position delta. The run correctly failed closed, but its evidence could not identify whether the helper, a person, virtual input, a remote session, the target application, or another process moved the shared cursor. The candidate was withdrawn; Windows and stock-Chrome acceptance were not started. The exact record is preserved in the [withdrawn attempt](../evidence/v0.12.9/computer/attempts/withdrawn-db624da-macos-semantic-hardware-cursor-change/README.md). The successor v0.12.10 run proved 69 earlier assertions but then received no separately authorized movement during its bounded handoff, so it stopped before the final action; that exact [negative record](../evidence/v0.12.10/computer/attempts/withdrawn-de59840-macos-deliberate-pointer-timeout/README.md) is preserved separately. Version 0.12.11 fixed that handoff but was [withdrawn before execution](../evidence/v0.12.11/computer/attempts/withdrawn-414dd7f-macos-dual-lane-receipt-gap/README.md) when review found that one receipt digest could not authenticate the two fresh, non-mergeable macOS lanes required by policy. Version 0.12.12 then produced three evidence-only negative attempts, all before product dispatch: one arm-deadline/probe-budget race and two pre-dispatch contamination outcomes. Version 0.12.20 passed its quiet lane but its mandatory physical-pointer lane timed out before product dispatch, demonstrating that an unrelated physical gesture is a brittle release dependency rather than proof of app-scoped concurrency.

Version 0.12.22 kept the same fail-closed action boundaries while moving the deliberate orchestration proof onto one exact non-product app-share surface and giving the Windows sentinel one stable title. Version 0.12.23 retained both surfaces and separated the sealed action-pointer classifier from the keyboard-aware independent-system classifier.

The exact v0.12.22 quiet run exposed a separate contract error: the semantic
action was Confirmed and its sealed pointer record was safe, but the harness
sent it through a classifier that additionally required independent
keyboard-monitor fields. It failed closed after 55 of 56 checks, before
deliberate macOS, Windows, or Chrome. Version 0.12.23 gave action and
independent system samples distinct schema-matched classifiers while retaining
the same fail-closed unknown and contamination outcomes.

The v0.12.23 design separates three claims:

1. **Exact-target sealed route.** Runtime `inputDelivery` records the route, exact target binding, dispatch attempt, support level, and whether shared-seat, global-HID, or cursor-mutation primitives were requested. Source contracts and the frozen packaged-helper audit independently reject known global pointer/HID APIs. This proves the helper path selected for the action, not operating-system delivery or target effect.
2. **API acceptance.** AX/UIA return values and Windows message-queue success stop at their documented boundary. The private macOS `SLEventPostToPid` function returns `void`, so the bridge records an attempt but no receipt. Neither case confirms what the application did.
3. **Target postcondition.** Only a fresh application-owned value, masked length, toggle/selection state, or other allowlisted read-back can make an action `Confirmed`.

The exact v0.12.23 packaged candidate proved that separation in its quiet lane,
which passed 208/208 checks. Its deliberate lane then accepted the exact
app-share start receipt and completed 89/89 recorded assertions, but retained
the last pre-handoff stream frame while the external action took 43.807 seconds.
The subsequent `computer.click` correctly returned HTTP 409
`COMPUTER_STALE_FRAME` before dispatch. No completion receipt, Windows,
stock-Chrome, publication, or Release followed. The exact ten-file result is
preserved on branch
[`evidence/v0.12.23-macos-app-share-stale-frame-32746618027`](https://github.com/flrngel/local-browser-bridge/tree/4e4db75a4ede915d982d139a82dacac8a6c4772a/evidence/v0.12.23/computer/attempts/withdrawn-9e50811-macos-app-share-stale-frame),
not relabelled as v0.12.24, v0.12.25, or v0.12.26 evidence. Version 0.12.24 added the
bounded successor-frame wait; version 0.12.26 retained the classifier separation,
and version 0.12.31 carries it forward unchanged while waiting within the
reserved handoff deadline for a strictly newer streamed
frame with the same share ID, exact target, and unchanged window/image geometry
before deriving the product click from that successor.

Apple exposes [`CGEventSourceStateID::Private`](https://developer.apple.com/documentation/coregraphics/cgeventsourcestateid) as a documented event-source state. The helper uses that state for generated target-routed events instead of borrowing `HIDSystemState`; this does not make the separate private SkyLight delivery primitive public or supported. [`counterForEventType`](https://developer.apple.com/documentation/coregraphics/cgeventsource/counterforeventtype(_:eventtype:)) can report that the HID-system source advanced across a boundary. That is activity evidence, not a device identity: physical mice, virtual HID, remote control, and other system routing can contribute. Apple also documents that [`CGWarpMouseCursorPosition`](https://developer.apple.com/documentation/coregraphics/cgwarpmousecursorposition(_:)) changes position without generating a mouse event, so a coordinate delta with no counter advance remains unknown rather than being assigned to the helper or user. Event fields such as [`eventSourceUnixProcessID`](https://developer.apple.com/documentation/coregraphics/cgeventfield/eventsourceunixprocessid) and [`eventSourceUserData`](https://developer.apple.com/documentation/coregraphics/cgeventfield/eventsourceuserdata), and a [`listenOnly` event tap](https://developer.apple.com/documentation/coregraphics/cgeventtapoptions/listenonly), can describe observed events but do not retroactively identify an eventless warp or establish physical-device provenance.

The result vocabulary follows that boundary. `cursorPositionUnchanged` remains diagnostic. `hidSystemPointerActivityObserved` and `pointerActivityMonitorHealthy` describe the macOS sampling interval. `sharedPointerBoundaryCorroborated` plus `sharedPointerBoundaryState` say whether the boundary is usable. `helperGlobalPointerPreservation` is `confirmed`, `unknown`, or `violated` from both the sealed route and that boundary. `sharedPointerActivityState` is `quiet`, `contaminated`, or `unknown`. A user moving the mouse during a sealed semantic action can therefore yield `contaminated` while helper preservation remains `confirmed`; a reset, implausible counter jump, uncorroborated delta, or unsealed route yields `unknown` and fails closed. HID-system activity is never labelled physical input.

Pinned open-source implementations support the separation rather than a single cursor-equality rule. DSH audits its native source and packaged binary for expected input APIs ([audit](https://github.com/Anionex/dsh-computer-use/blob/387eae931b1852e3c3433e0e004fa460d3da2883/scripts/check-native.mjs#L55-L82)) and creates a private macOS event source ([implementation](https://github.com/Anionex/dsh-computer-use/blob/387eae931b1852e3c3433e0e004fa460d3da2883/native/macos/Sources/Helper/TargetedPointer.swift#L86-L94)). Cua keeps semantic and pointer routes separate ([module](https://github.com/trycua/cua/blob/737dc2a069528abadee67526d138a907e1c52061/libs/cua-driver/rust/crates/platform-macos/src/input/mod.rs)), while its reviewed foreground mouse path deliberately warps and restores the global cursor ([implementation](https://github.com/trycua/cua/blob/737dc2a069528abadee67526d138a907e1c52061/libs/cua-driver/rust/crates/platform-macos/src/input/mouse.rs#L35-L158)); that is a different authority contract. BackgroundComputerUse's semantic setter performs AX write plus target read-back ([implementation](https://github.com/actuallyepic/background-computer-use/blob/52116acfe0f2f57174f5e0166881abe944cb6eeb/Sources/BackgroundComputerUse/Actions/SetValue/SetValueRouteService.swift#L227-L259)). [Karabiner-Elements development notes](https://github.com/pqrs-org/Karabiner-Elements/blob/main/DEVELOPMENT.md) document practical source-attribution gaps around virtual HID, reinforcing that process/source metadata should not be relabelled as a physical-user identity.

The evaluation literature points to the same operational split. [PUSV](https://arxiv.org/abs/2604.18860) requires fresh action-time state, [InterruptBench](https://arxiv.org/abs/2604.00892) measures interference explicitly, and [DeskCraft](https://arxiv.org/abs/2606.03103) emphasizes controlled desktop state and verification. [ParaGUIBench](https://arxiv.org/abs/2607.22689) obtains genuine parallelism with separate desktop instances; it does not claim that one shared pointer becomes independent. A community description of Cua as [“multi-cursor”](https://www.reddit.com/r/AgentsOfAI/comments/1sxs1hp/cua_driver_the_new_macos_driver_that_lets_any/) is useful discovery language for parallel app actuation, not an operating-system guarantee of multiple hardware cursors. Community reports remain failure-mode signals, never authority for API behavior.

## Isolation boundary

The bounded live feed satisfies a viewing requirement: the model and a person can observe one background window without capturing unrelated desktop windows. Target-routed input satisfies a cooperative non-interruption requirement on supported applications. The person and helper still share the login session, focus machinery, pointer, and application state. Neither satisfies independent concurrency or a hostile-workload isolation requirement.

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
- each native action distinguishes a sealed exact-target route from operating-system API acceptance and an application-owned postcondition;
- `cursorPositionUnchanged` remains diagnostic, while the advertised helper-global-pointer, shared-pointer-boundary, monitor-health, activity, foreground/focus, and Space conclusions are recorded truthfully;
- Windows compiles and runs the shared contracts, while representative Windows UIA/background runtime coverage remains explicitly identified if it is not executed on a real Windows host.

The exact v0.12.8 packaged macOS candidate passed 187/187 assertions and
produced six reviewed screenshots. Its same-candidate Windows run published a
fresh foreground-arm request but received no click and no received marker; it
timed out at `wait-foreground-arm` before the invariant baseline or any product
action. Stock-Chrome acceptance never started, the publication job was
canceled, and no v0.12.8 Release exists. Version 0.12.9 then started a fresh
cycle and was also withdrawn: its one exact packaged macOS run failed closed at
the first semantic action after observing a cursor-position delta that the old
record could not attribute. Windows and stock-Chrome acceptance were not
started, and no v0.12.9 Release exists. Version 0.12.10 then passed 69 exact
packaged macOS assertions but timed out after 300 seconds with no separately
authorized movement, stopped before the final action, and was withdrawn before
Windows or stock-Chrome acceptance.

Version 0.12.13 then completed 192 of 193 quiet-lane assertions before the
unchanged whole-run oracle detected unrelated shared-seat `mouseMoved`/cursor
activity. The candidate was withdrawn without running deliberate macOS,
Windows, or stock-Chrome acceptance; the failure was recorded as contamination,
not attributed to the helper.

Version 0.12.26 attempt 1 bound and ran only the macOS quiet lane. The lane
failed closed during `computer.typeText` after the independent monitor observed
shared-seat HID pointer activity; the contamination was not attributed to the
helper. Attempt 2 rebuilt the candidate but stopped before execution because a
byte-identical extension returned one valid attestation per workflow attempt
and the verifier incorrectly required every returned statement to name the
current attempt. Both attempts were preserved and canceled; Windows, stock
Chrome, and publication never started. Version 0.12.27 tightened exact-attempt
attestation selection and atomic `dist/` replacement, passed both packaged
macOS lanes, and then exposed an external Windows-launcher durability gap. The
repository runner process exited before creating evidence, while its streams,
exit fact, and process-start telemetry were not retained. Candidate-byte
execution could neither be proved nor excluded because the runner probes both
EXEs first, so the version was withdrawn without a UI action, Chrome run, or
Release. Version 0.12.28 then checkpointed provisional checked-in coordinator
source with flush-before-move create-once state, an owner-private allowlisted
worker environment, kill-on-close process-tree containment, persistent separate
stream files, and exact worker/runner PID-plus-start-time binding; it remained a
blocked source checkpoint and never became a candidate. Version 0.12.29
completes the Windows-native coordinator source gate. The stable admission mutex
precedes recovery; one monotonic deadline spans exact prior-Job termination,
zero-active observation, namespace disappearance, and one final
`CreateJobObject` call. A non-null handle is accepted only with last error zero;
every nonzero result, including `ERROR_ALREADY_EXISTS`, is closed and rejected
without adopting or retrying it. The Job is configured kill-on-close and the
worker bound before Worker or Intent state is published. Its eight GUID-scoped
native SelfTest scenarios passed under exact 64-bit system Windows PowerShell
5.1, and independent review found no P0/P1 issue. The read-only watcher still
starts only after the atomic foreground-arm request marker exists; repeated
`Follow` remains notification-only with `uiActionAllowed: false`. No packaged
0.12.29 candidate was built, downloaded, or executed, and no macOS, Windows
candidate, stock-Chrome, tag, or Release gate occurred. Product, protocol, and
platform behavior are unchanged.

Version 0.12.30 retained that Windows source milestone and changed the release
orchestration boundary. Candidate construction is tagless and creates no GitHub
environment deployment. Its schema-3 binding identifies the reviewed `main`
source, exact workflow run and attempt, five-file artifact set, manifest, and
attestations before any live acceptance begins. A separate publication workflow
verifies the candidate and complete platform/browser receipt before entering
the protected `release` environment; only that protected job may create the
annotated tag, publish the exact assets, make the Release immutable, and verify
the public bytes. This removes speculative candidates from the approval
environment without weakening packaged acceptance. Its first candidate passed
both macOS lanes, then failed closed before the Windows runner launched because
the staged worker-support loader referenced an undefined hex helper; stock
Chrome and publication did not run. Version 0.12.31 fixes that production-loader
dependency and adds a fresh-process regression self-test. It has no packaged
candidate or public Release proof yet.

Version 0.12.31 requires two fresh, sequential, non-mergeable runs of one exact
packaged macOS candidate. Before candidate execution in either lane, the source-bound
native SystemProbe requires a 30-second sampled quiet epoch with at least 60
stable 500 ms transitions. Pointer, foreground/focus/front-window, cursor, or
active-Space activity resets all progress under one immutable 30-minute deadline;
unknown monitoring is immediately fatal. This preflight reduces ambient
contamination but does not replace any later invariant. The quiet lane must
remain quiet for every evidence cell. Only after it passes may the
`deliberate-concurrency` lane run. That lane launches one nonactivating
acceptance app with an exact bundle identifier, stable window title, and unique
accessibility button. A separately authorized exact-app-share controller presses
that button once and then stops. The app verifies the runner request digest,
disables the button, writes a create-once start receipt, remains present across
the real bounded product action, and writes a completion receipt only after the
runner proves the target postcondition plus quiet product and independent
boundaries. After the start receipt and before click dispatch, the runner must
obtain a strictly newer streamed frame from the same share, exact target, and
unchanged geometry within the reserved deadline. Foreground, focus, Space,
cursor, and cumulative HID pointer/keyboard
counters must match at the required endpoints; ambiguity is fatal. The receipt
chain records an ordered app-share orchestration sequence, not a
notification-only signal, physical-human provenance, cryptographic controller identity, a
separate OS seat, or product-control authority. Its result uses
`acceptanceButtonActionObserved`,
`appShareSurfaceObservedAtProductBoundaries`, `sharedHidInputObserved`, and
`sampledSharedContextUnchanged`; the completion marker uses
`handoffStateSequenceBound`. Quiet records `sharedHidInputObserved: null` because
there is no app-share transaction, while deliberate records `false` for no HID
boundary activity during its transaction. These endpoint samples and cumulative
counters cannot prove zero transient programmatic changes, a continuous monitor,
atomic provider identity, or zero transient focus/window manipulation. A
create-once aggregate binds both byte-distinct result files, all twelve screenshots, the
three app-share markers, the clean source-bound harness, and the exact workflow
artifact. Neither lane may be retried, merged, or substituted. The same
candidate must also complete fresh interactive-Windows, stock-Chrome,
evidence-commit, and immutable-release gates. A source contract, API return, or
single screenshot cannot substitute for those version-specific packaged
results.

Transport success alone is diagnostic evidence. A platform/action combination is supported only after its exact-target route is sealed, any API acceptance is labelled only as such, a representative application-owned outcome is observed when confirmation is claimed, and the advertised non-interruption and pointer-attribution boundaries hold.
