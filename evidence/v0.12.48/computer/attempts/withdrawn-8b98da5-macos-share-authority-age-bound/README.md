# Withdrawn macOS candidate: post-receipt frame-age bound

This directory preserves the sanitized diagnostic record from the sole
v0.12.48 candidate. It is not release evidence.

The frozen candidate was bound to source commit
`c08f673790179c5fc1f32dbd0b843585fa31dc0f`, workflow run `33220576267`,
attempt `1`, release-candidate artifact `9705227058`, and raw artifact ZIP
SHA-256 `8b98da5e53963700f16fb5aeb7e9514dbd2a92723afd3a056b3f064df3f718df`.
The canonical checksum manifest SHA-256 was
`895be72638d61f70674fee10576b6867b1cd0d8d0ba75f1ab2e4e8886828498a`,
and the macOS archive SHA-256 was
`c8c0ac8cad113339f35ac51b3565a5e1b3ab414e042bdc81d1476ba5db29662b`.

The fresh quiet lane completed 207/207 checks with result SHA-256
`6cdc34bf8f3aeba53426511f1fedf870742de6869fde9d4a620a2b9e7af0eb74`.
Its six screenshots passed mandatory visual review. The deliberate lane then
reached the separately authorized exact-app-share handoff. The exact
`lbb-app-share-start` button was pressed once, its start receipt was observed,
the button became disabled, and the Computer Use session was reset
immediately afterward. Shared foreground, focus, cursor, HID counters, and
active Space all remained unchanged.

The candidate was withdrawn before product action dispatch because the
post-receipt evidence gate required a newly advanced share frame to have an
estimated age no greater than 1,000 ms. On this valid persistent SCStream the
six distinct successor candidates had a minimum estimated age of
1,040.609619140625 ms. The 5-second bounded refresh made 49 polls and rejected
all six candidates only as `staleAge`, then failed closed with
`fresh share action authority timed out`. The deliberate result therefore has
status `failed-release-candidate`, records 90/90 checks before the terminal
failure, and has SHA-256
`4be42d9c38d0638bf84c75bc191f0b2b62cd33dc6e4b9de57ebb95b8dc76bd27`.
No product action was dispatched and no target postcondition was claimed.

The acceptance protocol makes this failure terminal for the exact candidate.
The candidate was not retried. Windows native acceptance, stock Chrome
acceptance, evidence finalization, tagging, publication, and GitHub Release
creation did not start. The retained eighteen lane files are sanitized harness
output only and cannot be promoted, combined with another attempt, or reused
by a later version.
