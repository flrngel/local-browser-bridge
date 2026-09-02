# Local Browser Bridge

A local, loopback-only server that gives an AI agent authenticated control of
a real Chrome or Edge tab, and optionally one native desktop application
window, on the same computer. There is **no bundled agent, no MCP server, and
no cloud relay** — this repository is the bridge and connectors only, and the
AI client must be able to reach `127.0.0.1` on this machine.

[Install on Windows](docs/INSTALL_WINDOWS.md) ·
[Install on macOS](docs/INSTALL_MACOS.md) ·
[Agent integration](docs/AGENT_INTEGRATION.md) ·
[Latest release](https://github.com/flrngel/local-browser-bridge/releases/latest)

## Install in one command

> These commands install the Desktop Host, local shell, and Agent Fetch,
> which ship starting with release 0.12.69. If the latest published release
> is older: on macOS the installer fails before copying any files; on
> Windows it installs the older release and reports success anyway, with no
> tray icon. See [Troubleshooting](docs/TROUBLESHOOTING.md#macos-installer-fails-with-an-unexpected-layout-or-unknown-argument-error)
> / [Troubleshooting](docs/TROUBLESHOOTING.md#windows-installer-succeeds-but-no-tray-icon-appears-or-enableshell-fails-at-launch).

macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash
```

Windows PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-windows.ps1')))
```

Each downloads and verifies one immutable stable GitHub Release, installs
without administrator access, starts the server at login, and opens its
authenticated dashboard. Edge is documented but not exercised by this
project's own tests, which target Chrome. Linux is not packaged; build from
[source](docs/BUILD.md) instead.

## Load the extension and make your first call

Chrome and Edge do not let a downloaded program silently install an
extension. The installer opens the extensions page and copies the folder path
for you: enable **Developer mode**, select **Load unpacked**, paste the path,
then paste the copied token into the extension popup and select **Save and
connect**.

```bash
curl http://127.0.0.1:17373/health
# {"computerConnected":false,"extensionConnected":false,"ok":true,"shellEnabled":false,"version":"0.12.68"}
```

`extensionConnected` flips to `true` once the extension connects. Then copy
the **Agent Fetch base URL** from the dashboard, set it as a variable, and
make your first call:

```bash
export AGENT_FETCH_BASE_URL="paste-it-here"
curl "$AGENT_FETCH_BASE_URL/tabs.list"
```

Full walkthrough — bearer POST, `callId`, error handling — in
[Agent integration](docs/AGENT_INTEGRATION.md).

## What it can do

| Surface | What it gives an agent | Enable |
|---|---|---|
| **Browser control** | List/open tabs, read text/screenshots/elements, click/fill/select/scroll/type/navigate/run JS, in an existing signed-in Chrome/Edge profile | Load the extension (above) |
| **Computer use** | Observe and operate one selected native app window: semantic accessibility actions plus bounded pixel/key input | Off by default; **Start Computer Helper** in the tray/menu |
| **Shell** | Run a native shell command, bounded output/time, on the server host | Off by default; `--enable-shell` or `LBB_ENABLE_SHELL=1` |

Details, limits, troubleshooting: [Browser control](docs/BROWSER_CONTROL.md) ·
[Computer use](docs/COMPUTER_USE.md) · [Shell](docs/SHELL.md).

## Is it a good fit?

Good fit: the AI client runs on this same Windows or macOS computer, you want
to keep using an existing signed-in browser profile, and you want visible,
human-revocable control — Chrome's own "Local Browser Bridge started
debugging this browser" warning stays up, the controlled tab sits in a named
Local Browser Bridge tab group, and a person can always hit **Cancel** on
that warning or **Release control** in the extension popup — instead of
hidden automation.

Not a fit: the AI client only runs in a remote/cloud browser that cannot
reach `127.0.0.1`, you need a bundled agent or hosted relay, or your
deployment requires publisher-signed Windows binaries or a notarized macOS
package today (see [Security](SECURITY.md)). Full support matrix:
[Capabilities](docs/CAPABILITIES.md) and [Limitations](docs/LIMITATIONS.md).

## Safety model, in brief

- Full Access is the default browser mode and removes most action-level
  safety interlocks; use Safe mode (allowlist, sensitive-field blocking,
  click approvals) in the extension popup for anything sensitive.
- The bearer token and Agent Fetch URL are credentials with full command
  authority — never share, log, or paste them into an untrusted page.
- Shell access, when enabled, is full current-user command execution, not a
  sandbox.
- The computer helper shares your login session; on macOS, focus-capable
  input can briefly borrow and restore app focus.
- A human can always revoke browser control — Chrome's own debugging
  warning and its **Cancel** button, or **Release control** in the extension
  popup — or stop the helper, and every failed command carries a
  machine-readable error taxonomy so an agent knows whether to retry,
  re-observe, or hand back.

Full model: [Security](SECURITY.md); deeper implementation invariants:
[docs/internals/security-invariants.md](docs/internals/security-invariants.md).

## Uninstall in one command

Windows PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-windows.ps1')))
```

macOS Terminal:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-macos.sh | bash
```

Stops only product-folder processes, removes installer-owned files and the
startup entry, and never edits browser profile files. Dry-run and
credential-retention options: [Windows](docs/INSTALL_WINDOWS.md#one-command-uninstall) ·
[macOS](docs/INSTALL_MACOS.md#one-command-uninstall).

## Connect an agent that only implements Agent Skills

```bash
npx skills add flrngel/local-browser-bridge --skill local-browser-bridge -g
```

Or without Node.js, from a checked-out copy of this repository:

```bash
bash scripts/install-agent-skill.sh --target agents
```

For agents that do not support skills, [Agent integration](docs/AGENT_INTEGRATION.md)
is the complete portable contract, and
[docs/internals/PROTOCOL.md](docs/internals/PROTOCOL.md) is the underlying
wire specification.

## Documentation

| Goal | Document |
|---|---|
| Integrate an agent | [Agent integration](docs/AGENT_INTEGRATION.md) · [API reference](docs/API_REFERENCE.md) |
| Understand one surface | [Browser control](docs/BROWSER_CONTROL.md) · [Computer use](docs/COMPUTER_USE.md) · [Shell](docs/SHELL.md) |
| Install / verify / configure | [Windows](docs/INSTALL_WINDOWS.md) · [macOS](docs/INSTALL_MACOS.md) · [Verify a release](docs/VERIFY_RELEASE.md) · [Configuration](docs/CONFIGURATION.md) |
| Fix a problem | [Troubleshooting](docs/TROUBLESHOOTING.md) |
| Check what's supported | [Capabilities](docs/CAPABILITIES.md) · [Limitations](docs/LIMITATIONS.md) |
| Understand the trust model | [Security](SECURITY.md) |
| Understand the architecture | [Architecture](docs/ARCHITECTURE.md) |
| Build or contribute | [Build guide](docs/BUILD.md) · [Development](docs/maintainers/DEVELOPMENT.md) · [Release process](docs/maintainers/RELEASE.md) |
| Project history | [Release-attempt history](docs/history/release-attempts.md) · [Research](docs/research/) |

## License

See [LICENSE](LICENSE) for this project and
[THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt) for locked production
dependencies.
