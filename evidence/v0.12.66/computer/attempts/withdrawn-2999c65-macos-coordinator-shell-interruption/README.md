# Withdrawn macOS candidate: coordinator shell interruption

This directory preserves the sanitized macOS diagnostic record from workflow
run `33350856503`, attempt `1`. It is not release evidence.

The frozen v0.12.66 candidate was bound to source commit
`2dcc8eccebaed6837a3eb71012e4826d7e9de922`, release-candidate artifact
`9743748203`, and raw artifact ZIP SHA-256
`2999c652fb646aa1b93bb509df51eb85073e2fa7e2a97838e82013d40be7f7be`.
The canonical checksum manifest SHA-256 was
`f83df395e310c48e06539dcf067bef31f67293152da392449319114ef7c32ea6`,
and the packaged macOS archive SHA-256 was
`314ff2ff1a93b2c0aaf4be44a4d5accab7cb569e356488c9a3b5da81968b4f5c`.

The fresh packaged quiet lane passed 207 of 207 checks with result SHA-256
`650167a46bbf9829b255669641c5182859de5314e1ad36452b419705000d80b3`.
All six screenshots passed mandatory visual review: the primary fixture alone
showed the quiet lane, exact v0.12.66 semantic value, completed semantic action,
persistent stream, one background pixel click, and settled 820x520 resize.

The candidate was nevertheless withdrawn before Phase 2 began. While preparing
the separate exact-app control channel, the coordinator ran a read-only socket
inventory pipeline in the required persistent Bash shell. The queried directory
did not exist. Under the shell's active `set -euo pipefail` policy, that
nonzero `find` status terminated the shell and its GNU Screen session. The quiet
review digest had already been entered, but the acceptance contract requires the
same private shell to remain open across all three phases and makes an
interruption terminal.

No deliberate-lane runner started, no app-share window or button was accessed,
and no app-share UI action occurred. At `2026-08-31T04:29:05Z`, the coordinator
confirmed that no Screen socket and no candidate server, helper, acceptance
runner, or app-share process remained. No macOS aggregate, Windows acceptance,
stock-Chrome acceptance, evidence finalization, tag, publication, or GitHub
Release followed from this attempt.

The retained record is intentionally limited to this README and the eight
allowlisted quiet-lane outputs. It excludes packages, executables, scratch and
socket diagnostics, absolute paths, credentials, control-channel material, and
the operator-only review manifest. The separate static Windows trust preflight
had passed, but it was not live Windows acceptance and is not included here.

The lane was not retried or resumed. These retained files cannot be promoted,
combined with another attempt, relabeled as passing evidence, or reused by a
later workflow attempt.
