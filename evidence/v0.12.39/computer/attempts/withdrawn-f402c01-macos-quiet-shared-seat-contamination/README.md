# Withdrawn v0.12.39 macOS quiet-lane attempt

This directory preserves the sanitized terminal evidence from the sole macOS
quiet lane executed against release-candidate workflow run `33051081816`,
attempt `1`, artifact `9637896509`, and source
`f402c019edc1892ac2f040c5d6e4c60a8fe46e4a`.

The candidate passed source, artifact, provenance, package, architecture,
signature, permission, stream, semantic, pixel, resize, native text,
cancellation, replay, recovery, and stale-frame checks. It then failed closed
at the final cancellation/stop independent boundary because the native monitor
observed shared-seat pointer and keyboard activity plus foreground/focus
changes. The result is `failed-release-candidate` with 177 passing assertions
and one failed assertion.

The six retained screenshots were visually reviewed and contain only the
purpose-built v0.12.39 quiet-lane fixture. The retained JSON and log contain no
token, authorization header, GitHub credential, email address, user/home path,
temporary path, loopback endpoint, or unrelated screen content. Candidate
downloads, extracted packages, attestations, scratch state, and credentials
are not retained.

This is negative evidence only. It cannot satisfy a release gate and must not
be retried, relabeled, merged with another lane, or used to authorize Windows,
stock-Chrome, tagging, or publication for this workflow attempt.
