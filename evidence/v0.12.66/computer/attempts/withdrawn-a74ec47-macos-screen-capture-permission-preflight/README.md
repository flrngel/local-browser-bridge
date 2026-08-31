# Withdrawn macOS candidate: screen-capture permission preflight failure

This directory preserves the sanitized macOS negative result from workflow run
`33350856503`, attempt `2`. It is not release evidence.

The frozen v0.12.66 candidate was bound to source commit
`2dcc8eccebaed6837a3eb71012e4826d7e9de922`, release-candidate artifact
`9745800057`, and raw artifact ZIP SHA-256
`a74ec477f35de2e493c7e6e63ce979a3aad8f84d06e594bba05f12c7b80a17dc`.
The canonical checksum manifest SHA-256 was
`6972970276929bf23bdbed51975adbf95f562597c4f2532d9ac15f0c3049f5f4`,
and the packaged macOS archive SHA-256 was
`65c088e5b6c71ccc7911addf8c950f49122bd72e84dcfe4c1fb682c78a6d2762`.

The candidate trust binder and package-only extraction gates passed. The first
quiet-lane runner then failed closed at its preexisting screen-capture
permission preflight. Its exact fatal reason was `screen-capture permission
preflight: preexisting permission; no request was made`. The schema-9 result
recorded `failed-release-candidate`, 29 passed checks, one failed check, and 30
total checks. The result was captured at `2026-08-31T07:39:06.657Z`.

The failure occurred before candidate execution. The result records a null
lane start time, zero packaged-helper spawns, zero screenshots, a `notReached`
failure-diagnostic stage, and no dispatched action. A read-only post-failure
audit at `2026-08-31T07:41:47Z` found no candidate server, packaged helper,
acceptance runner, app-share process, or GNU Screen socket.

The responsible GUI application was Terminal. A read-only authorization audit
confirmed that Terminal already had Accessibility authorization but did not
have a Screen Capture authorization entry. The runner never requested that
permission and did not alter the host authorization state.

The retained generated files are byte-exact. `quiet/helper-results.json` has
SHA-256 `9f3076f158950574d7ee7b458d80769c021bfd3f07fd582147735473aacac876`,
and `quiet/helper-rig.log` has SHA-256
`ee8fe252535f8d57aa77cbc7f8bb75169f2fc7632510434ac5e2ed1fe7d6cfeb`.

The retained inventory is intentionally limited to this README and those two
sanitized runner outputs. It excludes candidate packages and executables,
the candidate binding, scratch material, absolute paths, credentials, control
channel material, and host authorization databases.

The quiet lane was not retried or resumed. No deliberate macOS lane, Windows
acceptance, stock-Chrome acceptance, evidence finalization, tag, publication,
or GitHub Release followed from this attempt. These files cannot be promoted,
combined with another attempt, relabeled as passing evidence, or reused by a
later workflow attempt. Continuing the release requires a full workflow rerun
with a new attempt, artifact, evidence set, and receipt.
