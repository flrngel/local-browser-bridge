# Withdrawn v0.12.55 local macOS move-observer diagnostic

This is terminal local negative evidence, not release evidence.

The source-built v0.12.55 package came from clean detached commit
`a74c910` after the full local deployment gate passed. Its local
checksum-manifest SHA-256 was
`34d2b90d321308551d0c8d60a3277b8aab5642996e38469558644e456f37a1ea`,
and its macOS universal archive SHA-256 was
`2fc6dadfa0b42f5199470b1fc4917bb7669c7140847f1e740311cc9fc61974c7`.
The release harness used explicitly synthetic local binding identifiers. No
GitHub candidate workflow or artifact existed.

The quiet diagnostic passed 158 checks, including the new target-only
make-key commit and repeated exact receiver proofs, then its evidence-only
`FixtureView.mouseMoved` counter remained `61 -> 61` for ten seconds. Review
found that the fixture set `NSWindow.acceptsMouseMovedEvents` but installed no
`NSTrackingArea`; its view override therefore depended on the current
first-responder chain instead of directly observing movement over the exact
view. That explains both the repeated false negative and the single earlier
incidental pass. The product action remained unacknowledged and all retained
foreground, focus, cursor, Space, HID, sibling-restoration, and functional
fixture invariants held.

The attempt was not retried. No GitHub candidate, deliberate macOS lane,
Windows, Chrome, aggregate, publication, tag, or Release existed. Candidate
and fixture processes stopped and scratch storage was removed. Only the
allowlisted sanitized result, log, and six fixture-only screenshots are
retained here.

Version 0.12.56 gives the exact fixture view a bounded `.mouseMoved`,
`.activeAlways`, `.inVisibleRect` tracking area owned by that view, removing
first-responder ambiguity while leaving the product route and safety boundary
unchanged.
