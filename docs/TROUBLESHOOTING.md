# Troubleshooting

Symptom → likely cause → fix. For the full error-code table, see
[API reference](API_REFERENCE.md#error-taxonomy).

## Installation

### macOS installer fails with "an unexpected layout" or "Unknown argument" error

The resolved release predates the feature this guide describes. The
Desktop Host, local shell, Agent Fetch, and one-command uninstaller all ship
starting with release 0.12.69; `install-macos.sh` checks the downloaded
archive's contents before it copies anything, so installing an older release
with a guide that assumes these features fails this way, before any files
land on disk. Check the
[releases page](https://github.com/flrngel/local-browser-bridge/releases) for
the latest version — both installers already resolve `latest` by default, so
this is not a matter of passing a version flag; if the newest release is
still older than 0.12.69, [build from source](BUILD.md) instead. See
[Install macOS](INSTALL_MACOS.md).

### Windows installer succeeds, but no tray icon appears or EnableShell fails at launch

Unlike the macOS script, `install-windows.ps1` does not check what a release
actually contains — it downloads and installs whatever `-Version` (or
`latest`) resolves to and reports success either way. Installing a release
older than 0.12.69 this way copies the binaries and creates working
shortcuts, but there is no Desktop Host yet: the shortcuts run the console
server binary instead, so no tray icon appears, and if you passed
`-EnableShell` that same process (and the sign-in Startup shortcut) exits
immediately with `Unknown argument: --enable-shell` because the older server
does not understand that flag. Check the
[releases page](https://github.com/flrngel/local-browser-bridge/releases):
if the newest release is still older than 0.12.69, [build from
source](BUILD.md) instead of installing. See
[Install Windows](INSTALL_WINDOWS.md).

### Port 17373 is already in use

Another process is bound to the default port. Stop it, or run the server on a
different port with `LBB_PORT=<port>` (see [Configuration](CONFIGURATION.md))
and use that port everywhere below (extension popup, dashboard URL, `/demo`).
The installer does not pick a different port automatically.

### No tray icon (Windows) or menu-bar icon (macOS) appears

Open `Local Browser Bridge.app` (macOS) or the Desktop Host executable
(Windows) directly from the install folder and check **Open Logs** in its
menu for a startup error. On Windows, confirm an older console-server Startup
shortcut was not left behind alongside the new one.

## Extension

### Extension shows disconnected, or a version mismatch

1. Confirm the server is running (`curl http://127.0.0.1:17373/health`) and
   that its `version` matches the extension popup's version.
2. Re-paste the current token in the popup and select **Save and connect** —
   a stale or empty token cannot connect.
3. If the server was restarted, a mismatched `sessionId` from an old
   connection can linger client-side; reload the extension
   (`chrome://extensions` → **Reload**).

### Extension connects, but `page.observe`/`page.click` never see a newly opened tab

Reload tabs that were open before the extension was installed or updated —
Chrome does not retroactively inject a content script into an already-loaded
page, and the observation/control content script (`dom-core.js`/
`content.js`) that binds the lease and reads the DOM is no exception.

## Control and permissions

### `423 HUMAN_CONTROL_PAUSED`

A human pressed **Release control** in the extension popup or Chrome's
**Cancel** on its debugging warning, which latches a pause that survives
restarts. No API call clears it — only a human
clicking **Resume** in the extension's own popup. Stop retrying and surface
this to the human operator. See
[Browser control](BROWSER_CONTROL.md#the-lease-model).

### `409 NO_BROWSER_OBSERVATION` / `409 NO_COMPUTER_FRAME` right after a cancel

Expected behavior, not a bug: canceling a command (or a client disconnecting
mid-action, or any `COMMAND_OUTCOME_UNKNOWN`) quarantines the affected
observation until it is explicitly refreshed. Call `page.observe` (browser)
or `computer.observe` / `computer.share.start` (computer) to recover, then
retry the original action with a fresh `ref`/`generation`/`frameId`. See
[Agent integration](AGENT_INTEGRATION.md#the-browser-control-loop).

### `403 HOST_REJECTED`

The request's `Host` header was not exactly `127.0.0.1`, `localhost`, or
`[::1]` (optionally with the bound port). This is a DNS-rebinding defense; it
rejects requests before authentication even runs. Point your client at
`127.0.0.1:<port>` (or the value from `LBB_PORT`) rather than a hostname, IP
alias, or a reverse proxy.

### `403 COMPUTER_PERMISSION_REQUIRED`

macOS is missing a TCC grant. Open System Settings → Privacy & Security →
**Screen Recording** and **Accessibility**, enable **Local Computer Helper**
under each, then relaunch the helper (permission changes do not apply to an
already-running process). See
[Computer use](COMPUTER_USE.md#enablement-and-permissions).

### Helper exits immediately, or `computer.status` never reports readiness

- **macOS**: start the server first, then the helper; confirm both TCC
  grants above; a bare `cargo run` binary never carries a TCC grant — use the
  packaged `.app`.
- **Windows**: the helper must run in the signed-in interactive session, not
  a service or Session 0. `inputReady`/`semanticReady: false` in
  `computer.status` means the helper could not read the input desktop,
  foreground window, or cursor — check that a user is actually logged in and
  the session is not locked.

## Shell

### `403 SHELL_DISABLED`

The server was started without `--enable-shell` or `LBB_ENABLE_SHELL=1`.
Restart it with that flag — there is no runtime toggle. See
[Shell](SHELL.md#enable-it).

## Updates

The server's startup check only reads GitHub release metadata; it never
downloads or installs anything. If it reports an error instead of a version
comparison, that is a network problem reaching the GitHub API, not a bridge
fault — retry, or disable the check with `--no-update-check` /
`LBB_DISABLE_UPDATE_CHECK=1` if you manage updates yourself. See
[Configuration](CONFIGURATION.md).

## Still stuck

Check [Capabilities](CAPABILITIES.md) and [Limitations](LIMITATIONS.md) for
whether the behavior you are seeing is a documented boundary rather than a
bug, and [internals/PROTOCOL.md](internals/PROTOCOL.md) for the exact wire
behavior if you are implementing a connector.
