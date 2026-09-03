# Troubleshooting

Find what you're seeing below. Each fix is listed shortest-first. For the
full error-code table (if you're writing a connector), see
[API reference](API_REFERENCE.md#error-taxonomy).

## The page says the browser add-on is not connected

1. Open the dashboard and click **Connect**. If that doesn't help, re-paste
   the current token in the extension's popup and click **Save and
   connect** — a stale or empty token cannot connect.
2. Confirm the server is running: `curl http://127.0.0.1:17373/health` should
   answer. If it doesn't, reopen the app from the tray/menu-bar icon.
3. Check that the server's `version` in that same response matches the
   extension popup's version. A mismatch after an update means the extension
   needs a reload: open `chrome://extensions` and click **Reload** on its
   card.
4. If the server was restarted, reload the extension the same way — a
   leftover connection from before the restart can linger.

## Nothing happens when my AI tries to click or type

1. Reload the tab your AI is working in. Chrome does not let an extension
   see a tab that was already open before the extension was installed or
   updated.
2. Check for **`423 HUMAN_CONTROL_PAUSED`** in the error your AI got back.
   That means a person pressed **Release control** in the extension popup,
   or **Cancel** on Chrome's own debugging warning. Only clicking **Resume**
   in the extension's popup clears it — no API call can. See
   [Browser control](BROWSER_CONTROL.md#the-lease-model).
3. Check for **`409`** errors right after a cancel or a dropped connection —
   expected, not a bug. Ask your AI to re-observe the page
   (`page.observe`) before trying again. See
   [Agent integration](AGENT_INTEGRATION.md#the-browser-control-loop).

## Windows blocked the app ("Windows protected your PC")

This app is not yet signed by a big company, so Windows shows this warning
for an unrecognized publisher — it does not mean the file is unsafe. Click
**More info**, then **Run anyway**. Keep SmartScreen turned on; the
installer never asks you to disable it.

## macOS won't open the app ("unidentified developer")

The app is not yet notarized. Keep Gatekeeper on; do not disable it or clear
quarantine globally. Right-click (or Control-click) the app, choose **Open**,
then confirm **Open** again in the dialog that appears — this one-time step
is enough.

## My AI can't reach it at all

The most common cause: your AI assistant runs in a cloud browser tab (for
example ChatGPT or Copilot opened as a web page), not on this computer.
Those cannot reach `127.0.0.1` on your machine, no matter how the request is
worded — this is not something either side can configure around. Use an
assistant that runs a local process on this same computer instead. See
[Which assistants can use this](../README.md#which-assistants-can-use-this).

If your assistant does run locally and still can't connect:

- Confirm the server answers: `curl http://127.0.0.1:17373/health`.
- Confirm you copied the current **Agent Fetch base URL** or bearer token
  from the dashboard, not an old one from a previous install.
- **`403 HOST_REJECTED`**: point your client at `127.0.0.1:<port>` exactly —
  not a hostname, IP alias, or reverse proxy. This is a deliberate
  DNS-rebinding defense.

## Desktop control (one app window) isn't working

- **macOS**: open System Settings → Privacy & Security → **Screen
  Recording** and **Accessibility**, turn on **Local Computer Helper** under
  each, then quit and reopen the helper — a permission change never applies
  to an already-running process. This is `403 COMPUTER_PERMISSION_REQUIRED`.
  See [Computer use](COMPUTER_USE.md#enablement-and-permissions).
- **Either platform**: confirm you are signed in and not locked/logged out —
  the helper needs an active interactive session, not a service.
- Desktop control is on by default, but can be switched off from the
  dashboard or tray/menu-bar icon; check that switch first.

## Shell commands fail with `403 SHELL_DISABLED`

Shell access is off. Turn it on from the dashboard or tray/menu-bar icon —
no restart needed with the switch; the old `--enable-shell` /
`LBB_ENABLE_SHELL=1` startup flags still work too but do need a restart. See
[Shell](SHELL.md#turn-it-off-or-back-on).

## Port 17373 is already in use

Something else on this computer is already using that port. Either stop it,
or run the server on a different port with `LBB_PORT=<port>` (see
[Configuration](CONFIGURATION.md)) and use that port everywhere — extension
popup, dashboard URL, `/demo`. The installer does not pick a different port
automatically.

## No tray icon (Windows) or menu-bar icon (macOS) appears

Open the app directly from the install folder and check **Open Logs** in its
menu for a startup error. On Windows, confirm an older console-server
Startup shortcut was not left behind alongside the new one.

## The installer fails or reports success but nothing works

### macOS installer fails with "an unexpected layout" or "Unknown argument" error

The resolved release predates the feature this guide describes. The
Desktop Host, local shell, Agent Fetch, and one-command uninstaller all ship
starting with release 0.12.70; `install-macos.sh` checks the downloaded
archive's contents before it copies anything, so installing an older release
with a guide that assumes these features fails this way, before any files
land on disk. Check the
[releases page](https://github.com/flrngel/local-browser-bridge/releases) for
the latest version — both installers already resolve `latest` by default, so
this is not a matter of passing a version flag; if the newest release is
still older than 0.12.70, [build from source](BUILD.md) instead. See
[Install macOS](INSTALL_MACOS.md).

### Windows installer succeeds, but no tray icon appears or EnableShell fails at launch

Unlike the macOS script, `install-windows.ps1` does not check what a release
actually contains — it downloads and installs whatever `-Version` (or
`latest`) resolves to and reports success either way. Installing a release
older than 0.12.70 this way copies the binaries and creates working
shortcuts, but there is no Desktop Host yet: the shortcuts run the console
server binary instead, so no tray icon appears, and if you passed
`-EnableShell` that same process (and the sign-in Startup shortcut) exits
immediately with `Unknown argument: --enable-shell` because the older server
does not understand that flag. Check the
[releases page](https://github.com/flrngel/local-browser-bridge/releases):
if the newest release is still older than 0.12.70, [build from
source](BUILD.md) instead of installing. See
[Install Windows](INSTALL_WINDOWS.md).

## Updates report an error

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
