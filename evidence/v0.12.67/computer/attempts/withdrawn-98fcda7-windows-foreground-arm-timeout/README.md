# Withdrawn v0.12.67 Windows foreground-arm attempt

This directory preserves the failed interactive-Windows acceptance run for the
exact v0.12.67 release candidate. The candidate passed the Windows trust gate,
but this Windows run did not receive its required arm click. The macOS
candidate lanes, stock-Chrome acceptance, tagging, publication, and a public
v0.12.67 GitHub Release did not follow. Nothing here is passing release
evidence.

## Frozen candidate binding

- Source commit: `4d47485499fa855fdfe788bf3a9317979727d1f4`
- Deploy workflow run: `33401392061`, attempt `1`
- Release-candidate artifact: `9761807586`
- Artifact ZIP bytes: `16146242`
- Artifact ZIP SHA-256:
  `98fcda7da90d9cf7d784b8754c66b7057751ea7d82a43fc761dbcf5337cc5c0f`
- `SHA256SUMS.txt` SHA-256:
  `48528a5e16205d134e3b6c8a71db73918e26166ac1c654ee4b34435e028617ad`
- Windows server SHA-256:
  `d4c7ef2249a56b642c5e8381e38dd742861df20681420bdf8ebc53af94c597b4`
- Windows helper SHA-256:
  `1481fa743c0703211f28755749d34ce31dca494204c68553c58aa9a81aa2cf1d`

Before execution, the Windows trust gate independently verified the workflow
and run-attempt identity, exact source commit, raw artifact digest, canonical
five-file inventory, checksum manifest, every payload digest, PE identity, all
five GitHub attestations, and the clean detached source checkout. The
coordinator then verified the packaged server and helper versions and hashes
before the one-shot reservation and runner launch.

## Result and withdrawal reason

The packaged server and helper established the expected authenticated protocol
session and passed the exact worker-image, parent, and interactive-session
topology gate. The runner delivered one fresh test-owned arm generation, saved
the protocol-readiness and request-delivery steps, and atomically published the
sanitized `operator/foreground-arm-request.json` notification. Its exact button
was enabled, its fixture-owned native topology matched, and the marker remained
notification only rather than acceptance authority.

No external click reached the fixture before the bounded 300-second interval
ended. The runner timed out at `wait-foreground-arm` while waiting for one exact
click and three stable native samples. It failed closed before the invariant
baseline or any product observation, capture, share, semantic, keyboard, pixel,
or cancellation action. The fixture recorded zero left-mouse-down attempts,
zero left-mouse-up attempts, and zero acknowledgements. No screenshot was
produced, and stock-Chrome acceptance was not started.

The persistent schema-2 reservation records `reserved-no-retry`, so this exact
candidate is terminal and must never be retried or relabeled. Version 0.12.68
must use fresh source, candidate bytes, artifact identity, reservation, and
acceptance evidence. The handoff marker still grants neither consent nor
authority; the fresh run must bind the separately authorized single click to
its own live request.

## Retained inventory

| File | Bytes | SHA-256 |
|---|---:|---|
| `fixture/fixture-events.ndjson` | 1024 | `6c45afdd286d5e68e8bff71dd695b2624e5ab24ebf35a602c847b99568e1805f` |
| `fixture/fixture-ready.json` | 201 | `d18e4d11cce9d47d9073349027b2ae8709fa9da27968fafddf18378781e6beda` |
| `fixture/fixture-state.json` | 1329 | `5810f843f7c92beb2785889762c6d9bfa8c9940a1c8adafe9052efaccff29397` |
| `operator/foreground-arm-request.json` | 1576 | `27f5638b97db111c4b22ad16109006b921a746c74d6694fb46c1ccf637fb58bb` |
| `steps/01-protocol-bound-helper-readiness.json` | 8878 | `9de6d4e46c30a24d0132978b9db8792c27ba25bc165c381b5c152e3e96cb6371` |
| `steps/02-foreground-arm-request-delivery.json` | 615 | `4d85714bbd03e6e2dcbba3d7f2fcd1174a852b0edea15772584719e58295c7fd` |
| `summary.json` | 17856 | `1bff73e45729958d7fa6cfa1821c9ce93697fdde7000c47aae764ec337a8e8c8` |
| `terminal-failure.json` | 311 | `c369a26243473cb955505128339fb310eab95f2ebe88c8f4eba1f0c016b181b9` |

The generated records remain byte-for-byte identical to the retained Windows
host evidence. Cleanup reported no issue: token-persistence verification
passed, no token was persisted, no unrelated process was terminated, and the
one-shot recovery event was released. No raw screenshot, candidate binary,
credential, bearer token, private path, or coordinator diagnostic is retained.
