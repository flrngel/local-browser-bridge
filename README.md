# Local Browser Bridge

## What is this?

A small app that lets an AI assistant running on **this same computer** see
and control a real Chrome or Edge tab — plus, if you turn them on, one app
window and your local shell. Nothing is sent to a cloud service by this
project; the AI you connect still needs its own model access, but the
control channel itself is local only.

## What do I need?

- Windows 10/11, or macOS.
- Chrome or Edge already installed.
- An AI assistant that runs **on this computer** (a desktop app or a local
  agent), not one that only runs in someone else's browser tab. See
  [Which assistants can use this](#which-assistants-can-use-this) below.

## Get it

### Windows

1. Go to the [latest release](https://github.com/flrngel/local-browser-bridge/releases/latest)
   page and download the file that ends in `.exe`.
2. Double-click it.
3. Windows will say **"Windows protected your PC"**. That warning shows up
   because this app is not yet signed by a big company — it does not mean
   the file is unsafe. Click **More info**, then **Run anyway**.
4. A dialog asks **"Set up Local Browser Bridge?"** — click **Yes**. No
   administrator password is needed.

A **Local Browser Bridge** icon appears in the system tray (bottom-right,
maybe under the **^** overflow arrow) — nothing opens on its own yet.
Right-click it; that menu is how you open the dashboard and finish setup,
next.

### macOS

One line in Terminal:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash
```

Prefer not to run a command? Download the archive from the
[latest release](https://github.com/flrngel/local-browser-bridge/releases/latest)
page instead, unzip it, and drag **Local Browser Bridge.app** into
**Applications**. Open it from there — a menu-bar icon appears and, same as
the Windows path above, nothing opens on its own; use its menu next.

## What do I do next?

The one-line installer commands above already opened the Chrome/Edge
extensions page (with a folder path copied for you) and your dashboard, in
two browser tabs — skip to step 1. Installed by double-clicking the `.exe`
or dragging the `.app` instead? Open your tray/menu-bar icon and click
**Finish Browser Extension Setup**, then **Open Dashboard**, to get the same
two tabs.

1. On the extensions page, turn on **Developer mode**.
2. Click **Load unpacked** and paste the copied path.
3. Switch back to the dashboard tab — it already has your credentials — and
   click **Connect**.

Reload any tabs that were already open. The dashboard now shows the browser
as connected — that page is your control center: it shows what is
connected, and has one-click switches for desktop control and shell access
(both are on by default; see below). If **Connect** doesn't finish the job,
the dashboard walks through the same **Load unpacked** steps again so you
can retry.

Point your AI assistant at the bridge next: open
[Agent integration](docs/AGENT_INTEGRATION.md), copy the block at the top,
and paste it into your assistant.

## Desktop control and shell access

Starting with this release, **desktop control** (letting the agent operate
one app window) and **shell access** (letting it run commands as you) are
both **on by default** — no extra setup step. Turn either off with one click:
open the dashboard page or the tray/menu-bar icon and use the switch next to
each. The change takes effect immediately, no restart needed.

Shell access is real command execution as your user account — see
[Shell](docs/SHELL.md) before you decide whether to leave it on.

## Which assistants can use this

Works with any assistant that runs a process **on this computer** and can
make a plain HTTP request to `127.0.0.1` — a local agent, a CLI tool, a
desktop app with its own model access.

Does **not** work with an assistant that only runs inside a browser tab on
someone else's server — for example ChatGPT or Copilot opened as a web page,
or "Microsoft 365 Copilot" in a browser. Those run on a remote machine that
cannot reach `127.0.0.1` on yours, no matter how the request is worded. If
that is what you have, this project is not a fit yet; wait for that
assistant to add local-agent support, or use one that already runs locally.

## Install in one command

Prefer a script (or need to automate an install)? Both platforms also have a
one-line installer.

> These commands install the Desktop Host, local shell, and Agent Fetch,
> which ship starting with release 0.12.70. If the latest published release
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

Full step-by-step for either platform:
[Windows](docs/INSTALL_WINDOWS.md) · [macOS](docs/INSTALL_MACOS.md).

## First API call

```bash
curl http://127.0.0.1:17373/health
# {"computerConnected":false,"extensionConnected":false,"ok":true,"shellEnabled":true,"version":"0.13.0"}
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
| Fix a problem | [Troubleshooting](docs/TROUBLESHOOTING.md) |
| Integrate an agent | [Agent integration](docs/AGENT_INTEGRATION.md) · [API reference](docs/API_REFERENCE.md) |
| Understand one surface | [Browser control](docs/BROWSER_CONTROL.md) · [Computer use](docs/COMPUTER_USE.md) · [Shell](docs/SHELL.md) |
| Install / verify / configure | [Windows](docs/INSTALL_WINDOWS.md) · [macOS](docs/INSTALL_MACOS.md) · [Verify a release](docs/VERIFY_RELEASE.md) · [Configuration](docs/CONFIGURATION.md) |
| Check what's supported | [Capabilities](docs/CAPABILITIES.md) · [Limitations](docs/LIMITATIONS.md) |
| Understand the trust model | [Security](SECURITY.md) |

## License

See [LICENSE](LICENSE) for this project and
[THIRD_PARTY_LICENSES.txt](THIRD_PARTY_LICENSES.txt) for locked production
dependencies.

---

## For developers

Everything below is for people building a connector, auditing the trust
model, or contributing to this repository — not needed to just use the app.

### What it can do

| Surface | What it gives an agent | Enable |
|---|---|---|
| **Browser control** | List/open tabs, read text/screenshots/elements, click/fill/select/scroll/type/navigate/run JS, in an existing signed-in Chrome/Edge profile | Load the extension (above) |
| **Computer use** | Observe and operate one selected native app window: semantic accessibility actions plus bounded pixel/key input | On by default; turn off in the dashboard or tray/menu |
| **Shell** | Run a native shell command, bounded output/time, on the server host | On by default; turn off in the dashboard or tray/menu, or `--no-shell` |

Details, limits, troubleshooting: [Browser control](docs/BROWSER_CONTROL.md) ·
[Computer use](docs/COMPUTER_USE.md) · [Shell](docs/SHELL.md).

### Is it a good fit?

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

### Safety model, in brief

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

### More documentation

| Goal | Document |
|---|---|
| Understand the architecture | [Architecture](docs/ARCHITECTURE.md) |
| Build or contribute | [Build guide](docs/BUILD.md) · [Development](docs/maintainers/DEVELOPMENT.md) · [Release process](docs/maintainers/RELEASE.md) |
| Project history | [Release-attempt history](docs/history/release-attempts.md) · [Research](docs/research/) |
