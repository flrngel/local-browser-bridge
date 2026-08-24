# Withdrawn v0.12.20 macOS deliberate-pointer attempt

This directory preserves the failed `deliberate-concurrency` macOS acceptance
lane for the exact v0.12.20 release candidate. The lane failed closed at its
separately authorized shared-pointer handoff. Nothing in this directory is
release-passing evidence, and the retained candidate must not be resumed,
retried, or reused.

## Frozen candidate binding

- Source commit: `3bdb972446fc273645408ac93d0e9d28b3f712f5`
- Annotated tag object: `c66919c9dfa156f25e46e4465c1c163dec479264`
- Tag: `v0.12.20`
- Deploy workflow run: `32723416074`, attempt `1`
- Release-candidate artifact: `9519086217`
- Artifact ZIP bytes: `10430058`
- Artifact ZIP SHA-256:
  `b39d7faf1799f088c9201980b2293c2979ff8bbb2937f0fc069606551ecb06ae`
- `SHA256SUMS.txt` SHA-256:
  `518c6ed517a073a32e2676d7e94910045488ba53daf21eaf4afab607c9a1383b`
- macOS archive SHA-256:
  `b0bc0e49cb41831ce70bcffbf15a336c20300b8367e9208b0d9ff5a30585ad99`
- Packaged server SHA-256:
  `bdf002df243d2839e4216c8a3a46c865aeaf703c22174f7409e396925d8c7695`
- Packaged helper SHA-256:
  `d4687810735400416c03e55a4e2a09cd0ccac493cbb88b424f9c7ba438eb8806`

The retained result binds the exact artifact, manifest, source, annotated tag,
workflow run, and run attempt. Before product execution, the lane also passed
its bounded manifest/archive checks, exact packaged server/helper checks,
universal `arm64`/`x86_64` architecture checks, strict code-signature checks,
permission preflight, clean detached harness-source checks, and 30-second quiet
seat stabilization.

## Result and withdrawal reason

`helper-results.json` is a schema-6 `failed-release-candidate` record. It
records 82 passing assertions and zero assertion failures before the required
external pointer boundary; that assertion count is not a passing lane result.
The exact run proved helper topology, exact-window observation, semantic
set-value and invoke, persistent ScreenCaptureKit streaming, target-routed
pixel input, sibling-window restoration, live-share continuity, and target
resize without disturbing the shared foreground, focus, cursor, or Space.

The runner then published the notification-only orange handoff request and
waited 300 seconds for separately authorized, sustained, click-free movement
of the shared pointer. No qualifying motion samples arrived. It failed closed
at `waitDeliberatePointerActivity` with `separately authorized deliberate
shared-pointer activity timed out`, dispatched no post-handoff action, stopped
the owned helper/server/fixture, and removed its scratch directory.

The sibling `quiet` lane files and `quiet-review.sha256` are intentionally not
copied into this withdrawn-attempt directory. Keeping the non-mergeable lanes
separate avoids presenting unrelated output as context for a passing release;
this branch makes no release-passing claim about that lane.

## Retained inventory

| File | Bytes | SHA-256 |
|---|---:|---|
| `computer-01-exact-window-observe.png` | 793272 | `9369a76a681d6ba2014261aeae9288d5a237157e18b44409fd997633984da26d` |
| `computer-02-semantic-set-value.png` | 810379 | `0a78c91028030e98f168731c868b1a162948527e78d9922e867a908c6ac6a281` |
| `computer-03-semantic-invoke.png` | 811249 | `b49fc0500c5746d420e76249c7043d3b120e8190a286e7361f837e2467300f0b` |
| `computer-04-persistent-scstream-start.png` | 683584 | `3b9d64e9c4a6854d7d47bc77b7508b4d08e24f9bdf20264030c36470f2e11a33` |
| `computer-05-live-share-pixel-action.png` | 684244 | `334ae1986299ae87fb0d2935fd6d9150e852d907ff4dd655e4014df94cff0bbb` |
| `computer-06-persistent-share-resize.png` | 549171 | `add0ac2d5c8c1db6ef8a8d81047323651fcb057883daf7d19d749388d42b0b26` |
| `helper-results.json` | 24868 | `2c881efc2d2ef30fa74a091418fcb5ee6730cc3adea8826e4a22f3734a1b70de` |
| `helper-rig.log` | 13381 | `b86a01fbdd3b2cbfe0bc860ded45d74efda3fb587d6978d76565f1faa9b518c0` |
| `operator/macos-pointer-concurrency-handoff-request.json` | 343 | `ab20ea176e956bf21f2f92028be0b3abe053a0898b6295887a8940e85d94bb01` |

The nine generated files are byte-for-byte copies of the retained lane. All
six screenshot byte counts and SHA-256 values match `helper-results.json`.
Visual review confirmed that every PNG contains only the deterministic LBB
fixture and no desktop, unrelated window, personal content, or secret. The
PNGs are 8-bit RGBA and expose no ancillary text, EXIF, profile, or timestamp
metadata markers.

A strict value-level leak scan found no bearer or authorization value, token,
email address, account-directory or temporary-directory pathname, command
line, credential, secret-bearing
URL, or title field. The exact machine records retain only scoped, ephemeral
test topology: two task-owned process IDs in the handoff request and one
task-owned target-window ID repeated across the six screenshot records. They
contain no user identity and are preserved because changing them would break
the exact evidence record. Raw cursor positions and platform activity counters
are explicitly not retained.
