# Withdrawn v0.12.33 Windows fixture-build attempt

This directory preserves the sanitized terminal record for the Windows
acceptance attempt bound to source
`81bda2ac54e5041d609da3ed97a3cc789dc38f77`, candidate workflow run
`32941353193` attempt `1`, and final artifact `9596875214`.

The checked-in trust gate passed before the live attempt. It independently
verified the 10,434,016-byte raw artifact ZIP with SHA-256
`da7f01e264c6d061f41b7ff19ab1e6dd07b03d03948a4308f6a1db4a23ecd17c`,
the checksum-manifest SHA-256
`b3b930eb8e9844b8ac8b19efc07cc7c245c03a04c39a16d46ce1c719417b04b2`,
the exact five-file inventory, both Windows PE identities, all payload hashes,
and all five exact-attempt GitHub attestations.

The checked-in coordinator then consumed the version-scoped no-retry
reservation and launched the acceptance runner exactly once. The runner failed
closed at `build-dedicated-fixture` because its isolated source-bound fixture
compiler child exited with code 1. This happened before a fixture executable,
candidate server, candidate computer helper, product listener, foreground-arm
request, screenshot, Computer Use action, or stock-Chrome action existed.

The coordinator terminal state is `runner-started-terminal`,
`retryAllowed: false`, and `reasonCode: watcher-handoff-missing`; the latter is
the expected coordinator consequence because the runner ended before it could
publish a foreground-arm marker. The first runner error is `The source-bound
dedicated Windows fixture build failed.` at
`scripts/test-windows-computer-use.ps1:3567`.

Post-failure diagnostics compiled the same byte-exact staged fixture
successfully both directly and under the coordinator's whitelisted environment.
The failure is therefore narrowed to the runner's suspended private-Job and
NUL-only handle-list launch boundary. The exact child error is unavailable by
design because that boundary routes child stdout and stderr to NUL.

Cleanup completed without an issue: the exact runner and worker exited, the
ephemeral fixture output was removed, the selected loopback port was reusable,
the recovery event was released, and no product, helper, fixture, or listener
remained. The candidate was not retried and Chrome was not started.

The committed inventory contains only the sanitized start and terminal fields
plus a reduced diagnostic binding that hashes the original create-once records,
runner result, and private negative summary. Private configuration, staged
candidate bytes, raw logs, local paths, process or window identifiers, command
lines, usernames, tokens, and environment identifiers are excluded.
