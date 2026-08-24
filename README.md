# Local Browser Bridge

Local Browser Bridge lets a browser-based AI agent work with real Chrome or Edge tabs on your computer. An optional companion helper can also view and operate one selected window from a supported macOS or Windows application without routing its actions through the global hardware pointer.

Everything runs locally through `127.0.0.1`. The server and computer helper are compiled Rust programs; end users do not need Node.js, Rust, or a package manager.

## What it can do

- Use tabs in an existing, signed-in Chrome or Edge profile, including creating a new tab directly at a policy-approved URL
- Read page text, screenshots, selection, and interactive elements
- Click, fill, select, hover, scroll, type, navigate, and run page JavaScript
- Work with supported cross-origin frames in Chrome or Edge
- Show Chrome's native debugging warning while trusted browser control is active
- Show an in-page status pill, synthetic pointer, and human-owned **Stop** button
- Use Full Access for broad control or Safe mode for allowlists and approvals
- Optionally observe and operate one exact desktop application window
- Share that window through native macOS or Windows capture with a requested 1–10 FPS cap
- Prefer macOS Accessibility or Windows UI Automation before pixel input
- Prove the exact target route, check the application's result when possible, and preserve the foreground and active desktop at supported action boundaries; macOS may briefly use and restore an Accessibility focus lease

The bridge has no cloud relay, telemetry, silent installer, or silent updater. It does not add shell, filesystem, clipboard, download, or process-launch tools to the computer helper.

## How it fits together

```text
browser-based agent -> localhost control page -> Rust server
                                               -> Chrome/Edge extension -> tabs
                                               -> optional helper -> one app window
```

The agent must use a browser running on the same computer. A cloud-hosted browser cannot reach your local `127.0.0.1` server.

Use the extension for Chrome and Edge web content. Use the optional helper for one already-open desktop window when that application's background route is supported. A separate Windows login/input seat is a different mode that requires another session or VM; it is not implied by exact-window sharing. The current helper is cooperative: it shares the person's login session, so unrelated pointer activity can occur while an action is running.

On macOS, the helper may briefly borrow and restore app focus while leaving the user's foreground window unchanged at checked boundaries. It reports shared pointer activity separately from proof about the helper's own sealed action route; see [Limitations](docs/LIMITATIONS.md) for details.

## Requirements

| Use | Requirement |
|---|---|
| Browser control, including cross-origin frames | Chrome or Edge 140+ |
| Windows desktop control | Windows 11 interactive user session |
| macOS package | macOS 13+; Screen Recording for desktop sharing and Accessibility for semantic/input features |

See [Capabilities](docs/CAPABILITIES.md) for the exact platform matrix and [Limitations](docs/LIMITATIONS.md) before relying on desktop background control.

## Install

Download one version-matched set from [GitHub Releases](https://github.com/flrngel/local-browser-bridge/releases/latest):

> Version 0.12.19 is the current source and release target. It is not published until its two macOS lanes, Windows, stock-Chrome, evidence-commit, and immutable-release gates all pass. Version 0.12.13 was withdrawn after its quiet macOS lane passed 192 of 193 assertions but the unchanged whole-run boundary detected unrelated shared-seat pointer activity; deliberate macOS, Windows, and stock-Chrome never started, publication was canceled, and it never became a Release. Version 0.12.15 was stopped before tagging when hosted PowerShell 7 coerced a canonical JSON timestamp into a `DateTime`; 0.12.16 fixed that path but stopped at the next source gate because modern .NET omits the legacy static `Directory.SetAccessControl` API. Version 0.12.17 selected the supported directory-ACL API, then stopped before tagging when PowerShell 7 rejected the legacy secure named-pipe constructor and an incomplete task-disposal error masked that primary failure. Version 0.12.18 passed its complete source matrix, but its protected-tag candidate was withdrawn before execution when an independent audit found the macOS aggregate producer emitted result schema 6 while the publication verifier still required schema 5; [workflow 32718436613](https://github.com/flrngel/local-browser-bridge/actions/runs/32718436613) was canceled and no Release exists. Version 0.12.19 preserves the prior runtime fixes and binds the macOS producer, aggregate, documentation, and publication verifier to the same lane-result schema. It also retains the 30-second native quiet-seat gate before candidate execution in both macOS lanes without weakening later action or whole-run checks. Its stock-Chrome gate uses one exact candidate-bound approval, autonomous digest-bound independent review, and a durable no-rerun ledger without requiring Node.js. Install only one version-matched set from an actual published Release.

The v0.12.13 negative record is retained only on branch `evidence/v0.12.13-macos-quiet-pointer-contamination-32695400912` at commit `bdcc3620e28260e31a3a78bf7e584adf1f0db44e`, under `evidence/v0.12.13/computer/attempts/withdrawn-7d2692d-macos-quiet-pointer-contamination/`; it is historical evidence, not v0.12.19 input.

- the server for your platform;
- `local-browser-bridge-extension-vVERSION.zip`; and
- the optional computer helper for desktop application control.

Checksums and GitHub build provenance are published with each release. The complete verification and permission flow is in the [installation guide](docs/INSTALL.md).

The extension ZIP includes the project `LICENSE`; the macOS archive includes both `LICENSE` and `THIRD_PARTY_LICENSES.txt`. Either executable prints the same embedded project and dependency notices with `--licenses`.

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
6. Reload any already-open tab you want to control so the extension's early Stop guard is present.

Use **Clear saved token** in the trusted popup when you want the extension to disconnect, discard any waiting approval, and forget that credential; pausing Bridge control alone keeps it saved.

When browser control starts, Chrome shows **Local Browser Bridge started debugging this browser**. The page also shows **Local Browser Bridge is using this tab**. If that page indicator disappears, the bridge ends control; Chrome's Cancel action and **Release control** in the popup remain available. See [Limitations](docs/LIMITATIONS.md) for the page-indicator boundary.

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

On macOS, relaunch the helper whenever the server stops or its connection is lost. Choosing **Stop share** only stops the current stream and leaves the helper open. On Windows, the helper's built-in supervisor restarts its disposable worker and reconnects after the server becomes available again.

## Use it with an agent

Tell the agent to open the complete authenticated control-page URL printed by the server, connect to the intended tab or app window, observe before acting, verify after each action, and stop control when finished.

Full Access is enabled by default and can act in signed-in sessions, sensitive fields, and consequential pages. Turn it off for Safe mode, or use a dedicated browser profile. Treat the optional computer helper like local remote-control software. Read the [security model](SECURITY.md) for the full trust boundary.

## Updates

At startup the server checks only the fixed public GitHub Releases metadata endpoint. It accepts only a canonical stable release that GitHub reports as immutable and that exposes the exact five uploaded, nonempty release assets with GitHub SHA-256 digests. It does not download or install anything. Disable the check with `--no-update-check` or `LBB_DISABLE_UPDATE_CHECK=1`.

Unpacked extensions do not update automatically. Replace the server, extension, and helper together with files from the same release; do not mix versions. To preserve one extension identity, release control, disable the existing extension at `chrome://extensions`, replace the contents of its existing unpacked folder with the verified new ZIP, then re-enable and reload that same card. If you use a new folder instead, remove the old extension card before **Load unpacked**. Confirm that exactly one Local Browser Bridge card remains, that its popup version matches the server, and reload each open target page before controlling it.

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

See [LICENSE](LICENSE) for this project and [THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt) for the exact locked production dependencies.
