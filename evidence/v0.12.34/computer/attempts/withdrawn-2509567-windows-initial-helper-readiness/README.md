# Withdrawn v0.12.34 Windows initial-helper-readiness attempt

This directory preserves the sanitized terminal record for the Windows
acceptance attempt bound to source
`25095671d1ed50e74281ec17d9ede8829c0d8483`, candidate workflow run
`32957505500` attempt `1`, and final artifact `9602880501`.

The checked-in trust gate passed before the live attempt. It independently
verified the 10,430,368-byte raw artifact ZIP with SHA-256
`a0179de547024a544f46f47229e00996c9ac72b549a87ab6915f4df926e590bf`,
the checksum-manifest SHA-256
`d2e3a7e98eb82f44842495eae53315d428d6cb411bb5e5bd7d676ca6fd4781db`,
the exact five-file inventory, both Windows PE identities, all payload hashes,
and all five exact-attempt GitHub attestations.

The checked-in coordinator then consumed the version-scoped no-retry
reservation and launched the acceptance runner exactly once. The dedicated
fixture passed its entry-point and exact-process binding checks, and the
candidate server and helper started. The runner then failed closed at
`bind-initial-helper-readiness`: the helper session connected and matched its
hello state, but the reported initial disposable worker never appeared as an
exact-image direct child before the bounded deadline. No acceptance step or
foreground-arm request was published.

The coordinator terminal state is `runner-started-terminal`,
`retryAllowed: false`, and `reasonCode: watcher-handoff-missing`; the latter is
the expected coordinator consequence because the runner ended before it could
publish a foreground-arm marker. The first sanitized runner error is `Timed
out waiting for the initial disposable helper worker.` at
`scripts/test-windows-computer-use.ps1:3620`.

Cleanup completed without an issue: the exact runner, worker, candidate
server, candidate helper, and fixture exited; the ephemeral fixture executable
was removed; the selected loopback port was reusable; the recovery event was
released; and no owned process or listener remained. Computer Use was never
initialized, the candidate was not retried, and Chrome was not started.

The committed inventory contains only the sanitized start and terminal fields
plus a reduced diagnostic binding that hashes the original create-once
records, runner result, and private negative summary. Private configuration,
staged candidate bytes, raw logs, local paths, process or window identifiers,
command lines, usernames, tokens, and environment identifiers are excluded.
