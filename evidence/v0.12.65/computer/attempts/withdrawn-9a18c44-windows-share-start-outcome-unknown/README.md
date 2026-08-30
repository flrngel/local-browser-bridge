# Withdrawn Windows attempt: share-start outcome unknown

This directory records the terminal, non-retryable Windows computer attempt for Local Browser Bridge v0.12.65. It is diagnostic failure evidence, not release-acceptance evidence. The candidate attempt is withdrawn and must not be retried or reused.

## Immutable binding

- Source commit: `d5d025d7f93a976c29a87a74d04f5839094323f2`
- Workflow run and attempt: `33286239632`, attempt `1`
- Artifact ID: `9724620232`
- Artifact ZIP SHA-256: `9a18c44d3e7f91b1ea502253ce6ee532e43a0e79ddf85a423bf4bd2ddeb2fee8`
- Checksum-manifest SHA-256: `61fc1408adb8dd6e450741b9cdd8b9da28c988da66a561fc989b9dace62727df`
- Summary SHA-256: `da5c72f0ee3b6f00dbdf4ba54a403b256d5e513ba06badd34ec2d56016605f97`
- Reservation: schema `2`, status `reserved-no-retry`, coordinator instance `a475603fb365409091c0ec1bf117ece8`, and `retryAllowed: false`

## One-shot sentinel handoff

Request and receipt ID `2639954a09ce4db7bb582e312feada53` completed exactly once. The evidence records one request, one acknowledgement, one left-mouse down, and one left-mouse up. No second UI action was attempted.

## Terminal failure

The coordinator failed closed at `runner-finished` with reason code `runner-summary-failed` and did not permit a retry. The `recovery-suite` failure recorded in `summary.json` is:

> Loopback command computer.share.start returned COMMAND_OUTCOME_UNKNOWN: Computer helper connection ended after the command was enqueued; its outcome is unknown: computer.share.start

The command outcome is unknown, so this attempt cannot support an acceptance claim.

## Cleanup and exclusions

Post-run verification found zero exact candidate product processes and zero listeners on the runner-owned loopback port. The summary records zero cleanup issues, verified non-persistence of the bearer token, zero removed token-bearing evidence files, release of the recovery event, removal of the fixture executable, and no termination of unrelated processes.

Stock Chrome never started. Stock-Chrome acceptance, browser evidence collection, tagging, and release publication did not run. This directory intentionally excludes screenshots, candidate binaries, private configuration, staged source, logs, credentials, absolute paths, token material, and browser evidence. It contains only the sanitized summary, fixture records, operator records, step records, and coordinator terminal-failure record.
