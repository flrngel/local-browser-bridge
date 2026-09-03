# Install on Windows

## 1. Download it

Go to the [latest release](https://github.com/flrngel/local-browser-bridge/releases/latest)
page and download the file that ends in `.exe`.

## 2. Run it

Double-click the file you downloaded.

Windows will show a warning that says **"Windows protected your PC"**. That
appears because this app is not yet signed by a big company — it does not
mean the file is unsafe. Click **More info**, then **Run anyway**.

## 3. Install it

A dialog asks **"Set up Local Browser Bridge?"** — click **Yes**. This needs
no administrator password. It installs the app under
`%LOCALAPPDATA%\Programs\Local Browser Bridge` and starts it now, and again
automatically each time you sign in.

A dashboard tab opens on its own, already carrying your credentials, because
the browser extension isn't connected yet. Look for the new **Local Browser
Bridge** icon in the system tray too (bottom-right, maybe under the **^**
overflow arrow) — its right-click menu is how you reach everything below.

## 4. Load the browser add-on

Chrome and Edge do not let a downloaded program silently install an
extension, so this part is manual:

1. Right-click the tray icon and click **Finish Browser Extension Setup**.
   It opens the Chrome/Edge extensions page and copies a folder path for
   you.
2. Turn on **Developer mode**.
3. Click **Load unpacked**.
4. Paste the folder path already copied for you and choose the
   `...\Local Browser Bridge\extension` folder.
5. Switch back to the dashboard tab that already opened (or right-click the
   tray icon and click **Open Dashboard**).
6. On that page, click **Connect**. It pairs the extension without typing
   anything.

Reload any tabs that were already open. Browser control is ready. If
**Connect** doesn't finish the job, the dashboard shows the **Load
unpacked** steps again so you can retry, or use the extension's own popup
icon to enter a token by hand — get one from the tray icon's **Copy Bridge
Token** menu item.

## You're set

Look for the **Local Browser Bridge** folder in the Windows Start menu — use
it to reopen the dashboard, redo extension setup, or uninstall.

Desktop control and shell access are both **on by default**, so an AI
assistant can already ask for them once you connect one — see
[Agent integration](AGENT_INTEGRATION.md). To turn either off, open the
dashboard or right-click the tray icon and flip the switch next to it; the
change applies immediately, no restart needed.

## Advanced

Everything below is optional. Most people never need it.

### One-command install

Prefer scripting the install, or reinstalling on a machine without a
browser handy? Open **Windows PowerShell** as your normal signed-in user and
run:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-windows.ps1')))
```

It installs the same app as double-clicking the `.exe`, after verifying the
release's SHA-256 digest and `SHA256SUMS.txt`, but unlike a plain
double-click it also drives the browser add-on setup for you: it opens the
extensions page and copies the folder path, then — once the app has written
its token — copies that token too and opens the dashboard, printing each
step (**Load unpacked**, paste the token into the extension popup, **Save
and connect**) as it happens.

### Local shell access and desktop control

Both are on by default once installed — no flag needed. That default lives
in a small `settings.json` file next to your bridge token, not in the
installer, so the dashboard/tray switch (see "You're set" above) is the
normal way to change either one, and it takes effect without a reinstall or
restart. See [Configuration](CONFIGURATION.md#settings-file) for the file
itself, and [Shell](SHELL.md) for what shell access grants before you decide
whether to leave it on.

The installer also accepts `-EnableShell`, which additionally bakes
`--enable-shell` into the sign-in Startup shortcut so shell access survives
even a hand-edited or deleted `settings.json`:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-windows.ps1'))) -EnableShell
```

`-StartHelper` similarly launches the computer helper right after install:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-windows.ps1'))) -StartHelper
```

### Update

Run the same one-command install again (or download and run the new `.exe`
again). It downloads one complete latest release, verifies it, stops only
programs running from the install folder, replaces every component
together, and reuses the stable extension folder. In `chrome://extensions`,
select **Reload** on the existing extension card.

To install a specific stable version instead of latest, substitute its
number (0.12.70 or later for the Desktop Host, shell, and Agent Fetch):

```powershell
$installer = [scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/install-windows.ps1')); & $installer -Version 0.12.70
```

### One-command uninstall

Close any work you are controlling, then run this from Windows PowerShell as
your normal signed-in user:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod 'https://raw.githubusercontent.com/flrngel/local-browser-bridge/main/scripts/uninstall-windows.ps1')))
```

You can also open **Start > Local Browser Bridge > Uninstall Local Browser
Bridge**. That launcher fetches the uninstaller from the same immutable
release tag that installed it.

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

For a custom installation, pass the same absolute `-InstallRoot`. Custom
roots require the installer's ownership marker and are never inferred.

### Options

```powershell
& $installer -NoStartup
& $installer -NoLaunch
& $installer -EnableShell
& $installer -StartHelper
& $installer -InstallRoot 'D:\Apps\Local Browser Bridge'
```

If `$installer` is not defined, run the assignment shown in "Update" first.

### Security notes

The executables are not yet Microsoft publisher-signed, so SmartScreen shows
**Unknown publisher**. Keep SmartScreen enabled. The installer does not
disable it, add exclusions, request elevation, open a firewall port, or accept
a mutable/prerelease release. For independent GitHub attestation verification,
use [Verify a published release](VERIFY_RELEASE.md#independent-provenance-check).

The complete dashboard URL and extension token are credentials. Do not paste
them into logs, screenshots, issue reports, or untrusted pages.

### Troubleshooting

Symptom-first fixes, including what to do if no tray icon appears or the
extension will not connect: [Troubleshooting](TROUBLESHOOTING.md).

If the newest published release is older than 0.12.70, it does not contain
the Desktop Host, shell, or the `-NoShell` flag yet, and this installer has
no way to detect that — see
[Troubleshooting](TROUBLESHOOTING.md#windows-installer-succeeds-but-no-tray-icon-appears-or-enableshell-fails-at-launch).
Check the
[releases page](https://github.com/flrngel/local-browser-bridge/releases);
if the newest release is still older than 0.12.70, [build from
source](BUILD.md) instead.
