# Local Browser Bridge

Local Browser Bridge lets browser-only AI agents read and control real Chrome or Edge tabs through a `localhost` control surface. A local browser agent such as Microsoft 365 Copilot Cowork opens `http://127.0.0.1:17373`, while the target Chromium extension connects to the same server through an outbound WebSocket.

This project does not copy any private OpenAI protocol. It independently implements publicly documented product behavior and established browser-extension patterns. See [docs/RESEARCH.md](docs/RESEARCH.md) for the research sources and feature comparison.

## Features

- List and switch tabs in a real, signed-in Chromium profile
- Navigate URLs, go back or forward, reload, create tabs, and close tabs
- Capture the current viewport, rendered text, selected text, and structured interactive-element references
- Click, fill, and select by element reference
- Click viewport coordinates, type into the focused control, and send arbitrary key chords
- Execute arbitrary JavaScript, including Promises, in the current page's main world
- **Full Access mode by default:** control all HTTP(S) sites, sensitive fields, risky clicks, and tab closing without approval
- Optional **Safe mode:** site allowlist, sensitive-field blocking, and popup approval for risky actions
- Control `file://` pages when Chrome's file-URL permission is enabled
- Redact URL query strings and fragments from returned tab metadata
- Loopback-only server, WebSocket token and extension-Origin validation, SameSite UI sessions, CSRF validation, CSP, and no CORS
- Accessible agent-facing DOM, activity log, and live SSE state updates
- Bearer-token REST command API and a Rust mock extension for testing
- Standalone Rust binary with the entire control UI embedded; Node.js is not required

## Architecture

```mermaid
flowchart LR
  A["M365 Copilot Cowork<br/>local Edge browser"] -->|"opens localhost UI"| B["Local control surface<br/>127.0.0.1:17373"]
  B --> C["Standalone Rust binary<br/>embedded UI + HTTP/SSE/WebSocket"]
  D["Target Chrome/Edge extension<br/>Manifest V3"] -->|"outbound token-auth WebSocket"| C
  C -->|"browser commands"| D
  D --> E["Browser tabs<br/>existing login/session"]
  E -->|"screenshot + DOM refs"| D
  F["Human"] -->|"toggle Full Access / Safe mode"| D
```

The agent browser must run on the same computer, as Microsoft local browser does. Microsoft-hosted browsers and cloud browser tasks cannot reach the user's `127.0.0.1`, so they cannot use this bridge.

## Install and run

End users do not need Node.js or Rust. They only need the compiled executable and Chrome 116+, Edge 116+, or a compatible Chromium browser.

### Windows executable

Run the versioned deployment artifact:

```powershell
.\local-browser-bridge-v0.4.0-windows-x86_64.exe
```

The executable embeds all UI assets, so no web files or runtime installation are required. On Windows, the generated token is stored at `%USERPROFILE%\.local-browser-bridge\token` by default.

### Build from source

Developer builds require Rust 1.85 or newer.

```bash
cargo build --release
./target/release/local-browser-bridge
```

The server prints:

```text
Control surface: http://127.0.0.1:17373
Extension token: <random token>
```

Load the extension in the target browser:

1. Open `chrome://extensions` in Chrome or `edge://extensions` in Edge.
2. Enable **Developer mode** and select **Load unpacked**.
3. Select the extracted extension ZIP directory or this repository's `extension/` directory.
4. Open the extension popup and save the token printed by the server with port `17373`.
5. Confirm that **Full Access mode** is enabled. It is ON by default.
6. To control `file://` pages, also enable **Allow access to file URLs** in the extension details page.

For M365 Copilot Cowork running with the local Edge browser, use a prompt such as:

```text
Open http://127.0.0.1:17373 in the browser. Follow the Browser Bridge
instructions to observe the target tab and complete [the requested task].
Observe again after each action to verify the result. Use Direct browser
control for coordinate clicks, arbitrary key input, or JavaScript when needed.
```

Full Access executes sensitive actions immediately. Turning Full Access off restores Safe mode, where risky actions wait for **Approve once** or **Reject** in the extension popup. Pending approvals expire after two minutes.

## Local demo

Use two terminals to test the embedded UI and protocol without installing the real extension:

```bash
LBB_TOKEN=demo-token cargo run --release
```

```bash
LBB_TOKEN=demo-token cargo run --release --bin mock-extension
```

Open `http://127.0.0.1:17373` and select **Observe target**. The mock tab and element references will appear. The `/demo` route also contains a local form for testing the real extension.

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `LBB_PORT` | `17373` | Loopback HTTP/WebSocket port |
| `LBB_TOKEN` | Generated automatically | Explicit bridge token |
| `LBB_TOKEN_PATH` | `~/.local-browser-bridge/token` | Generated-token storage path |

The server always binds to `127.0.0.1` and rejects non-loopback Host headers.

## REST command API

Local clients can issue the same commands without using the browser UI:

```bash
curl http://127.0.0.1:17373/api/v1/command \
  -H "Authorization: Bearer $LBB_TOKEN" \
  -H "Content-Type: application/json" \
  --data '{"method":"tabs.list","params":{}}'
```

See [docs/PROTOCOL.md](docs/PROTOCOL.md) for the supported commands and WebSocket envelopes.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --all-targets
```

The test suite covers version alignment, token persistence, command validation, CSRF and Origin enforcement, WebSocket Origin enforcement, command relay, observation storage, screenshot serving, and Full Access command parameters.

## Deployment contract

In this repository, a user request to `deploy` means generating and verifying all of the following in `dist/`:

- A Node.js-free Windows x86_64 `.exe`
- A matching-version Chromium extension ZIP
- `SHA256SUMS.txt` for both artifacts

Run `scripts/deploy.sh` directly on Windows. On macOS or Linux, it requires `cargo-xwin` or a MinGW cross-compiler. `.github/workflows/deploy.yml` produces the same Windows artifacts. A completed deployment must return direct paths to every artifact.

## Limitations

- This is a browser computer-use bridge. It does not include a native accessibility helper for other macOS or Windows desktop applications.
- Chromium does not allow control of `chrome://`, `edge://`, extension pages, or browser permission UI through this extension.
- File-page control requires the user to enable Chrome's file-URL permission for the extension.
- Element references expire when the page structure changes. Observe again before reusing them.
- Reference clicks use trusted Chrome DevTools Protocol input when possible and detach immediately. If DevTools is already attached, the extension falls back to a synthetic click and returns `trusted: false`.
- Full Access can enter passwords, payment data, and OTPs and can execute page JavaScript. Treat it as remote-control authority over the signed-in browser profile.
- Unpacked extensions are intended for development or personal installation. Organization-wide deployment requires signing, policy review, and privacy disclosures.
- Windows artifacts are not automatically code-signed, so SmartScreen may display a warning.

See [SECURITY.md](SECURITY.md) for the security model and trust boundaries.
