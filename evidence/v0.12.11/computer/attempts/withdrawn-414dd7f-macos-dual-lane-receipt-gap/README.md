# Withdrawn v0.12.11 candidate: macOS dual-lane receipt gap

Version 0.12.11 was withdrawn before any candidate executable ran. Its own
development and SOTA policies required fresh, non-mergeable `quiet` and
`deliberate-concurrency` macOS runs, while the publication receipt exposed only
one `macosResultSha256`. The documented command also ran only the concurrency
lane. One digest could not authenticate both required result files without
merging evidence that the policy explicitly kept separate.

GitHub Actions run `32643899149`, attempt `1`, built source
`414dd7f4a433147326a8a8162edf31db5011c2ce` from annotated tag object
`f983495c80c0f222cb99f88ecaacc6ef8c75d201`. Assembly job `97206852540`
created release-candidate artifact `9494521879`. An independent read-only trust
audit passed the exact five-file inventory, canonical LF manifest, payload
formats and hashes, and all five GitHub attestations. Both attestation attempt
fields resolved to the same exact workflow attempt. These trust checks licensed
execution; they did not repair the acceptance-policy gap and are not runtime
evidence.

The waiting publish job was canceled before it ran any step. No v0.12.11
Release or draft was created. The frozen candidate was never executed on
macOS, Windows, or Chrome and must not be retried or reused. Version 0.12.12
replaces the single result with separately bound lane results and a canonical
aggregate.

The sanitized, machine-readable candidate identity is in
[`candidate-metadata.json`](candidate-metadata.json). Raw private audit
materials are intentionally not committed.
