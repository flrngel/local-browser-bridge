# Withdrawn v0.12.57 local macOS cancellation-frame diagnostic

This is terminal local negative evidence, not release evidence.

The source-built v0.12.57 package came from clean detached commit
`fcf5a31` after the full local deployment gate passed. Its local
checksum-manifest SHA-256 was
`45e0d9ea4dc05ef619382ff44776c4c84ffc2c8e4ec6e8892443773aa9dbc000`,
and its macOS universal archive SHA-256 was
`9d35962ba5f0280d3d81b50bbaf6588f8ecad1c27226fc6f6c51c1e367b52777`.
The release harness used explicitly synthetic local binding identifiers. No
GitHub candidate workflow or artifact existed.

The first quiet diagnostic passed 158 checks, including exact-window
`NSWindow.sendEvent` instrumentation, then the cancellation-bound move did not
advance its target-side counter within ten seconds. All retained foreground,
focus, cursor, Space, HID, sibling-restoration, and functional fixture
invariants held. A separate unretained diagnostic source commit added only an
early-response binding and reproduced the failure as exact HTTP 409
`COMPUTER_STALE_FRAME`. The last validated share sample had become stale while
the harness queried share status and captured its fixture/system baselines;
the product therefore correctly refused the move before native dispatch.

The failed attempt was not retried. No GitHub candidate, deliberate macOS
lane, Windows, Chrome, aggregate, publication, tag, or Release existed.
Candidate and fixture processes stopped. Only the allowlisted sanitized result,
log, and six fixture-only screenshots are retained here.

Version 0.12.58 captures the read-only fixture and system baselines first,
then obtains a strictly newer exact-share frame and immediately starts the
bound move. It also races target-side delivery against an early command
response so any future pre-dispatch refusal is reported exactly instead of
being mislabeled as a native-delivery timeout.
