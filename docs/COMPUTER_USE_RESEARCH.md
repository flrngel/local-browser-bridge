# Computer-use research and architecture decision

Research snapshot: 2026-08-18

## Question and method

The product already lets a browser-only agent reach a real Chromium profile through a loopback web page and an outbound extension connection. The question was how to add native macOS and Windows control without turning the bridge into an opaque model runtime or an arbitrary remote-execution service.

The comparison used four evidence classes:

1. Current repository activity and adoption, collected from GitHub on the research date.
2. Representative implementation code, read at the pinned revisions below.
3. Primary research papers and benchmark reports.
4. Community reports, used only as qualitative evidence about setup cost and operational friction.

Star counts are a volatile popularity signal, not a quality score. “Active shortlist” below means recently maintained and/or widely adopted, not a claim about a specific GitHub Trending page position.

## Active open-source shortlist

| Project | Snapshot | Primary focus | Useful lesson for this bridge |
|---|---:|---|---|
| [UI-TARS Desktop](https://github.com/bytedance/UI-TARS-desktop) | 38.6k stars | End-to-end screenshot-model action loop in Electron | Keep normalized image coordinates and native input behind a narrow operator seam; do not embed its large model/runtime stack here |
| [Cua](https://github.com/trycua/cua) | 21.5k stars | Cross-platform computer-use infrastructure and the Rust Cua Driver | A long-lived permission-owning process, capability/version handshake, serialized physical input, and explicit frame geometry are the strongest production patterns |
| [Microsoft UFO](https://github.com/microsoft/UFO) | 9.5k stars | Windows application automation using UI Automation and agent planning | A future semantic layer should prefer UIA on Windows and retain pixel/Win32 fallbacks |
| [Microsoft OmniParser](https://github.com/microsoft/OmniParser) | 25.3k stars | Turning screenshots into grounded UI regions and labels | Screen parsing can improve grounding later, but it is a vision dependency rather than the native control boundary |
| [Agent S](https://github.com/simular-ai/Agent-S) | 12.2k stars | Hierarchical planning, visual grounding, OCR, and code-agent routing | Planning and OCR belong with the calling agent unless there is measured evidence that local preprocessing is needed |
| [OpenClaw](https://github.com/openclaw/openclaw) | 386.7k stars | Broad local agent runtime with a current Cua Driver integration | Bind coordinate actions to a specific delivered frame and provider generation; keep native resource identifiers opaque |
| [OpenKosmos](https://github.com/microsoft/open-kosmos) | 35 stars | Recent Electron computer-use implementation | Useful low-adoption architecture comparator: default-off control, pure coordinate mapping, structured failures, visible AI cursor, and real-cursor restoration |
| [OSWorld](https://github.com/xlang-ai/OSWorld) | 3.1k stars | Reproducible desktop-agent evaluation | Desktop success must be judged across multi-step real applications, not by whether a click API works once |

Counts were rounded from GitHub metadata captured on the research date. The code-reading revisions were UI-TARS `c2ad42e3eb9b27830db41a3e6f51ca7179d9b168`, Cua `9045b0c74f7c7de72fde3d9dc622f2cacf1cf848`, UFO `96983c73ed09e884a5f1d7ff8936c953b234b684`, Agent S `bffdb59c60cbbb38c3a190b2e91da12039e4063c`, and OpenKosmos `9e45cc51bae33d65e412824ab5915606f99ae038`.

## What the implementations optimize

### UI-TARS Desktop: a complete visual agent

The [Electron operator](https://github.com/bytedance/UI-TARS-desktop/blob/c2ad42e3eb9b27830db41a3e6f51ca7179d9b168/apps/ui-tars/src/main/agent/operator.ts) captures the primary display through `desktopCapturer`, resizes it for the model, JPEG-encodes it, and dispatches parsed actions through a NutJS operator. Its Windows text path uses clipboard paste and restores the old clipboard. The repository therefore optimizes for an integrated model-to-action application, normalized coordinates, and practical cross-platform fallbacks. It also brings Electron, Node.js, native modules, and a model-serving stack that conflict with this project’s small Node-free distribution.

### Agent S and OmniParser: grounding quality

Agent S’s [grounding implementation](https://github.com/simular-ai/Agent-S/blob/bffdb59c60cbbb38c3a190b2e91da12039e4063c/gui_agents/s3/agents/grounding.py) combines a visual grounding model, Tesseract OCR, text alignment, and a code-agent route. [OmniParser](https://arxiv.org/abs/2408.00203) focuses even more directly on parsing screenshots into interactable regions and meaningful icon descriptions. These projects address the hardest model-side question—“where is the requested thing?”—but neither is the minimal native authority needed to capture a screen and deliver bounded input.

The calling browser agent already supplies reasoning and vision. Bundling another planner, OCR service, or model would duplicate that layer, increase installation size, create GPU/runtime expectations, and make updates substantially harder. Local parsing remains a future measured optimization rather than a v0.6 dependency.

### UFO: semantic Windows control

UFO’s [control implementation](https://github.com/microsoft/UFO/tree/96983c73ed09e884a5f1d7ff8936c953b234b684/ufo/automator/ui_control) is Windows-first. It uses pywinauto/UI Automation for structured controls, Win32 APIs, and pyautogui-style fallbacks. Its screenshot code explicitly falls through pywinauto, `PrintWindow`, and desktop capture, including behavior for disconnected RDP sessions. The important lesson is that accessibility trees materially improve target selection and application semantics on Windows, but a correct implementation needs native per-platform work and a larger interactive test matrix.

### Cua Driver and OpenClaw: the native capability boundary

Cua’s [driver documentation](https://github.com/trycua/cua/blob/9045b0c74f7c7de72fde3d9dc622f2cacf1cf848/libs/cua-driver/README.md) describes in-process, worker, and daemon modes. The Rust workspace separates platform capture/input/accessibility backends from transport and testing. The design details most relevant here are:

- macOS Screen Recording and Accessibility grants attach to a stable application identity; a raw binary path is not a production permission owner;
- Windows control must run in the interactive user session rather than Session 0;
- the machine has one physical input stream, so input must be serialized;
- the client and provider exchange explicit version/capability metadata and should fail closed on incompatible capabilities;
- observation combines screenshots and accessibility information, while native APIs remain platform-specific;
- local transports still require authentication and browser-Origin defense.

OpenClaw’s [computer-use documentation](https://github.com/openclaw/openclaw/blob/main/docs/nodes/computer-use.md) and Cua integration add a particularly strong invariant: coordinate actions carry a `displayFrameId`, and the receiver rejects a frame if the provider generation or live display geometry no longer matches. Commands are one action per call and physical input is queued. That prevents a delayed action from clicking the same pixels after a monitor, resolution, scale, or frame change.

### OpenKosmos: understandable human UX

OpenKosmos’s [technical design](https://github.com/microsoft/open-kosmos/blob/9e45cc51bae33d65e412824ab5915606f99ae038/docs/computer-use-tech-doc.md) emphasizes default-off authority, OS permission checks, per-app allowlisting, structured recoverable results, deterministic multi-display mapping, a visible AI cursor, and restoration of the user’s real cursor after input. Its adoption is currently much smaller than the other projects, so it is not used as dominant evidence. It is useful confirmation that visible action feedback and non-hijacking cursor behavior are important UX improvements for a later native UI layer.

## Research evidence about difficulty

- [OSWorld](https://arxiv.org/abs/2404.07972) introduced 369 real desktop tasks. Its original evaluation reported 72.36% human success versus 12.24% for the best evaluated model, with GUI grounding and operational knowledge as major failures.
- [UI-TARS](https://arxiv.org/abs/2501.12326) argues for native screenshot-based agents and a unified action representation across platforms.
- [Agent S2](https://arxiv.org/abs/2504.00906) attributes gains to better grounding plus hierarchical planning, reinforcing that the effector alone is not the agent.
- [OSWorld-Human](https://arxiv.org/abs/2506.16042) reports that model planning and reflection dominate latency and that later task steps become much slower. A local helper should therefore keep capture/action overhead predictable rather than add a second planning loop.

A [LocalLLaMA community discussion](https://www.reddit.com/r/LocalLLaMA/comments/1kwzrh4/) praised UI-TARS’s open computer-use quality but reported very high local VRAM use for one deployment. This is anecdotal and hardware/configuration dependent; it supports only the narrower conclusion that bundling a visual model would create meaningful setup friction.

## Decision for Local Browser Bridge v0.6

The right v0.6 boundary is a thin, separately launched native capability provider:

```text
browser-only agent
      |
      v
loopback control page/API --- Rust server --- Chromium extension ---> browser tabs
                                  |
                                  +--- authenticated outbound helper ---> screen + mouse/keyboard
```

The helper is implemented in Rust with [xcap](https://github.com/nashaofu/xcap) for macOS/Windows capture and [Enigo](https://github.com/enigo-rs/enigo) for native input. It does not embed a model, OCR engine, shell, filesystem interface, process launcher, clipboard API, downloader, or telemetry client.

Implemented invariants:

- The helper owns no listening socket. It makes an outbound WebSocket connection to the existing loopback server.
- The handshake requires the shared random token and exact private Origin `lbb-computer-helper://local`.
- The helper advertises a versioned, fixed allowlist: status, observe, move, click, drag, scroll, type text, and key chord.
- The server intersects advertised capabilities with its own allowlist and rejects every other `computer.*` method.
- All browser and computer actions share one server-side action lock, preventing competing physical input streams.
- Captures are resized to at most one million pixels before lossless PNG/base64 transport so they remain within the bounded WebSocket message size.
- Every input command must reference the most recently delivered frame. Immediately before input, the helper re-enumerates the display and rejects changed identity, origin, dimensions, scale, or rotation.
- Image-space coordinates are mapped to the captured display’s live screen space, including negative multi-monitor origins.
- After every input action, the server requests and exposes a fresh observation for verification.
- macOS releases put the helper in a stable `.app` bundle that owns Screen Recording and Accessibility grants. Until Developer ID signing/notarization is added, an OS update or binary change can still require permission to be granted again.
- Windows releases provide a separate console executable that runs in the signed-in user’s interactive session.

## Local capture benchmark

The release-mode helper was measured on the development Mac after a warm build with:

```bash
cargo run --locked --release --bin local-computer-helper -- --benchmark
```

Environment: macOS 26.5.1, arm64, one selected 2560×1440 display at scale factor 2.0. Each of five iterations performed native capture, resize to 1333×750 (the one-million-pixel transport ceiling), PNG encoding, and base64 generation. It did not include WebSocket transport, model inference, or an action.

| Metric | Result |
|---|---:|
| Minimum | 71.2 ms |
| Median | 77.7 ms |
| Mean | 87.6 ms |
| Maximum | 129.9 ms |
| Median encoded data URL | 447,314 bytes |

This is a small local engineering sample, not a cross-project leaderboard. The compared agent repositories include different models, prompts, capture formats, resolutions, and planning loops, so an end-to-end latency ranking would be misleading. The result establishes that this helper’s local observation path is sub-100-ms at the median on the measured machine and that its payload stays well below the server’s 8 MB message limit.

## Deliberately deferred

Native accessibility trees and window/application targeting are the highest-value next layer. Cua and UFO show that semantic UI elements reduce dependence on pixel grounding, but implementing AX on macOS and UIA on Windows deserves its own typed protocol, privacy review, and real interactive test matrix. OCR/model bundling, arbitrary code execution, background autostart, silent updating, and hidden persistence are not part of the plan.

A future visible agent cursor and real-cursor restoration should also be evaluated. They improve human legibility, but cursor restoration changes hover/focus behavior and must be tested against drag, menu, and multi-display interactions rather than copied mechanically.

No implementation code was copied from the researched projects. Their source was used to compare architecture, invariants, and tradeoffs; this project uses its own protocol and implementation.
