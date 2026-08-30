# Withdrawn v0.12.63 macOS app-share stale-frame attempt

This is terminal negative evidence, not release evidence.

The candidate was built from exact main commit
`3439e633228ccad3e1b25456824afd482a4c932c` by GitHub Actions workflow run
`33281583008`, attempt `1`. The immutable candidate artifact ID was
`9723258152`; its raw Actions ZIP SHA-256 was
`7cfc4f040470bee29c16f384b46f4994a09cf4553cd062a8fb206d2aa1f351f3`.
The canonical checksum-manifest SHA-256 was
`c62cdf5d4bdaf6b1fc722a4fa0c87c664ffb31e5321e5e7ba54e2fa1a056d724`.

The macOS quiet lane passed all 207 checks. The deliberate-concurrency lane
then completed its 60-transition native quiet-seat gate and acknowledged the
single exact-app-share action for bundle
`dev.flrngel.local-browser-bridge.acceptance.app-share`, window
`LBB macOS Acceptance App Share`, and button `START APP-SHARE CHECK`. The
external action did not change foreground ownership, AX focus, the hardware
cursor, HID counters, or the active Space. The harness bound a strictly newer
frame from the same persistent share and accepted it as fresh at dispatch with
an estimated age of 1784.713 milliseconds. The product nevertheless returned
exact HTTP 409 `COMPUTER_STALE_FRAME` before a dispatch outcome could be
proven. Target fixture counters stayed unchanged.

The candidate was not retried. Windows acceptance, aggregate evidence,
publication, tag, and GitHub Release did not run. Candidate and fixture
processes stopped. Only the allowlisted quiet result, deliberate negative
result, logs, six fixture-only screenshots per lane, and the request/start
notification receipts are retained here. No completion receipt exists.

Version 0.12.64 tightens the post-handoff frame-authority handoff and retains
the server's exact rejection detail so this boundary cannot be misdiagnosed.
