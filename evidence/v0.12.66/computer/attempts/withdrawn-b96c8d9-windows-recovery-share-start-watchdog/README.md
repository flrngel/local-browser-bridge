# Withdrawn v0.12.66 Windows recovery share-start watchdog attempt

The sole Windows native acceptance attempt for the exact v0.12.66 release
candidate was reserved and executed on 2026-08-31. The trust gate passed for
source `2dcc8eccebaed6837a3eb71012e4826d7e9de922`, workflow run
`33350856503` attempt `5`, artifact `9752191469`, raw artifact ZIP SHA-256
`b96c8d9c1a2da895af83eef885ae879d040db702bcfa8769b380c5b9a2216119`,
and checksum-manifest SHA-256
`869e251d58d35eee2c9a6862a2da92cf93f8419d3184ce55b24762478bbf9a12`.
The retained summary SHA-256 is
`59a19a229a8d8165fd3112ed9ad0cb5d3939f5e360d1aa2155fdb80ebb7a7ce0`.

Both the initial and post-arm topology checks passed. They bound the
authenticated controller to the launched supervisor and the authenticated
worker to one exact-image direct child in the interactive session.

The fresh foreground-arm request and matching receipt used request ID
`4f9d24dc98204525a928aac7ca4d3278`. The exact sentinel action was accepted
once. The fixture recorded exactly one request, one acknowledgement, one
left-button down, and one left-button up, with three stable native samples and
arm-to-baseline continuity. No second UI action was attempted.

The baseline exact-window `computer.observe` and its sanitized screenshot step
both passed. The runner then failed closed in `recovery-suite` when
`computer.share.start` returned `COMPUTER_HELPER_WATCHDOG`: the computer helper
exceeded its 12-second command deadline and terminated so native authority was
revoked. No later native suite step or stock-Chrome acceptance ran.

The persistent attempt reservation is schema 2 with status
`reserved-no-retry`, coordinator instance
`c469b4d907f040f192dac6eeec5f7aa6`, and `retryAllowed: false`. The coordinator
terminal record is `failed-closed` at `runner-finished` with reason
`runner-summary-failed`. This candidate must not be retried, resumed, or used
to support a Windows acceptance claim.

Cleanup reported no issues, verified that the bearer token was not retained,
released the recovery event, removed the ephemeral fixture executable, and did
not terminate unrelated processes. The previously sealed macOS acceptance
evidence was not reproduced or modified. Stock-Chrome evidence collection,
tagging, release publication, and post-release verification did not run.

This directory contains only the runner-generated sanitized summary, fixture,
operator and step records, the sanitized coordinator terminal record, and this
explanatory note. It intentionally excludes the raw screenshot, candidate
binaries, private coordinator configuration, staged source, logs, credentials,
absolute paths, token material, and browser evidence.
