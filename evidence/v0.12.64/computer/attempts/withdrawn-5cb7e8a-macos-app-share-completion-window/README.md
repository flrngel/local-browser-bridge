# Withdrawn v0.12.64 macOS app-share completion-window attempt

This is terminal negative evidence, not release evidence.

The candidate was built from exact main commit
`a6ace08c7e2197e14ff9bed5b048f76eafafd0f0` by GitHub Actions workflow run
`33284072219`, attempt `1`. The immutable candidate artifact ID was
`9723949829`; its raw Actions ZIP SHA-256 was
`5cb7e8a0ab8974366b7442dc7977f76dab460caf5524b8c15ace37926685757f`.
The canonical checksum-manifest SHA-256 was
`653355de7c92b266a9c6f9bf0f14958f7497e33fb0299aa95f727c2f87ae1504`.

The macOS quiet lane passed all 207 checks and its six screenshots passed
visual review. The deliberate-concurrency lane then completed its
60-transition native quiet-seat gate and acknowledged the single exact-app
action for bundle `dev.flrngel.local-browser-bridge.acceptance.app-share`,
window `LBB macOS Acceptance App Share`, and button
`START APP-SHARE CHECK`. The external action did not change foreground
ownership, AX focus, the hardware cursor, HID counters, or the active Space.

The v0.12.64 aged-frame fairness change worked: the harness bound a strictly
newer frame from the same persistent share and accepted it at dispatch with an
estimated age of 986.503 milliseconds. The product action dispatched, the
exact target postcondition advanced, and both product and independent
shared-seat boundaries remained quiet.

The app produced a create-once completion receipt. Its start receipt was
created at `2026-08-30T01:05:02.132Z`, the bound product action ran from
`2026-08-30T01:05:02.406Z` through `2026-08-30T01:05:13.848Z`, and the
completion receipt was created at `2026-08-30T01:05:13.897Z`. The product
action therefore took 11,442 milliseconds and the start-to-receipt interval
took 11,765 milliseconds. Both were inside the app and runner's 18-second
completion grace, but the watcher, receipt reader, and finalizer still enforced
an inconsistent 10-second action-to-completion interval. They rejected the
otherwise bound receipt, and the runner failed closed with
`completePublicationAcknowledged: false` after 102 passing checks.

The candidate was not retried. Windows acceptance, stock-Chrome acceptance,
aggregate evidence, publication, tag, and GitHub Release did not run. Candidate
and fixture processes stopped. Only the allowlisted quiet result, deliberate
negative result, logs, six fixture-only screenshots per lane, and the three
request/start/completion receipts are retained here.

Version 0.12.65 aligns every completion-receipt validator and finalizer with
the existing 18-second app and runner completion grace and adds a regression
fixture covering a valid action longer than ten seconds.
