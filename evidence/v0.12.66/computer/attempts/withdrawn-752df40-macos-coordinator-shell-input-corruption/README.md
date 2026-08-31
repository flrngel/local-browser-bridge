# Withdrawn macOS candidate: coordinator shell input corruption

This directory preserves the sanitized quiet-lane diagnostic record from
workflow run `33350856503`, attempt `6`. It is not release-acceptance evidence.

The frozen v0.12.66 candidate was bound to source commit
`2dcc8eccebaed6837a3eb71012e4826d7e9de922`, release-candidate artifact
`9753424775`, and raw artifact ZIP SHA-256
`752df40c876db77115b7b4928b9bea440526121a5e5447d706fbf0f75361598b`.
The artifact ZIP contained `16140640` bytes. The canonical checksum manifest
SHA-256 was
`8236f9e36ddb9237b255878b62e6e6f5d50f57ecaecec6ed7a12db8e79a369f5`,
and the packaged macOS archive SHA-256 was
`81f0aefecd60672f0345b3e57e97d4e1129f5add5aed7bf6d3a2912663e0d80c`.

The fresh packaged quiet runner completed and wrote a schema-9
`passed-release-candidate` result. It recorded 207 passing assertions, zero
failed assertions, the requested `quiet` pointer lane, no concurrent or unknown
shared-seat activity, and no app-share handoff request. The result SHA-256 is
`8e06660a66e977424e5953c3e0babd5065bdba276ecc4a924ec22a6bc56fd1bd`.
Its embedded schema-3 release-candidate binding matches the source, run,
attempt, artifact, artifact ZIP, and checksum manifest above.

All six retained screenshots passed independent forensic visual review. They
contain only the deterministic primary fixture and show, in order, the quiet
baseline, exact v0.12.66 semantic value, completed semantic action, persistent
SCStream, one background pixel click, and the settled 820x520 resize. No sibling
window, desktop, unrelated application, personal content, credential, crop, or
stale frame is visible.

The candidate was nevertheless withdrawn before the quiet-lane postvalidation
and review gate completed. After the runner returned, terminal input to the
required persistent coordinator shell was corrupted while postvalidation was
being entered. The shell exited and its GNU Screen session terminated. The
coordinator therefore did not complete the exact inventory and screenshot-hash
postvalidation, did not create or verify the operator review manifest, and did
not accept a quiet review digest in that shell.

The 207/207 runner result cannot replace those coordinator gates. No deliberate
lane started, no app-share window or button was accessed, no app-share action
occurred, and no dual-lane macOS aggregate was created. Windows native and stock
Chrome acceptance, final evidence assembly, tagging, release publication, and
post-release verification did not follow from this attempt. These outputs must
not be promoted, combined with another workflow attempt, relabeled as complete
macOS acceptance, resumed, or reused.

## Retained inventory

| File | Bytes | SHA-256 |
|---|---:|---|
| `quiet/computer-01-exact-window-observe.png` | 787768 | `59a6627fb6442e21de9f4028d91c9571a5ef72c6e454ac717855a296b2c8c9e0` |
| `quiet/computer-02-semantic-set-value.png` | 805676 | `872476ed7124690daf9a21c94092f1d51fcc5fc5304be887b1896c879fa30c4a` |
| `quiet/computer-03-semantic-invoke.png` | 807790 | `02d108897e061f6db1382b05e9991d882f26c25ba1458a7955d2507be9282bd4` |
| `quiet/computer-04-persistent-scstream-start.png` | 677561 | `1cf976cfbe41a967d1a5a245bfaf7502c315f217f5786caabe183094737beb13` |
| `quiet/computer-05-live-share-pixel-action.png` | 678279 | `b3f19dd6f6df5201eb742c4749d76c62f7e1ce104428ec4357b83b1ac3c42bdc` |
| `quiet/computer-06-persistent-share-resize.png` | 542808 | `3d60d732eaccbe5c1768c41104ca3791bc5f3d27d31bf2e3e380c7ec0fe78b29` |
| `quiet/helper-results.json` | 125439 | `8e06660a66e977424e5953c3e0babd5065bdba276ecc4a924ec22a6bc56fd1bd` |
| `quiet/helper-rig.log` | 36844 | `9b37af30169af03c21bf0003900ea58178426d2e2b6b18948868c5abe7e88d67` |

The result and log contain no bearer token, authorization value, user or home
path, hostname, email address, signed URL, command-line secret, or unrelated
content. Each PNG is an ordinary metadata-free image whose dimensions, byte
length, and SHA-256 match its result record. The retained set intentionally
excludes the candidate package and binaries, the standalone candidate-binding
file, scratch and socket diagnostics, credentials, absolute paths, control
channel material, corrupted or incomplete shell input, and any operator review
artifact that was never validly completed.
