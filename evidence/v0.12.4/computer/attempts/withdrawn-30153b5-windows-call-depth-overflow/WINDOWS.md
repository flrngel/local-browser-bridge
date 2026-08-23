# Windows v0.12.4 fail-closed observation

FLRngel19 retained the exact failed evidence directory unchanged. The files are
not copied into this repository because the remote task transport returned a
sanitized report rather than a byte-preserving file channel. Their sizes and
SHA-256 digests are recorded here so the retained originals can be identified
without overstating this report as the evidence bundle itself.

| Relative path | Bytes | SHA-256 |
| --- | ---: | --- |
| `summary.json` | 8177 | `09ede23cb0d3ca59f51c66bf37229821f89029b257549d55785c5939b56ce207` |
| `fixture/fixture-events.ndjson` | 877 | `6e5c6344e4a1616278f758cb8b847f071e10c6b5084ac7c2e2d563c4ef7238af` |
| `fixture/fixture-ready.json` | 180 | `299498b8bd03cf761652d5d6b0ac14d3aedc8e8f43790ac0813c6a8fb21c00b0` |
| `fixture/fixture-state.json` | 1016 | `52b5244de07dfda7e8ab77901c7d63e114149fe1838453476a44037dabfa07fa` |
| `steps/01-protocol-bound-helper-readiness.json` | 7864 | `d2cf988552cbdb44e6aba5f75f3c2b11f6b39ef8557831273a408cc0fd7813c7` |

Observed result:

- `summary.passed` was `false`; all seven suites had been selected.
- The only completed step was protocol-bound helper readiness.
- One exact-image direct worker child matched the authenticated nonzero worker
  PID, helper session, interactive session, and `computer.status` round trip
  across two stable polls.
- Failure text was `The script failed due to call depth overflow.`
- No Windows screenshots were produced, so Chrome acceptance did not start.
- `cleanupIssues` was empty; token persistence scanning passed with no token
  retained or removed; the recovery event was released; no unrelated process
  was terminated.
- An independent post-run check found no candidate server/helper process and no
  listener on the selected run port or the product's default port.

The run preserved Windows and browser security behavior: it did not unblock a
download, bypass a warning, edit Chrome, install the extension, reuse v0.12.3
bytes, approve a deployment, or publish a release.
