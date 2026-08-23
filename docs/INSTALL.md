# Installation and update guide

Local Browser Bridge has two required matching components—the standalone server and Chromium extension—and one optional matching computer helper for native desktop control. Always use every installed component from the same release version.

Browser control, including recursive cross-origin iframe control, requires Chrome or Edge 140 or later. Version 0.12.8 retains this floor because Chromium 140 is the first supported line in which the extension can restrict persisted local storage to trusted extension contexts. The complete macOS archive requires macOS 13 or later; native Windows support is intended for Windows 11 in the signed-in interactive session.

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
gh release verify-asset vVERSION PATH_TO_DOWNLOADED_FILE -R flrngel/local-browser-bridge
gh attestation verify PATH_TO_DOWNLOADED_FILE \
  -R flrngel/local-browser-bridge \
  --source-ref refs/tags/vVERSION \
  --signer-workflow flrngel/local-browser-bridge/.github/workflows/deploy.yml \
  --deny-self-hosted-runners
```

The first two commands bind the download to the immutable GitHub release. The last command additionally requires provenance from this repository's tagged release workflow on a GitHub-hosted runner. You can also compare the local SHA-256 value with the release's `SHA256SUMS.txt` manifest.

On macOS, `shasum -a 256 FILE` prints the local SHA-256 value. On Windows PowerShell, use `Get-FileHash FILE -Algorithm SHA256`.

The macOS archive contains `LICENSE` and `THIRD_PARTY_LICENSES.txt`, and the extension ZIP contains `LICENSE`. Both Windows executables and both macOS executables expose the same embedded notices without starting the server or helper:

```text
PROGRAM --licenses
```

## 3. Run the server

By default, the token lives in the dedicated `.local-browser-bridge` directory under the current user's profile. The bridge may create and narrow permissions on that one exact default directory because it owns the directory. If `$HOME` on Unix or `%USERPROFILE%` on Windows is unavailable, empty, or not absolute, startup fails instead of placing the token under the process working directory; use an explicit `LBB_TOKEN` or a token file in a pre-created private `LBB_TOKEN_PATH` parent. Every `LBB_TOKEN_PATH` value that differs from the computed default is custom—even when its parent is also named `.local-browser-bridge`. A custom parent must already exist and already be private: mode `0700` and current-user ownership on Unix, or a protected current-user-only DACL on Windows. The bridge validates a custom parent without changing it and fails before creating a token when the check does not pass. Create a separate private directory first; never point the variable at Desktop, Documents, a repository, or another general-purpose directory.

The validation and token read-or-create operation use one retained directory capability. On Unix, every token and temporary-file operation is relative to the validated directory descriptor. On Windows, every child open and create is resolved by `NtCreateFile` relative to the retained no-follow directory handle; target inspection, atomic replacement, and failed-temporary cleanup also stay on retained file or directory handles. Stable parent and replaced-file identities are checked after mutation, but pathname checks are detection rather than authority. Replacing any parent pathname during the transaction therefore cannot redirect a child operation into a decoy directory.

### Windows 11

Run the downloaded `.exe` from PowerShell or Explorer. It prints a control-surface URL and a random extension token. No Node.js installation is required.

The generated token is stored under `%USERPROFILE%\.local-browser-bridge\token`. The server protects that directory and file with a non-inherited DACL granting only the signed-in user full control. It retains the validated ordinary final parent as the root capability, rejects a reparse-point final parent and reparse children, and keeps each typed private temporary-file handle open with delete authority through the atomic handle-based rename. Custom token leaf names containing alternate-stream syntax, Win32-reserved characters or device names, control characters, or trailing dot/space ambiguity are rejected, and exact-case lookup avoids selecting a case-only sibling. Windows can traverse an ancestor profile junction while opening the parent; the bridge tolerates that redirection only while the final parent's identity and DACL remain stable. If the final parent or token is a reparse point, the token has another hard-link name, the parent identity changes, or the filesystem cannot retain and report the required ACL, startup fails without weakening that path.

The binary is not yet signed with a Microsoft publisher certificate. Microsoft SmartScreen can therefore report an unknown publisher. Keep SmartScreen enabled. Verify the GitHub release, checksum, and provenance first; do not run the file if its hash or origin differs.

### macOS

Extract and run the universal binary:

```bash
tar -xzf local-browser-bridge-vVERSION-macos-universal.tar.gz
./local-browser-bridge
```

The archive supports both Apple silicon and Intel. It is not yet signed with an Apple Developer ID or notarized. Gatekeeper may block the first launch. Keep Gatekeeper enabled. After verifying the source and artifact, macOS provides a per-app **Open Anyway** action under **System Settings → Privacy & Security**; do not disable Gatekeeper globally.

## 4. Start native desktop control only when wanted

The helper is optional. Its console states what it can do. Stop it with `Ctrl+C` or by closing its console. It shares the server's generated token file automatically, so there is no second secret to paste. The Windows launcher stays open and supervises disposable workers. The macOS helper stays open only for its current server connection and deliberately exits when that connection ends.

### Windows 11

From a second PowerShell window:

```powershell
.\local-computer-helper-vVERSION-windows-x86_64.exe
```

Run it as the signed-in user, not as a Windows service and not from Session 0. Administrator rights are not required for ordinary desktop control. The server UI should show **Computer connected**.

The executable you launch is a supervisor. If its hidden worker loses the server transport, the supervisor replaces that worker with backoff and reconnects when the server is available. Choosing **Stop share** stops only the active capture stream and leaves the worker running.

### macOS

The helper is packaged as a stable application identity so Screen Recording and Accessibility are granted to the helper rather than to an arbitrary executable path. From a second Terminal window, first request/check both permissions:

```bash
./Local\ Computer\ Helper.app/Contents/MacOS/local-computer-helper --request-permissions
```

If macOS opens System Settings, enable **Local Computer Helper** under **Privacy & Security → Screen & System Audio Recording** and **Privacy & Security → Accessibility**. Then start the helper for the current server session:

```bash
./Local\ Computer\ Helper.app/Contents/MacOS/local-computer-helper
```

The macOS helper deliberately terminates after either an intentional server shutdown or an unexpected loss of its server transport. Relaunch it after the server is available again. Choosing **Stop share** is an in-process operation: it stops the current `SCStream` but does not exit the helper.

The permission check reports `screenCaptureReady`, `inputReady`, and `semanticReady` separately. Screenshots can work while Accessibility is unavailable, but `inputReady` stays false because ordinary click, drag, scroll, key, and text routes may need the AX-backed exact-window focus lease. Semantic element refs are also omitted until Accessibility is granted. Pointer trajectory has a no-focus implementation, but readiness advertises the requirements of the complete input backend rather than a partial exception.

Live sharing uses a ScreenCaptureKit stream for the exact window chosen in the bridge control page. The operating system can show its current capture indicator and Stop affordance, but the helper does not present Apple's system content picker. The target must be on screen, non-minimized, and have a nonzero area when the share starts.

The app bundle is ad-hoc signed for internal consistency, but it is not Developer ID-signed or notarized. A new build can require the grants again. Do not grant Accessibility to an unrelated shell or globally weaken Gatekeeper.

## 5. Load the extension

1. Extract `local-browser-bridge-extension-vVERSION.zip` to a stable folder.
2. Open `chrome://extensions` in Chrome or `edge://extensions` in Edge.
3. Enable **Developer mode**.
4. Select **Load unpacked** and choose the extracted folder containing `manifest.json`.
5. Open the Local Browser Bridge popup and confirm that its version matches the server version.
6. Paste the token printed by the server, keep port `17373`, and select **Save and connect**.
7. Reload every already-open page you plan to control. The trusted Stop guard is installed at `document_start`; an old page that predates the extension install or update fails control start until a normal reload installs that early guard.

The popup stores that token until the extension is removed or you select **Clear saved token** and confirm. Clearing revokes active browser control, cancels work that has not reached Chrome yet, discards any waiting approval, disconnects the connector, removes the `token` entry from extension storage, and reports the connection as not configured even if Bridge control was paused. Turning off **Bridge control** only pauses the connector; it deliberately keeps the saved token for later use.

Chrome and Edge 140 or later are required. Older builds are refused by the manifest because they cannot enforce the extension's persisted-storage access boundary.

The extension includes no remote code, analytics, cookie API, native messaging host, downloader, or external update endpoint. Chrome does not auto-update unpacked extensions on Windows or macOS. The server's metadata-only checker accepts only a canonical stable release marked immutable by GitHub with the exact five uploaded, nonempty assets and GitHub SHA-256 digests.

When the local UI reports a new release, repeat the download and verification steps, release browser and computer control, and stop the old server and helper. Update all three components together. For the unpacked extension, use one of these procedures:

1. To preserve its identity and saved settings, disable the existing card at `chrome://extensions`, replace the contents of that card's existing unpacked folder with the verified new ZIP contents, then re-enable and reload the same card.
2. To extract to a new folder, first remove the old extension card, then select **Load unpacked** for the new folder and enter the server's new token again.

Finish by confirming that `chrome://extensions` shows exactly one Local Browser Bridge card and that its popup version matches the server and helper, then reload every open target page before controlling it. Loading a second extracted path without removing the first creates another unpacked extension identity and can leave the old connector active.

## 6. Understand the authority granted

Full Access mode is enabled by default and can control all regular HTTP(S) tabs in the selected Chromium profile, enter sensitive text, close tabs, send keys, click coordinates, and evaluate page JavaScript. Use a dedicated browser profile if you do not want the bridge to reach personal sessions. Turn Full Access off to use the site allowlist and one-time approvals in Safe mode.

The computer helper can take one-shot observations and start a persistent native stream with a requested 1–10 FPS cap for the exact application window selected in the control page. macOS uses ScreenCaptureKit; Windows uses Windows Graphics Capture and leaves capture indication under operating-system control. The helper routes supported background mouse or keyboard events only to that `(process, window)` target. It does not use global HID input, move the hardware cursor, change the active desktop/Space, or silently fall back to foreground control.

On macOS, focus-capable input may briefly release the saved user's Accessibility `AXFrontmost` state and make the exact target `AXFrontmost=true` under a private focus lease, then restore and verify both applications. WindowServer's user-front process/window remains unchanged before and after; the helper does not call `AXRaise` or `_SLPSSetFrontProcessWithOptions`. On Windows, the backend does not call `SetForegroundWindow`. These are before/after guarantees for an accepted action, not proof of zero visible or focus-state interruption. The helper also offers no shell, filesystem, clipboard, process-launch, downloader, or telemetry commands.

Exact-window capture and target-routed input still share the user's login session; they are not a VM, remote desktop, or separate input seat. Stop the helper whenever native application authority is not needed. Review the current [capability matrix](CAPABILITIES.md) and [limitations](LIMITATIONS.md) before using desktop control with consequential applications.

Stopping the server immediately breaks both connector sessions. The macOS helper then exits and must be relaunched after the server returns. On Windows, the supervisor keeps replacing disconnected workers with backoff and reconnects one when the server returns. You can also stop the helper, pause browser control in the extension popup, select **Clear saved token** there to disconnect and forget the extension credential, or remove the extension from `chrome://extensions`.
