# Withdrawn v0.12.59 Windows helper-readiness attempt

The sole Windows acceptance attempt for the exact v0.12.59 release candidate
was reserved and started on 2026-08-29. The trust gate passed for source
`10dc3f6413d87444ac4af4005f167bd9de810df0`, workflow run `33264929249`
attempt `1`, artifact `9718475730`, raw artifact ZIP SHA-256
`ece060c68be4e8e8ef49de6ef6c7d52ab69082a37e2fcf41c5c0e11e4e3f1313`,
and checksum-manifest SHA-256
`e68f0fd26bae5ab7bd1a5df7c198ba2653ec5ec6e58d9f680ccbc9c0d4206658`.

The native runner failed closed at `bind-initial-helper-readiness` after 273
polls. The authenticated helper connection and hello state matched, and the
reported controller matched the exact launched supervisor. The reported
worker was present in protocol state, but its exact live full-image-path check
did not match the candidate helper. Toolhelp reported no conflicting exact-image
direct child. Stable readiness therefore remained at zero polls and no product
acceptance action was dispatched.

The runner exited nonzero with a sanitized schema-2 summary. The persistent
attempt reservation is `reserved-no-retry`, and the coordinator terminal record
is `failed-closed` with reason `runner-summary-failed`; this candidate must not
be retried or resumed.

The foreground-arm request was never published. Computer Use was not
initialized, the sentinel was not clicked, stock-Chrome acceptance was not
claimed or started, and no Chrome UI action occurred. Cleanup reported no
issues, removed the ephemeral fixture executable, released the recovery event,
left no task-owned candidate process, and left no listener on the selected
loopback port. The sealed macOS evidence was not reproduced or modified.

This directory contains only the runner-generated sanitized summary and fixture
records, the sanitized coordinator terminal record, and this explanatory note.
It contains no candidate bytes, credentials, absolute paths, raw screenshots,
or browser evidence.
