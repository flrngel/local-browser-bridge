# Withdrawn v0.12.31 macOS exact-candidate attempt

This directory preserves the failed deliberate-concurrency macOS acceptance
lane for the exact v0.12.31 release candidate. The quiet lane passed, but the
required dual-lane macOS aggregate was never finalized. Windows and stock
Chrome acceptance were not started, no publication workflow was dispatched,
and no `v0.12.31` tag or GitHub Release was created. Nothing here is evidence
for a shipped release.

## Frozen candidate binding

- Source commit: `c54a1e95c8a83f3a701607e605eb88de2ede0507`
- Candidate workflow run: `32932886969`, attempt `1`
- Release-candidate artifact: `9594074941`
- Artifact ZIP bytes: `10431706`
- Artifact ZIP SHA-256:
  `9d6efc73f806300245cb5ef03cf8075b70a5110b8572abc659f80d40ede31d95`
- `SHA256SUMS.txt` SHA-256:
  `d8554f777947671abf0b0377f7134e1c30eda009a8060025034a97d5e7caa292`
- macOS archive SHA-256:
  `467bbe1ba3619691af396b36837c3eb39cbbdf89383390d2403ee84ac4b0faf2`
- Packaged server SHA-256:
  `6621fadaf2ce0f44463798028c5fd8a1e8e2175244f8e694f410bd1366856de1`
- Packaged helper SHA-256:
  `f4f19efa5c355b06390040932277c5b0736d09be7f353446338d55fe4cb62b13`

Before execution, the coordinator verified the candidate API binding, raw
artifact digest, exact five-file inventory, canonical manifest, payload
hashes, source/workflow/run-attempt identity, GitHub-hosted runner identity,
and all five GitHub attestations. The checkout was clean and detached at the
exact source. Both packaged macOS executables were universal `arm64`/`x86_64`,
reported version 0.12.31, and passed strict structural signature checks.

## Result and withdrawal reason

`helper-results.json` is a schema-8 `failed-release-candidate` record. It
retains 89 passing assertions and zero assertion failures before the fatal
gate; those prior checks do not make the candidate releasable. The lane proved
exact-window observation, semantic set-value and invoke, one persistent native
ScreenCaptureKit stream, target-routed pixel input, resize continuity, and an
exact separately authorized app/window/button action with unchanged foreground,
focus, cursor, HID counters, and active Space.

After the one-shot app-share receipt, the rig required a strictly newer,
same-share, same-target frame before granting short-lived product action
authority. No acceptable frame arrived inside the bounded refresh interval, so
it failed closed with
`share-after-app-share-action-authority share frame timed out`. The post-resize
product click was never dispatched, the completion marker was never published,
and the candidate was not retried or reused.

This is a release-gate timeout in the post-receipt frame-authority transition.
A successor must retain the fresh-frame, exact-share, exact-target, geometry,
and maximum-age checks while giving the native stream a robust bounded refresh
window.

## Retained inventory

The preceding quiet lane is retained under `quiet/` because both lanes belong
to the same exact candidate attempt. Its schema-8 result is
`passed-release-candidate` with 206 of 206 assertions passing. That isolated
lane pass cannot replace the missing dual-lane aggregate or make v0.12.31
releasable.

| Quiet-lane file | Bytes | SHA-256 |
|---|---:|---|
| `quiet/computer-01-exact-window-observe.png` | 787569 | `2a72d47079a6be8deca579a917eff27de580d269d461447ffc7be913df452c18` |
| `quiet/computer-02-semantic-set-value.png` | 804081 | `729894af747deb8dd040b9c870cc213e871443da0e229528006e606d30670447` |
| `quiet/computer-03-semantic-invoke.png` | 805679 | `daab05cbb9cf4a16622d3aa7140b47f045b4ae95520f71236f89eb8e4e0d572f` |
| `quiet/computer-04-persistent-scstream-start.png` | 676339 | `29a3578b066e8055b61203f15f906ff21555396f656b00d2554a61e10dedabb2` |
| `quiet/computer-05-live-share-pixel-action.png` | 676261 | `59551cd439e5914a8e85fa7052ba6187e3edde47902b31642fb51a2aadad6f48` |
| `quiet/computer-06-persistent-share-resize.png` | 540857 | `e059295ae05a10a9ef016fd73262646433dfc70c9545cc8f8a4b04a1ecef0c92` |
| `quiet/helper-results.json` | 124848 | `2637f8ee3022980a30c6434ad0d6463d9f9d52d9f058279ce9a9e3c3f28177bf` |
| `quiet/helper-rig.log` | 36654 | `ef2b26884cb06cbd318a68c1b6c87b3f663c9d8ce0bac5e96fc740f5966fe79c` |
| `quiet-review.sha256` | 789 | `1f73f9f65e49a151c9ffb772376d4a7a16d326f84a9725e46aa0f7a74c10944e` |

The failed deliberate-concurrency lane is retained at the attempt root:

| File | Bytes | SHA-256 |
|---|---:|---|
| `computer-01-exact-window-observe.png` | 792159 | `7ba8b4526863e51e713dfeb8bd0b9b65e767bcefd849240ae364fbc507417405` |
| `computer-02-semantic-set-value.png` | 808965 | `a082a5b5f908f5a46df8b3ba62618894d45f631358ddbfcb4affc8cbc150a604` |
| `computer-03-semantic-invoke.png` | 810377 | `1467ac31866b0fc25929e6bcba1b0e065d4eeb4814ea1ee9e904aafc7d3105f8` |
| `computer-04-persistent-scstream-start.png` | 682675 | `aa934a2344258f7d237667f178ea51c4cc084854fadd20988836d0f1e2d2953b` |
| `computer-05-live-share-pixel-action.png` | 682134 | `01821e98f4c2812d59b5361a70c43d873dd00f93e4707d76b4e69f1945181ae7` |
| `computer-06-persistent-share-resize.png` | 547371 | `3b2282637030919b57e9b3f83a01d8cf41ba00b233cc5c96f9425e968717f524` |
| `helper-results.json` | 30142 | `a97d86df79a4ebb2b1792337358d462b587b6dc90c8b8e5c35e1958f2510c6c4` |
| `helper-rig.log` | 17375 | `5ac81d0eb46de1c5cc8d0a28cf0f5fe82e52087f2cf87c8dc5ce2d721e28da34` |
| `operator/macos-app-share-concurrency-handoff-request.json` | 799 | `3b0bb541830e923652d3c837badb588d63634f16ff804558a587aa1ede2ae7a6` |
| `operator/macos-app-share-concurrency-handoff-start.json` | 443 | `d2ca01e230113e5e53865d5791499de683b5dfd8e1d5b9c0918a00b105b196a1` |

The six PNG hashes and dimensions match the screenshot records. Visual review
confirmed they contain only the deterministic fixture, with no desktop,
unrelated application, personal content, or secret. The result, log, and
notification-only operator markers contain no bearer/auth value, email,
user/home path, hostname, signed URL, credential, command-line secret, or
unrelated title. Cleanup stopped the owned helper, server, fixture, and prompt
and removed the scratch directory.
