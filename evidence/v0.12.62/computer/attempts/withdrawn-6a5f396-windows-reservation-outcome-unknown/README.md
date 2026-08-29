# Withdrawn v0.12.62 Windows reservation outcome-unknown attempt

The sole Windows acceptance attempt for the exact v0.12.62 release candidate
became terminal on 2026-08-29. The candidate was bound to source
`1c89c3dd43595dca2610a4622e35f875fcbab605`, workflow run `33275797498`
attempt `1`, artifact `9721601410`, raw artifact ZIP SHA-256
`6a5f39651d5f1c0fe3e8f7570cc495902b5bf6afeb760a9c3fd36fb777c36754`,
and checksum-manifest SHA-256
`db3966eadd78118fc976514422a7666f445f2d34867315a94c29bd2d0386af24`.

The Windows host retained the create-once schema-2 attempt reservation with
status `reserved-no-retry` and `retryAllowed: false`. A bounded read-only audit
found zero matching v0.12.62 coordinator directories, evidence directories,
exact candidate-image processes, port-17373 listeners, or stock-Chrome ledger
records. No retained record proves whether candidate execution began.

The checked-in protocol classifies any attempt with a persistent reservation
but no conclusive terminal record as `candidate-execution-unknown`. The exact
v0.12.62 candidate must therefore never be retried, resumed, or published.
Stock-Chrome acceptance did not run, and no v0.12.62 tag or public Release was
created.

The fresh macOS quiet and deliberate-concurrency results remain diagnostic
history only. They are not reused or relabeled as evidence for a later version.

This directory contains only the sanitized reservation and this explanatory
note. It contains no candidate bytes, credentials, absolute paths, screenshots,
raw coordinator data, or browser evidence.
