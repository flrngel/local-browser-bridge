# Withdrawn v0.12.61 Windows WGC timestamp-ahead attempt

The sole Windows acceptance attempt for the exact v0.12.61 release candidate
was reserved and executed on 2026-08-29. The trust gate passed for source
`b7b3f6a07196d4600cdfdc3730853630a4afa0e3`, workflow run `33271808677`
attempt `1`, artifact `9720457547`, raw artifact ZIP SHA-256
`619e04307360074d59dd569132ce675886e8fc873a05dc8e08d4f91cda63fa70`,
and checksum-manifest SHA-256
`8e8eda4f5ff8f82702371d4241463fcd8466de694de40fe119edaa99fbfe5060`.

Both the initial and post-arm topology checks passed. They bound the
authenticated controller to the launched supervisor and the authenticated
worker to one exact-image direct child in the interactive session.

The fresh foreground-arm request was published and the exact sentinel action
was accepted once. The fixture recorded exactly one left-button down and one
left-button up, matching request and acknowledgement generations, stable
foreground, focus, cursor, input desktop, and arm-to-baseline continuity.

The runner then failed closed at `baseline-status-and-observation`. The first
`computer.observe` returned `COMPUTER_CAPTURE_FAILED` with the sanitized message
`The WGC compositor timestamp is ahead of the monotonic clock`. No later native
suite step or stock-Chrome acceptance ran.

The persistent attempt reservation is `reserved-no-retry`, and the coordinator
terminal record is `failed-closed` with reason `runner-summary-failed`. This
candidate must not be retried or resumed.

Cleanup reported no issues, removed the ephemeral fixture executable, released
the recovery event, left no task-owned candidate process or listener, and left
no v0.12.61 stock-Chrome claim. Computer Use session memory was cleared after
the single action. The sealed macOS aggregate was not reproduced or modified.

This directory contains only the runner-generated sanitized summary, fixture,
operator and step records, the sanitized coordinator terminal record, and this
explanatory note. It contains no candidate bytes, credentials, absolute paths,
raw screenshots, or browser evidence.
