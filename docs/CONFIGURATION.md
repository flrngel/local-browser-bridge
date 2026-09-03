# Configuration

Every flag and environment variable for the three shipped executables, plus
file locations and ports. This page is the single reference; other pages link
here instead of repeating the tables.

## `local-browser-bridge` (console server)

| Flag | Effect |
|---|---|
| `--enable-shell` | Force shell access on at startup, overriding the settings file |
| `--no-shell` | Force shell access off at startup, overriding the settings file |
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
| `LBB_ENABLE_SHELL` | unset | Same effect as `--enable-shell` when set; accepts `1/true/yes/on` |
| `LBB_DISABLE_UPDATE_CHECK` | `false` | Same effect as `--no-update-check`; accepts `1/true/yes/on` |

## `local-browser-bridge-desktop` (tray / menu-bar host)

Same environment variables as the console server, and all its flags **except
`--check-updates`** — passing that one to the desktop host exits with
`Unknown argument`, not a metadata check — plus:

| Flag | Effect |
|---|---|
| `--start-helper` | Launch the computer helper immediately after the server starts |
| `--extension-setup` | Open the guided browser-extension setup dialog and exit |
| `--install` (Windows only) | Install as an app under this account and exit |
| `--uninstall` (Windows only) | Remove the app installed under this account and exit |
| `--no-install` (Windows only) | Skip the one-time "set up as an app?" prompt |

| Environment variable | Default | Purpose |
|---|---|---|
| `LBB_INSTALL_ROOT` | the executable's own directory (Windows) or its enclosing `.app`'s parent (macOS), falling back to the current directory | Where the desktop host looks for its own install layout (extension folder, launchers); the installer sets this so a moved/renamed install still resolves correctly |
| `LBB_JUST_INSTALLED` | unset | Internal: set by a successful first-run self-install on the relaunched installed process it starts, so that process (not the installer process, which already exited) opens the dashboard once on its own. Not meant to be set by hand. |

## `local-computer-helper`

| Flag | Effect |
|---|---|
| `--request-permissions` | Request or check screen-capture and input permissions, then exit |
| `--benchmark` | Benchmark five screen observations, then exit |
| `--licenses` | Print project and third-party license notices, then exit |
| `-V`, `--version` | Print the installed version and exit |
| `-h`, `--help` | Print usage and exit |
| `--worker` (Windows only) | Run as the input-worker subprocess; not for direct use |
| `--controller-process-id=<pid>` (Windows only) | Nonzero parent process ID the worker supervises; not for direct use |

| Environment variable | Default | Purpose |
|---|---|---|
| `LBB_PORT` | `17373` | Server port the helper connects to |
| `LBB_TOKEN` | none | Explicit bridge token; without it the helper reads the token file at `LBB_TOKEN_PATH`, or the same default path the server uses |
| `LBB_TOKEN_PATH` | computed profile path (see below) | Token file location, read only when `LBB_TOKEN` is unset |

Without options, the helper connects to Local Browser Bridge on loopback and
waits. It never opens a listening socket.

## Settings file

Shell access and desktop control default to **on**, and that default (plus
whether the app starts at login) lives in one small JSON file, not a
command-line flag:

| Platform | Default path |
|---|---|
| All | `<home>/.local-browser-bridge/settings.json`, or the path in the `LBB_SETTINGS_PATH` environment variable |

```json
{
  "version": 1,
  "shellEnabled": true,
  "desktopControlEnabled": true,
  "startAtLogin": true
}
```

A toggle on the dashboard page or the tray/menu-bar menu writes this file
immediately (atomically — a crash mid-write never leaves a half-written
file) and takes effect without a restart; see
[`POST /api/settings`](API_REFERENCE.md#post-apisettings). A missing file,
an unreadable file, corrupt JSON, or an unrecognized `version` are all
treated the same way: every key falls back to `true` rather than blocking
startup. `--enable-shell`/`--no-shell` (or `LBB_ENABLE_SHELL`) at process
start always override whatever this file says for that one run, but never
edit the file themselves.

## File locations

| Item | Default path |
|---|---|
| Bridge token | `~/.local-browser-bridge/token` (on Unix, the directory is created with mode `0700` and the file with mode `0600`; on Windows, both get a protected TokenUser-only DACL instead — see [security-invariants.md](internals/security-invariants.md)) |
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
