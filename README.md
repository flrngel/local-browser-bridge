# Local Browser Bridge

Local Browser Bridge lets a browser-based AI agent work with real Chrome or Edge tabs on your computer. An optional companion helper also lets the agent view and operate one selected macOS or Windows application window without taking over your foreground pointer.

Everything runs locally through `127.0.0.1`. The server and computer helper are compiled Rust programs; end users do not need Node.js, Rust, or a package manager.

## What it can do

- Use tabs in an existing, signed-in Chrome or Edge profile
- Read page text, screenshots, selection, and interactive elements
- Click, fill, select, hover, scroll, type, navigate, and run page JavaScript
- Work with same-origin frames and, on Chromium 125 or later, cross-origin frames
- Show Chrome's native debugging warning while trusted browser control is active
- Show an in-page status pill, synthetic pointer, and human-owned **Stop** button
- Use Full Access for broad control or Safe mode for allowlists and approvals
- Optionally observe and operate one exact desktop application window
- Share that window through native macOS or Windows capture with a requested 1–10 FPS cap
- Prefer macOS Accessibility or Windows UI Automation before pixel input
- Preserve the user's foreground window and hardware pointer on supported desktop apps

The bridge has no cloud relay, telemetry, silent installer, or silent updater. It does not add shell, filesystem, clipboard, download, or process-launch tools to the computer helper.

## How it fits together

```text
browser-based agent -> localhost control page -> Rust server
                                               -> Chrome/Edge extension -> tabs
                                               -> optional helper -> one app window
```

The agent must use a browser running on the same computer. A cloud-hosted browser cannot reach your local `127.0.0.1` server.

Use the extension for Chrome and Edge web content. Use the optional helper for one already-open desktop window when that application's background route is supported. A separate Windows login/input seat is a different mode that requires an RDP child session or VM; it is not implied by exact-window sharing.

## Requirements

| Use | Requirement |
|---|---|
| Browser control | Chrome or Edge 118+ |
| Cross-origin frame control | Chrome or Edge 125+ |
| Windows desktop control | Windows 11 interactive user session |
| macOS package | macOS 13+; Screen Recording for desktop sharing and Accessibility for semantic/input features |

See [Capabilities](docs/CAPABILITIES.md) for the exact platform matrix and [Limitations](docs/LIMITATIONS.md) before relying on desktop background control.

## Install

Download one version-matched set from [GitHub Releases](https://github.com/flrngel/local-browser-bridge/releases/latest):

> Version 0.12.1 is the current source and release target. Release artifacts, evidence, and capability claims are versioned; install the server, extension, and helper from one matching release.

- the server for your platform;
- `local-browser-bridge-extension-vVERSION.zip`; and
- the optional computer helper for desktop application control.

Checksums and GitHub build provenance are published with each release. The complete verification and permission flow is in the [installation guide](docs/INSTALL.md).

### 1. Start the server

Windows:

```powershell
.\local-browser-bridge-vVERSION-windows-x86_64.exe
```

macOS:

```bash
tar -xzf local-browser-bridge-vVERSION-macos-universal.tar.gz
./local-browser-bridge
```

The server prints an authenticated control-page URL and an extension token. Open the complete URL, including its fragment, in the agent's local browser.

### 2. Load the extension

1. Extract the extension ZIP to a stable folder.
2. Open `chrome://extensions` or `edge://extensions`.
3. Enable **Developer mode**.
4. Select **Load unpacked** and choose the folder containing `manifest.json`.
5. Open the extension popup, enter the printed token, keep port `17373`, and connect.

When browser control starts, Chrome shows **Local Browser Bridge started debugging this browser**. The page also shows **Local Browser Bridge is using this tab**. Chrome's Cancel action, the page's **Stop** button, or **Release control** in the popup revokes authority.

### 3. Start desktop control only when needed

Windows, in a second PowerShell window:

```powershell
.\local-computer-helper-vVERSION-windows-x86_64.exe
```

macOS, from the extracted archive:

```bash
./Local\ Computer\ Helper.app/Contents/MacOS/local-computer-helper --request-permissions
./Local\ Computer\ Helper.app/Contents/MacOS/local-computer-helper
```

Choose a window in the control page before observing or sharing it. Live sharing uses ScreenCaptureKit on macOS and Windows Graphics Capture on Windows. Stop the helper when desktop authority is no longer needed.

## Use it with an agent

Tell the agent to open the complete authenticated control-page URL printed by the server, connect to the intended tab or app window, observe before acting, verify after each action, and stop control when finished.

Full Access is enabled by default and can act in signed-in sessions, sensitive fields, and consequential pages. Turn it off for Safe mode, or use a dedicated browser profile. Treat the optional computer helper like local remote-control software. Read the [security model](SECURITY.md) for the full trust boundary.

## Updates

At startup the server checks only the fixed public GitHub Releases metadata endpoint. It does not download or install anything. Disable the check with `--no-update-check` or `LBB_DISABLE_UPDATE_CHECK=1`.

Unpacked extensions do not update automatically. Replace the server, extension, and helper together with files from the same release. Do not mix versions.

## Build and test

Developer builds require Rust 1.88 or later:

```bash
cargo build --release
cargo test --locked --all-targets
```

See [Development](docs/DEVELOPMENT.md) for local testing, browser fixtures, release checks, and the repository's deployment contract.

## Documentation

- [Capabilities](docs/CAPABILITIES.md) — current feature and platform matrix
- [Architecture](docs/ARCHITECTURE.md) — processes, capture, input, and authority flow
- [Limitations](docs/LIMITATIONS.md) — current constraints and evidence gaps
- [Installation](docs/INSTALL.md) — downloads, checksums, permissions, and updates
- [Security](SECURITY.md) — trust boundaries and sensitive-data handling
- [Protocol](docs/PROTOCOL.md) — command and connector contracts
- [Browser research](docs/RESEARCH.md) and [computer-use research](docs/COMPUTER_USE_RESEARCH.md)
- [SOTA audit](docs/SOTA_AUDIT.md) and [release evidence](evidence/)

This project independently implements documented browser and operating-system capabilities. It does not copy or claim access to an unpublished OpenAI, Anthropic, Microsoft, or browser-vendor protocol.

## License

See [LICENSE](LICENSE).
