# Install on Windows

The recommended install is one PowerShell command. It needs no administrator
access, Node.js, Rust, package manager, or manual checksum work.

## One-command install

Open **Windows PowerShell** as your normal signed-in user and run:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-windows.ps1')))
```

The installer:

- accepts only an immutable stable GitHub Release with the exact expected asset
  inventory;
- verifies GitHub's SHA-256 digest and `SHA256SUMS.txt` before running a binary;
- installs the tray-based Desktop Host, optional helper, and extension under
  `%LOCALAPPDATA%\Programs\Local Browser Bridge`;
- starts the loopback-only server inside the Desktop Host and opens its
  authenticated dashboard;
- starts the Desktop Host at future sign-ins using a `.lnk` shortcut in the
  current user's Startup folder without opening a Command Prompt window;
  and
- opens the browser extensions page and extension folder, copies the exact
  folder path, and shows a numbered setup dialog that cannot be missed.

Chrome and Edge deliberately do not allow an ordinary downloaded program to
silently install an unpacked extension. Complete this one browser step:

1. In the browser page opened by the installer, enable **Developer mode**.
2. Select **Load unpacked**.
3. Paste the folder path already copied by the installer and choose the
   `...\Local Browser Bridge\extension` folder.
4. Return to the installer's guide and choose **OK**. It now copies the bridge
   token without printing it.
5. Open the extension popup, paste the copied token, and select **Save and
   connect**.

Reload tabs that were already open. Browser control is then ready.

After installation, use the **Local Browser Bridge** folder in the Windows
Start menu:

- **Local Browser Bridge** starts the tray app if needed, or opens the
  authenticated dashboard when it is already running;
- **Finish Browser Extension Setup** reopens the extension page, folder, and
  copied folder path; and
- **Uninstall Local Browser Bridge** runs the version-matched safe uninstaller.

Use the notification-area icon for day-to-day control. Its menu reports live
server, extension, helper, shell, and update state and provides dashboard,
setup, token, helper, logs, update, and quit actions.

## Local shell access (optional)

Shell access is full command access as your Windows user and is off by default.
Enable persistent PowerShell/`cmd.exe` support only for a trusted local agent:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-windows.ps1'))) -EnableShell
```

Rerun the normal install command without `-EnableShell` to turn shell access
back off at startup.

## Desktop control (optional)

The installer keeps desktop authority off by default. Select **Start Computer
Helper** from the tray menu only when you want one selected native app window
to be controllable. The Desktop Host launches it without a console window.

Or install/update and launch the helper in one operation:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-windows.ps1'))) -StartHelper
```

Close the helper when desktop control is no longer needed. It is not installed
as a Windows service and does not open a listening port.

## Update

Run the same one-command install again. It downloads one complete latest
release, verifies it, stops only programs running from the install folder,
replaces every component together, and reuses the stable extension folder.
In `chrome://extensions`, select **Reload** on the existing extension card.

To install a specific stable version:

```powershell
$installer = [scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-windows.ps1')); & $installer -Version 0.12.55
```

## One-command uninstall

Close any work you are controlling, then run this from Windows PowerShell as
your normal signed-in user:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-windows.ps1')))
```

You can also open **Start > Local Browser Bridge > Uninstall Local Browser
Bridge**. That launcher fetches the uninstaller from the same immutable release
tag that installed it.

The default removal:

- stops only executables running from the exact install directory;
- removes the exact current-user Startup item and known Start-menu launchers;
- removes only version-matched product executables, the extension directory,
  manifest, launchers, and ownership marker;
- removes the saved bridge token; and
- opens installed Chrome and Edge extension pages for the final browser-owned
  **Remove** click.

It does not edit a browser profile, delete an unfamiliar file, traverse a
reparse point, remove a service or scheduled task, change the firewall, or
touch unrelated developer/recovery tools. If the install directory contains an
unknown file, that file, the ownership marker, and the directory are retained
and reported.

Preview without changing anything:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-windows.ps1'))) -DryRun
```

Optional retention switches:

```powershell
# Keep the saved bridge token.
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-windows.ps1'))) -KeepToken

# Do not open browser extension pages or show the final dialog.
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-windows.ps1'))) -NoBrowser
```

For a custom installation, pass the same absolute `-InstallRoot`. Custom roots
require the installer's ownership marker and are never inferred.

## Options

```powershell
& $installer -NoStartup
& $installer -NoLaunch
& $installer -EnableShell
& $installer -InstallRoot 'D:\Apps\Local Browser Bridge'
```

If `$installer` is not defined, run the assignment shown above first.

## Security notes

The executables are not yet Microsoft publisher-signed, so SmartScreen may
show **Unknown publisher**. Keep SmartScreen enabled. The installer does not
disable it, add exclusions, request elevation, open a firewall port, or accept
a mutable/prerelease release. For independent GitHub attestation verification,
use [Verify a published release](INSTALL.md#independent-provenance-check).

The complete dashboard URL and extension token are credentials. Do not paste
them into logs, screenshots, issue reports, or untrusted pages.

## Troubleshooting

- **Extension does not connect:** confirm the server and popup versions match,
  paste the current token again, and reload the target tab.
- **Port 17373 is busy:** stop the other listener before rerunning the installer.
- **No tray icon appears:** confirm that the standard release executable is the
  Desktop Host and that an older console-server Startup item was removed.
- **Helper cannot control an elevated app:** keep the bridge non-elevated and
  use a non-elevated target; Windows integrity boundaries are intentional.
- **Need the manual procedure:** use [Manual and independent verification](INSTALL.md).
