# Install on macOS

The recommended install is one Terminal command. It needs no Homebrew, Node.js,
Rust, package manager, or manual checksum work.

## One-command install

Open Terminal as your normal signed-in user and run:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash
```

The installer:

- accepts only an immutable stable GitHub Release with the exact expected asset
  inventory;
- verifies GitHub's SHA-256 digest and `SHA256SUMS.txt` before running a binary;
- installs the menu-bar Desktop Host, universal console server, optional helper
  app, and extension under
  `$HOME/Applications/Local Browser Bridge`;
- creates a current-user LaunchAgent and starts the menu-bar app, which owns the
  loopback-only server in-process and restarts only after an abnormal exit;
- opens its authenticated dashboard; and
- opens the browser extensions page and extension folder, copies the exact
  folder path, and shows a numbered setup dialog that cannot be missed.

Chrome and Edge deliberately do not allow an ordinary downloaded program to
silently install an unpacked extension. Complete this one browser step:

1. In the browser page opened by the installer, enable **Developer mode**.
2. Select **Load unpacked**.
3. Paste the folder path already copied by the installer and choose the
   `~/Applications/Local Browser Bridge/extension` folder.
4. Return to the installer's guide and choose **OK**. It now copies the bridge
   token without printing it.
5. Open the extension popup, paste the copied token, and select **Save and
   connect**.

Reload tabs that were already open. Browser control is then ready.

The install folder contains **Local Browser Bridge.app**. It is an
`LSUIElement` app, so it appears in the menu bar without a Dock icon or Terminal
window. Its menu reports live server, extension, helper, shell, and update state
and provides dashboard, setup, token, helper, logs, update, and quit actions.

The folder also contains four maintenance launchers:

- **Open Local Browser Bridge.command** opens the menu-bar app;
- **Finish Browser Extension Setup.command** repeats the extension setup; and
- **Start Computer Helper.command** opens optional desktop control; and
- **Uninstall Local Browser Bridge.command** runs the version-matched safe
  uninstaller.

## Local shell access (optional)

Shell access is full command access as your macOS user and is off by default.
Enable persistent `zsh` and `/bin/sh` support only for a trusted local agent:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash -s -- --enable-shell
```

Rerun the normal install command without `--enable-shell` to turn shell access
back off in the LaunchAgent.

## Desktop control (optional)

The installer keeps desktop authority off by default. Select **Start Computer
Helper** from the menu-bar menu only when you want one selected app window to
be controllable.

On first use, macOS asks for **Screen & System Audio Recording** and
**Accessibility** access for `Local Computer Helper`. Grant only the
capabilities you intend to use, then reopen the helper. Browser-only control
does not require either permission.

To install/update and open the helper in one operation:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash -s -- --start-helper
```

## Update

Run the same one-command install again. It downloads and verifies one complete
latest release, replaces every component together, and reloads the current-user
LaunchAgent. In `chrome://extensions`, select **Reload** on the existing
extension card.

To install a specific stable version:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash -s -- --version 0.12.51
```

## One-command uninstall

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

## Options

Download once when you want several options:

```bash
installer="$(mktemp)"
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh -o "$installer"
bash "$installer" --no-startup
bash "$installer" --no-launch
bash "$installer" --enable-shell
bash "$installer" --install-root "$HOME/My Apps/LBB"
rm -f "$installer"
```

## Security notes

The helper is ad-hoc signed but the package is not yet Developer ID-signed or
notarized, so macOS may show an unknown-developer warning. Keep Gatekeeper
enabled. The installer does not clear quarantine globally, weaken privacy
settings, request administrator access, or accept a mutable/prerelease release.
For independent GitHub attestation verification, use
[Verify a published release](INSTALL.md#independent-provenance-check).

The complete dashboard URL and extension token are credentials. Do not paste
them into logs, screenshots, issue reports, or untrusted pages.

## Troubleshooting

- **Extension does not connect:** confirm the server and popup versions match,
  paste the current token again, and reload the target tab.
- **Helper exits or lacks capture/input:** start the server first, confirm the
  two privacy grants, and reopen the helper.
- **Port 17373 is busy:** stop the other listener before rerunning the installer.
- **No menu-bar icon appears:** open `Local Browser Bridge.app` from the install
  folder and inspect **Open Logs** from its menu if startup failed.
- **Need the manual procedure:** use [Manual and independent verification](INSTALL.md).
