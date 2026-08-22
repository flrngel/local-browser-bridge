# Withdrawn macOS candidate: fresh-share recovery refusal

This directory preserves the exact sanitized negative result from tagged
candidate commit `a52d7610689f62f7ec568ba24455c73abca2f3d4`. It is not release
evidence, and version `0.12.2` was not published.

The candidate was cryptographically bound to the canonical release manifest
whose SHA-256 was
`f675892323b816dc30d4df42f467e1128efc247c5a6d0db4b218f67e14b5c700`.
The packaged macOS archive matched SHA-256
`063596241accc544c7e552eb7c8436590e36bf0be02757100c2bec84d0718511`.

The harness passed 160 checks and retained six screenshots before failing
closed. It proved cancellation, duplicate-call refusal, conservative unknown
outcome handling, authority teardown, explicit one-shot observation recovery,
and stale-frame refusal. A subsequent fresh `computer.share.start` was then
rejected with `COMPUTER_INVALID_OBSERVATION` because the recovered one-shot
observation was still interpreted as carrying revoked share authority.

The failure was not bypassed and the candidate was not approved. The JSON,
log, and screenshots are retained byte-for-byte so the recovery-state fix can
be regression-tested without rewriting the historical result.
