# Local Browser Bridge

Use the Chrome or Edge session already open on your computer from a local AI
control surface. Add the optional computer helper when the agent also needs to
observe or operate one selected desktop application window.

[Download the latest verified release](https://github.com/flrngel/local-browser-bridge/releases/latest)
· [Install on Windows](docs/INSTALL_WINDOWS.md)
· [Install on macOS](docs/INSTALL_MACOS.md)
· [Build from source](docs/BUILD.md)

> **Know the boundary first:** this repository provides the local bridge,
> extension, helper, dashboard, and API. It does not include an AI agent, an MCP
> server, or a native ChatGPT, Claude, or Copilot connector. The AI client must
> be able to open the authenticated local dashboard or call the local REST API
> from the same computer. A cloud-only browser cannot reach `127.0.0.1`.

## Choose your setup

| What you want to control | What you install | Start here |
|---|---|---|
| Chrome or Edge tabs | Server + unpacked extension | [Windows](docs/INSTALL_WINDOWS.md) or [macOS](docs/INSTALL_MACOS.md) |
| Browser tabs and one desktop app window | Server + unpacked extension + optional helper | [Windows](docs/INSTALL_WINDOWS.md) or [macOS](docs/INSTALL_MACOS.md) |
| Current development source | Rust toolchain + native platform SDK | [Windows and macOS build guide](docs/BUILD.md) |

Published packages do not need Node.js, Rust, administrator access, or a
package manager. There is currently no installer or browser-store package, so
the extension is loaded once through `chrome://extensions` or
`edge://extensions`.

## Is it a good fit?

Local Browser Bridge is a good fit when:

- the AI client can reach a loopback URL on the same Windows or macOS computer;
- you want to keep using an existing signed-in Chrome or Edge profile;
- you want visible, human-revocable browser control instead of hidden control;
  and
- one exact native app window is enough when desktop control is enabled.

It is not a direct fit when:

- the AI client runs only in a remote cloud browser;
- you need a bundled agent, hosted relay, MCP façade, or browser-store install;
- you need a separate virtual desktop or a non-interrupting second input seat;
  or
- your deployment requires publisher-signed Windows binaries or a notarized
  macOS package today.

See [Capabilities](docs/CAPABILITIES.md) and
[Limitations](docs/LIMITATIONS.md) for the complete support matrix.

## What it can do

### Browser control

- List and open tabs in an existing Chrome or Edge profile
- Read page text, screenshots, selections, frames, and interactive elements
- Click, fill, select, hover, scroll, type, navigate, and run page JavaScript
- Keep Chrome's native debugging warning and an extension-owned in-page
  **Stop** control visible during an active control lease
- Use Full Access for broad control or Safe mode for URL allowlists and action
  approvals

### Optional desktop control

- Enumerate native windows and select one exact application window
- Capture only that window through native Windows or macOS APIs
- Use semantic accessibility actions when available and bounded pixel input
  when needed
- Keep the helper separate from the browser extension and server process

The helper does not offer shell, filesystem, clipboard, download,
process-launch, or telemetry commands. Desktop control still shares the
signed-in user's session: it is cooperative, not an isolated desktop.

## Install a published release

Use only a version shown on the public
[Releases page](https://github.com/flrngel/local-browser-bridge/releases/latest).
The `main` branch and workflow candidates can be newer than the latest public
release; they are development artifacts, not installable releases.

1. Download one version-matched set:
   - Windows: server `.exe`, optional helper `.exe`, extension ZIP, and
     `SHA256SUMS.txt`.
   - macOS: universal server/helper archive, extension ZIP, and
     `SHA256SUMS.txt`.
2. Verify the downloaded SHA-256 values before running anything.
3. Start the server and open the complete authenticated URL it prints.
4. Extract the extension ZIP. Open `chrome://extensions` or
   `edge://extensions`, enable **Developer mode**, select **Load unpacked**, and
   choose the extracted folder containing `manifest.json`.
5. Open the extension popup, confirm its version matches the server, enter the
   printed token, and reload any already-open target tab.
6. Start the optional helper only if you need desktop application control.

Use the platform guide for copy-and-paste commands, permissions, verification,
updates, troubleshooting, and removal:

- [Windows 11 installation](docs/INSTALL_WINDOWS.md)
- [macOS 13 or later installation](docs/INSTALL_MACOS.md)
- [Shared verification and update policy](docs/INSTALL.md)

Windows executables are not yet Microsoft publisher-signed. The macOS package
is ad-hoc signed but not Developer ID-signed or notarized. Keep SmartScreen and
Gatekeeper enabled, verify the release, and follow the platform-specific guide
instead of disabling operating-system protections globally.

## Try it safely

Use the built-in demo before opening a consequential site:

1. Start the server and extension, then open the authenticated dashboard URL.
2. Open `http://127.0.0.1:17373/demo` in the connected browser. If you changed
   `LBB_PORT`, use that port instead.
3. In the dashboard, refresh the tab list, select the demo tab, and start a
   short control lease.
4. Observe the demo page and try a harmless field fill, click, or scroll through
   your AI client or the documented local API.
5. End the lease with the controlled page's **Stop** button, Chrome's **Cancel**
   action, or **Release control** in the extension popup.

Full Access is the current default. For a first run, turn **Full Access mode**
off in the extension popup to use Safe mode, or use a dedicated browser profile.
Enable Full Access only for a client you trust with the selected tabs.

## What a healthy connection looks like

- The server prints its version, loopback address, token source, and a complete
  authenticated dashboard URL.
- The extension popup shows the same version and reports **Connected**.
- The dashboard lists the intended browser connector and tabs.
- Starting control shows **Local Browser Bridge is using this tab** on the
  controlled target page and Chrome's browser-owned debugging warning.
- If the helper is running, the dashboard reports **Computer connected**.

The authenticated URL and extension token are credentials. Do not place them
in screenshots, logs, issue reports, or untrusted pages.

## How it works

```text
AI client on this computer
        |
        v
authenticated local dashboard / REST API
        |
        v
Rust server on 127.0.0.1
        |---------------- Chromium extension ---> selected browser tabs
        |
        `---------------- optional helper ------> one selected app window
```

The server accepts loopback connections only. The helper also connects outbound
to the server on loopback and opens no listening socket. Browser and desktop
authority can be released independently.

## Supported systems

| Component | Supported system |
|---|---|
| Browser control | Chrome or Edge satisfying the selected release's declared minimum version |
| Windows helper | 64-bit Windows 11 in the signed-in interactive user session |
| macOS server and helper | macOS 13 or later; Screen Recording and Accessibility permissions are helper-only requirements |

On macOS, focus-capable input can briefly borrow and restore app focus through
an Accessibility focus lease. The project verifies foreground state before and
after the action; it does not claim that no transient focus change occurred.

## Updates and removal

The server's update check reads metadata only from the fixed public GitHub
Releases API. It never downloads or installs an update. Disable the startup
check with `--no-update-check` or `LBB_DISABLE_UPDATE_CHECK=1`.

The unpacked extension does not update automatically. Stop control, then
replace the server, helper, and extension together with verified files from one
release. Never mix component versions. The platform guides include safe update,
credential reset, and uninstall steps.

## Build from source

Development builds require Rust 1.88 or later plus the native Windows or macOS
toolchain. Node.js is not a runtime dependency; Node.js 24 is needed only for
the complete developer contract suite.

The [build guide](docs/BUILD.md) covers clean-machine setup, Windows MSVC and
SDK requirements, macOS universal packaging, extension packaging, output paths,
and test levels. [Development](docs/DEVELOPMENT.md) covers protocol testing and
release-evidence workflows.

## Documentation

| Goal | Document |
|---|---|
| Install, verify, update, or remove | [Installation overview](docs/INSTALL.md) |
| Install on Windows | [Windows guide](docs/INSTALL_WINDOWS.md) |
| Install on macOS | [macOS guide](docs/INSTALL_MACOS.md) |
| Build the server, helper, and extension | [Build guide](docs/BUILD.md) |
| Check features and platform support | [Capabilities](docs/CAPABILITIES.md) |
| Understand the process and authority model | [Architecture](docs/ARCHITECTURE.md) |
| Review known constraints and evidence gaps | [Limitations](docs/LIMITATIONS.md) |
| Review security boundaries | [Security policy](SECURITY.md) |
| Integrate with the local API | [Protocol](docs/PROTOCOL.md) |
| Review implementation research | [Browser research](docs/RESEARCH.md) and [computer-use research](docs/COMPUTER_USE_RESEARCH.md) |
| Review versioned live results | [Release evidence](evidence/) |
| Prepare the planned rebrand | [Brand and rebranding brief](brand.md) |

This project independently implements documented browser and operating-system
capabilities. It does not copy or claim access to an unpublished OpenAI,
Anthropic, Microsoft, or browser-vendor protocol.

## License

See [LICENSE](LICENSE) for this project and
[THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt) for locked production
dependencies.
