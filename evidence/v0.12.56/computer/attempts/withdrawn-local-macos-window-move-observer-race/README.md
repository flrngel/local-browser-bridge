# Withdrawn v0.12.56 local macOS window-move observer diagnostic

This is terminal local negative evidence, not release evidence.

The source-built v0.12.56 package came from clean detached commit
`0e69d68` after the full local deployment gate passed. Its local
checksum-manifest SHA-256 was
`de672a1af4db2cc7cfd5aa404cb4d922152d430b25c2a54fc8088ffc50c278fa`,
and its macOS universal archive SHA-256 was
`4ce3a384e00a2db0dd379155651f16527be4758413863dbc1e7033f2bfab52a7`.
The release harness used explicitly synthetic local binding identifiers. No
GitHub candidate workflow or artifact existed.

The first quiet diagnostic passed 158 checks, including exact receiver proofs
and an exact-view `.activeAlways` tracking area, then its evidence-only
`FixtureView.mouseMoved` counter remained `61 -> 61` for ten seconds. The
product action remained unacknowledged and all retained foreground, focus,
cursor, Space, HID, sibling-restoration, and functional fixture invariants
held. A separate unretained diagnostic source commit added only early-response
logging; the unchanged package then passed 207 of 207 checks with target-side
move delivery observed as `61 -> 62` after 2,331 milliseconds. This established
that the view callback was a timing-dependent observation boundary rather than
a deterministic exact-window delivery receipt.

The failed attempt was not retried. No GitHub candidate, deliberate macOS
lane, Windows, Chrome, aggregate, publication, tag, or Release existed.
Candidate and fixture processes stopped. Only the allowlisted sanitized result,
log, and six fixture-only screenshots are retained here.

Version 0.12.57 moves evidence-only delivery instrumentation to the exact
fixture window's `sendEvent` boundary and checks the event's window number
before incrementing the bounded counter. The view callback remains ordinary
AppKit behavior, while the release gate no longer depends on later tracking-area
dispatch timing.
