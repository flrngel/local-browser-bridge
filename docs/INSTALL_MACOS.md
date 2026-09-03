# Install on macOS

## 1. Get it

Open Terminal and run one line:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash
```

Prefer not to run a command? Download the archive from the
[latest release](https://github.com/flrngel/local-browser-bridge/releases/latest)
page instead, unzip it, and drag **Local Browser Bridge.app** into
**Applications**, then open it there. A dashboard tab still opens on its
own, because the extension isn't connected yet — use the menu-bar icon's
**Open Dashboard** and **Finish Browser Extension Setup** items for the rest
of this page.

## 2. What it does

The one-line install:

- installs the menu-bar app, server, optional helper app, and extension
  folder under `$HOME/Applications/Local Browser Bridge`;
- starts it now, and again automatically each time you sign in;
- opens its dashboard page in your browser; and
- opens the Chrome/Edge extensions page and copies a folder path for you.

## 3. Load the browser add-on

Chrome and Edge do not let a downloaded program silently install an
extension, so this one step is manual:

1. In the page that opened, turn on **Developer mode**.
2. Click **Load unpacked**.
3. Paste the folder path already copied for you and choose the
   `~/Applications/Local Browser Bridge/extension` folder.
4. Back in the installer's own guide, click **OK**. It copies the connection
   token for you (without printing it).
5. Open the extension's popup icon, paste the token, and click **Save and
   connect**. Faster alternative: switch to the dashboard tab already open
   in your browser and click **Connect** instead — it carries your
   credentials already, so there is nothing to paste.

Reload any tabs that were already open. Browser control is ready.

## You're set

**Local Browser Bridge.app** lives in the menu bar (no Dock icon, no Terminal
window). Its install folder also has four maintenance launchers: **Open
Local Browser Bridge.command**, **Finish Browser Extension Setup.command**,
**Start Computer Helper.command**, and **Uninstall Local Browser
Bridge.command**.

Desktop control and shell access are both **on by default**, so an AI
assistant can already ask for them once you connect one — see
[Agent integration](AGENT_INTEGRATION.md). To turn either off, open the
dashboard or the menu-bar icon and flip the switch next to it; the change
applies immediately, no restart needed.

Desktop control (one app window) additionally needs two macOS permission
grants the first time you use it — see below.

## Advanced

Everything below is optional. Most people never need it.

### One-command install

The one-line command in step 1 is already the scriptable form; run it again
any time (see "Update" below). It verifies GitHub's SHA-256 digest and
`SHA256SUMS.txt` before running anything.

### Desktop control permissions

Click **Start Computer Helper** from the menu-bar menu (or double-click
**Start Computer Helper.command**) when you want one selected app window to
be controllable. On first use, macOS asks for **Screen & System Audio
Recording** and **Accessibility** access for `Local Computer Helper`. Grant
only what you intend to use, then reopen the helper. Browser-only control
needs neither permission.

### Local shell access and desktop control, from the command line

Both are on by default once installed — no flag needed. That default lives
in a small `settings.json` file next to your bridge token, not in the
installer, so the dashboard/menu-bar switch (see "You're set" above) is the
normal way to change either one, and it takes effect without a reinstall or
restart. See [Configuration](CONFIGURATION.md#settings-file) for the file
itself, and [Shell](SHELL.md) for what shell access grants before you decide
whether to leave it on.

The installer also accepts `--enable-shell`, which additionally bakes it
into the LaunchAgent so shell access survives even a hand-edited or deleted
`settings.json`:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash -s -- --enable-shell
```

`--start-helper` similarly opens the computer helper right after install:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash -s -- --start-helper
```

### Update

Run the same one-command install again, or download and drag the app again.
It downloads and verifies one complete latest release, replaces every
component together, and reloads the current-user LaunchAgent. In
`chrome://extensions`, select **Reload** on the existing extension card.

To install a specific stable version instead of latest, substitute its
number (0.12.70 or later for the Desktop Host, shell, and Agent Fetch):

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash -s -- --version 0.13.0
```

### One-command uninstall

Close any work you are controlling, then run:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-macos.sh | bash
```

You can also double-click **Uninstall Local Browser Bridge.command** in the
install folder. That launcher fetches the uninstaller from the same immutable
release tag that installed it.

The default removal:

- unloads the exact current-user LaunchAgent;
- stops only processes whose command starts inside the exact install folder;
- removes only the installer-owned Desktop Host app, server, helper app,
  extension directory, manifest, and launchers;
- removes the saved bridge token and the helper's exact macOS privacy grants;
  and
- opens installed Chrome and Edge extension pages for the final browser-owned
  **Remove** click.

It does not edit a browser profile, delete an unfamiliar file, follow a
symlink, remove a system service, or touch unrelated developer/recovery tools.
If the install directory contains an unknown file, that file, the ownership
marker, and the directory are retained and reported.

Preview without changing anything:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-macos.sh | bash -s -- --dry-run
```

Optional retention switches:

```bash
# Keep the saved bridge token.
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-macos.sh | bash -s -- --keep-token

# Keep the helper's Screen Recording/Accessibility grants.
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-macos.sh | bash -s -- --keep-permissions

# Do not open browser extension pages or show the final dialog.
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-macos.sh | bash -s -- --no-browser
```

For a custom installation, pass the same absolute `--install-root`. Custom
roots require the installer's ownership marker and are never inferred.

### Options

Download once when you want several options:

```bash
installer="$(mktemp)"
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh -o "$installer"
bash "$installer" --no-startup
bash "$installer" --no-launch
bash "$installer" --enable-shell
bash "$installer" --start-helper
bash "$installer" --install-root "$HOME/My Apps/LBB"
rm -f "$installer"
```

### Security notes

The helper is ad-hoc signed but the package is not yet Developer ID-signed or
notarized, so macOS may show an unknown-developer warning. Keep Gatekeeper
enabled. The installer does not clear quarantine globally, weaken privacy
settings, request administrator access, or accept a mutable/prerelease release.
For independent GitHub attestation verification, use
[Verify a published release](VERIFY_RELEASE.md#independent-provenance-check).

The complete dashboard URL and extension token are credentials. Do not paste
them into logs, screenshots, issue reports, or untrusted pages.

### Troubleshooting

Symptom-first fixes, including what to do if no menu-bar icon appears or the
extension will not connect: [Troubleshooting](TROUBLESHOOTING.md).

If the newest published release is older than 0.12.70, it does not contain
the Desktop Host or shell yet, and this check runs before any files are
copied — see
[Troubleshooting](TROUBLESHOOTING.md#macos-installer-fails-with-an-unexpected-layout-or-unknown-argument-error).
Check the
[releases page](https://github.com/flrngel/local-browser-bridge/releases);
if the newest release is still older than 0.12.70, [build from
source](BUILD.md) instead.
