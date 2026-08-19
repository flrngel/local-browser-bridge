# Computer-use and browser-control audit

Audit date: 2026-08-18. Implementation version: 0.8.0.

This audit treats a delivered event as insufficient proof. A supported action must be scoped to the observed target, refuse stale state, and verify a platform-owned or application-owned postcondition whenever the platform exposes one.

## Dominant evidence reviewed

- [Cua Driver macOS internals](https://github.com/trycua/cua/blob/main/blog/inside-macos-window-internals.md) and [Windows internals](https://github.com/trycua/cua/blob/main/blog/inside-windows-computer-use.md): hybrid semantic and pixel routes, exact-window scoping, no implicit foreground fallback, accessibility-token snapshots, and post-action verification.
- [Cua platform support](https://cua.ai/docs/reference/cua-driver/platform-support): representative application-owned results define support; a successful API return alone does not.
- [agent-browser](https://github.com/vercel-labs/agent-browser): compact snapshots and refs, typed actions, policy boundaries, session state, traces, and recordings.
- [BrowserGym](https://github.com/ServiceNow/BrowserGym): combined screenshot, DOM, accessibility, and CDP observation for reproducible browser-agent benchmarks.
- [Nanobrowser](https://github.com/nanobrowser/nanobrowser): open-shadow-root traversal, iframe handling, replay/history, sanitization, and guardrails.
- [Browser Use issue 4471](https://github.com/browser-use/browser-use/issues/4471) and [issue 4579](https://github.com/browser-use/browser-use/issues/4579): an open CDP/WebSocket transport is not proof of a healthy command path; each operation needs a deadline and explicit connection failure.
- [OSWorld 2.0](https://arxiv.org/abs/2606.29537), [WindowsWorld](https://arxiv.org/abs/2604.27776), and [BrowserArena](https://arxiv.org/abs/2510.02418): long-horizon tasks fail on stale state, dynamic UI, cross-source dependencies, visual precision, and weak completion verification.
- [WASP](https://arxiv.org/abs/2504.18575), [Chrome's agent security guidance](https://developer.chrome.com/docs/agents/security), and [Google's agentic security architecture](https://blog.google/security/architecting-security-for-agentic/): rendered content is untrusted input; capability scope, target binding, and consequential-action policy remain separate concerns.
- [UFO2](https://arxiv.org/abs/2504.14603): an isolated virtual desktop with picture-in-picture is the strongest concurrent-user architecture when full isolation is required.

## Implementation matrix

| Requirement | v0.8.0 implementation | Verification |
| --- | --- | --- |
| Non-interrupting local desktop control | Exact background window capture; macOS SkyLight routing; Windows UIA first and exact-HWND messages for pixel actions; no global HID or implicit foreground fallback | macOS fixture kept foreground window, focus, hardware cursor, and Space unchanged for move, click, drag, scroll, text, and key |
| Semantic-first action route | macOS Accessibility and Windows UI Automation return frame-bound element refs with role, name, value, actions, enabled state, and bounds | Native text-field value read-back and button state-change proof |
| Exact target resolution | PID plus native window id; Chrome-hosted macOS Open/Save surfaces require one unique same-process AX container with matching WindowServer geometry | Real Chrome `chrome://extensions` Open panel resolved to 46 dialog-only elements and closed through `AXPress` |
| Stale-state refusal | Computer refs are bound to a UUID frame; browser refs and direct coordinate/text/key operations are bound to the observation generation | Replayed browser coordinate action returned `STALE_SNAPSHOT` |
| Postcondition verification | Semantic value read-back, masked-field length proof, target-window close, element disappearance/change, or whole-window semantic change; the server re-observes after browser actions | Safe native fixture plus real Chrome load/connect flow |
| Inactive-tab screenshots | CDP `Page.captureScreenshot`; observation never activates an inactive tab | Foreground remained `chrome://extensions` while the background demo tab was captured |
| Debugger health | Per-operation attach, command, and detach deadlines; detach always runs | Static contract tests and live command matrix |
| Composed DOM | Interactive elements inside open shadow roots are included and marked `tree: "shadow"` | Shadow action was observed and clicked with a trusted CDP event |
| Cross-platform semantic backend | macOS AX is live-tested; Windows UIA invokes Invoke, Toggle, SelectionItem, ExpandCollapse, and Value patterns | Windows UIA module passes a standalone `x86_64-pc-windows-msvc` compile; Windows runtime execution remains a release-runner responsibility |
| Reproducible evidence | Sanitized method results and screenshots are stored under `evidence/v0.8.0` | `cargo test --locked --all-targets`, Clippy, release builds, extension load/reload, and live REST matrices |

## Remaining boundaries

- This host-driver mode preserves the person's foreground session, but it is not a security boundary. For untrusted sites, credentials, or destructive workflows, use an isolated VM/desktop session. A PiP or screen-share viewer can display that session without sharing host input state.
- Open shadow roots are covered. Cross-origin iframe semantic merging is not yet implemented; pixel/CDP control remains available, and this gap must not be represented as semantic coverage.
- macOS background pixel delivery uses private SkyLight symbols and can require maintenance after an OS update. Unsupported delivery refuses instead of silently switching to foreground control.
- Windows UIA compiled for the release target but was not executed on this macOS host. GitHub's Windows release job must build and run the Rust contract suite; a representative Windows UIA fixture should be added before calling Windows runtime coverage complete.
- The browser extension deliberately grants broad HTTP, HTTPS, and file access in Full Access mode. Loopback authentication, origin checks, self-control blocking, stale refs, and debugger timeouts reduce accidental misuse; they do not make untrusted prompt content safe.

## Acceptance rule

Do not call a new platform/action combination supported until a live fixture proves the application-owned result and the foreground/cursor invariants. A transport-level success is diagnostic evidence only.
