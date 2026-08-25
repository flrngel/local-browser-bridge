# Install on Windows

This guide installs a published Local Browser Bridge release on 64-bit Windows
11. It does not build the project from source. For development setup, use
[Building from source](BUILD.md#windows-build).

## Requirements

- 64-bit Windows 11
- A signed-in interactive user session
- Chrome or Edge at or above the minimum version declared by the selected
  release's extension
- No administrator access, Node.js, Rust, Visual C++ Redistributable, or package
  manager is required

The helper must run as the signed-in user. Do not run it as a Windows service,
from Session 0, or with administrator privileges for ordinary desktop control.

## 1. Download one matching release

Open the [latest stable release](https://github.com/flrngel/local-browser-bridge/releases/latest)
and note its exact `vVERSION` tag. Download these files from that same release:

```text
local-browser-bridge-vVERSION-windows-x86_64.exe
local-computer-helper-vVERSION-windows-x86_64.exe
local-browser-bridge-extension-vVERSION.zip
SHA256SUMS.txt
```

The helper is optional, but its version must match if you install it. Do not
download a binary from a workflow artifact, source branch, pull request, or a
different release.

Create a stable, user-owned folder such as:

```text
%LOCALAPPDATA%\Programs\Local Browser Bridge
```

Place the two executables and `SHA256SUMS.txt` there. Keep the extension in its
own stable subfolder after extraction so a later update can preserve its
extension identity.

## 2. Verify the downloads

In PowerShell, change to the folder containing the downloads and calculate each
digest:

```powershell
Get-FileHash .\local-browser-bridge-vVERSION-windows-x86_64.exe -Algorithm SHA256
Get-FileHash .\local-computer-helper-vVERSION-windows-x86_64.exe -Algorithm SHA256
Get-FileHash .\local-browser-bridge-extension-vVERSION.zip -Algorithm SHA256
Get-FileHash .\SHA256SUMS.txt -Algorithm SHA256
Get-Content .\SHA256SUMS.txt
```

The first three values must match their exact filename entries in
`SHA256SUMS.txt`. Bind the manifest itself to the release with
`gh release verify-asset` rather than trusting a checksum file obtained through
the same download path. For GitHub release and workflow-provenance verification,
use the commands in
[Verify a published release](INSTALL.md#verify-a-published-release).

The executables are not yet signed with a Microsoft publisher certificate, so
SmartScreen can show **Unknown publisher**. Keep SmartScreen enabled. Continue
only after the tag, asset name, checksum, release attestation, and provenance
match. Do not disable SmartScreen or add a broad security exclusion.

## 3. Check the versions

Substitute the selected release number for `VERSION`:

```powershell
& .\local-browser-bridge-vVERSION-windows-x86_64.exe --version
& .\local-computer-helper-vVERSION-windows-x86_64.exe --version
```

Expected output:

```text
local-browser-bridge VERSION
local-computer-helper VERSION
```

These commands do not start either service loop.

## 4. Start the server

From PowerShell or Explorer, run:

```powershell
.\local-browser-bridge-vVERSION-windows-x86_64.exe
```

The server prints:

- its version;
- an authenticated control-page URL;
- the extension token; and
- the token-file location.

Open the complete control-page URL, including its `#token=...` fragment, in the
local browser used by the agent. Do not share or log that URL. The server binds
only to `127.0.0.1`; it does not require an inbound firewall rule. Leave the
PowerShell window open. Press `Ctrl+C` to stop the server.

The default token file is:

```text
%USERPROFILE%\.local-browser-bridge\token
```

The server creates and protects that file for the current user. Use the printed
token rather than opening the file.

## 5. Install the extension

1. Extract `local-browser-bridge-extension-vVERSION.zip` into a stable folder.
   Windows Explorer's **Extract all** or PowerShell's `Expand-Archive` is
   sufficient.
2. Confirm that `manifest.json` is directly inside the selected folder, not one
   directory deeper.
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

## 6. Start desktop control only when needed

Open a second PowerShell window in the install folder and run:

```powershell
.\local-computer-helper-vVERSION-windows-x86_64.exe
```

The visible process is a supervisor. It creates a disposable worker in the same
interactive session, reconnects with bounded backoff if the server is
temporarily unavailable, and exits when you close its console or press
`Ctrl+C`. Administrator rights are not required.

The control page should report **Computer connected**. Select one application
window before observing or sharing it. **Stop share** ends only the active
Windows Graphics Capture stream; it leaves the helper connected. Stop the
helper process when desktop authority is no longer needed.

## 7. Confirm first-run behavior

Verify that:

- the server, helper, and extension popup show the same version;
- the popup reports connected;
- exactly one Local Browser Bridge extension card exists;
- the control page shows the intended browser connector;
- the optional helper appears only while it is running; and
- a reloaded target page shows the bridge status surface when control starts.

Chrome displays **Local Browser Bridge started debugging this browser** while a
trusted debugger lease is active. Chrome's Cancel action, the page's **Stop**
button, and **Release control** in the popup can revoke it.

## Update

Follow the shared [update procedure](INSTALL.md#update-all-components-together).
On Windows, stop the helper and server before replacing either `.exe`; Windows
normally prevents replacing a running executable.

Preserve the same extracted extension folder if you want to preserve the
extension identity and saved settings. Disable its card, replace only that
folder's contents with the verified new ZIP contents, then re-enable and reload
the same card. Otherwise remove the old card before loading a new folder.

## Uninstall or reset

1. Release control and stop the helper and server.
2. Select **Clear saved token** in the popup and remove the extension card.
3. Delete the user-owned install and extracted-extension folders.
4. To invalidate the bridge token as well, run this only after both executables
   have stopped:

```powershell
$tokenPath = Join-Path $env:USERPROFILE ".local-browser-bridge\token"
Remove-Item -LiteralPath $tokenPath -ErrorAction SilentlyContinue
```

The application installed no Windows service, scheduled task, startup entry, or
system-wide uninstaller.

## Troubleshooting

### The extension reports a version or protocol mismatch

Run both executables with `--version`, check the popup version, and replace the
outlier with the matching release asset. Do not mix release versions.

### An existing page cannot start control

Reload the page normally after installing or updating the extension. The Stop
guard must be present from `document_start`.

### The helper does not connect

Start the server first, confirm both use the same Windows account and default
port, and check the helper console. Do not run one process elevated and the
other as an ordinary user.

### Port 17373 is already in use

Stop the conflicting process or choose another unprivileged port. Set the same
`LBB_PORT` value in the server and helper PowerShell windows and enter that port
in the extension popup. The bridge still binds only to loopback.

### An elevated application cannot be controlled

Windows integrity boundaries can block UI Automation or window messages to an
elevated target. Do not elevate Local Browser Bridge as a workaround. Use a
non-elevated target or review the documented [limitations](LIMITATIONS.md).
