# Research findings and design rationale

Research date: 2026-08-18

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

## Release and update evidence

GitHub artifact attestations use Sigstore to provide verifiable build provenance, while immutable releases lock published assets and their tag against later replacement. Version 0.5.0 therefore builds each platform in GitHub Actions, attests each release asset, publishes checksums, and enables release immutability. The update checker reports metadata only and leaves download and installation to the user.

- [GitHub: Artifact attestations](https://docs.github.com/en/actions/concepts/security/artifact-attestations)
- [GitHub: Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases)
- [GitHub: Verify release integrity](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/verify-release-integrity)

Microsoft documents that unsigned applications cannot inherit publisher reputation and can trigger SmartScreen on each version. Apple documents Developer ID signing and notarization as the normal outside-App-Store trust path. This release does not claim either identity without the required certificates; the install guide discloses the limitation and directs users to per-artifact verification without globally disabling OS protection.

- [Microsoft: SmartScreen reputation for app developers](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
- [Apple: Developer ID](https://developer.apple.com/support/developer-id/)
- [Apple: Safely open apps on Mac](https://support.apple.com/en-nz/102445)

Chrome recommends narrowly reviewed permissions and notes that Windows/macOS external extension installation and updates must use the Chrome Web Store. A Developer-mode unpacked ZIP therefore cannot honestly promise silent self-update. The extension has an exact permission/package contract test, contains no downloader or remote code, and relies on a visible matching-version notice in the local UI.

- [Chrome: Declare permissions](https://developer.chrome.com/docs/extensions/develop/concepts/declare-permissions)
- [Chrome: Manifest V3 and remote code](https://developer.chrome.com/docs/extensions/develop/migrate/what-is-mv3)
- [Chrome: Alternative installation methods](https://developer.chrome.com/docs/extensions/how-to/distribute/install-extensions)

The Update Framework research shows why transport security alone is not a complete software-update design and emphasizes authenticated metadata, version freshness, and compromise resilience. This small project does not claim a full TUF implementation; it deliberately avoids automatic installation, validates semantic versions and fixed repository links, limits metadata size and redirects, and publishes independent checksums plus GitHub provenance.

- [The Update Framework](https://theupdateframework.org/)
- [Survivable Key Compromise in Software Update Systems](https://theupdateframework.io/papers/survivable-key-compromise-ccs2010.pdf)
- [USENIX: Secure Software Updates: Not Really](https://www.usenix.org/conference/15th-usenix-security-symposium/secure-software-updates-not-really)

Community discussions from independent Windows developers repeatedly describe unknown-publisher warnings as a major installation trust barrier. These reports informed the first-run disclosure and verification UX, while the technical security decisions above rely on the platform vendors and research literature.

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
