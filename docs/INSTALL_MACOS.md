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
- installs the universal server, optional helper app, and extension under
  `$HOME/Applications/Local Browser Bridge`;
- creates a current-user LaunchAgent and starts the loopback-only server;
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

The install folder also contains three double-click launchers:

- **Open Local Browser Bridge.command** starts the server if needed and opens
  the authenticated dashboard;
- **Finish Browser Extension Setup.command** repeats the extension setup; and
- **Start Computer Helper.command** opens optional desktop control.

## Local shell access (optional)

Shell access is full command access as your macOS user and is off by default.
Enable persistent `zsh` and `/bin/sh` support only for a trusted local agent:

```bash
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash -s -- --enable-shell
```

Rerun the normal install command without `--enable-shell` to turn shell access
back off in the LaunchAgent.

## Desktop control (optional)

The installer keeps desktop authority off by default. Open the helper only
when you want one selected app window to be controllable:

```bash
open "$HOME/Applications/Local Browser Bridge/Local Computer Helper.app"
```

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
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh | bash -s -- --version 0.12.37
```

## Options

Download once when you want several options:

```bash
installer="$(mktemp)"
curl -fsSL https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-macos.sh -o "$installer"
bash "$installer" --no-startup
bash "$installer" --no-launch
bash "$installer" --enable-shell
bash "$installer" --install-root "$HOME/My Apps/LBB"
bash "$installer" --uninstall
bash "$installer" --uninstall --reset-token
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
- **Need the manual procedure:** use [Manual and independent verification](INSTALL.md).
