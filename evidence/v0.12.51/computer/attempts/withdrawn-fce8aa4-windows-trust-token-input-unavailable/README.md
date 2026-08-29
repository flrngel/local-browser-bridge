# Withdrawn v0.12.51 Windows trust-token input attempt

This directory preserves the sanitized terminal record for the Windows
acceptance task bound to source
`2c4ce50f403e48683955e3d3a71139a072c860b8`, candidate workflow run
`33227697546` attempt `1`, and release-candidate artifact `9707553849`.

The checked-in Windows release-candidate verifier was invoked exactly once.
It created a fresh owner-private destination, cloned and validated the exact
clean source, and then reached its required least-privilege GitHub-token prompt.
The captured noninteractive invocation had no inherited token and no writable
input session, so it could not advance. The exact task-owned trust child was
stopped, and the durable capture recorded exit code `-1`, zero stdout bytes,
zero stderr bytes, and no exact trust success line. The verifier was not rerun.

The expected raw artifact ZIP binding was 16,136,194 bytes with SHA-256
`fce8aa47412dbbca8d605efc0c2962cc950a47d328b0f007b5981e0e1e30e2d7`.
The verifier never reached the GitHub workflow or artifact API, downloaded no
artifact byte, extracted no payload, created no `candidate-binding.json`, and
verified no manifest or attestation. The expected artifact binding is retained
only as task input, not as a successful trust result.

Windows native acceptance and stock-user Chrome acceptance did not start. No
per-version reservation, coordinator, runner, candidate product process,
listener, foreground handoff, Computer Use initialization or action, Chrome
control initialization, Chrome mutation, screenshot, or browser evidence
existed. This v0.12.51 candidate is terminal and must never be retried.

Cleanup moved only the exact validated task-owned trust destination and capture
root to the Windows Recycle Bin after confirming current-user ownership, exact
bounded inventories, and zero reparse points. No relevant process or listener
remained. The committed inventory contains only reduced sanitized facts. It
excludes local paths, usernames, process identifiers, command lines,
credentials, tokens, environment identifiers, source clones, candidate bytes,
and raw logs.
