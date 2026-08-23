# Withdrawn v0.12.7 Windows foreground-arm attempt

This directory preserves the failed interactive-Windows acceptance run for the
exact v0.12.7 release candidate. The candidate was withdrawn, its protected
publication job was canceled without approval, and no v0.12.7 GitHub Release
was created. Nothing in this directory is evidence for a shipped release.

## Frozen candidate binding

- Source commit: `0749953d9cd7d6228a4056f245294136c74a35d4`
- Annotated tag object: `750896e9a733b2036b1042337291bb3114482169`
- Tag: `v0.12.7`
- Deploy workflow run: `32623338611`, attempt `1`
- Release-candidate artifact: `9489187568`
- Artifact ZIP bytes: `10352306`
- Artifact ZIP SHA-256:
  `d7324ec8103cd58c910142703bfabfb7d849a97565b3fc5bc7dfb37b4cfa822f`
- `SHA256SUMS.txt` SHA-256:
  `e2f9a33fdee21e05d66960e63e2cd7757585b6f98969dab98a321cebda9df0a9`
- Windows server SHA-256:
  `2fd10fc0a079615d2fc5ffe7505c997cbbdcfac3ab3e1474cf29145cda0de232`
- Windows helper SHA-256:
  `6c1f8e970f6bad1117b72bbc8a940aeb9221a902660aef392c6d838e82c346b1`

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
topology gate. The runner then delivered one fresh test-owned arm generation to
the fixture and saved both the protocol-bound readiness and request-delivery
steps. The exact fixture button was enabled and the request count was one.

No human or scripted click occurred. The retained state records zero left-mouse
downs, zero left-mouse ups, and zero acknowledgements. The runner therefore
timed out at `wait-foreground-arm` while waiting for the fresh click receipt and
three stable native samples. It failed closed before the invariant baseline,
observation, capture, sharing, semantic action, keyboard action, pixel action,
or cancellation suites. No screenshot was created and stock-Chrome acceptance
was not started.

This is an unsatisfied interactive acceptance gate, not a computer-action
failure: no product mutation was attempted. A later candidate must make the
action-time handoff observable and complete the same exact click-bound receipt
without synthesizing input, stealing focus, or treating a shown window as
foreground proof.

## Retained inventory

| File | Bytes | SHA-256 |
|---|---:|---|
| `fixture/fixture-events.ndjson` | 1025 | `d8bb290dcddeacd7dffb7c9f985096791ee048bcf6014038755e1a12df8476a0` |
| `fixture/fixture-ready.json` | 209 | `1903aa7c34f3ad9c4088b4285e7cd5d66a1bf8a447f9c2ff9b76d553559e2d5f` |
| `fixture/fixture-state.json` | 1336 | `f9880cb98a1a7a9a686ec0f6c011f0664a1d0529408893d38ca3f170ce350cf8` |
| `steps/01-protocol-bound-helper-readiness.json` | 7863 | `9ee1dd964734eda9d216acbc8b955b172d12fe0707ae38a3f9de31b854b39865` |
| `steps/02-foreground-arm-request-delivery.json` | 616 | `391e7573abd178bd1bafccedbf235fd4c1d2d72d9f09402ed9d5cc5564f20900` |
| `summary.json` | 12110 | `639179342894f075f672e18bbbd7c62865bf842cc0751a10bd97a4ddc4ce1f95` |

The records preserve the exact retained bytes from the Windows host. Cleanup
completed without a reported issue: token-persistence verification passed, no
token was persisted, no unrelated process was terminated, the one-shot
recovery event was released, and an independent post-run check found no owned
candidate process or relevant loopback listener.
