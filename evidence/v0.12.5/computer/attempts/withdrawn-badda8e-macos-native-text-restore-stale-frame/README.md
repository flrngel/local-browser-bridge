# Withdrawn v0.12.5 macOS exact-candidate attempt

This directory preserves the failed macOS acceptance run for the exact v0.12.5
release candidate. The candidate was withdrawn, its protected publication job
was canceled without approval, and no v0.12.5 GitHub Release was created.
Nothing in this directory is evidence for a shipped release.

## Frozen candidate binding

- Source commit: `badda8ee969e2a5ade9a56a57ba797b14c44046f`
- Annotated tag object: `9bc4519e82d89e50433b1b034dc2731d646ef1f4`
- Tag: `v0.12.5`
- Deploy workflow run: `32615241833`, attempt `1`
- Release-candidate artifact: `9486916941`
- Artifact ZIP SHA-256:
  `31fb822e0f34a5b2e6a1d3b01785c2b0283970e9db8e56a03e4b5d8440ff55b0`
- `SHA256SUMS.txt` SHA-256:
  `6c33a9229de67da543ad3696e74495fd43fde3aebada12de9ce6c56924c9d85e`
- macOS archive SHA-256:
  `1b48a8ef92f67f7d3f548bcece8af364205192744dac4b47bda87352953ceb11`
- Packaged server SHA-256:
  `3e1e9c36bfef6d7759b41f0a242b8098d8c8741e0a3b8c1015e31379b20249fe`
- Packaged helper SHA-256:
  `d0f86127c04a8d75bdf12ced31fab059077d714934e86e3dc76d5d9a1505d46c`

Before either executable ran, two independent coordinator checks verified the
exact five-file inventory, canonical four-line manifest, every payload hash,
platform formats, source/tag/workflow/run-attempt identity, GitHub-hosted
runner identity, and all five GitHub attestations. Both macOS executables were
universal `arm64`/`x86_64`, reported version 0.12.5, and passed strict
structural code-signature checks.

## Result

`helper-results.json` is a schema-3 `failed-release-candidate` record. The rig
recorded 82 passing assertions and zero assertion failures before a fatal error;
that assertion count is not a passing result. Native `computer.typeText`
succeeded, independent fixture read-back proved that exactly 20 temporary ASCII
characters reached only the primary field, the genuine same-process sibling
remained unmodified, and all foreground, focused-window, cursor, and Space
invariants held. The following cleanup `computer.setValue` was refused before
dispatch with HTTP 409 `COMPUTER_STALE_FRAME`.

The rig requested a one-shot observation while its 10 FPS persistent share was
still active. The next stream frame correctly retired that unbound one-shot
before the mutation could dispatch. v0.12.4 contained the same timing race and
passed only because its cleanup happened before the next stream publication.
The repair belongs in the client-side evidence harness: consume a recent frame
from the same active share and keep the server's stale-frame authority checks
unchanged.

## Retained inventory

| File | Bytes | SHA-256 |
|---|---:|---|
| `computer-01-exact-window-observe.png` | 778851 | `ed745a8fa3d848b5ed43d6da8262ea8ae72ae07af1ccead8c32f841fd7afa2e2` |
| `computer-02-semantic-set-value.png` | 794958 | `962072bc743ad6cbb0a851e3966ab90f6f9e9770c5c3f6bb917aff2a701fcfca` |
| `computer-03-semantic-invoke.png` | 796845 | `28b84811f5f45c5dd0cbed80aa0c782170838f6931574a95d0ba9439db3b82d3` |
| `computer-04-persistent-scstream-start.png` | 662884 | `f6bf68d6dcf377667fbb805e7564ccbd4d4ad6bb36c161e1db94146995c36963` |
| `computer-05-live-share-pixel-action.png` | 662836 | `9cb668da5cdfc8548dbe2aae450af3030f3c11d325c12ca3dd56e721ca13354d` |
| `computer-06-persistent-share-resize.png` | 531962 | `f7e5894e44582d53df112459e086e16e137fb73db0927d748bbad3d92069c2bf` |
| `helper-results.json` | 19439 | `79b1b337f6f0669c0399f47eec474e7a6f570dc6a43e94f34e2f9fd0c2659269` |
| `helper-rig.log` | 10719 | `a290e8996936ecc7a31490bb2f4dcb29c5407b3e872dec418970108d05131ea9` |

All six screenshot records match their retained files. Visual review confirmed
that the images contain only the deterministic primary fixture; no sibling
window, desktop, unrelated application, personal content, or secret is
visible. Frames 4-6 show the macOS capture indicator, and frame 5 also shows
the bridge's synthetic pointer. The random native-text suffix, bearer token,
authorization data, user path, username, hostname, email address, and unrelated
URLs are absent from the screenshots, result, and log.

After the fatal error the log records the owned helper, server, and fixture
stopping and the rig scratch directory being removed. A separate current-state
check found no related process or listener. The run did not reach its normal
controlled-teardown and listener-closure assertions, and the sanitized failure
record intentionally does not retain the ephemeral port, so this directory
does not claim that the later protocol-cleanup suite passed.
