# Install on macOS

This guide installs a published universal Local Browser Bridge release on
macOS. It does not build the project from source. For development setup, use
[Building from source](BUILD.md#macos-build).

## Requirements

- macOS 13 or later on Apple silicon or Intel
- Chrome or Edge at or above the minimum version declared by the selected
  release's extension
- Screen Recording permission for desktop capture
- Accessibility permission for semantic inspection and the complete input
  backend
- No Node.js, Rust, Homebrew, or package manager is required

Browser-only control does not require the optional computer helper or its macOS
permissions.

## 1. Download one matching release

Open the [latest stable release](https://github.com/flrngel/local-browser-bridge/releases/latest)
and note its exact `vVERSION` tag. Download these files from that same release:

```text
local-browser-bridge-vVERSION-macos-universal.tar.gz
local-browser-bridge-extension-vVERSION.zip
SHA256SUMS.txt
```

The macOS archive contains:

```text
local-browser-bridge
Local Computer Helper.app/
LICENSE
THIRD_PARTY_LICENSES.txt
```

Do not download a package from a workflow artifact, source branch, pull request,
or a different release.

## 2. Verify the downloads

In Terminal, change to the download folder and calculate each digest:

```bash
shasum -a 256 local-browser-bridge-vVERSION-macos-universal.tar.gz
shasum -a 256 local-browser-bridge-extension-vVERSION.zip
shasum -a 256 SHA256SUMS.txt
grep -F 'local-browser-bridge-vVERSION-macos-universal.tar.gz' SHA256SUMS.txt
grep -F 'local-browser-bridge-extension-vVERSION.zip' SHA256SUMS.txt
```

Each package digest must match its exact filename entry in `SHA256SUMS.txt`.
For GitHub release and workflow-provenance verification, use the commands in
[Verify a published release](INSTALL.md#verify-a-published-release).

The package is ad-hoc signed for internal consistency, but it is not signed with
an Apple Developer ID and is not notarized. Keep Gatekeeper enabled. Continue
only after the tag, asset name, checksum, release attestation, and provenance
match. Do not remove quarantine attributes or disable Gatekeeper globally.

## 3. Extract into a stable folder

Choose a stable user-owned location before granting helper permissions. For
example:

```bash
install_dir="$HOME/Applications/Local Browser Bridge"
mkdir -p "$install_dir"
tar -xzf local-browser-bridge-vVERSION-macos-universal.tar.gz -C "$install_dir"
cd "$install_dir"
```

Keeping `Local Computer Helper.app` in a stable location helps macOS associate
privacy grants with the intended application. A new package build can still
require the permissions again because the project is not Developer ID-signed.

## 4. Check the versions

Substitute the selected release number for `VERSION`:

```bash
./local-browser-bridge --version
"./Local Computer Helper.app/Contents/MacOS/local-computer-helper" --version
```

Expected output:

```text
local-browser-bridge VERSION
local-computer-helper VERSION
```

These commands do not start either service loop.

## 5. Start the server

Run:

```bash
./local-browser-bridge
```

The server prints:

- its version;
- an authenticated control-page URL;
- the extension token; and
- the token-file location.

Open the complete control-page URL, including its `#token=...` fragment, in the
local browser used by the agent. Do not share or log that URL. The server binds
only to `127.0.0.1`. Leave Terminal open and press `Control-C` to stop it.

The default token file is:

```text
$HOME/.local-browser-bridge/token
```

The server creates it in a private current-user directory. Use the printed
token rather than opening the file.

### If Gatekeeper blocks the first launch

After attempting the verified binary once, open **System Settings → Privacy &
Security** and use the per-application **Open Anyway** control for Local Browser
Bridge. Confirm that the displayed application is the file you verified. Do not
disable Gatekeeper globally and do not run a command that strips quarantine
from unrelated files.

## 6. Install the extension

1. Extract `local-browser-bridge-extension-vVERSION.zip` into a stable folder.
2. Confirm that `manifest.json` is directly inside the selected folder.
3. Open `chrome://extensions` or `edge://extensions`.
4. Enable **Developer mode**.
5. Select **Load unpacked** and choose the folder containing `manifest.json`.
6. Open the Local Browser Bridge popup and confirm that its version matches the
   server.
7. Paste the token printed by the server, keep port `17373`, and select **Save
   and connect**.
8. Reload every already-open page you plan to control.

See [Load the Chrome or Edge extension](INSTALL.md#load-the-chrome-or-edge-extension)
for its credential and Stop-guard behavior.

## 7. Grant helper permissions

The helper is optional. From a second Terminal window in the stable install
folder, request and inspect its permissions:

```bash
"./Local Computer Helper.app/Contents/MacOS/local-computer-helper" \
  --request-permissions
```

If System Settings opens, enable **Local Computer Helper** under:

- **Privacy & Security → Screen & System Audio Recording** (called **Screen
  Recording** on some macOS versions); and
- **Privacy & Security → Accessibility**.

Run `--request-permissions` again to inspect the resulting
`screenCaptureReady`, `inputReady`, and `semanticReady` values. Screenshots can
be available while Accessibility-backed input and semantic inspection remain
unavailable.

Do not grant Accessibility to Terminal as a substitute for the packaged helper
identity.

## 8. Start desktop control only when needed

After the server is running, start the helper normally:

```bash
"./Local Computer Helper.app/Contents/MacOS/local-computer-helper"
```

The control page should report **Computer connected**. Select one application
window before observing or sharing it. Live sharing uses ScreenCaptureKit for
that exact window. The target must be on screen, non-minimized, and have a
nonzero area when sharing starts.

The helper exits when the server closes or its server connection is lost.
Relaunch it after the server is available again. **Stop share** ends only the
active ScreenCaptureKit stream and leaves the helper process connected. Stop the
helper with `Control-C` when desktop authority is no longer needed.

## 9. Confirm first-run behavior

Verify that:

- the server, helper, and extension popup show the same version;
- the popup reports connected;
- exactly one Local Browser Bridge extension card exists;
- the control page shows the intended browser connector;
- the helper appears only while it is running; and
- a reloaded target page shows the bridge status surface when control starts.

Chrome displays **Local Browser Bridge started debugging this browser** while a
trusted debugger lease is active. Chrome's Cancel action, the page's **Stop**
button, and **Release control** in the popup can revoke it.

## Update

Follow the shared [update procedure](INSTALL.md#update-all-components-together).
Stop the helper and server before replacing the extracted package. Keep the
newly verified files together and re-run the helper permission check after an
update; macOS can require the grants again for an ad-hoc-signed build.

Preserve the same extracted extension folder if you want to preserve the
extension identity and saved settings. Disable its card, replace only that
folder's contents with the verified new ZIP contents, then re-enable and reload
the same card. Otherwise remove the old card before loading a new folder.

## Uninstall or reset

1. Release control and stop the helper and server.
2. Select **Clear saved token** in the popup and remove the extension card.
3. Delete the user-owned server/helper and extracted-extension folders.
4. Remove **Local Computer Helper** from Screen Recording and Accessibility in
   System Settings if macOS retains those entries.
5. To invalidate the bridge token as well, run this only after both programs
   have stopped:

```bash
rm -f -- "$HOME/.local-browser-bridge/token"
```

This exact command removes only the saved token file. The application installed
no daemon, LaunchAgent, login item, or system-wide uninstaller.

## Troubleshooting

### The extension reports a version or protocol mismatch

Run both packaged executables with `--version`, check the popup version, and
replace the outlier with the matching release asset. Do not mix versions.

### An existing page cannot start control

Reload the page normally after installing or updating the extension. The Stop
guard must be present from `document_start`.

### The helper exits immediately

Start the server first, then run the helper again. The macOS helper intentionally
exits when its server connection ends.

### Screen capture or input is unavailable

Run the packaged helper's `--request-permissions` command again, confirm both
privacy grants, and restart the helper. Do not use a raw `cargo run` helper when
testing the packaged permission identity.

### Port 17373 is already in use

Stop the conflicting process or choose another unprivileged port. Set the same
`LBB_PORT` value in both Terminal windows and enter that port in the extension
popup. The bridge still binds only to loopback.

### A window cannot be captured or operated

Restore it from minimized state and keep it on screen. Protected content,
cross-Space behavior, framework-specific accessibility, or unsupported private
input routes can still fail closed. Review the documented
[limitations](LIMITATIONS.md).
