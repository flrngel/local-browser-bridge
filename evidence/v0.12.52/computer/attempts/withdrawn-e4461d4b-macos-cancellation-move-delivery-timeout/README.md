# Withdrawn macOS candidate: cancellation move delivery timed out

This directory preserves the sanitized terminal record from the sole macOS
quiet-lane run for the v0.12.52 candidate. It is not release evidence.

The frozen candidate was bound to source commit
`c2ff85fe4f1cbea63d31c76a8751c45a91e5c426`, workflow run `33229820191`,
attempt `1`, release-candidate artifact `9708225443`, and raw artifact ZIP
SHA-256 `e4461d4bf7a49b6e8b96e20b28ccfb1c35c663a93c2f5bc5a810614788967952`.
The canonical checksum manifest SHA-256 was
`e5ea450b30ff877ef3718390a383b5aeb54409fdf4312332ab5b3c205cde985c`,
and the macOS archive SHA-256 was
`8dd29d2d16a864d9b5f9e74e841df9f019f97d83d49fdf20cb04db08b0c3d63a`.

The exact packaged quiet lane passed 158 checks before the cancellation
boundary. It proved the clean detached source, exact candidate bytes,
universal architectures, strict code signatures, the native persistent share,
semantic actions, two background pixel clicks, native text delivery and
restoration, the 820x520 resize, and unchanged foreground, focus, hardware
cursor, keyboard, pointer, and active-Space boundaries. All six retained
screenshots were independently reviewed and contain only the expected primary
quiet-lane fixture.

The lane then started its exact 2,000 ms `computer.move` cancellation request
from a current same-share resized frame. The target application's mouse-move
counter did not advance within the bounded 10,000 ms dispatch-proof interval,
so the harness failed closed with
`fixture target-routed cancellation move dispatch timed out`. The sanitized
failure result records 158 passed checks, zero failed assertions, unchanged
functional fixture counters, the prior same-process sibling restored as main
and focused, no shared-seat input, and no retained raw pointer data. The
runner terminated the helper, server, and fixture and removed its scratch
directory.

The acceptance protocol makes this candidate terminal. It was not retried.
The deliberate macOS lane, app-share UI action, Windows native acceptance,
stock-Chrome acceptance, evidence finalization, tagging, publication, and
GitHub Release creation did not occur. These files cannot be promoted,
combined with another attempt, or reused by a later version.
