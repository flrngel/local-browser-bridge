# Withdrawn v0.12.60 Windows WGC frame-age attempt

The sole Windows acceptance attempt for the exact v0.12.60 release candidate
was reserved and executed on 2026-08-29. The trust gate passed for source
`57d9e11f59f25393b6f2b819949a739442f5de88`, workflow run `33269177753`
attempt `1`, artifact `9719674279`, raw artifact ZIP SHA-256
`7ceb294977635f2856750bf0a144bdd60811520026e65dcbd9b9d1fbb0743217`,
and checksum-manifest SHA-256
`f662e9191ffb39ea399f6864f504910a4d4d9a3aaa9c9864fad6cb9e1d3677e8`.

The v0.12.60 file-identity repair passed in live candidate execution. Both the
initial and post-arm topology checks bound the authenticated controller to the
launched supervisor and the authenticated worker to one exact-image direct
child in the interactive session.

The fresh foreground-arm request was published and the exact sentinel action
was accepted once. The fixture recorded exactly one left-button down and one
left-button up, matching request and acknowledgement generations, stable
foreground, focus, cursor, input desktop, and arm-to-baseline continuity.

The runner then failed closed at `baseline-status-and-observation`. The first
`computer.observe` returned `COMPUTER_CAPTURE_FAILED` with the sanitized message
`WGC compositor frame age exceeded the monotonic range`. No later native suite
step or browser acceptance ran.

The persistent attempt reservation is `reserved-no-retry`, and the coordinator
terminal record is `failed-closed` with reason `runner-summary-failed`. This
candidate must not be retried or resumed.

Cleanup reported no issues, removed the ephemeral fixture executable, released
the recovery event, left no task-owned candidate process or listener, and left
no v0.12.60 stock-Chrome claim. Computer Use session memory was cleared after
the single action. The sealed macOS aggregate was not reproduced or modified.

This directory contains only the runner-generated sanitized summary, fixture,
operator and step records, the sanitized coordinator terminal record, and this
explanatory note. It contains no candidate bytes, credentials, absolute paths,
raw screenshots, or browser evidence.
