# Withdrawn v0.12.62 Windows recovery share-start attempt

The sole Windows acceptance attempt for the exact v0.12.62 release candidate
was reserved and executed on 2026-08-29. The trust gate passed for source
`1c89c3dd43595dca2610a4622e35f875fcbab605`, workflow run `33275797498`
attempt `1`, artifact `9721601410`, raw artifact ZIP SHA-256
`6a5f39651d5f1c0fe3e8f7570cc495902b5bf6afeb760a9c3fd36fb777c36754`,
and checksum-manifest SHA-256
`db3966eadd78118fc976514422a7666f445f2d34867315a94c29bd2d0386af24`.

Both the initial and post-arm topology checks passed. They bound the
authenticated controller to the launched supervisor and the authenticated
worker to one exact-image direct child in the interactive session.

The fresh foreground-arm request was published and the exact sentinel action
was accepted once. The fixture recorded exactly one left-button down and one
left-button up, matching request and acknowledgement generations, stable
foreground, focus, cursor, input desktop, and arm-to-baseline continuity.

The baseline exact-window `computer.observe` and its sanitized screenshot step
both passed, including the v0.12.62 future-compositor-timestamp boundary. The
runner then failed closed in `recovery-suite`: `computer.share.start` returned
`COMMAND_OUTCOME_UNKNOWN` because the helper connection ended after the command
was enqueued, so the command outcome is unknown. No later native suite step or
stock-Chrome acceptance ran.

The persistent attempt reservation is `reserved-no-retry`, and the coordinator
terminal record is `failed-closed` with reason `runner-summary-failed`. This
candidate must not be retried or resumed.

Cleanup reported no issues, removed the ephemeral fixture executable, released
the recovery event, and left no task-owned candidate process or listener. No
v0.12.62 stock-Chrome claim was created. Computer Use session memory was cleared
after the single action, and the process-scoped GitHub token state was restored.
The sealed macOS aggregate was not reproduced or modified.

This directory contains only the runner-generated sanitized summary, fixture,
operator and step records, the sanitized coordinator terminal record, and this
explanatory note. It contains no candidate bytes, credentials, absolute paths,
raw screenshots, or browser evidence.
