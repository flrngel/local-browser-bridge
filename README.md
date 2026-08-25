# Local Browser Bridge

Local Browser Bridge lets a browser-based AI agent work with real Chrome or
Edge tabs on your computer. An optional helper can also observe and operate one
selected macOS or Windows application window.

The server and helper are standalone Rust executables. They communicate only
through `127.0.0.1`; there is no cloud relay, telemetry, silent installer, or
automatic installer. End users do not need Node.js, Rust, or a package manager.

## What it can do

- Use tabs in an existing signed-in Chrome or Edge profile
- Read page text, screenshots, selection, frames, and interactive elements
- Click, fill, select, hover, scroll, type, navigate, and run page JavaScript
- Keep Chrome's native debugging warning and a human-owned **Stop** control
  visible while trusted browser control is active
- Use Full Access for broad control or Safe mode for allowlists and approvals
- Optionally capture and operate one exact desktop application window through
  native macOS or Windows APIs

The optional computer helper does not expose shell, filesystem, clipboard,
download, process-launch, or telemetry commands.

## How it fits together

```text
local browser agent -> authenticated control page -> Rust server
                                                -> Chrome/Edge extension -> tabs
                                                -> optional helper -> one app window
```

The agent's browser must run on the same computer. A cloud-hosted browser
cannot reach your local `127.0.0.1` server.

## Supported systems

| Component | Supported system |
|---|---|
| Browser control | The Chrome or Edge version declared by the extension in the selected release |
| Windows helper | Windows 11 in the signed-in interactive user session |
| macOS package | macOS 13 or later; Screen Recording and Accessibility permissions are needed for full helper functionality |

Desktop control is cooperative and shares the signed-in session with the user.
It is not a separate virtual desktop or input seat. Read
[Capabilities](docs/CAPABILITIES.md) and [Limitations](docs/LIMITATIONS.md)
before using it with consequential applications.

On macOS, focus-capable input may briefly borrow and restore app focus through
an Accessibility focus lease while checked foreground boundaries remain
unchanged. This is a before-and-after guarantee, not proof of zero transient
focus changes.

## Install a stable release

Install only a version that appears on the public
[GitHub Releases page](https://github.com/flrngel/local-browser-bridge/releases/latest).
The source tree can be newer than the latest published release; files built
from current source are development builds, not published release artifacts.

Download one version-matched set:

- Windows: server `.exe`, optional helper `.exe`, and extension ZIP
- macOS: universal server/helper archive and extension ZIP
- Both platforms: `SHA256SUMS.txt`

Then:

1. Verify the downloaded assets.
2. Start the server and open the complete authenticated URL it prints.
3. Extract the extension ZIP, open `chrome://extensions` or
   `edge://extensions`, enable **Developer mode**, and choose **Load unpacked**.
4. Enter the server's token in the extension popup and reload existing target
   pages.
5. Start the optional helper only when desktop application control is needed.

Use the platform guide for exact commands, permissions, updates, and uninstall:

- [Windows installation](docs/INSTALL_WINDOWS.md)
- [macOS installation](docs/INSTALL_MACOS.md)
- [Installation and update overview](docs/INSTALL.md)

When browser control starts, Chrome shows **Local Browser Bridge started
debugging this browser** and the page shows **Local Browser Bridge is using
this tab**. Chrome's Cancel action, the page's **Stop** button, and **Release
control** in the extension popup remain available.

## Updates

The server performs a metadata-only check against the fixed public GitHub
Releases API. It never downloads or installs an update. Disable the startup
check with `--no-update-check` or `LBB_DISABLE_UPDATE_CHECK=1`.

The unpacked extension does not update automatically. Stop control and replace
the server, helper, and extension together with verified files from one release.
Do not mix versions.

## Build from source

Source builds require Rust 1.88 or later and the native platform toolchain.
Node.js is not a runtime dependency; Node.js 24 is needed only for the complete
developer test suite.

See [Building from source](docs/BUILD.md) for Windows and macOS setup, exact
commands, output paths, extension packaging, and test dependencies. See
[Development](docs/DEVELOPMENT.md) for protocol testing and release-evidence
workflows.

## Documentation

- [Installation](docs/INSTALL.md) — release selection, verification, updates, and removal
- [Building](docs/BUILD.md) — native Windows and macOS development builds
- [Capabilities](docs/CAPABILITIES.md) — feature and platform matrix
- [Architecture](docs/ARCHITECTURE.md) — processes, capture, input, and authority flow
- [Limitations](docs/LIMITATIONS.md) — current constraints and evidence gaps
- [Security](SECURITY.md) — trust boundaries and sensitive-data handling
- [Protocol](docs/PROTOCOL.md) — command and connector contracts
- [Research](docs/RESEARCH.md) and [computer-use research](docs/COMPUTER_USE_RESEARCH.md)
- [Release evidence](evidence/) — versioned positive and negative results

This project independently implements documented browser and operating-system
capabilities. It does not copy or claim access to an unpublished OpenAI,
Anthropic, Microsoft, or browser-vendor protocol.

## License

See [LICENSE](LICENSE) for this project and
[THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt) for the locked production
dependencies.
