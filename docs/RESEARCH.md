# Research findings and design rationale

Research date: 2026-08-17

## Product research

OpenAI's public documentation describes the Codex Chrome extension as operating on the user's existing Chrome profile, signed-in sessions, open tabs, and extension state. Its documented features include reading and manipulating pages, adding open tabs and selected text to chat context, retrieving timestamped YouTube transcripts, site-level allow/block controls, and confirmations for sensitive actions. The built-in browser uses a separate profile and is suited to localhost previews.

- [OpenAI: Chrome extension](https://learn.chatgpt.com/docs/chrome-extension)
- [OpenAI: Browser](https://learn.chatgpt.com/docs/browser)
- [OpenAI: Computer Use](https://learn.chatgpt.com/docs/computer-use)

Microsoft's public documentation states that the M365 Copilot Cowork local browser runs in a hidden Edge tab on the user's device and uses existing SSO, cookies, and sessions. It is currently supported in Cowork web with Edge, and sensitive changes require approval cards. This project's localhost UI can therefore work with Cowork's local browser, but it cannot work with a Microsoft-managed hosted browser that cannot reach the user's loopback interface.

- [Microsoft: Use the local browser with Copilot Cowork](https://learn.microsoft.com/en-us/microsoft-365/copilot/cowork/cowork-local-browser)
- [Microsoft: Configure where computer use runs](https://learn.microsoft.com/en-us/microsoft-copilot-studio/configure-where-computer-use-runs)

## Extension platform evidence

Manifest V3 extension service workers can remain active through WebSocket keepalive traffic sent at intervals shorter than 30 seconds in Chrome 116 and newer. This supports an architecture in which the extension opens an outbound WebSocket to `127.0.0.1`, avoiding both a native messaging host and an exposed Chrome debugging port.

- [Chrome: Use WebSockets in service workers](https://developer.chrome.com/docs/extensions/how-to/web-platform/websockets)
- [Chrome: Extension service worker lifecycle](https://developer.chrome.com/docs/extensions/develop/concepts/service-workers/lifecycle)

Chrome's security guidance recommends minimum privileges, an explicit CSP, distrust of content scripts, validated messages and input, and keeping sensitive Chrome API operations in the service worker. This implementation limits the isolated content script to DOM snapshots and reference resolution while keeping mode policy, WebSocket transport, tab operations, and debugger operations in the service worker. Full Access is an explicit high-risk mode that prioritizes compatibility over those minimum-privilege recommendations; Safe mode remains available.

- [Chrome: Stay secure](https://developer.chrome.com/docs/extensions/develop/security-privacy/stay-secure)
- [Chrome: Protect user privacy](https://developer.chrome.com/docs/extensions/develop/security-privacy/user-privacy)

`captureVisibleTab` is limited to roughly two calls per second, so screenshot capture is throttled to one call every 550 milliseconds per window.

- [Chrome: chrome.tabs / captureVisibleTab](https://developer.chrome.com/docs/extensions/reference/api/tabs#method-captureVisibleTab)

## Research literature

WebArena demonstrates that realistic multi-step web tasks are substantially harder than synthetic tasks; its initial GPT-4 baseline achieved a 14.41% end-to-end success rate. BrowserGym formalizes the separation of browser-agent observation and action spaces and supports high-level action primitives as a more controllable alternative to arbitrary code. Safe mode preserves a compact action vocabulary, while Full Access adds coordinate, arbitrary-key, and JavaScript escape hatches for compatibility.

- [WebArena: A Realistic Web Environment for Building Autonomous Agents](https://arxiv.org/abs/2307.13854)
- [The BrowserGym Ecosystem for Web Agent Research](https://arxiv.org/abs/2412.05467)
- [OSWorld: Benchmarking Multimodal Agents for Open-Ended Tasks in Real Computer Environments](https://arxiv.org/abs/2404.07972)

The bridge provides screenshots and structured DOM/text observations together rather than relying on only one representation. Pixels and structured representations have different failure modes, and the agent must re-observe the page to verify the actual outcome.

## Public implementations and community signals

Public browser-bridge implementations and community discussions repeatedly converge on a dominant pattern: local server, Manifest V3 extension, outbound WebSocket, loopback binding, and token authentication. This preserves the real signed-in browser profile without requiring an open debugging port or native-host installation. This project adopts that transport pattern and adds a localhost UI that browser-only agents can operate without an MCP client. Since version 0.3.0, the server and UI ship as a single Rust binary that runs on Windows without Node.js.

- [vitalysim/browser-bridge](https://github.com/vitalysim/browser-bridge) — MIT-licensed local server, Manifest V3 extension, and outbound WebSocket
- [koltyakov/browser-bridge](https://github.com/koltyakov/browser-bridge) — explicitly enabled browser scopes and structured browser access
- [Community discussion: Browser Bridge](https://www.reddit.com/r/ClaudeAI/comments/1v5fz09/browser_bridge_an_mcp_server_that_drives_your/) — demand for real-session bridges, per-site controls, and risky-action confirmations

No code was copied from those projects. This repository was implemented independently using public architecture patterns and official platform APIs.

## Scope decisions

| Researched capability | This implementation | Decision |
|---|---:|---|
| Real signed-in browser profile | Yes | Core purpose |
| Tab listing, switching, and navigation | Yes | Basic action space for browser-only agents |
| Screenshot plus DOM/text | Yes | Complementary observation channels |
| Selected text | Yes | Narrow contextual data transfer |
| Site allow/block | Full Access: all; Safe mode: allowlist | Function-first default with a reversible safe mode |
| Sensitive-action confirmation | Full Access: immediate; Safe mode: popup approval | User-selected policy |
| YouTube transcript specialization | No | Product-specific feature beyond general DOM text |
| ChatGPT chat/sidebar synchronization | No | The bridge is model- and chat-provider independent |
| Network, cookie, or localStorage extraction | No | Avoid unnecessary credential-extraction surfaces |
| Arbitrary JavaScript execution | Full Access only | Compatibility escape hatch explicitly marked high risk |
| Desktop application control | No | Requires a separate native accessibility trust boundary |
