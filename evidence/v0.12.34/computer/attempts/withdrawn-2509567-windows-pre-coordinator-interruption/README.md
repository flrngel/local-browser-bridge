# Withdrawn v0.12.34 Windows pre-coordinator interruption

This directory preserves the sanitized terminal record for the Windows
acceptance attempt bound to source
`25095671d1ed50e74281ec17d9ede8829c0d8483`, candidate workflow run
`32957505500` attempt `1`, and final artifact `9602880501`.

The candidate trust gate passed before the live Windows attempt. The macOS
quiet lane and deliberate-concurrency lane also passed separately. Those
results establish candidate provenance and macOS acceptance only; they do not
substitute for Windows acceptance.

The persistent Windows no-retry reservation was created for this exact
candidate. Execution was then interrupted before coordinator state progressed.
A bounded post-interruption observation found no v0.12.34 coordinator
directory, no v0.12.34 evidence directory, no candidate product process, and no
listener on port 17373. It also found no Computer Use action, Chrome action,
v0.12.34 tag, or v0.12.34 release.

Those absences are bounded observations made after the interruption. They are
not proof that the observed objects never existed outside that interval.

The attempt state remains `not-started`, but the persistent reservation is
`reserved-no-retry` and `retryAllowed` is `false`. The ambiguity cannot be
resolved by deleting or bypassing the reservation. Candidate v0.12.34 is
withdrawn and must not be retried or published.

The committed inventory is limited to this explanation, the byte-exact
reservation record, and a reduced diagnostic binding. Local paths, usernames,
host or environment identifiers, commands, process identifiers, tokens, and
other secrets are excluded from the diagnostic.
