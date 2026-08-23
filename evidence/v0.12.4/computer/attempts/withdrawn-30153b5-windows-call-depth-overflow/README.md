# Withdrawn v0.12.4 exact-candidate attempt

This directory preserves the successful macOS half of the exact v0.12.4
release-candidate run. The candidate was withdrawn because the corresponding
interactive Windows acceptance failed closed before its first screenshot. It
was never published as a GitHub Release, and nothing in this directory is
release evidence for a shipped version.

## Frozen candidate binding

- Source commit: `30153b55df4d8e51a66ad9b22f9062ccf02978a7`
- Tag: `v0.12.4`
- Deploy workflow run: `32612926953`, attempt `1`
- Release-candidate artifact: `9486218327`
- Artifact ZIP SHA-256:
  `00f432bcf2096cf37fbce72358ab0568a9478c1d5b69f10a9a36049ba726bac0`
- `SHA256SUMS.txt` SHA-256:
  `bbf182742ca34ca9cd0a2d6f05cfef4b0a755fdd23b15dd24937a15b3b659b0a`
- macOS archive SHA-256:
  `ba0ff0a081188b094699ea55913808bdc034df92eeb37a5f9eecc50ebf68f9b9`

Before either executable ran, the coordinator independently verified the
five-file inventory, canonical checksum manifest, exact source/tag/workflow/run
provenance, GitHub-hosted runner identity, and all five GitHub attestations.

## macOS result

The packaged universal server and helper passed `181/181` live assertions.
The run exercised exact-window observation, semantic set-value and invoke,
persistent ScreenCaptureKit streaming, background pixel input, resize, native
text delivery and restoration, acknowledgement/drop sequencing, explicit
cancellation, authority recovery, fresh sharing, exact-target close, and
controlled teardown. Both binaries contained `arm64` and `x86_64` slices and
passed strict structural code-signature verification.

All six screenshot digests match `helper-results.json`. A separate visual audit
confirmed that every PNG contains only the primary deterministic fixture: no
sibling window, unrelated application, desktop content, personal data, or
secret is visible. The persistent-share frames display macOS's native capture
indicator, while `computer-05-live-share-pixel-action.png` also shows the
bridge's synthetic pointer result.

The result record and log contain no bearer token, authorization header,
credential, username, hostname, email address, personal path, or unrelated URL.
The rig terminated its own fixture, helper, and server; the helper was never
respawned and the loopback listener closed.

## Withdrawal reason

The same candidate passed the Windows PowerShell 5.1 host self-test and its
initial live process-bound helper readiness check, including exact image,
parent, interactive session, authenticated session/process identity, and a
`computer.status` round trip. The next fixture predicate entered a recursive
PowerShell call and failed with `CallDepthOverflow` before any Windows
screenshot was captured.

The defect was in the acceptance runner, not the native event probe or helper
handshake: `Wait-ForFixtureProof` nested a callback named `$Condition` inside
`Wait-Condition`, whose own parameter used the same name. Windows PowerShell's
dynamic scope resolved the inner reference to the wrapper itself. The release
workflow was canceled while still waiting at its protected environment, and no
v0.12.4 Release was created. See [WINDOWS.md](WINDOWS.md) for the retained
sanitized Windows inventory and cleanup facts.
