# Withdrawn macOS candidate: coordinator shell interruption

This directory preserves the sanitized macOS diagnostic record from workflow
run `33350856503`, attempt `3`. It is not release evidence.

The frozen v0.12.66 candidate was bound to source commit
`2dcc8eccebaed6837a3eb71012e4826d7e9de922`, release-candidate artifact
`9750103384`, and raw artifact ZIP SHA-256
`95c002c369cf811a8f4bf289d993ef4f152e188e7a92bb821e07ad8cd729225c`.
The canonical checksum manifest SHA-256 was
`d282d2863b3d5466e181cb010aeccdfb9e56e9b2e36a7801b9eefffc949aee84`,
and the packaged macOS archive SHA-256 was
`71c836b856358d45eccee2973577396f898759d3983f8325fdde15d99d2b1b9a`.

The fresh packaged quiet lane passed 207 of 207 checks with result SHA-256
`4c6864ec0d4c6543d738e2e19ec6b69e66980ea1b6fbd932afa714d09341a9ad`.
All six screenshots passed mandatory visual review: the primary fixture alone
showed the quiet lane, exact v0.12.66 semantic value, completed semantic action,
persistent stream, one background pixel click, and settled 820x520 resize.

The candidate was nevertheless withdrawn before Phase 2 began. While preparing
the separate exact-app control channel, the coordinator ran a read-only session
log search in the required persistent Bash shell. The `rg` executable was not
available. Under the shell's active `set -euo pipefail` policy, the resulting
command-not-found status terminated the shell and its GNU Screen session. The
quiet review digest had already been entered, but the acceptance contract
requires the same private shell to remain open across all three phases and makes
an interruption terminal.

No deliberate-lane runner started, no app-share window or button was accessed,
and no app-share UI action occurred. At `2026-08-31T08:25:09Z`, the coordinator
confirmed that no Screen socket and no candidate server, helper, acceptance
runner, or app-share process remained. No macOS aggregate, Windows acceptance,
stock-Chrome acceptance, evidence finalization, tag, publication, or GitHub
Release followed from this attempt. The exact Actions artifact was subsequently
deleted so that it cannot be selected or published.

The retained record is intentionally limited to this README and the eight
allowlisted quiet-lane outputs. It excludes packages, executables, scratch and
socket diagnostics, absolute paths, credentials, control-channel material, and
the operator-only review manifest. The separate static Windows trust preflight
had passed, but it was not live Windows acceptance and is not included here.

The lane was not retried or resumed. These retained files cannot be promoted,
combined with another attempt, relabeled as passing evidence, or reused by a
later workflow attempt.
