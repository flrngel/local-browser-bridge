# Withdrawn v0.12.30 Windows worker-ownership attempt

This directory preserves the sanitized terminal record for the Windows
acceptance attempt bound to source
`92f90b7e02c26ae9289a053928670e44745b0de5`, candidate workflow run
`32928403540` attempt `1`, and final artifact `9592511093`.

The candidate trust gate passed before the live attempt. It independently
verified the raw artifact ZIP SHA-256
`27830d6fe5732cb7d1e8eec0f80ad5050ad8ebf95e17e78f9d207e73996be58d`,
the checksum-manifest SHA-256
`c80c42db588f0ce121390e04893c041821aa67c6c8af796a64a85be9112c2632`,
the exact five-file inventory, and all five exact-attempt GitHub attestations.

The checked-in coordinator then failed closed at `worker-ownership`, before
runner launch or candidate execution. The retained worker bootstrap called
`ConvertTo-LowerHex` while verifying its staged lifetime-support assembly, but
that function was not present in the retained worker scope. The durable
coordinator result is `attemptState: not-started`, `retryAllowed: false`, and
`reasonCode: worker-ownership-unavailable`.

No server, computer helper, fixture, product listener, Computer Use action, or
stock-Chrome action occurred. The candidate was not retried. This directory is
negative diagnostic evidence only and cannot satisfy Windows, browser, release,
or publication acceptance.

The committed inventory contains only the sanitized start and terminal fields
plus a reduced diagnostic binding that retains the SHA-256 of each original
create-once record. Private configuration, staged
candidate bytes, local paths, process command lines, usernames, tokens, and raw
worker logs are excluded.
