# Withdrawn v0.12.8 Windows foreground-arm attempt

This directory preserves the failed interactive-Windows acceptance run for the
exact v0.12.8 release candidate. The same frozen candidate passed the macOS
187-assertion gate, but this Windows run did not receive its required arm click.
The protected publication job was canceled without approval, and no v0.12.8
GitHub Release was created. Nothing here is evidence for a shipped release.

## Frozen candidate binding

- Source commit: `532d603b3a7ab25edf71adcd68b8ed2f89b55983`
- Annotated tag object: `47e80c6cc7b09f73b0ea7a123147fb600598c5a8`
- Tag: `v0.12.8`
- Deploy workflow run: `32626882934`, attempt `1`
- Release-candidate artifact: `9490112028`
- Artifact ZIP bytes: `10354856`
- Artifact ZIP SHA-256:
  `aef500aabd07c66f20129ea6be2664fb486f7b145fd65af9784bfde307bc113e`
- `SHA256SUMS.txt` SHA-256:
  `45d550e394a38b56aeb8a67bde3c3792d3c1728d9088d61f9576622506273a28`
- Windows server SHA-256:
  `1f4bec49f08d3bb1dccf8953d94cc52fc3b4a781bda6015496c52908589dc35b`
- Windows helper SHA-256:
  `e3011cad9ca96bf4373f45a53063ff097708ed0172016b8d11baef8483d58c60`

Before execution, the coordinator and Windows host independently verified the
artifact API binding, raw artifact digest, exact five-file inventory, canonical
four-line checksum manifest, every payload hash, PE32+ x86-64 identity, clean
detached source, annotated tag, workflow/run-attempt identity, GitHub-hosted
runner identity, and all five GitHub attestations. A fresh short-path source
checkout used Git long-path support and passed the source, tag, cleanliness,
materialization, `git fsck --full`, runner self-test, and fixture self-test
gates before either supplied executable ran.

## Result and withdrawal reason

The packaged server and helper established the expected authenticated protocol
session and passed the exact worker-image, parent, and interactive-session
topology gate. The runner delivered one fresh test-owned arm generation, saved
the protocol readiness and request-delivery steps, and atomically published the
sanitized `operator/foreground-arm-request.json` notification. Its exact button
was enabled, its fixture-owned native topology matched, and the marker stated
both `notificationOnly: true` and `acceptedAsAuthority: false`.

The asynchronous relay reached the coordinator, but no human or separately
trusted Computer Use click reached the fixture before the bounded 300-second
interval ended. The received marker was therefore never created, and the
runner timed out at `wait-foreground-arm` while waiting for one exact click and
three stable native samples. It failed closed before the invariant baseline or
any product observation, capture, share, semantic, keyboard, pixel, or
cancellation action. No screenshot was produced, and stock-Chrome acceptance
was not started.

This run records an orchestration failure before any computer action was
attempted: the runner's observable handoff worked, but no external UI actuator
consumed it. Version 0.12.9 therefore prefers reserving its separately trusted
Windows Computer Use app-share before the runner starts, with a human-on-session
fallback. That is an orchestration preference, not new authority. Each run must
still bind one fresh action-time authorization to the request and refuse an
ambiguous frame, an already-`ARMED` state, or an unknown-outcome retry. The
marker remains notification only; the fixture's one-down, one-up,
acknowledgement, foreground, focus, cursor, desktop, and stable-sample proof
remains the only acceptance authority.

## Retained inventory

| File | Bytes | SHA-256 |
|---|---:|---|
| `fixture/fixture-events.ndjson` | 1024 | `9f7a994c51a5dd49702e38ad302749244c23bf5007fad55b3dc4581501515122` |
| `fixture/fixture-ready.json` | 207 | `7e664f44a15267eaafd3f5771e89e814794d2a58f939b37e8c2f896d8c2e8c2b` |
| `fixture/fixture-state.json` | 1337 | `1e4eb43c645201c01a4154aa5f8037d25e8f449d93a54626a4ff13fa445d7b96` |
| `operator/foreground-arm-request.json` | 837 | `8e3c365c08eb74d9fb6043e581e3b11dc1c517136ca9e093807e6852a83a0faf` |
| `steps/01-protocol-bound-helper-readiness.json` | 7864 | `50054c61bd8aed997ccaa5d3d0c3154581e983ab0490599381525c89e4948d0e` |
| `steps/02-foreground-arm-request-delivery.json` | 615 | `9b3f793dd38c64df589bab0821f4ab35d1add16a8e2ac36e4930c405900a0a4f` |
| `summary.json` | 12058 | `a06062501fc5050cf09b49799b64ec059118b5719b92259efe6606ebdd75726d` |

The generated records remain byte-for-byte identical to the retained Windows
host evidence. Cleanup completed without a reported issue: token-persistence
verification passed, no token was persisted, no unrelated process was
terminated, the one-shot recovery event was released, and an independent
post-run check found no owned candidate process or relevant loopback listener.
