# Local Browser Bridge

Use the Chrome or Edge session already open on your computer from a local AI
control surface. Add the optional computer helper when the agent also needs to
observe or operate one selected desktop application window.

[Install on Windows](docs/INSTALL_WINDOWS.md)
· [Install on macOS](docs/INSTALL_MACOS.md)
· [Latest verified release](https://github.com/flrngel/local-browser-bridge/releases/latest)
· [Build from source](docs/BUILD.md)

> **Know the boundary first:** this repository provides the local bridge,
> extension, helper, dashboard, and API. It does not include an AI agent, an MCP
> server, or a native ChatGPT, Claude, or Copilot connector. The AI client must
> be able to open the authenticated local dashboard or call the local REST API
> from the same computer. A cloud-only browser cannot reach `127.0.0.1`.

## Choose your setup

| What you want to control | What you install | Start here |
|---|---|---|
| Chrome or Edge tabs | Desktop Host + unpacked extension | [Windows](docs/INSTALL_WINDOWS.md) or [macOS](docs/INSTALL_MACOS.md) |
| Browser tabs and one desktop app window | Desktop Host + unpacked extension + optional helper | [Windows](docs/INSTALL_WINDOWS.md) or [macOS](docs/INSTALL_MACOS.md) |
| Current development source | Rust toolchain + native platform SDK | [Windows and macOS build guide](docs/BUILD.md) |

Published packages do not need Node.js, Rust, administrator access, or a
package manager. The one-command per-user installer downloads and verifies the
complete release, starts the server at login, and prepares the extension.
Chrome or Edge still requires one explicit **Load unpacked** selection because
ordinary programs cannot silently install a local extension.

## How it runs on each operating system

| System | Normal user experience | Sign-in startup | Headless/developer path |
|---|---|---|---|
| Windows 11 x86_64 | A notification-area tray icon; the standard release executable is linked as a Windows GUI application and does not create a Command Prompt window | A `.lnk` shortcut starts the Desktop Host from the current user's Startup folder | Build and run the separate `local-browser-bridge.exe` console binary |
| macOS 13+ | `Local Browser Bridge.app` appears only in the menu bar because its bundle declares `LSUIElement` | A current-user LaunchAgent starts the app and restarts it only after abnormal exit | Run the raw `local-browser-bridge` binary from Terminal |
| Linux and other systems | No packaged Desktop Host | Not supported | The Rust server may compile, but browser/desktop packages are unsupported |

The tray or menu-bar menu shows server, browser, desktop-helper, shell, and
update state. It can open the authenticated dashboard, repeat extension setup,
copy the bridge token, start or stop the optional helper, open logs, and quit.
Quitting is a deliberate clean exit and does not trigger the macOS crash-restart
policy.

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

### Optional local shell

- Run PowerShell or `cmd.exe` on Windows and `zsh` or `/bin/sh` on macOS
- Return bounded stdout, stderr, exit status, duration, and timeout state
- Stay off by default until the server is installed or started with explicit
  shell authority
- Use the same authenticated POST API or the basic GET-only Agent Fetch API

Shell access is intentionally separate from the computer helper and is full
current-user command access, not a sandbox. Enable it only for a local agent you
trust with every file and program available to your signed-in account.

## Install in one command

Use the platform guide and paste its first command. The installer accepts only
an immutable stable GitHub Release, verifies both GitHub and manifest SHA-256
digests, installs without administrator access, starts the loopback server,
opens the authenticated dashboard, and prepares a stable extension folder.

- [Windows 11 installation](docs/INSTALL_WINDOWS.md)
- [macOS 13 or later installation](docs/INSTALL_MACOS.md)
- [Manual and independent provenance verification](docs/INSTALL.md)

The installer opens the browser extensions page and extension folder, copies
the folder path, and shows the remaining **Developer mode** / **Load unpacked**
steps in a visible dialog. It also installs a **Local Browser Bridge** launcher
and a repeatable **Finish Browser Extension Setup** guide. Desktop
control and shell access stay off until explicitly enabled.

## Uninstall in one command

The uninstaller stops only programs launched from the product folder, removes
the current-user startup entry, deletes installer-owned files, and invalidates
the saved bridge token. It never edits Chrome or Edge profile files.

Windows PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-windows.ps1')))
```

macOS Terminal:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-macos.sh | bash
```

The extensions page opens at the end. If the browser still shows the unpacked
Local Browser Bridge card, click **Remove** once. This last browser-owned click
is intentional: the uninstaller does not rewrite browser profile databases.
Use the platform guides for dry-run and keep-token options.

## Connect an agent with only Web Fetch

Open the dashboard and copy its private **Agent Fetch base URL**. A client that
can only perform a plain GET can append a method and query parameters:

```text
GET {AGENT_FETCH_BASE_URL}/status
GET {AGENT_FETCH_BASE_URL}/tabs.list
GET {AGENT_FETCH_BASE_URL}/tabs.activate?callId=choose-tab-1&tabId=7
```

All action-like GETs require a stable `callId`; a network retry then replays the
recorded result instead of running the action twice. Nested or type-sensitive
parameters can be supplied as URL-encoded JSON in `params`. The URL is a
credential derived separately from the master bridge token, so do not paste it
into issues, analytics-enabled pages, or shared logs. See the
[Agent Fetch protocol](docs/PROTOCOL.md#agent-fetch-get-only-api).

## Install the optional Agent Skill

Agents that implement the open Agent Skills format can load a compact router
instead of reading the entire protocol for every task. The skill discloses only
its short instructions first and loads one generated protocol section when a
browser, native-computer, transport, or HTTP detail is actually needed.

Install it through the cross-client Skills CLI:

```bash
npx skills add flrngel/local-browser-bridge --skill local-browser-bridge -g
```

Or install the checked-out source without Node.js on macOS or Linux into the
cross-client location:

```bash
bash scripts/install-agent-skill.sh --target agents
```

Use `--target codex` or `--target claude` for a client-native user directory.
The installer refuses to overwrite a different existing skill; `--check`
verifies an installation byte-for-byte. For agents that do not support skills,
the canonical [protocol document](docs/PROTOCOL.md) remains the complete,
portable integration contract.

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

- The tray or menu-bar icon reports **Server: Running** and opens the
  authenticated dashboard without exposing its credential in a console.
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
| macOS Desktop Host, server, and helper | macOS 13 or later; Screen Recording and Accessibility permissions are helper-only requirements |

On macOS, focus-capable input can briefly borrow and restore app focus through
an Accessibility focus lease. The project verifies foreground state before and
after the action; it does not claim that no transient focus change occurred.

## Updates and removal

The server's startup check reports a newer immutable stable release but never
changes files by itself. Rerun the same platform install command to download,
verify, and replace all components together. Then select **Reload** on the
existing unpacked extension card. Disable the metadata-only startup check with
`--no-update-check` or `LBB_DISABLE_UPDATE_CHECK=1`.

Use the one-command uninstaller above for a complete current-user removal. A
matching **Uninstall Local Browser Bridge** launcher is also installed in the
Windows Start menu and macOS install folder. See the
[Windows](docs/INSTALL_WINDOWS.md#one-command-uninstall) or
[macOS](docs/INSTALL_MACOS.md#one-command-uninstall) guide for dry-run,
credential-retention, and browser-page options.

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
| Install on-demand agent guidance | [Local Browser Bridge skill](skills/local-browser-bridge/SKILL.md) |
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
