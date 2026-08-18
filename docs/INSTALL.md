# Installation and update guide

Local Browser Bridge has two matching components: one standalone server binary for your operating system and one Chromium extension ZIP. Always install both from the same release version.

## 1. Download the matching files

Open the [official GitHub Releases page](https://github.com/flrngel/local-browser-bridge/releases/latest) and download:

- Windows: `local-browser-bridge-vVERSION-windows-x86_64.exe`
- macOS Intel or Apple silicon: `local-browser-bridge-vVERSION-macos-universal.tar.gz`
- Every platform: `local-browser-bridge-extension-vVERSION.zip`
- Verification: `SHA256SUMS.txt`

There is no installer, background service, browser hijack, or silent updater. The server runs only while you launch it and listens only on `127.0.0.1`.

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

## 4. Load the extension

1. Extract `local-browser-bridge-extension-vVERSION.zip` to a stable folder.
2. Open `chrome://extensions` in Chrome or `edge://extensions` in Edge.
3. Enable **Developer mode**.
4. Select **Load unpacked** and choose the extracted folder containing `manifest.json`.
5. Open the Local Browser Bridge popup and confirm that its version matches the server version.
6. Paste the token printed by the server, keep port `17373`, and select **Save and connect**.

The extension includes no remote code, analytics, cookie API, native messaging host, downloader, or external update endpoint. Chrome does not auto-update unpacked extensions on Windows or macOS. When the local UI reports a new release, repeat the download and verification steps and replace both components together.

## 5. Understand the authority granted

Full Access mode is enabled by default and can control all regular HTTP(S) tabs in the selected Chromium profile, enter sensitive text, close tabs, send keys, click coordinates, and evaluate page JavaScript. Use a dedicated browser profile if you do not want the bridge to reach personal sessions. Turn Full Access off to use the site allowlist and one-time approvals in Safe mode.

Stopping the server immediately breaks the bridge connection. You can also pause control in the extension popup or remove the extension from `chrome://extensions`.
