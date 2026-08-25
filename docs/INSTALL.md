# Installation and update guide

This guide is for published stable releases. The source tree can be newer than
the latest release, so do not infer that the version in `Cargo.toml` is
available for download. A version is installable only when it appears on the
public [GitHub Releases page](https://github.com/flrngel/local-browser-bridge/releases/latest).

For a development build from the current source, use [Building from
source](BUILD.md) instead.

## Choose a platform

- [Install on Windows 11](INSTALL_WINDOWS.md)
- [Install on macOS 13 or later](INSTALL_MACOS.md)

Both installations use the same unpacked Chromium extension procedure below.

## Components

Every installation needs two matching components and can use a third:

| Component | Required | Purpose |
|---|---:|---|
| Local Browser Bridge server | Yes | Hosts the authenticated control page and loopback connector |
| Chrome/Edge extension | Yes | Connects real browser tabs to the server |
| Local Computer Helper | No | Observes and operates one selected desktop application window |

Always use all installed components from one release. The server refuses a
helper or extension whose package or protocol version does not match.

The release contains exactly these five nonempty assets:

```text
local-browser-bridge-vVERSION-windows-x86_64.exe
local-computer-helper-vVERSION-windows-x86_64.exe
local-browser-bridge-vVERSION-macos-universal.tar.gz
local-browser-bridge-extension-vVERSION.zip
SHA256SUMS.txt
```

The macOS archive contains both the server and `Local Computer Helper.app`.
Windows provides the server and helper as separate executables.

There is no installer, service, login item, browser store package, or silent
updater. The programs run only while you launch them. The helper opens no
listening socket; it connects outbound to the server on loopback.

## Verify a published release

Download `SHA256SUMS.txt` with the platform assets. Compare each local SHA-256
digest before running an executable. The platform guides include native hash
commands.

For stronger binding, use a current GitHub CLI that provides `gh release
verify` and `gh release verify-asset`:

```bash
gh release verify vVERSION -R flrngel/local-browser-bridge
gh release verify-asset vVERSION PATH_TO_DOWNLOADED_FILE \
  -R flrngel/local-browser-bridge
tag_object="$(gh api \
  repos/flrngel/local-browser-bridge/git/ref/tags/vVERSION \
  --jq '.object.sha')"
source_sha="$(gh api \
  repos/flrngel/local-browser-bridge/git/tags/$tag_object \
  --jq '.object.sha')"
if ! gh attestation verify PATH_TO_DOWNLOADED_FILE \
  -R flrngel/local-browser-bridge \
  --source-ref refs/heads/main \
  --source-digest "$source_sha" \
  --signer-workflow flrngel/local-browser-bridge/.github/workflows/deploy.yml \
  --deny-self-hosted-runners; then
  # Compatibility path for releases built by the older tag-triggered workflow.
  gh attestation verify PATH_TO_DOWNLOADED_FILE \
    -R flrngel/local-browser-bridge \
    --source-ref refs/tags/vVERSION \
    --source-digest "$source_sha" \
    --signer-workflow flrngel/local-browser-bridge/.github/workflows/deploy.yml \
    --deny-self-hosted-runners
fi
```

Run the asset and provenance commands for every downloaded file, including
`SHA256SUMS.txt`. The release command binds the tag and asset inventory to
GitHub's release attestation. The two API calls peel the required annotated
release tag to its accepted source commit. The provenance check then requires
the artifact to come from that exact source through this repository's candidate
workflow on a GitHub-hosted runner. The fallback covers older immutable releases
whose builder ran directly from the release tag; it does not relax the source
digest or signer-workflow checks.

The published Windows executables are not yet signed with a Microsoft publisher
certificate. The macOS package is ad-hoc signed but is not Developer ID-signed
or notarized. SmartScreen or Gatekeeper can therefore show an unknown-developer
warning. Keep those protections enabled, verify the release first, and do not
weaken operating-system security globally.

## Load the Chrome or Edge extension

1. Extract `local-browser-bridge-extension-vVERSION.zip` into a stable folder.
   The ZIP has `manifest.json` at its root.
2. Open `chrome://extensions` in Chrome or `edge://extensions` in Edge.
3. Enable **Developer mode**.
4. Select **Load unpacked** and choose the extracted folder containing
   `manifest.json`.
5. Open the Local Browser Bridge popup and confirm that its version matches the
   server.
6. Paste the token printed by the server, keep port `17373`, and select **Save
   and connect**.
7. Reload every already-open page you plan to control. The trusted Stop guard is
   installed at `document_start`; pages opened before installation or update
   must be reloaded.

The selected release's `manifest.json` declares its minimum Chromium version.
Use that version or a newer Chrome or Edge build. The extension contains no
remote code, analytics, cookie API, native-messaging host, downloader, or
external update endpoint.

The popup stores the bridge token in extension-local storage. **Clear saved
token** disconnects, revokes active browser control, discards any waiting
approval, and removes that credential. Turning off **Bridge control** only
pauses the connector and deliberately keeps the token.

## Confirm the installation

Before giving an agent control, verify all of the following:

- the server reports the expected version with `--version`;
- the extension popup shows that same version and reports connected;
- exactly one Local Browser Bridge card exists on the extensions page;
- the control page reports the intended browser tab connector;
- if the helper is running, the control page reports **Computer connected**;
- a reloaded target page shows the in-page status surface when control starts;
  and
- Chrome displays its browser-owned debugging warning during the trusted lease.

The complete control-page URL contains a bearer token in its fragment. Treat it
as a credential. Do not paste it into logs, screenshots, issue reports, or
untrusted pages.

## Update all components together

At startup the server checks only the fixed public GitHub Releases metadata
endpoint. It accepts only a canonical stable, immutable release with the exact
five uploaded assets and GitHub SHA-256 digests. It does not download or install
anything. `--check-updates` performs the same metadata-only check and exits.
Use `--no-update-check` or `LBB_DISABLE_UPDATE_CHECK=1` to disable the startup
request.

When an update is available:

1. Download and verify the new version before stopping the old one.
2. Release browser control and stop any active computer share.
3. Stop the old helper and server.
4. Replace the platform binaries or macOS package with the matching new
   release.
5. Update the unpacked extension with one of these methods:
   - To preserve its extension identity and saved settings, disable the existing
     card, replace the contents of its existing folder with the verified ZIP
     contents, then re-enable and reload that same card.
   - To use a new folder, remove the old card before selecting **Load unpacked**
     for the new folder, then enter the current server token.
6. Confirm that exactly one extension card remains and that the server, helper,
   and popup versions match.
7. Reload each open target page before starting control.

Unpacked extensions do not update automatically. Loading a new extracted folder
without removing the previous card creates a second extension identity and can
leave an old connector active.

## Uninstall or reset

1. Release browser and computer control.
2. Stop the helper and server.
3. Use **Clear saved token** in the extension popup, then remove the extension
   card from `chrome://extensions` or `edge://extensions`.
4. Delete the downloaded executable/archive folders.
5. If you also want to invalidate the saved bridge credential, remove the token
   file only after both programs have stopped:
   - Windows: `%USERPROFILE%\.local-browser-bridge\token`
   - macOS: `$HOME/.local-browser-bridge/token`

Removing the token causes the next server start to generate a new one, so any
remaining extension configuration will no longer connect. The application
creates no service, scheduled task, login item, or system-wide uninstaller.

## Authority and limitations

Full Access is enabled by default and can act in signed-in browser sessions,
enter sensitive text, and interact with consequential pages. Use Safe mode or a
dedicated browser profile when broad access is inappropriate.

The helper is cooperative local remote-control software. It shares the signed-in
session and does not provide an independent virtual desktop, pointer, or input
queue. Stop it whenever native application authority is not needed. Review the
[security model](../SECURITY.md), [capability matrix](CAPABILITIES.md), and
[limitations](LIMITATIONS.md) before using it with sensitive applications.

On macOS, focus-capable input can briefly release the saved user's
Accessibility `AXFrontmost` state and set the exact target's
`AXFrontmost=true` under a private focus lease, then restore and verify both
applications. These sampled before-and-after boundaries are not proof of zero visible or focus-state interruption.
