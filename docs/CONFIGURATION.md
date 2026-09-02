# Configuration

Every flag and environment variable for the three shipped executables, plus
file locations and ports. This page is the single reference; other pages link
here instead of repeating the tables.

## `local-browser-bridge` (console server)

| Flag | Effect |
|---|---|
| `--enable-shell` | Grant API clients full current-user native shell access (`shell.run`) |
| `--check-updates` | Check GitHub release metadata once and exit |
| `--no-update-check` | Start without the one-time background metadata check |
| `--licenses` | Print project and third-party license notices, then exit |
| `-V`, `--version` | Print the installed version and exit |
| `-h`, `--help` | Print usage and exit |

| Environment variable | Default | Purpose |
|---|---|---|
| `LBB_PORT` | `17373` | Loopback HTTP/WebSocket port |
| `LBB_TOKEN` | none | Explicit bridge token; skips reading/writing the token file |
| `LBB_TOKEN_PATH` | computed profile path (see below) | Token file location |
| `LBB_ENABLE_SHELL` | `false` | Same effect as `--enable-shell`; accepts `1/true/yes/on` |
| `LBB_DISABLE_UPDATE_CHECK` | `false` | Same effect as `--no-update-check`; accepts `1/true/yes/on` |

## `local-browser-bridge-desktop` (tray / menu-bar host)

Same flags and environment variables as the console server, plus:

| Flag | Effect |
|---|---|
| `--start-helper` | Launch the computer helper immediately after the server starts |
| `--extension-setup` | Open the guided browser-extension setup dialog and exit |

| Environment variable | Default | Purpose |
|---|---|---|
| `LBB_INSTALL_ROOT` | the executable's own directory (Windows) or its enclosing `.app`'s parent (macOS), falling back to the current directory | Where the desktop host looks for its own install layout (extension folder, launchers); the installer sets this so a moved/renamed install still resolves correctly |

## `local-computer-helper`

| Flag | Effect |
|---|---|
| `--request-permissions` | Request or check screen-capture and input permissions, then exit |
| `--benchmark` | Benchmark five screen observations, then exit |
| `--licenses` | Print project and third-party license notices, then exit |
| `-V`, `--version` | Print the installed version and exit |
| `-h`, `--help` | Print usage and exit |

| Environment variable | Default | Purpose |
|---|---|---|
| `LBB_PORT` | `17373` | Server port the helper connects to |
| `LBB_TOKEN` | none | Explicit bridge token; without it the helper reads the same token file the server uses |

Without options, the helper connects to Local Browser Bridge on loopback and
waits. It never opens a listening socket.

## File locations

| Item | Default path |
|---|---|
| Bridge token | `~/.local-browser-bridge/token` (the directory is created with mode `0700`, the file with mode `0600`; see [Security](../SECURITY.md)) |
| macOS install root | `$HOME/Applications/Local Browser Bridge` |
| Windows install root | `%LOCALAPPDATA%\Programs\Local Browser Bridge` |
| macOS LaunchAgent | current-user LaunchAgent, restarts the app only after an abnormal exit |
| Windows Startup shortcut | a `.lnk` in the current user's Startup folder |
| Desktop host logs | opened from the tray/menu-bar **Open Logs** item |

## Ports

Local Browser Bridge binds one loopback TCP port (`17373` by default, or
`LBB_PORT`) for HTTP, SSE, and both connector WebSockets (`/bridge`,
`/computer`). It never opens a second port and never binds a non-loopback
address. See [Troubleshooting](TROUBLESHOOTING.md#port-17373-is-already-in-use)
if that port is already in use.

## Installer switches

The one-command installers accept additional flags not listed above, because
they configure the *installed* startup entry rather than one process
invocation. See [Windows](INSTALL_WINDOWS.md#options) and
[macOS](INSTALL_MACOS.md#options).
