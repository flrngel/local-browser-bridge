# Installation and update guide

Local Browser Bridge has two required matching components—the standalone server and Chromium extension—and one optional matching computer helper for native desktop control. Always use every installed component from the same release version.

Core browser control requires Chrome or Edge 118 or later. Recursive cross-origin iframe control requires Chrome or Edge 125 or later. The complete macOS archive requires macOS 13 or later; native Windows support is intended for Windows 11 in the signed-in interactive session.

## 1. Download the matching files

Open the [official GitHub Releases page](https://github.com/flrngel/local-browser-bridge/releases/latest) and download:

- Windows: `local-browser-bridge-vVERSION-windows-x86_64.exe` and, for desktop control, `local-computer-helper-vVERSION-windows-x86_64.exe`
- macOS Intel or Apple silicon: `local-browser-bridge-vVERSION-macos-universal.tar.gz`, which contains the server and `Local Computer Helper.app`
- Every platform: `local-browser-bridge-extension-vVERSION.zip`
- Verification: `SHA256SUMS.txt`

There is no installer, background service, browser hijack, autostart entry, or silent updater. The server and optional helper run only while you launch them. The helper opens no listening socket; it connects outbound to the loopback server.

## 2. Verify before running

The release page is public, its workflow is reviewable, and each artifact has GitHub build provenance. With the GitHub CLI installed, verify the release assets directly:

```bash
gh release verify vVERSION -R flrngel/local-browser-bridge
gh attestation verify PATH_TO_DOWNLOADED_FILE -R flrngel/local-browser-bridge
```

You can also compare a downloaded artifact with the immutable release or checksum manifest:

```bash
gh release verify-asset vVERSION PATH_TO_DOWNLOADED_FILE -R flrngel/local-browser-bridge
```

On macOS, `shasum -a 256 FILE` prints the local SHA-256 value. On Windows PowerShell, use `Get-FileHash FILE -Algorithm SHA256`.

The macOS archive contains `LICENSE` and `THIRD_PARTY_LICENSES.txt`, and the extension ZIP contains `LICENSE`. Both Windows executables and both macOS executables expose the same embedded notices without starting the server or helper:

```text
PROGRAM --licenses
```

## 3. Run the server

### Windows 11

Run the downloaded `.exe` from PowerShell or Explorer. It prints a control-surface URL and a random extension token. No Node.js installation is required.

The binary is not yet signed with a Microsoft publisher certificate. Microsoft SmartScreen can therefore report an unknown publisher. Keep SmartScreen enabled. Verify the GitHub release, checksum, and provenance first; do not run the file if its hash or origin differs.

### macOS

Extract and run the universal binary:

```bash
tar -xzf local-browser-bridge-vVERSION-macos-universal.tar.gz
./local-browser-bridge
```

The archive supports both Apple silicon and Intel. It is not yet signed with an Apple Developer ID or notarized. Gatekeeper may block the first launch. Keep Gatekeeper enabled. After verifying the source and artifact, macOS provides a per-app **Open Anyway** action under **System Settings → Privacy & Security**; do not disable Gatekeeper globally.

## 4. Start native desktop control only when wanted

The helper is optional. Its console states what it can do and stays open while desktop control is available. Stop it with `Ctrl+C` or by closing its console. It shares the server's generated token file automatically, so there is no second secret to paste.

### Windows 11

From a second PowerShell window:

```powershell
.\local-computer-helper-vVERSION-windows-x86_64.exe
```

Run it as the signed-in user, not as a Windows service and not from Session 0. Administrator rights are not required for ordinary desktop control. The server UI should show **Computer connected**.

### macOS

The helper is packaged as a stable application identity so Screen Recording and Accessibility are granted to the helper rather than to an arbitrary executable path. From a second Terminal window, first request/check both permissions:

```bash
./Local\ Computer\ Helper.app/Contents/MacOS/local-computer-helper --request-permissions
```

If macOS opens System Settings, enable **Local Computer Helper** under **Privacy & Security → Screen & System Audio Recording** and **Privacy & Security → Accessibility**. Then start the long-running helper:

```bash
./Local\ Computer\ Helper.app/Contents/MacOS/local-computer-helper
```

The permission check reports `screenCaptureReady`, pixel `inputReady`, and `semanticReady` separately. If `semanticReady` is false, screenshots and supported pixel routes can still work, but semantic element refs are deliberately omitted until Accessibility is granted.

Live sharing uses a ScreenCaptureKit stream for the exact window chosen in the bridge control page. The operating system can show its current capture indicator and Stop affordance, but the helper does not present Apple's system content picker. The target must be on screen, non-minimized, and have a nonzero area when the share starts.

The app bundle is ad-hoc signed for internal consistency, but it is not Developer ID-signed or notarized. A new build can require the grants again. Do not grant Accessibility to an unrelated shell or globally weaken Gatekeeper.

## 5. Load the extension

1. Extract `local-browser-bridge-extension-vVERSION.zip` to a stable folder.
2. Open `chrome://extensions` in Chrome or `edge://extensions` in Edge.
3. Enable **Developer mode**.
4. Select **Load unpacked** and choose the extracted folder containing `manifest.json`.
5. Open the Local Browser Bridge popup and confirm that its version matches the server version.
6. Paste the token printed by the server, keep port `17373`, and select **Save and connect**.

Chrome and Edge 118–124 support the core tab and top-level-page features. Use version 125 or later when the task must observe or click through cross-origin iframes.

The extension includes no remote code, analytics, cookie API, native messaging host, downloader, or external update endpoint. Chrome does not auto-update unpacked extensions on Windows or macOS. The server's metadata-only checker accepts only a canonical stable release marked immutable by GitHub. When the local UI reports a new release, repeat the download and verification steps and replace the server, helper, and extension together.

## 6. Understand the authority granted

Full Access mode is enabled by default and can control all regular HTTP(S) tabs in the selected Chromium profile, enter sensitive text, close tabs, send keys, click coordinates, and evaluate page JavaScript. Use a dedicated browser profile if you do not want the bridge to reach personal sessions. Turn Full Access off to use the site allowlist and one-time approvals in Safe mode.

The computer helper can take one-shot observations and start a persistent native stream with a requested 1–10 FPS cap for the exact application window selected in the control page. macOS uses ScreenCaptureKit; Windows uses Windows Graphics Capture and leaves capture indication under operating-system control. The helper routes supported background mouse or keyboard events only to that `(process, window)` target. It does not use global HID input, move the hardware cursor, activate the target app, change the active desktop, or silently fall back to foreground control. It also offers no shell, filesystem, clipboard, process-launch, downloader, or telemetry commands.

Exact-window capture and target-routed input still share the user's login session; they are not a VM, remote desktop, or separate input seat. Stop the helper whenever native application authority is not needed. Review the current [capability matrix](CAPABILITIES.md) and [limitations](LIMITATIONS.md) before using desktop control with consequential applications.

Stopping the server immediately breaks both connections. You can also stop the helper, pause browser control in the extension popup, or remove the extension from `chrome://extensions`.
